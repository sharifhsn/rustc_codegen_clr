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
/// return the count. The HEAD fd absorbs the timeout (the READ poll blocks up to
/// `min(timeout, POLL_CAP_MICROS)`); the rest probe with micros=0. timeout_ms<0
/// caps at POLL_CAP_MICROS too (NOT infinite — a single socket's infinite Poll
/// would starve the multi-fd set / deadlock the tokio reactor). mio re-polls.
///
/// Per fd we poll BOTH `SelectRead` AND `SelectWrite` and OR the results: mio
/// registers a fd with its full interest mask (a stream is `EPOLLIN|EPOLLOUT`),
/// so a single-mode poll mis-classified accept-readiness as write-readiness and
/// hung. EPOLLET-flagged fds are edge-gated (report only on a rising readiness
/// edge) via `RclEpollReg.last_ready` — else a never-drained edge-triggered fd
/// (the tokio eventfd waker) re-fires every sweep and busy-spins the reactor.
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
        // NOTE (the tokio accept bug): mio registers a fd with the FULL interest
        // mask it wants, e.g. a listener for accept is `EPOLLIN|EPOLLOUT|EPOLLRDHUP`
        // (events=0x80002005). The OLD code derived a SINGLE poll mode = Write if
        // EPOLLOUT set — so it polled the LISTENER for SelectWrite (never true for
        // accept) and never reported the pending connection -> accept().await hung.
        // FIX: poll BOTH SelectRead (if EPOLLIN) and SelectWrite (if EPOLLOUT) and
        // OR the results (block 7 reads, block 12 writes). `store_mode` is gone;
        // L_MODE is repurposed as the edge-trigger `prev` scratch in block 11.
        let store_mode = {
            // placeholder no-op root to keep block-2 shape (init L_MODE = 0).
            let z = asm.alloc_node(Const::I32(0));
            asm.alloc_root(CILRoot::StLoc(L_MODE, z))
        };
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

        // ---- block 3: first fd — compute a BOUNDED blocking micros, clear `first` ----
        // CRITICAL (tokio deadlock fix): `Socket.Poll(h, -1, mode)` blocks FOREVER
        // on a SINGLE socket. With a multi-fd interest set (tokio: waker + listener
        // + connections) the HEAD dict entry is arbitrary, so blocking infinitely on
        // it deadlocks the reactor when readiness is on a DIFFERENT fd. Instead the
        // head fd blocks for at most a small CAP (POLL_CAP_MICROS); every sweep then
        // returns within the cap (count possibly 0) and mio/tokio simply re-call
        // epoll_wait — bounded poll latency, NO deadlock, NO busy-spin (the cap is
        // the block, not a spin). The requested timeout is honoured up to the cap:
        //   micros = (timeout<0) ? CAP : min(timeout*1000, CAP).
        // (A 0 timeout still maps to 0 via the min -> a pure non-blocking sweep.)
        const POLL_CAP_MICROS: i32 = 20_000; // 20ms head-fd block ceiling.
        let zero_set_first = asm.alloc_node(Const::I32(0));
        let clear_first = asm.alloc_root(CILRoot::StLoc(L_FIRST, zero_set_first));
        // micros = timeout < 0 ? CAP : timeout*1000  (block 3); then clamp in block 4.
        let timeout2 = asm.alloc_node(CILNode::LdArg(3));
        let zero_t = asm.alloc_node(Const::I32(0));
        let br_neg = asm.alloc_root(CILRoot::Branch(Box::new((
            4,
            0,
            Some(BranchCond::Lt(timeout2, zero_t, crate::ir::cilroot::CmpKind::Signed)),
        ))));
        // (timeout >= 0): micros = timeout*1000; goto 9 (clamp to CAP).
        let timeout = asm.alloc_node(CILNode::LdArg(3));
        let thousand = asm.alloc_node(Const::I32(1000));
        let micros = asm.alloc_node(CILNode::BinOp(timeout, thousand, BinOp::Mul));
        let store_micros = asm.alloc_root(CILRoot::StLoc(L_MICROS, micros));
        let goto_clamp = asm.alloc_root(CILRoot::Branch(Box::new((9, 0, None))));

        // ---- block 4: timeout<0 -> micros = CAP; goto 7 (already at the ceiling) ----
        let cap1 = asm.alloc_node(Const::I32(POLL_CAP_MICROS));
        let store_neg = asm.alloc_root(CILRoot::StLoc(L_MICROS, cap1));
        let goto_poll2 = asm.alloc_root(CILRoot::Branch(Box::new((7, 0, None))));

        // ---- block 9: clamp micros to CAP (micros = min(micros, CAP)); goto 7 ----
        let micros_c = asm.alloc_node(CILNode::LdLoc(L_MICROS));
        let cap2 = asm.alloc_node(Const::I32(POLL_CAP_MICROS));
        let over = asm.alloc_root(CILRoot::Branch(Box::new((
            10,
            0,
            Some(BranchCond::Gt(micros_c, cap2, crate::ir::cilroot::CmpKind::Signed)),
        ))));
        let goto_poll_clamped = asm.alloc_root(CILRoot::Branch(Box::new((7, 0, None))));
        // ---- block 10: micros = CAP; goto 7 ----
        let cap3 = asm.alloc_node(Const::I32(POLL_CAP_MICROS));
        let store_cap = asm.alloc_root(CILRoot::StLoc(L_MICROS, cap3));
        let goto_poll_capped = asm.alloc_root(CILRoot::Branch(Box::new((7, 0, None))));

        // ---- block 6: non-first fd — micros = 0; goto 7 ----
        let zero_micros = asm.alloc_node(Const::I32(0));
        let store_zero_micros = asm.alloc_root(CILRoot::StLoc(L_MICROS, zero_micros));
        let goto_poll3 = asm.alloc_root(CILRoot::Branch(Box::new((7, 0, None))));

        // ---- block 7: rd = poll(h, micros, SelectRead=0); L_READY = rd; goto 12 ----
        // We poll BOTH read and write and OR them (mio registers streams with the
        // full IN|OUT mask). The READ poll absorbs the head-fd blocking `micros`;
        // the WRITE poll (block 12) is always non-blocking.
        let poll = dotnet_mref(
            asm,
            "rcl_dotnet_socket_poll",
            &[void_ptr, Type::Int(Int::I32), Type::Int(Int::I32)],
            Type::Int(Int::I32),
        );
        let hh = asm.alloc_node(CILNode::LdLoc(L_H));
        let micros_l = asm.alloc_node(CILNode::LdLoc(L_MICROS));
        let sel_read = asm.alloc_node(Const::I32(0)); // SelectMode.SelectRead
        let rd = asm.alloc_node(CILNode::call(poll, [hh, micros_l, sel_read]));
        let store_ready = asm.alloc_root(CILRoot::StLoc(L_READY, rd));
        let goto_write_poll = asm.alloc_root(CILRoot::Branch(Box::new((12, 0, None))));

        // ---- block 12: wr = poll(h, 0, SelectWrite=1); L_READY |= wr; goto 11 ----
        let poll2 = dotnet_mref(
            asm,
            "rcl_dotnet_socket_poll",
            &[void_ptr, Type::Int(Int::I32), Type::Int(Int::I32)],
            Type::Int(Int::I32),
        );
        let hh2 = asm.alloc_node(CILNode::LdLoc(L_H));
        let zero_micros_w = asm.alloc_node(Const::I32(0));
        let sel_write = asm.alloc_node(Const::I32(1)); // SelectMode.SelectWrite
        let wr = asm.alloc_node(CILNode::call(poll2, [hh2, zero_micros_w, sel_write]));
        let prev_ready = asm.alloc_node(CILNode::LdLoc(L_READY));
        let combined = asm.alloc_node(CILNode::BinOp(prev_ready, wr, BinOp::Or));
        let store_combined = asm.alloc_root(CILRoot::StLoc(L_READY, combined));
        let goto_edge = asm.alloc_root(CILRoot::Branch(Box::new((11, 0, None))));

        // ---- block 11: edge-trigger gate ----
        // prev = reg.last_ready; reg.last_ready = ready (always update).
        // if ready==0 -> loop(1). if (reg.events & EPOLLET)==0 -> report(8) [level].
        // else (EPOLLET): prev==0 -> report(8) [rising edge] else loop(1) [suppress].
        const EPOLLET: i32 = 0x8000_0000_u32 as i32;
        let last_ready_field = epoll_reg_last_ready_field(asm);
        let reg_e = asm.alloc_node(CILNode::LdLoc(L_REG));
        let prev = asm.alloc_node(CILNode::LdField { addr: reg_e, field: last_ready_field });
        // store prev in a scratch local (reuse L_MODE — no longer needed past poll).
        let store_prev = asm.alloc_root(CILRoot::StLoc(L_MODE, prev));
        // reg.last_ready = ready.
        let reg_e2 = asm.alloc_node(CILNode::LdLoc(L_REG));
        let ready_for_store = asm.alloc_node(CILNode::LdLoc(L_READY));
        let upd_lr = asm.alloc_root(CILRoot::SetField(Box::new((last_ready_field, reg_e2, ready_for_store))));
        // if ready==0 -> loop(1).
        let ready_l = asm.alloc_node(CILNode::LdLoc(L_READY));
        let zero_r = asm.alloc_node(Const::I32(0));
        let br_notready = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(ready_l, zero_r)),
        ))));
        // et = reg.events & EPOLLET; if et==0 -> report(8) [level].
        let events_field_e = epoll_reg_events_field(asm);
        let reg_e3 = asm.alloc_node(CILNode::LdLoc(L_REG));
        let ev_e = asm.alloc_node(CILNode::LdField { addr: reg_e3, field: events_field_e });
        let et_mask = asm.alloc_node(Const::I32(EPOLLET));
        let et = asm.alloc_node(CILNode::BinOp(ev_e, et_mask, BinOp::And));
        let zero_et = asm.alloc_node(Const::I32(0));
        let br_level = asm.alloc_root(CILRoot::Branch(Box::new((
            8,
            0,
            Some(BranchCond::Eq(et, zero_et)),
        ))));
        // EPOLLET: if prev==0 -> report(8) [edge] else loop(1) [suppress].
        let prev_l = asm.alloc_node(CILNode::LdLoc(L_MODE));
        let zero_p = asm.alloc_node(Const::I32(0));
        let br_edge = asm.alloc_root(CILRoot::Branch(Box::new((
            8,
            0,
            Some(BranchCond::Eq(prev_l, zero_p)),
        ))));
        let goto_suppress = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

        // ---- block 8: ready — write events[count] (events@0, token@4); count++; loop ----
        // base = (i8*)events + count*12.
        let evbase = asm.alloc_node(CILNode::LdArg(1));
        let ev_isize_w = asm.alloc_node(CILNode::PtrCast(evbase, Box::new(PtrCastRes::ISize)));
        let count_l = asm.alloc_node(CILNode::LdLoc(L_COUNT));
        let off = epoll_event_offset(asm, count_l);
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

        let _ = (EPOLLIN, EPOLLOUT);
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
                BasicBlock::new(vec![clear_first, br_neg, store_micros, goto_clamp], 3, None),
                BasicBlock::new(vec![store_neg, goto_poll2], 4, None),
                BasicBlock::new(vec![ret], 5, None),
                BasicBlock::new(vec![store_zero_micros, goto_poll3], 6, None),
                BasicBlock::new(vec![store_ready, goto_write_poll], 7, None),
                BasicBlock::new(
                    vec![store_base, st_events, st_token, store_inc, br_full, goto_loop2],
                    8,
                    None,
                ),
                BasicBlock::new(vec![over, goto_poll_clamped], 9, None),
                BasicBlock::new(vec![store_cap, goto_poll_capped], 10, None),
                BasicBlock::new(
                    vec![store_prev, upd_lr, br_notready, br_level, br_edge, goto_suppress],
                    11,
                    None,
                ),
                BasicBlock::new(vec![store_combined, goto_edge], 12, None),
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

/// Computes the byte offset of one `epoll_event` slot in the same native integer width used by
/// pointer arithmetic. Keeping this conversion at the boundary prevents `isize + i64` IR on
/// 64-bit targets and remains correct if the backend gains a 32-bit .NET target.
fn epoll_event_offset(asm: &mut Assembly, count: Interned<CILNode>) -> Interned<CILNode> {
    let count = asm.int_cast(count, Int::ISize, ExtendKind::SignExtend);
    let stride = asm.alloc_node(Const::ISize(EPOLL_EVENT_STRIDE));
    asm.alloc_node(CILNode::BinOp(count, stride, BinOp::Mul))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoll_event_offsets_are_native_width() {
        let mut asm = Assembly::default();
        let sig = asm.sig([Type::Int(Int::I32)], Type::Void);
        let count = asm.alloc_node(CILNode::LdArg(0));
        let offset = epoll_event_offset(&mut asm, count);
        let offset = asm[offset].clone();

        assert_eq!(
            offset.typecheck(sig, &[], &mut asm).unwrap(),
            Type::Int(Int::ISize)
        );
    }
}

/// `eventfd(initval, flags) -> i32` — the mio Waker primitive (tokio's I/O driver
/// UNCONDITIONALLY constructs one). Realised as a self-readable loopback UDP
/// socket: `rcl_dotnet_eventfd()` (cilly/src/ir/builtins/dotnet.rs) builds a
/// `Socket` connected to its OWN bound `127.0.0.1:port`, registered here as a
/// real `FD_KIND_SOCKET` fd. Because the returned fd is an ordinary SOCKET fd,
/// read/write/close/epoll_wait ALREADY kind-dispatch correctly through the net
/// path — NO new plumbing:
///   * `write(fd, &1u64)` -> `rcl_dotnet_net_send` -> datagram to self -> the
///     socket becomes READABLE;
///   * `epoll_wait`'s per-fd `Socket.Poll(SelectRead)` sweep fires;
///   * `read(fd, &mut [0u8;8])` -> `rcl_dotnet_net_recv` drains it.
/// The 8-byte eventfd counter degrades to a readiness EDGE — mio's waker only
/// reads readiness, never the exact count (LIBC_SHIM_SCOPE §2.2). `initval`/
/// `flags` are ignored (the socket is created non-blocking == EFD_NONBLOCK).
///
/// This SUPERSEDES the Cap-1 placeholder (a null-handle EVENTFD entry that was
/// never ready, sufficient only because `pal_mio` never fired a Waker). `pal_mio`
/// now also exercises this via its real OwnedFd-backed waker arm.
fn insert_eventfd(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("eventfd");
    let generator = move |_, asm: &mut Assembly| {
        let void_ptr = asm.nptr(Type::Void);
        // handle = rcl_dotnet_eventfd()  -> the self-readable UDP Socket's GCHandle.
        let make = dotnet_mref(asm, "rcl_dotnet_eventfd", &[], void_ptr);
        let handle = asm.alloc_node(CILNode::call(make, []));
        // Register it as a SOCKET fd so read/write/poll/close dispatch via the net
        // path (rcl_dotnet_net_{send,recv,close} + Socket.Poll).
        let fd = call_fdtable_insert(asm, handle, FD_KIND_SOCKET, 0);
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
