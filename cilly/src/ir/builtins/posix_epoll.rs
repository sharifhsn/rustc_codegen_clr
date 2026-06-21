// ===========================================================================
// posix_epoll.rs — the epoll readiness cluster (included by posix_symbols.rs).
//
// Readiness over a *set* of fds is a per-fd `Socket.Poll(micros, SelectMode)`
// loop (D1's no-IList wall: NO Socket.Select). LIBC_SHIM_SCOPE §3.3.
//
// MULTI-FD (Cap-2): each epoll instance owns an INTEREST SET = a managed
// `Dictionary<i32, object>` (key = registered fd, value = a boxed `RclEpollReg`
// {events, token}). The dict is pinned by `GCHandle.Alloc` and its `IntPtr`
// stored in the EPOLL fd-table entry's `handle` field (the same GCHandle round-
// trip sockets/files use). epoll_ctl ADD/MOD `set_Item`s; DEL `Remove`s.
// epoll_wait ENUMERATES the dict (the IDictionaryEnumerator template from
// utilis.rs) and `Socket.Poll`s each fd, packing every ready fd into the
// caller's `epoll_event[]` array (stride 12, `#[repr(C, packed)]`). This is the
// Cap-1 single-fd path generalized to the SET mio registers (listener + each
// connection + the Waker).
//
// Linux epoll constants:
//   EPOLL_CTL_ADD=1, MOD=3, DEL=2; EPOLLIN=0x001, EPOLLOUT=0x004.
//   `struct epoll_event` is #[repr(C, packed)]: events: u32 @0, data: u64 @4
//   (12-byte stride on x86_64).
// ===========================================================================

const EPOLLIN: i32 = 0x001;
const EPOLLOUT: i32 = 0x004;
const EPOLL_CTL_ADD: i32 = 1;
const EPOLL_CTL_DEL: i32 = 2;
const EPOLL_CTL_MOD: i32 = 3;
/// `sizeof(epoll_event)` on x86_64-linux (`#[repr(C, packed)]`): u32 + u64 = 12.
const EPOLL_EVENT_STRIDE: i64 = 12;

/// The epoll instance's interest dict is a `Dictionary<i32, object>` (fd ->
/// boxed RclEpollReg), the same managed type as the fd-table itself.
fn epoll_dict_type(asm: &mut Assembly) -> Interned<ClassRef> {
    fd_dict(asm)
}

/// Load the interest dict for the epoll instance whose entry is in local
/// `entry_local`: `(Dictionary<i32,object>)handle_to_obj((isize)entry.handle)`.
fn load_epoll_dict(asm: &mut Assembly, entry_local: u32) -> Interned<CILNode> {
    let handle_field = fd_entry_handle_field(asm);
    let e = asm.alloc_node(CILNode::LdLoc(entry_local));
    let h = asm.alloc_node(CILNode::LdField { addr: e, field: handle_field });
    let h_isize = asm.alloc_node(CILNode::PtrCast(h, Box::new(PtrCastRes::ISize)));
    let handle_to_obj = asm.alloc_string("handle_to_obj");
    let main_module = asm.main_module();
    let h2o = asm.class_ref(*main_module).clone().static_mref(
        &[Type::Int(Int::ISize)],
        Type::PlatformObject,
        handle_to_obj,
        asm,
    );
    let obj = asm.alloc_node(CILNode::call(h2o, [h_isize]));
    let dict = epoll_dict_type(asm);
    let dict_ty = asm.alloc_type(Type::ClassRef(dict));
    asm.alloc_node(CILNode::CheckedCast(obj, dict_ty))
}

/// `epoll_create1(flags) -> i32` — allocate the per-instance interest dict,
/// pin it with `GCHandle.Alloc`, and register an EPOLL fd carrying that handle.
fn insert_epoll_create1(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("epoll_create1");
    let generator = move |_, asm: &mut Assembly| {
        // d = new Dictionary<i32,object>();
        let dict = epoll_dict_type(asm);
        let dict_ctor = asm[dict].clone().ctor(&[], asm);
        let new_dict = asm.alloc_node(CILNode::call(dict_ctor, []));
        // handle = (void*)GCHandle.Alloc(d).
        let store_dict = asm.alloc_root(CILRoot::StLoc(0, new_dict));
        let handle = CILNode::LdLoc(0).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let void = asm.alloc_type(Type::Void);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        // fd = rcl_fdtable_insert(handle, EPOLL, 0); ret fd.
        let fd = call_fdtable_insert(asm, handle, FD_KIND_EPOLL, 0);
        let ret = asm.alloc_root(CILRoot::Ret(fd));

        let dict_ty = asm.alloc_type(Type::ClassRef(dict));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store_dict, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("dict")), dict_ty)],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `epoll_ctl(epfd, op, fd, *event) -> i32` — ADD/MOD store a fresh
/// `RclEpollReg{events,token}` for `fd` in the instance's interest dict; DEL
/// removes `fd`. Returns 0.
fn insert_epoll_ctl(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("epoll_ctl");
    let generator = move |_, asm: &mut Assembly| {
        // entry = (RclFdEntry)tbl[epfd]; dict = load_epoll_dict(entry).
        let entry = entry_of_fd(asm, 0);
        let store_entry = asm.alloc_root(CILRoot::StLoc(0, entry));
        let dict = load_epoll_dict(asm, 0);
        let store_dict = asm.alloc_root(CILRoot::StLoc(1, dict));

        // op == EPOLL_CTL_DEL -> goto 1 (remove) else fall to set.
        let op = asm.alloc_node(CILNode::LdArg(1));
        let del_c = asm.alloc_node(Const::I32(EPOLL_CTL_DEL));
        let br_del = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(op, del_c)),
        ))));
        let goto_set = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // block 1 (DEL): dict.Remove(fd); goto 3.
        let dict_d = epoll_dict_type(asm);
        let remove_name = asm.alloc_string("Remove");
        let remove = asm.class_ref(dict_d).clone().virtual_mref(
            &[Type::PlatformGeneric(0, GenericKind::TypeGeneric)],
            Type::Bool,
            remove_name,
            asm,
        );
        let dict1 = asm.alloc_node(CILNode::LdLoc(1));
        let fd_del = asm.alloc_node(CILNode::LdArg(2));
        let removed = asm.alloc_node(CILNode::call(remove, [dict1, fd_del]));
        let pop_rm = asm.alloc_root(CILRoot::Pop(removed));
        let goto_done1 = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // block 2 (ADD/MOD): read events@0(u32) + data@4(u64) from *event;
        //   dict.set_Item(fd, new RclEpollReg(events, token)); goto 3.
        let ev_ptr = asm.alloc_node(CILNode::LdArg(3));
        let u32_ty = asm.alloc_type(Type::Int(Int::U32));
        let ev_ptr_u32 = asm.alloc_node(CILNode::PtrCast(ev_ptr, Box::new(PtrCastRes::Ptr(u32_ty))));
        let events = asm.alloc_node(CILNode::LdInd { addr: ev_ptr_u32, tpe: u32_ty, volatile: false });
        let events_i32 = asm.int_cast(events, Int::I32, ExtendKind::ZeroExtend);
        // data@4: (u64*)((i8*)ev + 4).
        let ev_ptr2 = asm.alloc_node(CILNode::LdArg(3));
        let ev_isize = asm.alloc_node(CILNode::PtrCast(ev_ptr2, Box::new(PtrCastRes::ISize)));
        let four = asm.alloc_node(Const::ISize(4));
        let data_addr = asm.alloc_node(CILNode::BinOp(ev_isize, four, BinOp::Add));
        let u64_ty = asm.alloc_type(Type::Int(Int::U64));
        let data_addr = asm.alloc_node(CILNode::PtrCast(data_addr, Box::new(PtrCastRes::Ptr(u64_ty))));
        let token = asm.alloc_node(CILNode::LdInd { addr: data_addr, tpe: u64_ty, volatile: false });
        let token_i64 = asm.int_cast(token, Int::I64, ExtendKind::ZeroExtend);
        // reg = new RclEpollReg(events, token).
        let reg = new_epoll_reg(asm, events_i32, token_i64);
        let store_reg = asm.alloc_root(CILRoot::StLoc(2, reg));
        // dict.set_Item(fd, reg).
        let set_item = asm.alloc_string("set_Item");
        let set = asm.class_ref(dict_d).clone().virtual_mref(
            &[
                Type::PlatformGeneric(0, GenericKind::TypeGeneric),
                Type::PlatformGeneric(1, GenericKind::TypeGeneric),
            ],
            Type::Void,
            set_item,
            asm,
        );
        let dict2 = asm.alloc_node(CILNode::LdLoc(1));
        let fd_add = asm.alloc_node(CILNode::LdArg(2));
        let reg_l = asm.alloc_node(CILNode::LdLoc(2));
        let do_set = asm.alloc_root(CILRoot::call(set, [dict2, fd_add, reg_l]));
        let goto_done2 = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // block 3: ret 0.
        let zero = asm.alloc_node(Const::I32(0));
        let ret = asm.alloc_root(CILRoot::Ret(zero));

        let _ = (EPOLL_CTL_ADD, EPOLL_CTL_MOD);
        let entry_cls = fd_entry_class(asm);
        let entry_ty = asm.alloc_type(Type::ClassRef(entry_cls));
        let dict_ty = asm.alloc_type(Type::ClassRef(dict_d));
        let reg_cls = epoll_reg_class(asm);
        let reg_ty = asm.alloc_type(Type::ClassRef(reg_cls));
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_entry, store_dict, br_del, goto_set], 0, None),
                BasicBlock::new(vec![pop_rm, goto_done1], 1, None),
                BasicBlock::new(vec![store_reg, do_set, goto_done2], 2, None),
                BasicBlock::new(vec![ret], 3, None),
            ],
            locals: vec![
                (Some(asm.alloc_string("entry")), entry_ty),
                (Some(asm.alloc_string("dict")), dict_ty),
                (Some(asm.alloc_string("reg")), reg_ty),
            ],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `epoll_wait(epfd, *events, maxevents, timeout_ms) -> i32` — enumerate the
/// instance's interest dict; per registered fd `Socket.Poll` its socket; pack
/// every ready fd's `epoll_event` (events@0, token@4) into the caller's array;
/// return the count. The HEAD fd absorbs the real timeout (Poll blocks up to
/// `micros`); the rest probe with micros=0. A single sweep is sufficient for the
/// mio loop (mio itself re-polls). timeout_ms<0 → infinite on the head fd.
///
/// SelectMode per fd: (events & EPOLLOUT)!=0 → 1(Write) else 0(Read).
fn insert_epoll_wait(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("epoll_wait");
    let generator = move |_, asm: &mut Assembly| {
        let void_ptr = asm.nptr(Type::Void);
        let dict_cls = epoll_dict_type(asm);
        let reg_cls = epoll_reg_class(asm);
        let keyval_tpe = ClassRef::dictionary_entry(asm);

        // ---- local layout ----
        // 0 entry, 1 dict, 2 iter, 3 keyval, 4 count, 5 fd, 6 reg, 7 h, 8 mode,
        // 9 micros, 10 ready, 11 first (1 on the first probe -> absorbs timeout).
        const L_ENTRY: u32 = 0;
        const L_DICT: u32 = 1;
        const L_ITER: u32 = 2;
        const L_KEYVAL: u32 = 3;
        const L_COUNT: u32 = 4;
        const L_FD: u32 = 5;
        const L_REG: u32 = 6;
        const L_H: u32 = 7;
        const L_MODE: u32 = 8;
        const L_MICROS: u32 = 9;
        const L_READY: u32 = 10;
        const L_FIRST: u32 = 11;

        // ---- block 0: init ----
        let entry = entry_of_fd(asm, 0);
        let store_entry = asm.alloc_root(CILRoot::StLoc(L_ENTRY, entry));
        let dict = load_epoll_dict(asm, L_ENTRY);
        let store_dict = asm.alloc_root(CILRoot::StLoc(L_DICT, dict));
        let zero_count = asm.alloc_node(Const::I32(0));
        let store_count = asm.alloc_root(CILRoot::StLoc(L_COUNT, zero_count));
        let one_first = asm.alloc_node(Const::I32(1));
        let store_first = asm.alloc_root(CILRoot::StLoc(L_FIRST, one_first));
        // iter = dict.GetEnumerator() (via IDictionary.GetEnumerator).
        let i_dictionary = ClassRef::i_dictionary(asm);
        let dictionary_iterator = ClassRef::dictionary_iterator(asm);
        let get_enum_name = asm.alloc_string("GetEnumerator");
        let get_enum = asm.class_ref(i_dictionary).clone().virtual_mref(
            &[],
            Type::ClassRef(dictionary_iterator),
            get_enum_name,
            asm,
        );
        let dict_l = asm.alloc_node(CILNode::LdLoc(L_DICT));
        let dict_as_idict = {
            let idict_ty = asm.alloc_type(Type::ClassRef(i_dictionary));
            asm.alloc_node(CILNode::CheckedCast(dict_l, idict_ty))
        };
        let iter = asm.alloc_node(CILNode::call(get_enum, [dict_as_idict]));
        let store_iter = asm.alloc_root(CILRoot::StLoc(L_ITER, iter));
        let goto_loop = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

        // ---- block 1: loop head — if !iter.MoveNext() goto 5 (ret count) ----
        let i_enumerator = ClassRef::i_enumerator(asm);
        let move_next_name = asm.alloc_string("MoveNext");
        let move_next = asm.class_ref(i_enumerator).clone().virtual_mref(
            &[],
            Type::Bool,
            move_next_name,
            asm,
        );
        let iter_l = asm.alloc_node(CILNode::LdLoc(L_ITER));
        let iter_as_enum = {
            let enum_ty = asm.alloc_type(Type::ClassRef(i_enumerator));
            asm.alloc_node(CILNode::CheckedCast(iter_l, enum_ty))
        };
        let has_next = asm.alloc_node(CILNode::call(move_next, [iter_as_enum]));
        let br_end = asm.alloc_root(CILRoot::Branch(Box::new((
            5,
            0,
            Some(BranchCond::False(has_next)),
        ))));
        let goto_body = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // ---- block 2: read (fd, reg) from iterator; compute mode + micros ----
        // keyval = (DictionaryEntry)iter.get_Current; fd=(i32)keyval.get_Key;
        //   reg=(RclEpollReg)keyval.get_Value.
        let get_current_name = asm.alloc_string("get_Current");
        let get_current = asm.class_ref(i_enumerator).clone().virtual_mref(
            &[],
            Type::PlatformObject,
            get_current_name,
            asm,
        );
        let iter_l2 = asm.alloc_node(CILNode::LdLoc(L_ITER));
        let iter_as_enum2 = {
            let enum_ty = asm.alloc_type(Type::ClassRef(i_enumerator));
            asm.alloc_node(CILNode::CheckedCast(iter_l2, enum_ty))
        };
        let cur = asm.alloc_node(CILNode::call(get_current, [iter_as_enum2]));
        let keyval_ty = asm.alloc_type(Type::ClassRef(keyval_tpe));
        let unboxed = asm.unbox_any(cur, keyval_ty);
        let store_keyval = asm.alloc_root(CILRoot::StLoc(L_KEYVAL, unboxed));
        // get_Key / get_Value take &DictionaryEntry (LdLocA), return object.
        let keyval_ref = asm.nref(Type::ClassRef(keyval_tpe));
        let get_key_name = asm.alloc_string("get_Key");
        let key_sig = asm.sig([keyval_ref], Type::PlatformObject);
        let get_key = asm.alloc_methodref(MethodRef::new(
            keyval_tpe,
            get_key_name,
            key_sig,
            MethodKind::Instance,
            [].into(),
        ));
        let kv1 = asm.alloc_node(CILNode::LdLocA(L_KEYVAL));
        let key_obj = asm.alloc_node(CILNode::call(get_key, [kv1]));
        // fd = (i32)(boxed i32) — unbox.
        let i32_ty = asm.alloc_type(Type::Int(Int::I32));
        let fd_unboxed = asm.unbox_any(key_obj, i32_ty);
        let store_fd = asm.alloc_root(CILRoot::StLoc(L_FD, fd_unboxed));
        let get_value_name = asm.alloc_string("get_Value");
        let value_sig = asm.sig([keyval_ref], Type::PlatformObject);
        let get_value = asm.alloc_methodref(MethodRef::new(
            keyval_tpe,
            get_value_name,
            value_sig,
            MethodKind::Instance,
            [].into(),
        ));
        let kv2 = asm.alloc_node(CILNode::LdLocA(L_KEYVAL));
        let val_obj = asm.alloc_node(CILNode::call(get_value, [kv2]));
        let reg_ty = asm.alloc_type(Type::ClassRef(reg_cls));
        let reg = asm.alloc_node(CILNode::CheckedCast(val_obj, reg_ty));
        let store_reg = asm.alloc_root(CILRoot::StLoc(L_REG, reg));
        // h = rcl_fdtable_handle(fd).
        let fd_l = asm.alloc_node(CILNode::LdLoc(L_FD));
        let h = call_fdtable_handle(asm, fd_l);
        let store_h = asm.alloc_root(CILRoot::StLoc(L_H, h));
        // mode = (reg.events & EPOLLOUT)!=0 ? 1(Write) : 0(Read).
        let events_field = epoll_reg_events_field(asm);
        let reg_l = asm.alloc_node(CILNode::LdLoc(L_REG));
        let ev = asm.alloc_node(CILNode::LdField { addr: reg_l, field: events_field });
        let out_mask = asm.alloc_node(Const::I32(EPOLLOUT));
        let masked = asm.alloc_node(CILNode::BinOp(ev, out_mask, BinOp::And));
        let zero_m = asm.alloc_node(Const::I32(0));
        // mode = 1 - (masked == 0)  (CIL `not` is bitwise; logical-negate by arith).
        let is_zero = asm.alloc_node(CILNode::BinOp(masked, zero_m, BinOp::Eq));
        let is_zero = asm.int_cast(is_zero, Int::I32, ExtendKind::ZeroExtend);
        let one_m = asm.alloc_node(Const::I32(1));
        let mode = asm.alloc_node(CILNode::BinOp(one_m, is_zero, BinOp::Sub));
        let store_mode = asm.alloc_root(CILRoot::StLoc(L_MODE, mode));
        // micros: head fd (first==1) absorbs timeout; others probe with 0.
        //   if first==0 -> micros=0 (goto 6 directly).
        let first_l = asm.alloc_node(CILNode::LdLoc(L_FIRST));
        let zero_first = asm.alloc_node(Const::I32(0));
        let br_not_first = asm.alloc_root(CILRoot::Branch(Box::new((
            6,
            0,
            Some(BranchCond::Eq(first_l, zero_first)),
        ))));
        let goto_first_timeout = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // ---- block 3: first fd — compute micros from timeout, clear `first` ----
        // micros = timeout<0 ? -1 : timeout*1000; first = 0; goto 7 (poll).
        let timeout = asm.alloc_node(CILNode::LdArg(3));
        let thousand = asm.alloc_node(Const::I32(1000));
        let micros = asm.alloc_node(CILNode::BinOp(timeout, thousand, BinOp::Mul));
        let store_micros = asm.alloc_root(CILRoot::StLoc(L_MICROS, micros));
        let zero_set_first = asm.alloc_node(Const::I32(0));
        let clear_first = asm.alloc_root(CILRoot::StLoc(L_FIRST, zero_set_first));
        let timeout2 = asm.alloc_node(CILNode::LdArg(3));
        let zero_t = asm.alloc_node(Const::I32(0));
        let br_neg = asm.alloc_root(CILRoot::Branch(Box::new((
            4,
            0,
            Some(BranchCond::Lt(timeout2, zero_t, crate::ir::cilroot::CmpKind::Signed)),
        ))));
        let goto_poll_first = asm.alloc_root(CILRoot::Branch(Box::new((7, 0, None))));

        // ---- block 4: micros = -1 (infinite); goto 7 ----
        let neg1 = asm.alloc_node(Const::I32(-1));
        let store_neg = asm.alloc_root(CILRoot::StLoc(L_MICROS, neg1));
        let goto_poll2 = asm.alloc_root(CILRoot::Branch(Box::new((7, 0, None))));

        // ---- block 6: non-first fd — micros = 0; goto 7 ----
        let zero_micros = asm.alloc_node(Const::I32(0));
        let store_zero_micros = asm.alloc_root(CILRoot::StLoc(L_MICROS, zero_micros));
        let goto_poll3 = asm.alloc_root(CILRoot::Branch(Box::new((7, 0, None))));

        // ---- block 7: ready = poll(h, micros, mode); if 0 -> loop (1) else 8 ----
        let poll = dotnet_mref(
            asm,
            "rcl_dotnet_socket_poll",
            &[void_ptr, Type::Int(Int::I32), Type::Int(Int::I32)],
            Type::Int(Int::I32),
        );
        let hh = asm.alloc_node(CILNode::LdLoc(L_H));
        let micros_l = asm.alloc_node(CILNode::LdLoc(L_MICROS));
        let mode_l = asm.alloc_node(CILNode::LdLoc(L_MODE));
        let ready = asm.alloc_node(CILNode::call(poll, [hh, micros_l, mode_l]));
        let store_ready = asm.alloc_root(CILRoot::StLoc(L_READY, ready));
        let ready_l = asm.alloc_node(CILNode::LdLoc(L_READY));
        let zero_r = asm.alloc_node(Const::I32(0));
        let br_notready = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(ready_l, zero_r)),
        ))));
        let goto_write = asm.alloc_root(CILRoot::Branch(Box::new((8, 0, None))));

        // ---- block 8: ready — write events[count] (events@0, token@4); count++; loop ----
        // base = (i8*)events + count*12.
        let evbase = asm.alloc_node(CILNode::LdArg(1));
        let ev_isize_w = asm.alloc_node(CILNode::PtrCast(evbase, Box::new(PtrCastRes::ISize)));
        let count_l = asm.alloc_node(CILNode::LdLoc(L_COUNT));
        let count_i64 = asm.int_cast(count_l, Int::I64, ExtendKind::SignExtend);
        let stride = asm.alloc_node(Const::I64(EPOLL_EVENT_STRIDE));
        let off = asm.alloc_node(CILNode::BinOp(count_i64, stride, BinOp::Mul));
        let base = asm.alloc_node(CILNode::BinOp(ev_isize_w, off, BinOp::Add));
        let store_base = asm.alloc_root(CILRoot::StLoc(L_H, base)); // reuse L_H as scratch base
        // events@0 = reg.events (u32).
        let events_field2 = epoll_reg_events_field(asm);
        let reg_l2 = asm.alloc_node(CILNode::LdLoc(L_REG));
        let regev = asm.alloc_node(CILNode::LdField { addr: reg_l2, field: events_field2 });
        let regev_u32 = asm.int_cast(regev, Int::U32, ExtendKind::ZeroExtend);
        let u32_ty = asm.alloc_type(Type::Int(Int::U32));
        let base0 = asm.alloc_node(CILNode::LdLoc(L_H));
        let ev0_ptr = asm.alloc_node(CILNode::PtrCast(base0, Box::new(PtrCastRes::Ptr(u32_ty))));
        let st_events = asm.alloc_root(CILRoot::StInd(Box::new((ev0_ptr, regev_u32, Type::Int(Int::U32), false))));
        // token@4 = reg.token (u64).
        let token_field = epoll_reg_token_field(asm);
        let reg_l3 = asm.alloc_node(CILNode::LdLoc(L_REG));
        let regtok = asm.alloc_node(CILNode::LdField { addr: reg_l3, field: token_field });
        let regtok_u64 = asm.int_cast(regtok, Int::U64, ExtendKind::ZeroExtend);
        let base1 = asm.alloc_node(CILNode::LdLoc(L_H));
        let base1_isize = asm.alloc_node(CILNode::PtrCast(base1, Box::new(PtrCastRes::ISize)));
        let four_w = asm.alloc_node(Const::ISize(4));
        let tok_addr = asm.alloc_node(CILNode::BinOp(base1_isize, four_w, BinOp::Add));
        let u64_ty = asm.alloc_type(Type::Int(Int::U64));
        let tok_ptr = asm.alloc_node(CILNode::PtrCast(tok_addr, Box::new(PtrCastRes::Ptr(u64_ty))));
        let st_token = asm.alloc_root(CILRoot::StInd(Box::new((tok_ptr, regtok_u64, Type::Int(Int::U64), false))));
        // count++.
        let count_l2 = asm.alloc_node(CILNode::LdLoc(L_COUNT));
        let one_c = asm.alloc_node(Const::I32(1));
        let inc = asm.alloc_node(CILNode::BinOp(count_l2, one_c, BinOp::Add));
        let store_inc = asm.alloc_root(CILRoot::StLoc(L_COUNT, inc));
        // if count >= maxevents -> ret (block 5) else continue loop (block 1).
        let count_l3 = asm.alloc_node(CILNode::LdLoc(L_COUNT));
        let maxev = asm.alloc_node(CILNode::LdArg(2));
        let br_full = asm.alloc_root(CILRoot::Branch(Box::new((
            5,
            0,
            Some(BranchCond::Ge(count_l3, maxev, crate::ir::cilroot::CmpKind::Signed)),
        ))));
        let goto_loop2 = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

        // ---- block 5: ret count ----
        let count_ret = asm.alloc_node(CILNode::LdLoc(L_COUNT));
        let ret = asm.alloc_root(CILRoot::Ret(count_ret));

        let _ = EPOLLIN;
        let entry_cls = fd_entry_class(asm);
        let entry_ty = asm.alloc_type(Type::ClassRef(entry_cls));
        let dict_ty = asm.alloc_type(Type::ClassRef(dict_cls));
        let dict_iter_cls = ClassRef::dictionary_iterator(asm);
        let iter_ty = asm.alloc_type(Type::ClassRef(dict_iter_cls));
        let keyval_ty2 = asm.alloc_type(Type::ClassRef(keyval_tpe));
        let i32_ty2 = asm.alloc_type(Type::Int(Int::I32));
        let reg_ty2 = asm.alloc_type(Type::ClassRef(reg_cls));
        let void_ptr_idx = asm.alloc_type(void_ptr);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(
                    vec![store_entry, store_dict, store_count, store_first, store_iter, goto_loop],
                    0,
                    None,
                ),
                BasicBlock::new(vec![br_end, goto_body], 1, None),
                BasicBlock::new(
                    vec![
                        store_keyval, store_fd, store_reg, store_h, store_mode,
                        br_not_first, goto_first_timeout,
                    ],
                    2,
                    None,
                ),
                BasicBlock::new(vec![store_micros, clear_first, br_neg, goto_poll_first], 3, None),
                BasicBlock::new(vec![store_neg, goto_poll2], 4, None),
                BasicBlock::new(vec![ret], 5, None),
                BasicBlock::new(vec![store_zero_micros, goto_poll3], 6, None),
                BasicBlock::new(vec![store_ready, br_notready, goto_write], 7, None),
                BasicBlock::new(
                    vec![store_base, st_events, st_token, store_inc, br_full, goto_loop2],
                    8,
                    None,
                ),
            ],
            locals: vec![
                (Some(asm.alloc_string("entry")), entry_ty),
                (Some(asm.alloc_string("dict")), dict_ty),
                (Some(asm.alloc_string("iter")), iter_ty),
                (Some(asm.alloc_string("keyval")), keyval_ty2),
                (Some(asm.alloc_string("count")), i32_ty2),
                (Some(asm.alloc_string("fd")), i32_ty2),
                (Some(asm.alloc_string("reg")), reg_ty2),
                (Some(asm.alloc_string("h")), void_ptr_idx),
                (Some(asm.alloc_string("mode")), i32_ty2),
                (Some(asm.alloc_string("micros")), i32_ty2),
                (Some(asm.alloc_string("ready")), i32_ty2),
                (Some(asm.alloc_string("first")), i32_ty2),
            ],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `eventfd(initval, flags) -> i32` — the mio Waker primitive. The MUST-LAND
/// `pal_mio` never fires the Waker (single managed thread, no cross-thread wake),
/// so a placeholder EVENTFD fd (handle 0) that registers + is never ready is
/// sufficient: mio constructs the Waker lazily only on `Waker::new`, which
/// pal_mio does not call. A self-connected loopback socket eventfd is the
/// tokio-net upgrade (LIBC_SHIM_SCOPE §2.2).
fn insert_eventfd(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("eventfd");
    let generator = move |_, asm: &mut Assembly| {
        let void_ptr = asm.nptr(Type::Void);
        let null = asm.alloc_node(Const::ISize(0));
        let null = asm.cast_ptr(null, void_ptr);
        let fd = call_fdtable_insert(asm, null, FD_KIND_EVENTFD, 0);
        let ret = asm.alloc_root(CILRoot::Ret(fd));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

fn insert_epoll(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    insert_epoll_create1(asm, patcher);
    insert_epoll_ctl(asm, patcher);
    insert_epoll_wait(asm, patcher);
    insert_eventfd(asm, patcher);
}
