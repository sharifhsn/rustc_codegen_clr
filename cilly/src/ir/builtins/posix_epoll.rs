// ===========================================================================
// posix_epoll.rs — the epoll readiness cluster (included by posix_symbols.rs).
//
// Readiness over a *set* of fds is a per-fd `Socket.Poll(micros, SelectMode)`
// loop (D1's no-IList wall: NO Socket.Select). LIBC_SHIM_SCOPE §3.3.
//
// SLICE SIMPLIFICATION (documented): this proof tracks a SINGLE registered fd per
// epoll instance (the EPOLL entry's `target`/`flags`/`token` fields), which is all
// the FLOOR probe (`pal_libc`) needs (one readiness wait on one socket). A
// multi-fd interest dict (Dictionary<i32,i32> enumerated per sweep) is the Phase-1
// generalization; the single-fd path proves the epoll_*→Socket.Poll mechanism
// end-to-end without managed-enumerator CIL.
//
// Linux epoll constants:
//   EPOLL_CTL_ADD=1, MOD=3, DEL=2; EPOLLIN=0x001, EPOLLOUT=0x004.
//   `struct epoll_event` is #[repr(packed)]: events: u32 @0, data: u64 @4.
// ===========================================================================

const EPOLLIN: i32 = 0x001;
const EPOLLOUT: i32 = 0x004;
const EPOLL_CTL_DEL: i32 = 2;

/// `epoll_create1(flags) -> i32` — register an EPOLL fd (handle 0; target/token 0).
fn insert_epoll_create1(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("epoll_create1");
    let generator = move |_, asm: &mut Assembly| {
        let zero_h = asm.alloc_node(Const::ISize(0));
        // handle 0, kind EPOLL, flags 0.
        let zero_h = asm.alloc_node(CILNode::PtrCast(zero_h, Box::new(PtrCastRes::ISize)));
        let _ = zero_h;
        let void_ptr = asm.nptr(Type::Void);
        let null = asm.alloc_node(Const::ISize(0));
        let null = asm.cast_ptr(null, void_ptr);
        let fd = call_fdtable_insert(asm, null, FD_KIND_EPOLL, 0);
        let ret = asm.alloc_root(CILRoot::Ret(fd));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `epoll_ctl(epfd, op, fd, *event) -> i32` — ADD/MOD store the (fd, events,
/// token) into the EPOLL entry; DEL clears `target` to -1. Returns 0.
fn insert_epoll_ctl(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("epoll_ctl");
    let generator = move |_, asm: &mut Assembly| {
        // entry = (RclFdEntry)tbl[epfd]; (LdArg 0 = epfd).
        let entry = entry_of_fd(asm, 0);
        let store_entry = asm.alloc_root(CILRoot::StLoc(0, entry));

        // op == EPOLL_CTL_DEL -> goto 1 (clear) else fall to set.
        let op = asm.alloc_node(CILNode::LdArg(1));
        let del_c = asm.alloc_node(Const::I32(EPOLL_CTL_DEL));
        let br_del = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(op, del_c)),
        ))));
        let goto_set = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // block 1 (DEL): entry.target = -1; goto 3.
        let entry_class = fd_entry_class(asm);
        let target_field = fd_entry_target_field(asm);
        let e1 = asm.alloc_node(CILNode::LdLoc(0));
        let neg1 = asm.alloc_node(Const::I64(-1));
        let set_target_del = asm.alloc_root(CILRoot::SetField(Box::new((target_field, e1, neg1))));
        let goto_done1 = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // block 2 (ADD/MOD): read events@0(u32) + data@4(u64) from *event;
        //   entry.target = fd; entry.flags = events; entry.token = data; goto 3.
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

        let target_field2 = fd_entry_target_field(asm);
        let flags_field = fd_entry_flags_field(asm);
        let token_field = fd_entry_token_field(asm);
        let e2 = asm.alloc_node(CILNode::LdLoc(0));
        let fd = asm.alloc_node(CILNode::LdArg(2));
        let fd_i64 = asm.int_cast(fd, Int::I64, ExtendKind::SignExtend);
        let set_target = asm.alloc_root(CILRoot::SetField(Box::new((target_field2, e2, fd_i64))));
        let e3 = asm.alloc_node(CILNode::LdLoc(0));
        let set_flags = asm.alloc_root(CILRoot::SetField(Box::new((flags_field, e3, events_i32))));
        let e4 = asm.alloc_node(CILNode::LdLoc(0));
        let set_token = asm.alloc_root(CILRoot::SetField(Box::new((token_field, e4, token_i64))));
        let goto_done2 = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // block 3: ret 0.
        let zero = asm.alloc_node(Const::I32(0));
        let ret = asm.alloc_root(CILRoot::Ret(zero));

        let _ = entry_class;
        let entry_cls = fd_entry_class(asm);
        let entry_ty = asm.alloc_type(Type::ClassRef(entry_cls));
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_entry, br_del, goto_set], 0, None),
                BasicBlock::new(vec![set_target_del, goto_done1], 1, None),
                BasicBlock::new(
                    vec![set_target, set_flags, set_token, goto_done2],
                    2,
                    None,
                ),
                BasicBlock::new(vec![ret], 3, None),
            ],
            locals: vec![(Some(asm.alloc_string("entry")), entry_ty)],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `epoll_wait(epfd, *events, maxevents, timeout_ms) -> i32` — poll the single
/// registered fd's socket via `rcl_dotnet_socket_poll`. If ready, write one
/// `epoll_event` (events@0, token@4) into the caller's array and return 1; else 0.
///
/// SelectMode: EPOLLOUT→1(Write), else 0(Read). A single Poll with the caller's
/// timeout (Poll already blocks up to `micros`), so no manual sleep loop is
/// needed for the floor. timeout_ms<0 → infinite (micros -1).
fn insert_epoll_wait(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("epoll_wait");
    let generator = move |_, asm: &mut Assembly| {
        let void_ptr = asm.nptr(Type::Void);

        // entry = (RclFdEntry)tbl[epfd]; tgt = (i32)entry.target.
        let entry = entry_of_fd(asm, 0);
        let store_entry = asm.alloc_root(CILRoot::StLoc(0, entry));
        let target_field = fd_entry_target_field(asm);
        let e0 = asm.alloc_node(CILNode::LdLoc(0));
        let tgt = asm.alloc_node(CILNode::LdField { addr: e0, field: target_field });
        let tgt_i32 = asm.int_cast(tgt, Int::I32, ExtendKind::SignExtend);
        let store_tgt = asm.alloc_root(CILRoot::StLoc(1, tgt_i32));
        // if tgt < 0 (none registered) -> ret 0 (block 4).
        let tgt0 = asm.alloc_node(CILNode::LdLoc(1));
        let zero_c = asm.alloc_node(Const::I32(0));
        let br_none = asm.alloc_root(CILRoot::Branch(Box::new((
            4,
            0,
            Some(BranchCond::Lt(tgt0, zero_c, crate::ir::cilroot::CmpKind::Signed)),
        ))));
        let fall = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

        // block 1: h = rcl_fdtable_handle(tgt); flags = entry.flags;
        //   mode = (flags & EPOLLOUT)!=0 ? 1 : 0;
        //   micros = timeout<0 ? -1 : timeout*1000;
        //   ready = rcl_dotnet_socket_poll(h, micros, mode);
        //   if ready==0 -> ret 0 (block 4) else goto 2.
        let tgt1 = asm.alloc_node(CILNode::LdLoc(1));
        let h = call_fdtable_handle(asm, tgt1);
        let store_h = asm.alloc_root(CILRoot::StLoc(2, h));
        let flags_field = fd_entry_flags_field(asm);
        let e1 = asm.alloc_node(CILNode::LdLoc(0));
        let flags = asm.alloc_node(CILNode::LdField { addr: e1, field: flags_field });
        let out_mask = asm.alloc_node(Const::I32(EPOLLOUT));
        let masked = asm.alloc_node(CILNode::BinOp(flags, out_mask, BinOp::And));
        let zero_m = asm.alloc_node(Const::I32(0));
        // mode = (masked != 0) ? 1(Write) : 0(Read). NOTE: CIL `not` is BITWISE
        // (`not 1` == -2), so logical negation must be `1 - (masked == 0)`.
        let is_zero = asm.alloc_node(CILNode::BinOp(masked, zero_m, BinOp::Eq));
        let is_zero = asm.int_cast(is_zero, Int::I32, ExtendKind::ZeroExtend);
        let one_m = asm.alloc_node(Const::I32(1));
        let mode = asm.alloc_node(CILNode::BinOp(one_m, is_zero, BinOp::Sub));
        let store_mode = asm.alloc_root(CILRoot::StLoc(3, mode));

        // micros: timeout<0?-1:timeout*1000. (timeout in local: LdArg(3).)
        let timeout = asm.alloc_node(CILNode::LdArg(3));
        let thousand = asm.alloc_node(Const::I32(1000));
        let micros = asm.alloc_node(CILNode::BinOp(timeout, thousand, BinOp::Mul));
        let store_micros = asm.alloc_root(CILRoot::StLoc(4, micros));
        // if timeout >= 0 keep; else set micros = -1. Branch: timeout<0 -> block 5 (set -1), else block 6.
        let timeout2 = asm.alloc_node(CILNode::LdArg(3));
        let zero_t = asm.alloc_node(Const::I32(0));
        let br_neg = asm.alloc_root(CILRoot::Branch(Box::new((
            5,
            0,
            Some(BranchCond::Lt(timeout2, zero_t, crate::ir::cilroot::CmpKind::Signed)),
        ))));
        let goto_poll = asm.alloc_root(CILRoot::Branch(Box::new((6, 0, None))));

        // block 5: micros = -1; goto 6.
        let neg1 = asm.alloc_node(Const::I32(-1));
        let store_neg = asm.alloc_root(CILRoot::StLoc(4, neg1));
        let goto_poll2 = asm.alloc_root(CILRoot::Branch(Box::new((6, 0, None))));

        // block 6: ready = poll(h, micros, mode); if ready==0 -> block 4 (ret 0) else block 2.
        let poll = dotnet_mref(
            asm,
            "rcl_dotnet_socket_poll",
            &[void_ptr, Type::Int(Int::I32), Type::Int(Int::I32)],
            Type::Int(Int::I32),
        );
        let hh = asm.alloc_node(CILNode::LdLoc(2));
        let micros_l = asm.alloc_node(CILNode::LdLoc(4));
        let mode_l = asm.alloc_node(CILNode::LdLoc(3));
        let ready = asm.alloc_node(CILNode::call(poll, [hh, micros_l, mode_l]));
        let store_ready = asm.alloc_root(CILRoot::StLoc(5, ready));
        let ready0 = asm.alloc_node(CILNode::LdLoc(5));
        let zero_r = asm.alloc_node(Const::I32(0));
        let br_notready = asm.alloc_root(CILRoot::Branch(Box::new((
            4,
            0,
            Some(BranchCond::Eq(ready0, zero_r)),
        ))));
        let goto_write = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // block 2: write events[0]: events@0 = entry.flags (the registered mask);
        //   token@4 = entry.token. ret 1.
        let evbase = asm.alloc_node(CILNode::LdArg(1));
        let ev_isize = asm.alloc_node(CILNode::PtrCast(evbase, Box::new(PtrCastRes::ISize)));
        let u32_ty = asm.alloc_type(Type::Int(Int::U32));
        // events@0:
        let flags_field2 = fd_entry_flags_field(asm);
        let e2 = asm.alloc_node(CILNode::LdLoc(0));
        let regflags = asm.alloc_node(CILNode::LdField { addr: e2, field: flags_field2 });
        let regflags_u32 = asm.int_cast(regflags, Int::U32, ExtendKind::ZeroExtend);
        let ev0_ptr = asm.alloc_node(CILNode::PtrCast(ev_isize, Box::new(PtrCastRes::Ptr(u32_ty))));
        let st_events = asm.alloc_root(CILRoot::StInd(Box::new((ev0_ptr, regflags_u32, Type::Int(Int::U32), false))));
        // token@4:
        let evbase2 = asm.alloc_node(CILNode::LdArg(1));
        let ev_isize2 = asm.alloc_node(CILNode::PtrCast(evbase2, Box::new(PtrCastRes::ISize)));
        let four = asm.alloc_node(Const::ISize(4));
        let tok_addr = asm.alloc_node(CILNode::BinOp(ev_isize2, four, BinOp::Add));
        let u64_ty = asm.alloc_type(Type::Int(Int::U64));
        let tok_addr = asm.alloc_node(CILNode::PtrCast(tok_addr, Box::new(PtrCastRes::Ptr(u64_ty))));
        let token_field = fd_entry_token_field(asm);
        let e3 = asm.alloc_node(CILNode::LdLoc(0));
        let tok = asm.alloc_node(CILNode::LdField { addr: e3, field: token_field });
        let tok_u64 = asm.int_cast(tok, Int::U64, ExtendKind::ZeroExtend);
        let st_token = asm.alloc_root(CILRoot::StInd(Box::new((tok_addr, tok_u64, Type::Int(Int::U64), false))));
        let one = asm.alloc_node(Const::I32(1));
        let ret1 = asm.alloc_root(CILRoot::Ret(one));

        // block 4: ret 0 (no fd ready / none registered).
        let zero4 = asm.alloc_node(Const::I32(0));
        let ret0 = asm.alloc_root(CILRoot::Ret(zero4));

        let _ = EPOLLIN;
        let entry_cls = fd_entry_class(asm);
        let entry_ty = asm.alloc_type(Type::ClassRef(entry_cls));
        let i32_ty = asm.alloc_type(Type::Int(Int::I32));
        let void_ptr_idx = asm.alloc_type(void_ptr);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_entry, store_tgt, br_none, fall], 0, None),
                BasicBlock::new(
                    vec![store_h, store_mode, store_micros, br_neg, goto_poll],
                    1,
                    None,
                ),
                BasicBlock::new(vec![st_events, st_token, ret1], 2, None),
                BasicBlock::new(vec![store_neg, goto_poll2], 5, None),
                BasicBlock::new(vec![store_ready, br_notready, goto_write], 6, None),
                BasicBlock::new(vec![ret0], 4, None),
            ],
            locals: vec![
                (Some(asm.alloc_string("entry")), entry_ty),
                (Some(asm.alloc_string("tgt")), i32_ty),
                (Some(asm.alloc_string("h")), void_ptr_idx),
                (Some(asm.alloc_string("mode")), i32_ty),
                (Some(asm.alloc_string("micros")), i32_ty),
                (Some(asm.alloc_string("ready")), i32_ty),
            ],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `eventfd(initval, flags) -> i32` — the mio Waker primitive. Out of the FLOOR
/// scope (pal_libc does not exercise it); register a placeholder EVENTFD fd
/// (handle 0) so a reference resolves. A self-connected loopback UDP socket is the
/// Phase-1 implementation (LIBC_SHIM_SCOPE §2.2).
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
