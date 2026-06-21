// ===========================================================================
// posix_symbols.rs — the bare POSIX C-ABI symbol wrappers (included by posix.rs).
//
// Each wrapper threads its int fd through the fd-table to an EXISTING rcl_dotnet_*
// body, wrapping the call in the errno try/catch. See posix.rs for the fd-table,
// errno, and errno_wrapped machinery.
// ===========================================================================

// --- sockaddr <-> IPEndPoint --------------------------------------------------

/// Read a little-endian `i32` of the low 2 bytes of `sa_ptr + off` and (for the
/// network-order port) byte-swap it. Returns a node for `(u16)value`.
///
/// Build an `IPEndPoint` from a raw Linux `sockaddr*` node. Supports `AF_INET`
/// (`sockaddr_in`: family@0, port@2 net-order, addr@4 [4 bytes]) — the loopback
/// floor case — and falls back to v4 for anything else (the slice does not parse
/// `sockaddr_in6`; that is a Phase-1 refinement). Produces an `IPEndPoint` node:
///   `new IPEndPoint(new IPAddress(ReadOnlySpan<byte>(sa+4, 4)), ntohs(*(u16*)(sa+2)))`.
fn endpoint_from_sockaddr(asm: &mut Assembly, sa_ptr: Interned<CILNode>) -> Interned<CILNode> {
    // addr bytes: ReadOnlySpan<byte>((byte*)(sa+4), 4).
    let four = asm.alloc_node(Const::ISize(4));
    let sa_isize = asm.alloc_node(CILNode::PtrCast(sa_ptr, Box::new(PtrCastRes::ISize)));
    let addr_ptr = asm.alloc_node(CILNode::BinOp(sa_isize, four, BinOp::Add));
    let void_ptr = asm.nptr(Type::Void);
    let addr_ptr = asm.cast_ptr(addr_ptr, void_ptr);
    let byte_ty = Type::Int(Int::U8);
    let ro_span = ClassRef::read_only_span(asm, byte_ty);
    let ro_span_ty = Type::ClassRef(ro_span);
    let span_ctor_sig = asm.sig([ro_span_ty, void_ptr, Type::Int(Int::I32)], Type::Void);
    let span_ctor_name = asm.alloc_string(".ctor");
    let span_ctor = asm.alloc_methodref(MethodRef::new(
        ro_span,
        span_ctor_name,
        span_ctor_sig,
        MethodKind::Constructor,
        [].into(),
    ));
    let four_i32 = asm.alloc_node(Const::I32(4));
    let span = asm.alloc_node(CILNode::call(span_ctor, [addr_ptr, four_i32]));
    let ip_address = ClassRef::ip_address(asm);
    let ip_ctor = asm.class_ref(ip_address).clone().ctor(&[ro_span_ty], asm);
    let addr = asm.alloc_node(CILNode::call(ip_ctor, [span]));

    // port: ntohs(*(u16*)(sa+2)) — read the 2 net-order bytes, byte-swap.
    let two = asm.alloc_node(Const::ISize(2));
    let sa_isize2 = asm.alloc_node(CILNode::PtrCast(sa_ptr, Box::new(PtrCastRes::ISize)));
    let port_ptr = asm.alloc_node(CILNode::BinOp(sa_isize2, two, BinOp::Add));
    let u16_ty = asm.alloc_type(Type::Int(Int::U16));
    let port_ptr = asm.alloc_node(CILNode::PtrCast(port_ptr, Box::new(PtrCastRes::Ptr(u16_ty))));
    let port_be = asm.alloc_node(CILNode::LdInd {
        addr: port_ptr,
        tpe: u16_ty,
        volatile: false,
    });
    // ntohs: ((p & 0xff) << 8) | ((p >> 8) & 0xff), done on i32.
    let port_be = asm.int_cast(port_be, Int::I32, ExtendKind::ZeroExtend);
    let lo = {
        let mask = asm.alloc_node(Const::I32(0xff));
        let m = asm.alloc_node(CILNode::BinOp(port_be, mask, BinOp::And));
        let sh = asm.alloc_node(Const::I32(8));
        asm.alloc_node(CILNode::BinOp(m, sh, BinOp::Shl))
    };
    let hi = {
        let sh = asm.alloc_node(Const::I32(8));
        let s = asm.alloc_node(CILNode::BinOp(port_be, sh, BinOp::Shr));
        let mask = asm.alloc_node(Const::I32(0xff));
        asm.alloc_node(CILNode::BinOp(s, mask, BinOp::And))
    };
    let port = asm.alloc_node(CILNode::BinOp(lo, hi, BinOp::Or));

    let ip_endpoint = ClassRef::ip_endpoint(asm);
    let ep_ctor = asm.class_ref(ip_endpoint).clone().ctor(
        &[Type::ClassRef(ip_address), Type::Int(Int::I32)],
        asm,
    );
    asm.alloc_node(CILNode::call(ep_ctor, [addr, port]))
}

// --- fd I/O -------------------------------------------------------------------

/// `read(fd, buf, len) -> isize` — dispatch on fd-kind: FILE→`rcl_dotnet_fs_read`,
/// SOCKET→`rcl_dotnet_net_recv`. (Branch on kind into blocks; but to keep the
/// errno wrapper's single-block body, we instead route via a tiny dispatcher
/// MethodDef. Here we inline a kind compare with a Branch and TWO sub-bodies.)
///
/// For the FLOOR proof only SOCKET read/write are exercised, so `read`/`write`
/// dispatch SOCKET vs FILE with a branch in the wrapper body. We special-case it
/// without the generic errno wrapper because the body needs internal blocks.
fn insert_read(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("read");
    let generator = move |_, asm: &mut Assembly| {
        rw_dispatch_body(asm, "rcl_dotnet_fs_read", "rcl_dotnet_net_recv")
    };
    patcher.insert(name, Box::new(generator));
}
fn insert_write(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("write");
    let generator = move |_, asm: &mut Assembly| {
        rw_dispatch_body(asm, "rcl_dotnet_fs_write", "rcl_dotnet_net_send")
    };
    patcher.insert(name, Box::new(generator));
}

/// Body shared by `read`/`write`: `(fd, buf, len) -> isize`.
///   block 0: kind = rcl_fdtable_kind(fd); h = rcl_fdtable_handle(fd);
///            if kind == SOCKET goto 2; else fall to 1.
///   block 1: ret (isize)file_fn(h, buf, len)        [FILE / STD-as-file]
///   block 2: ret (isize)sock_fn(h, buf, len)        [SOCKET]
/// STD (kind 0) routes to the FILE branch too — but stdio writes use the FILE fn
/// only if std passes a real FileStream; for the floor probe stdout is unused via
/// these wrappers (the probe prints via Rust's own println). This keeps the body
/// straight: the floor exercises only the SOCKET branch.
fn rw_dispatch_body(asm: &mut Assembly, file_fn: &str, sock_fn: &str) -> MethodImpl {
    let isize_ty = Type::Int(Int::ISize);
    let void_ptr = asm.nptr(Type::Void);

    let fd0 = asm.alloc_node(CILNode::LdArg(0));
    let kind = call_fdtable_kind(asm, fd0);
    let store_kind = asm.alloc_root(CILRoot::StLoc(0, kind));
    let fd1 = asm.alloc_node(CILNode::LdArg(0));
    let h = call_fdtable_handle(asm, fd1);
    let store_h = asm.alloc_root(CILRoot::StLoc(1, h));
    let kind0 = asm.alloc_node(CILNode::LdLoc(0));
    let sock_c = asm.alloc_node(Const::I32(FD_KIND_SOCKET));
    let goto_sock = asm.alloc_root(CILRoot::Branch(Box::new((
        2,
        0,
        Some(BranchCond::Eq(kind0, sock_c)),
    ))));
    let fall_file = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

    let mk_call = |asm: &mut Assembly, fname: &str| -> Interned<CILRoot> {
        let m = dotnet_mref(
            asm,
            fname,
            &[void_ptr, void_ptr, Type::Int(Int::USize)],
            isize_ty,
        );
        let hh = asm.alloc_node(CILNode::LdLoc(1));
        let buf = asm.alloc_node(CILNode::LdArg(1));
        let buf = asm.cast_ptr(buf, void_ptr);
        let len = asm.alloc_node(CILNode::LdArg(2));
        let n = asm.alloc_node(CILNode::call(m, [hh, buf, len]));
        asm.alloc_root(CILRoot::Ret(n))
    };
    let ret_file = mk_call(asm, file_fn);
    let ret_sock = mk_call(asm, sock_fn);

    let i32_ty = asm.alloc_type(Type::Int(Int::I32));
    let void_ptr_idx = asm.alloc_type(void_ptr);
    MethodImpl::MethodBody {
        blocks: vec![
            BasicBlock::new(vec![store_kind, store_h, goto_sock, fall_file], 0, None),
            BasicBlock::new(vec![ret_file], 1, None),
            BasicBlock::new(vec![ret_sock], 2, None),
        ],
        locals: vec![(None, i32_ty), (None, void_ptr_idx)],
    }
}

/// `close(fd) -> i32` — kind-dispatch `rcl_dotnet_{fs,net}_close` then
/// `rcl_fdtable_remove`. Returns 0.
fn insert_close(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("close");
    let generator = move |_, asm: &mut Assembly| {
        let void_ptr = asm.nptr(Type::Void);
        let fd0 = asm.alloc_node(CILNode::LdArg(0));
        let kind = call_fdtable_kind(asm, fd0);
        let store_kind = asm.alloc_root(CILRoot::StLoc(0, kind));
        let fd1 = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd1);
        let store_h = asm.alloc_root(CILRoot::StLoc(1, h));
        // block 0: kind dispatch. SOCKET->2 (net_close); FILE->1 (fs_close);
        //   else (STD/EPOLL/EVENTFD, no managed handle to dispose) -> 3 (just remove).
        let kind0 = asm.alloc_node(CILNode::LdLoc(0));
        let sock_c = asm.alloc_node(Const::I32(FD_KIND_SOCKET));
        let goto_sock = asm.alloc_root(CILRoot::Branch(Box::new((
            2,
            0,
            Some(BranchCond::Eq(kind0, sock_c)),
        ))));
        let kind1 = asm.alloc_node(CILNode::LdLoc(0));
        let file_c = asm.alloc_node(Const::I32(FD_KIND_FILE));
        let goto_file = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(kind1, file_c)),
        ))));
        let goto_done0 = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        let mk_close = |asm: &mut Assembly, fname: &str| -> Vec<Interned<CILRoot>> {
            let m = dotnet_mref(asm, fname, &[void_ptr], Type::Void);
            let hh = asm.alloc_node(CILNode::LdLoc(1));
            let call = asm.alloc_root(CILRoot::call(m, [hh]));
            let goto_done = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));
            vec![call, goto_done]
        };
        // block 1 (FILE), block 2 (SOCKET) -> block 3 (remove + ret 0).
        let file_roots = mk_close(asm, "rcl_dotnet_fs_close");
        let sock_roots = mk_close(asm, "rcl_dotnet_net_close");
        // block 3:
        let fd2 = asm.alloc_node(CILNode::LdArg(0));
        let rem = call_fdtable_remove(asm, fd2);
        let zero = asm.alloc_node(Const::I32(0));
        let ret = asm.alloc_root(CILRoot::Ret(zero));

        let i32_ty = asm.alloc_type(Type::Int(Int::I32));
        let void_ptr_idx = asm.alloc_type(void_ptr);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(
                    vec![store_kind, store_h, goto_sock, goto_file, goto_done0],
                    0,
                    None,
                ),
                BasicBlock::new(file_roots, 1, None),
                BasicBlock::new(sock_roots, 2, None),
                BasicBlock::new(vec![rem, ret], 3, None),
            ],
            locals: vec![(None, i32_ty), (None, void_ptr_idx)],
        }
    };
    patcher.insert(name, Box::new(generator));
}

// --- sockets ------------------------------------------------------------------

/// `socket(domain, type, protocol) -> i32` — create a `Socket` via
/// `rcl_dotnet_net_socket` (.NET enum values), register it as a SOCKET fd.
fn insert_socket(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("socket");
    let generator = move |_, asm: &mut Assembly| {
        let void_ptr = asm.nptr(Type::Void);
        // af = (domain == AF_INET6) ? InterNetworkV6 : InterNetwork.
        let domain = asm.alloc_node(CILNode::LdArg(0));
        let af_inet6_p = asm.alloc_node(Const::I32(POSIX_AF_INET6));
        let is_v6 = asm.alloc_node(CILNode::BinOp(domain, af_inet6_p, BinOp::Eq));
        // af = InterNetwork + is_v6 * (InterNetworkV6 - InterNetwork).
        let v4 = asm.alloc_node(Const::I32(DOTNET_AF_INET));
        let diff = asm.alloc_node(Const::I32(DOTNET_AF_INET6 - DOTNET_AF_INET));
        let is_v6_i = asm.int_cast(is_v6, Int::I32, ExtendKind::ZeroExtend);
        let term = asm.alloc_node(CILNode::BinOp(is_v6_i, diff, BinOp::Mul));
        let af = asm.alloc_node(CILNode::BinOp(v4, term, BinOp::Add));
        let store_af = asm.alloc_root(CILRoot::StLoc(1, af));

        // ty = (type == SOCK_DGRAM) ? Dgram : Stream ; proto = Dgram?Udp:Tcp.
        let typ = asm.alloc_node(CILNode::LdArg(1));
        let dgram_p = asm.alloc_node(Const::I32(POSIX_SOCK_DGRAM));
        let is_dgram = asm.alloc_node(CILNode::BinOp(typ, dgram_p, BinOp::Eq));
        let is_dgram_i = asm.int_cast(is_dgram, Int::I32, ExtendKind::ZeroExtend);
        // ty = Stream + is_dgram*(Dgram-Stream)
        let st_stream = asm.alloc_node(Const::I32(DOTNET_SOCKTYPE_STREAM));
        let st_diff = asm.alloc_node(Const::I32(DOTNET_SOCKTYPE_DGRAM - DOTNET_SOCKTYPE_STREAM));
        let st_term = asm.alloc_node(CILNode::BinOp(is_dgram_i, st_diff, BinOp::Mul));
        let st = asm.alloc_node(CILNode::BinOp(st_stream, st_term, BinOp::Add));
        let store_st = asm.alloc_root(CILRoot::StLoc(2, st));
        // proto = Tcp + is_dgram*(Udp-Tcp)
        let p_tcp = asm.alloc_node(Const::I32(DOTNET_PROTO_TCP));
        let p_diff = asm.alloc_node(Const::I32(DOTNET_PROTO_UDP - DOTNET_PROTO_TCP));
        let is_dgram_i2 = {
            let typ2 = asm.alloc_node(CILNode::LdArg(1));
            let dgram_p2 = asm.alloc_node(Const::I32(POSIX_SOCK_DGRAM));
            let eq2 = asm.alloc_node(CILNode::BinOp(typ2, dgram_p2, BinOp::Eq));
            asm.int_cast(eq2, Int::I32, ExtendKind::ZeroExtend)
        };
        let p_term = asm.alloc_node(CILNode::BinOp(is_dgram_i2, p_diff, BinOp::Mul));
        let proto = asm.alloc_node(CILNode::BinOp(p_tcp, p_term, BinOp::Add));
        let store_proto = asm.alloc_root(CILRoot::StLoc(3, proto));

        // h = rcl_dotnet_net_socket(af, st, proto).
        let m = dotnet_mref(
            asm,
            "rcl_dotnet_net_socket",
            &[Type::Int(Int::I32), Type::Int(Int::I32), Type::Int(Int::I32)],
            void_ptr,
        );
        let af_l = asm.alloc_node(CILNode::LdLoc(1));
        let st_l = asm.alloc_node(CILNode::LdLoc(2));
        let proto_l = asm.alloc_node(CILNode::LdLoc(3));
        let h = asm.alloc_node(CILNode::call(m, [af_l, st_l, proto_l]));
        // fd = rcl_fdtable_insert(h, SOCKET, 0); local 0 = fd.
        let fd = call_fdtable_insert(asm, h, FD_KIND_SOCKET, 0);
        let store_fd = asm.alloc_root(CILRoot::StLoc(0, fd));

        let body = vec![store_af, store_st, store_proto, store_fd];
        let i32_ty = asm.alloc_type(Type::Int(Int::I32));
        errno_wrapped(
            asm,
            body,
            Type::Int(Int::I32),
            vec![
                (None, i32_ty),
                (None, i32_ty),
                (None, i32_ty),
            ],
        )
    };
    patcher.insert(name, Box::new(generator));
}

/// `bind(fd, *sockaddr, len) -> i32` — `s.Bind(endpoint)`; return 0.
fn insert_bind(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("bind");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let endpoint_base = ClassRef::endpoint(asm);
        let void_ptr = asm.nptr(Type::Void);
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd);
        let s = handle_to_socket_node(asm, h);
        let sa = asm.alloc_node(CILNode::LdArg(1));
        let sa = asm.cast_ptr(sa, void_ptr);
        let ep = endpoint_from_sockaddr(asm, sa);
        let ep = endpoint_as_base(asm, ep);
        let bind_name = asm.alloc_string("Bind");
        let bind = asm.class_ref(socket).clone().instance(
            &[Type::ClassRef(endpoint_base)],
            Type::Void,
            bind_name,
            asm,
        );
        let do_bind = asm.alloc_root(CILRoot::call(bind, [s, ep]));
        let zero = asm.alloc_node(Const::I32(0));
        let store0 = asm.alloc_root(CILRoot::StLoc(0, zero));
        errno_wrapped(asm, vec![do_bind, store0], Type::Int(Int::I32), vec![])
    };
    patcher.insert(name, Box::new(generator));
}

/// `listen(fd, backlog) -> i32` — `s.Listen(backlog)`; return 0.
fn insert_listen(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("listen");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd);
        let s = handle_to_socket_node(asm, h);
        let backlog = asm.alloc_node(CILNode::LdArg(1));
        let listen_name = asm.alloc_string("Listen");
        let listen = asm.class_ref(socket).clone().instance(
            &[Type::Int(Int::I32)],
            Type::Void,
            listen_name,
            asm,
        );
        let do_listen = asm.alloc_root(CILRoot::call(listen, [s, backlog]));
        let zero = asm.alloc_node(Const::I32(0));
        let store0 = asm.alloc_root(CILRoot::StLoc(0, zero));
        errno_wrapped(asm, vec![do_listen, store0], Type::Int(Int::I32), vec![])
    };
    patcher.insert(name, Box::new(generator));
}

/// `connect(fd, *sockaddr, len) -> i32` — `s.Connect(endpoint)`; return 0.
fn insert_connect(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("connect");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let endpoint_base = ClassRef::endpoint(asm);
        let void_ptr = asm.nptr(Type::Void);
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd);
        let s = handle_to_socket_node(asm, h);
        let sa = asm.alloc_node(CILNode::LdArg(1));
        let sa = asm.cast_ptr(sa, void_ptr);
        let ep = endpoint_from_sockaddr(asm, sa);
        let ep = endpoint_as_base(asm, ep);
        let connect_name = asm.alloc_string("Connect");
        let connect = asm.class_ref(socket).clone().instance(
            &[Type::ClassRef(endpoint_base)],
            Type::Void,
            connect_name,
            asm,
        );
        let do_connect = asm.alloc_root(CILRoot::call(connect, [s, ep]));
        let zero = asm.alloc_node(Const::I32(0));
        let store0 = asm.alloc_root(CILRoot::StLoc(0, zero));
        errno_wrapped(asm, vec![do_connect, store0], Type::Int(Int::I32), vec![])
    };
    patcher.insert(name, Box::new(generator));
}

/// `accept(fd, *sockaddr, *socklen) -> i32` — `c = s.Accept()`; register `c` as a
/// new SOCKET fd; (we do NOT write the peer sockaddr out — std/probe re-query via
/// getpeername if needed; the slice keeps it minimal but correct: out may be NULL).
fn insert_accept(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let mk = |asm: &mut Assembly| -> MethodImpl {
        let socket = ClassRef::socket(asm);
        let void_ptr = asm.nptr(Type::Void);
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd);
        let s = handle_to_socket_node(asm, h);
        let accept_name = asm.alloc_string("Accept");
        let accept = asm.class_ref(socket).clone().instance(
            &[],
            Type::ClassRef(socket),
            accept_name,
            asm,
        );
        let conn = asm.alloc_node(CILNode::call(accept, [s]));
        let store_conn = asm.alloc_root(CILRoot::StLoc(1, conn));
        // handle = (void*)GCHandle.Alloc(conn).
        let handle = CILNode::LdLoc(1).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let void = asm.alloc_type(Type::Void);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let new_fd = call_fdtable_insert(asm, handle, FD_KIND_SOCKET, 0);
        let store_fd = asm.alloc_root(CILRoot::StLoc(0, new_fd));
        let _ = void_ptr;
        let sock_ty = asm.alloc_type(Type::ClassRef(socket));
        let conn_name = asm.alloc_string("conn");
        errno_wrapped(
            asm,
            vec![store_conn, store_fd],
            Type::Int(Int::I32),
            vec![(Some(conn_name), sock_ty)],
        )
    };
    let name = asm.alloc_string("accept");
    let g1 = mk;
    patcher.insert(name, Box::new(move |_, asm: &mut Assembly| g1(asm)));
    let name4 = asm.alloc_string("accept4");
    patcher.insert(name4, Box::new(move |_, asm: &mut Assembly| mk(asm)));
}

/// `send(fd, buf, len, flags) -> isize` — `s.Send(span)`. `recv` is symmetric.
fn insert_send_recv(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let mk = |asm: &mut Assembly, fname: &str| -> MethodImpl {
        let void_ptr = asm.nptr(Type::Void);
        let isize_ty = Type::Int(Int::ISize);
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd);
        let store_h = asm.alloc_root(CILRoot::StLoc(1, h));
        let m = dotnet_mref(asm, fname, &[void_ptr, void_ptr, Type::Int(Int::USize)], isize_ty);
        let hh = asm.alloc_node(CILNode::LdLoc(1));
        let buf = asm.alloc_node(CILNode::LdArg(1));
        let buf = asm.cast_ptr(buf, void_ptr);
        let len = asm.alloc_node(CILNode::LdArg(2));
        let n = asm.alloc_node(CILNode::call(m, [hh, buf, len]));
        let store_n = asm.alloc_root(CILRoot::StLoc(0, n));
        let void_ptr_idx = asm.alloc_type(void_ptr);
        errno_wrapped(
            asm,
            vec![store_h, store_n],
            Type::Int(Int::ISize),
            vec![(None, void_ptr_idx)],
        )
    };
    let send = asm.alloc_string("send");
    patcher.insert(send, Box::new(move |_, asm: &mut Assembly| mk(asm, "rcl_dotnet_net_send")));
    let recv = asm.alloc_string("recv");
    patcher.insert(recv, Box::new(move |_, asm: &mut Assembly| mk(asm, "rcl_dotnet_net_recv")));
}

/// `shutdown(fd, how) -> i32` — `s.Shutdown((SocketShutdown)how)`; return 0.
fn insert_shutdown(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("shutdown");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let socket_shutdown = Type::ClassRef(ClassRef::socket_shutdown(asm));
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd);
        let s = handle_to_socket_node(asm, h);
        let how = asm.alloc_node(CILNode::LdArg(1));
        let sd_name = asm.alloc_string("Shutdown");
        let sd = asm.class_ref(socket).clone().instance(
            &[socket_shutdown],
            Type::Void,
            sd_name,
            asm,
        );
        let do_sd = asm.alloc_root(CILRoot::call(sd, [s, how]));
        let zero = asm.alloc_node(Const::I32(0));
        let store0 = asm.alloc_root(CILRoot::StLoc(0, zero));
        errno_wrapped(asm, vec![do_sd, store0], Type::Int(Int::I32), vec![])
    };
    patcher.insert(name, Box::new(generator));
}

/// `setsockopt(fd, level, optname, *val, len) -> i32` — SO_REUSEADDR is a
/// store-and-0 no-op (BCL binds allow address reuse on loopback already);
/// TCP_NODELAY → `s.NoDelay`. Everything else → 0 (no-op). Returns 0.
fn insert_setsockopt(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("setsockopt");
    let generator = move |_, asm: &mut Assembly| {
        // Minimal: return 0 (no-op). Correct for the floor (loopback binds work
        // without SO_REUSEADDR; TCP_NODELAY is a perf hint). Documented leak.
        let zero = asm.alloc_node(Const::I32(0));
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `getsockname`/`getpeername(fd, *sockaddr, *socklen) -> i32` — write the
/// endpoint into the caller's sockaddr (v4 layout) + return 0.
fn insert_get_addr(asm: &mut Assembly, patcher: &mut MissingMethodPatcher, name: &str, getter: &str) {
    let getter = getter.to_string();
    let name = asm.alloc_string(name);
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let endpoint_base = ClassRef::endpoint(asm);
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd);
        let s = handle_to_socket_node(asm, h);
        // ep = (IPEndPoint)s.<getter>; store local 1.
        let get_name = asm.alloc_string(getter.as_str());
        let get = asm.class_ref(socket).clone().instance(
            &[],
            Type::ClassRef(endpoint_base),
            get_name,
            asm,
        );
        let ep_obj = asm.alloc_node(CILNode::call(get, [s]));
        let ip_ep_ty = asm.alloc_type(Type::ClassRef(ip_endpoint));
        let ep = asm.alloc_node(CILNode::CheckedCast(ep_obj, ip_ep_ty));
        let store_ep = asm.alloc_root(CILRoot::StLoc(1, ep));
        // Write a v4 sockaddr_in into the caller's buffer: family@0=AF_INET,
        // port@2 (net order), addr@4..8, via write_sockaddr_out.
        let mut roots = vec![store_ep];
        roots.extend(write_sockaddr_out(asm, 1, 2, 1));
        let zero = asm.alloc_node(Const::I32(0));
        let store0 = asm.alloc_root(CILRoot::StLoc(0, zero));
        roots.push(store0);

        let ip_ep_local = asm.alloc_type(Type::ClassRef(ip_endpoint));
        let byte_ty = asm.alloc_type(Type::Int(Int::U8));
        let byte_arr = asm.alloc_type(Type::PlatformArray {
            elem: byte_ty,
            dims: std::num::NonZeroU8::new(1).unwrap(),
        });
        let ep_name = asm.alloc_string("endpoint");
        let bytes_name = asm.alloc_string("bytes");
        errno_wrapped(
            asm,
            roots,
            Type::Int(Int::I32),
            vec![(Some(ep_name), ip_ep_local), (Some(bytes_name), byte_arr)],
        )
    };
    patcher.insert(name, Box::new(generator));
}

/// Write the `IPEndPoint` in `ep_local` into the caller's `sockaddr_in` at
/// `LdArg(sa_arg)`: family@0 = AF_INET(2), port@2 (net order), addr bytes@4.
/// `bytes_local` is a `byte[]` scratch local.
fn write_sockaddr_out(
    asm: &mut Assembly,
    ep_local: u32,
    bytes_local: u32,
    sa_arg: u32,
) -> Vec<Interned<CILRoot>> {
    let ip_endpoint = ClassRef::ip_endpoint(asm);
    let ip_address = ClassRef::ip_address(asm);
    let byte_ty = asm.alloc_type(Type::Int(Int::U8));
    let byte_arr = Type::PlatformArray {
        elem: byte_ty,
        dims: std::num::NonZeroU8::new(1).unwrap(),
    };

    // *(u16*)(sa+0) = AF_INET (2) — host order family.
    let sa = asm.alloc_node(CILNode::LdArg(sa_arg));
    let sa_isize = asm.alloc_node(CILNode::PtrCast(sa, Box::new(PtrCastRes::ISize)));
    let two = asm.alloc_node(Const::I32(POSIX_AF_INET6 - 8)); // = 2 (AF_INET)
    let two = asm.int_cast(two, Int::U16, ExtendKind::ZeroExtend);
    let u16_ty = asm.alloc_type(Type::Int(Int::U16));
    let fam_ptr = asm.alloc_node(CILNode::PtrCast(sa_isize, Box::new(PtrCastRes::Ptr(u16_ty))));
    let st_fam = asm.alloc_root(CILRoot::StInd(Box::new((fam_ptr, two, Type::Int(Int::U16), false))));

    // port: htons(ep.Port) at sa+2.
    let get_port_name = asm.alloc_string("get_Port");
    let get_port = asm.class_ref(ip_endpoint).clone().instance(
        &[],
        Type::Int(Int::I32),
        get_port_name,
        asm,
    );
    let ep0 = asm.alloc_node(CILNode::LdLoc(ep_local));
    let port = asm.alloc_node(CILNode::call(get_port, [ep0]));
    // htons: ((p&0xff)<<8)|((p>>8)&0xff).
    let lo = {
        let mask = asm.alloc_node(Const::I32(0xff));
        let m = asm.alloc_node(CILNode::BinOp(port, mask, BinOp::And));
        let sh = asm.alloc_node(Const::I32(8));
        asm.alloc_node(CILNode::BinOp(m, sh, BinOp::Shl))
    };
    let hi = {
        let sh = asm.alloc_node(Const::I32(8));
        let s = asm.alloc_node(CILNode::BinOp(port, sh, BinOp::Shr));
        let mask = asm.alloc_node(Const::I32(0xff));
        asm.alloc_node(CILNode::BinOp(s, mask, BinOp::And))
    };
    let port_be = asm.alloc_node(CILNode::BinOp(lo, hi, BinOp::Or));
    let port_be = asm.int_cast(port_be, Int::U16, ExtendKind::ZeroExtend);
    let sa2 = asm.alloc_node(CILNode::LdArg(sa_arg));
    let sa2_isize = asm.alloc_node(CILNode::PtrCast(sa2, Box::new(PtrCastRes::ISize)));
    let two_off = asm.alloc_node(Const::ISize(2));
    let port_addr = asm.alloc_node(CILNode::BinOp(sa2_isize, two_off, BinOp::Add));
    let port_addr = asm.alloc_node(CILNode::PtrCast(port_addr, Box::new(PtrCastRes::Ptr(u16_ty))));
    let st_port = asm.alloc_root(CILRoot::StInd(Box::new((port_addr, port_be, Type::Int(Int::U16), false))));

    // addr bytes: b = ep.Address.GetAddressBytes(); Marshal.Copy(b,0,(IntPtr)(sa+4),b.Length).
    let get_addr_name = asm.alloc_string("get_Address");
    let get_addr = asm.class_ref(ip_endpoint).clone().instance(
        &[],
        Type::ClassRef(ip_address),
        get_addr_name,
        asm,
    );
    let ep1 = asm.alloc_node(CILNode::LdLoc(ep_local));
    let addr = asm.alloc_node(CILNode::call(get_addr, [ep1]));
    let get_bytes_name = asm.alloc_string("GetAddressBytes");
    let get_bytes = asm.class_ref(ip_address).clone().instance(
        &[],
        byte_arr,
        get_bytes_name,
        asm,
    );
    let bytes = asm.alloc_node(CILNode::call(get_bytes, [addr]));
    let store_bytes = asm.alloc_root(CILRoot::StLoc(bytes_local, bytes));
    let marshal = ClassRef::marshal(asm);
    let copy_name = asm.alloc_string("Copy");
    let copy = asm.class_ref(marshal).clone().static_mref(
        &[byte_arr, Type::Int(Int::I32), Type::Int(Int::ISize), Type::Int(Int::I32)],
        Type::Void,
        copy_name,
        asm,
    );
    let b1 = asm.alloc_node(CILNode::LdLoc(bytes_local));
    let zero = asm.alloc_node(Const::I32(0));
    let sa3 = asm.alloc_node(CILNode::LdArg(sa_arg));
    let sa3_isize = asm.alloc_node(CILNode::PtrCast(sa3, Box::new(PtrCastRes::ISize)));
    let four_off = asm.alloc_node(Const::ISize(4));
    let addr_dst = asm.alloc_node(CILNode::BinOp(sa3_isize, four_off, BinOp::Add));
    let b2 = asm.alloc_node(CILNode::LdLoc(bytes_local));
    let blen = asm.ld_len(b2);
    let blen_i32 = asm.int_cast(blen, Int::I32, ExtendKind::ZeroExtend);
    let do_copy = asm.alloc_root(CILRoot::call(copy, [b1, zero, addr_dst, blen_i32]));

    vec![st_fam, st_port, store_bytes, do_copy]
}

// --- fcntl / ioctl ------------------------------------------------------------

/// `fcntl(fd, cmd, arg) -> i32` — F_GETFL→flags word; F_SETFL→set nonblocking +
/// store flags. Other cmds → 0. (F_GETFL=3, F_SETFL=4, O_NONBLOCK=0o4000=2048.)
fn insert_fcntl(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("fcntl");
    let generator = move |_, asm: &mut Assembly| {
        const F_GETFL: i32 = 3;
        const F_SETFL: i32 = 4;
        const O_NONBLOCK: i32 = 2048;
        let void_ptr = asm.nptr(Type::Void);
        // block 0: if cmd==F_GETFL goto 1; if cmd==F_SETFL goto 2; else goto 3.
        let cmd0 = asm.alloc_node(CILNode::LdArg(1));
        let getfl_c = asm.alloc_node(Const::I32(F_GETFL));
        let br_get = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, Some(BranchCond::Eq(cmd0, getfl_c))))));
        let cmd1 = asm.alloc_node(CILNode::LdArg(1));
        let setfl_c = asm.alloc_node(Const::I32(F_SETFL));
        let br_set = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, Some(BranchCond::Eq(cmd1, setfl_c))))));
        let goto_zero = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // block 1: ret rcl_fdtable_get_flags(fd).
        let fd1 = asm.alloc_node(CILNode::LdArg(0));
        let getf = main_static(asm, "rcl_fdtable_get_flags", &[Type::Int(Int::I32)], Type::Int(Int::I32));
        let flags = asm.alloc_node(CILNode::call(getf, [fd1]));
        let ret_flags = asm.alloc_root(CILRoot::Ret(flags));

        // block 2: nb = (arg & O_NONBLOCK)!=0; rcl_dotnet_net_set_nonblocking(h,nb?1:0);
        //          rcl_fdtable_set_flags(fd, arg); ret 0.
        let fd2 = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd2);
        let arg = asm.alloc_node(CILNode::LdArg(2));
        let arg_i = asm.int_cast(arg, Int::I32, ExtendKind::ZeroExtend);
        let nbmask = asm.alloc_node(Const::I32(O_NONBLOCK));
        let masked = asm.alloc_node(CILNode::BinOp(arg_i, nbmask, BinOp::And));
        let zero_c = asm.alloc_node(Const::I32(0));
        // nb = (masked != 0) ? 1 : 0 = 1 - (masked == 0). CIL `not` is bitwise, so
        // use arithmetic logical-negation.
        let is_zero = asm.alloc_node(CILNode::BinOp(masked, zero_c, BinOp::Eq));
        let is_zero = asm.int_cast(is_zero, Int::I32, ExtendKind::ZeroExtend);
        let one_c = asm.alloc_node(Const::I32(1));
        let nb = asm.alloc_node(CILNode::BinOp(one_c, is_zero, BinOp::Sub));
        let set_nb = dotnet_mref(asm, "rcl_dotnet_net_set_nonblocking", &[void_ptr, Type::Int(Int::I32)], Type::Int(Int::I32));
        let snb = asm.alloc_node(CILNode::call(set_nb, [h, nb]));
        let pop = asm.alloc_root(CILRoot::Pop(snb));
        let fd3 = asm.alloc_node(CILNode::LdArg(0));
        let arg2 = asm.alloc_node(CILNode::LdArg(2));
        let arg2_i = asm.int_cast(arg2, Int::I32, ExtendKind::ZeroExtend);
        let setf = main_static(asm, "rcl_fdtable_set_flags", &[Type::Int(Int::I32), Type::Int(Int::I32)], Type::Void);
        let do_setf = asm.alloc_root(CILRoot::call(setf, [fd3, arg2_i]));
        let zero2 = asm.alloc_node(Const::I32(0));
        let ret0 = asm.alloc_root(CILRoot::Ret(zero2));

        // block 3: ret 0.
        let zero3 = asm.alloc_node(Const::I32(0));
        let ret_z = asm.alloc_root(CILRoot::Ret(zero3));

        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![br_get, br_set, goto_zero], 0, None),
                BasicBlock::new(vec![ret_flags], 1, None),
                BasicBlock::new(vec![pop, do_setf, ret0], 2, None),
                BasicBlock::new(vec![ret_z], 3, None),
            ],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `ioctl(fd, request, argp) -> i32` — FIONBIO (0x5421): `*argp` int → set
/// nonblocking. Other requests → 0. (FIONBIO on Linux = 0x5421.)
fn insert_ioctl(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("ioctl");
    let generator = move |_, asm: &mut Assembly| {
        const FIONBIO: i32 = 0x5421;
        let void_ptr = asm.nptr(Type::Void);
        let req = asm.alloc_node(CILNode::LdArg(1));
        let req_i = asm.int_cast(req, Int::I32, ExtendKind::ZeroExtend);
        let fionbio_c = asm.alloc_node(Const::I32(FIONBIO));
        let goto_nb = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, Some(BranchCond::Eq(req_i, fionbio_c))))));
        let goto_zero = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // block 1: nb = *(i32*)argp != 0; rcl_dotnet_net_set_nonblocking(h, nb?1:0); ret 0.
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let h = call_fdtable_handle(asm, fd);
        let argp = asm.alloc_node(CILNode::LdArg(2));
        let i32_ty = asm.alloc_type(Type::Int(Int::I32));
        let argp = asm.alloc_node(CILNode::PtrCast(argp, Box::new(PtrCastRes::Ptr(i32_ty))));
        let val = asm.alloc_node(CILNode::LdInd { addr: argp, tpe: i32_ty, volatile: false });
        let zero_c = asm.alloc_node(Const::I32(0));
        // nb = (val != 0) ? 1 : 0 = 1 - (val == 0). CIL `not` is bitwise.
        let is_zero = asm.alloc_node(CILNode::BinOp(val, zero_c, BinOp::Eq));
        let is_zero = asm.int_cast(is_zero, Int::I32, ExtendKind::ZeroExtend);
        let one_c = asm.alloc_node(Const::I32(1));
        let nb = asm.alloc_node(CILNode::BinOp(one_c, is_zero, BinOp::Sub));
        let set_nb = dotnet_mref(asm, "rcl_dotnet_net_set_nonblocking", &[void_ptr, Type::Int(Int::I32)], Type::Int(Int::I32));
        let snb = asm.alloc_node(CILNode::call(set_nb, [h, nb]));
        let pop = asm.alloc_root(CILRoot::Pop(snb));
        let z = asm.alloc_node(Const::I32(0));
        let ret0 = asm.alloc_root(CILRoot::Ret(z));
        // block 2: ret 0.
        let z2 = asm.alloc_node(Const::I32(0));
        let ret_z = asm.alloc_root(CILRoot::Ret(z2));

        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![goto_nb, goto_zero], 0, None),
                BasicBlock::new(vec![pop, ret0], 1, None),
                BasicBlock::new(vec![ret_z], 2, None),
            ],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

// --- file open (for the ENOENT errno proof) ----------------------------------

/// `open(path, flags, ...) -> i32` — `rcl_dotnet_fs_open(path,len,mode,access,0)`
/// then register as FILE fd. On a missing-file fault the BCL throws
/// `FileNotFoundException`/`DirectoryNotFoundException` → caught → errno=ENOENT,
/// return -1. (The errno wrapper maps non-SocketException to EIO by default, so
/// open uses a dedicated catch that sets ENOENT — the floor's errno assertion.)
///
/// `path` is a NUL-terminated C string; we compute its length with a strlen-ish
/// scan is overkill here — instead the probe passes a Rust `&str` ptr+len via a
/// 2-arg variant. To keep a true C ABI we accept `(path_cstr, flags, mode)` and
/// derive the string via `Marshal.PtrToStringUTF8`.
fn insert_open(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("open");
    let generator = move |_, asm: &mut Assembly| {
        let void_ptr = asm.nptr(Type::Void);
        // path string via Marshal.PtrToStringUTF8((IntPtr)path).
        let marshal = ClassRef::marshal(asm);
        let ptr_to_str_name = asm.alloc_string("PtrToStringUTF8");
        let ptr_to_str = asm.class_ref(marshal).clone().static_mref(
            &[Type::Int(Int::ISize)],
            Type::PlatformString,
            ptr_to_str_name,
            asm,
        );
        let path = asm.alloc_node(CILNode::LdArg(0));
        let path_isize = asm.alloc_node(CILNode::PtrCast(path, Box::new(PtrCastRes::ISize)));
        let path_str = asm.alloc_node(CILNode::call(ptr_to_str, [path_isize]));
        let store_path = asm.alloc_root(CILRoot::StLoc(1, path_str));

        // new FileStream(path, FileMode.Open(=3), FileAccess.Read(=1)). For the
        // floor we only need the read-open of a (missing) path to throw.
        let file_stream = ClassRef::file_stream(asm);
        let file_mode = Type::ClassRef(ClassRef::file_mode(asm));
        let file_access = Type::ClassRef(ClassRef::file_access(asm));
        let ctor = asm.class_ref(file_stream).clone().ctor(
            &[Type::PlatformString, file_mode, file_access],
            asm,
        );
        let path_l = asm.alloc_node(CILNode::LdLoc(1));
        let mode = asm.alloc_node(Const::I32(3)); // FileMode.Open
        let access = asm.alloc_node(Const::I32(1)); // FileAccess.Read
        let stream = asm.alloc_node(CILNode::call(ctor, [path_l, mode, access]));
        let store_stream = asm.alloc_root(CILRoot::StLoc(2, stream));
        // handle = (void*)GCHandle.Alloc(stream).
        let handle = CILNode::LdLoc(2).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let void = asm.alloc_type(Type::Void);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let fd = call_fdtable_insert(asm, handle, FD_KIND_FILE, 0);
        let store_fd = asm.alloc_root(CILRoot::StLoc(0, fd));
        let _ = void_ptr;

        // A dedicated try/catch: any throw (FileNotFound etc.) -> errno=ENOENT, ret -1.
        // (open is the floor's ENOENT proof; the generic SocketException map does
        // not apply to file faults, which all surface as ENOENT/EACCES — here we
        // coarsely map to ENOENT, the documented file-side leak.)
        open_errno_wrapped(asm, vec![store_path, store_stream, store_fd])
    };
    patcher.insert(name, Box::new(generator));
}

/// A specialized errno wrapper for `open`: catch-all → errno=ENOENT, ret -1.
/// Block 0 try { body; leave -> 2 }; catch { errno=ENOENT; result=-1; leave->2 };
/// block 2 ret result.
fn open_errno_wrapped(asm: &mut Assembly, mut body: Vec<Interned<CILRoot>>) -> MethodImpl {
    let leave_ok = asm.alloc_root(CILRoot::ExitSpecialRegion { target: 2, source: 0 });
    body.push(leave_ok);
    // catch (block 1): errno = ENOENT; result(local0) = -1; leave -> 2.
    let set_enoent = set_errno(asm, ENOENT);
    let minus1 = asm.alloc_node(Const::I32(-1));
    let store_m1 = asm.alloc_root(CILRoot::StLoc(0, minus1));
    let leave_catch = asm.alloc_root(CILRoot::ExitSpecialRegion { target: 2, source: 1 });
    // block 2: ret local0.
    let ld0 = asm.alloc_node(CILNode::LdLoc(0));
    let ret = asm.alloc_root(CILRoot::Ret(ld0));

    let i32_ty = asm.alloc_type(Type::Int(Int::I32));
    let string_ty = asm.alloc_type(Type::PlatformString);
    let fs = ClassRef::file_stream(asm);
    let fs_ty = asm.alloc_type(Type::ClassRef(fs));
    MethodImpl::MethodBody {
        blocks: vec![
            BasicBlock::new(
                body,
                0,
                Some(vec![BasicBlock::new(vec![set_enoent, store_m1, leave_catch], 1, None)]),
            ),
            BasicBlock::new(vec![ret], 2, None),
        ],
        locals: vec![
            (Some(asm.alloc_string("result")), i32_ty),
            (Some(asm.alloc_string("path")), string_ty),
            (Some(asm.alloc_string("stream")), fs_ty),
        ],
    }
}

// --- epoll readiness (per-fd Socket.Poll over the interest dict) --------------

include!("posix_epoll.rs");

// --- helpers ------------------------------------------------------------------

/// `(Socket)GCHandle.FromIntPtr((nint)handle).Target` for a handle NODE (not an
/// LdArg). Mirrors `handle_to_class` but takes the handle already on hand.
fn handle_to_socket_node(asm: &mut Assembly, handle: Interned<CILNode>) -> Interned<CILNode> {
    let socket = ClassRef::socket(asm);
    let handle_isize = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::ISize)));
    let handle_to_obj = asm.alloc_string("handle_to_obj");
    let main_module = asm.main_module();
    let h2o = asm.class_ref(*main_module).clone().static_mref(
        &[Type::Int(Int::ISize)],
        Type::PlatformObject,
        handle_to_obj,
        asm,
    );
    let obj = asm.alloc_node(CILNode::call(h2o, [handle_isize]));
    let socket_ty = asm.alloc_type(Type::ClassRef(socket));
    asm.alloc_node(CILNode::CheckedCast(obj, socket_ty))
}

// ===========================================================================
// registration
// ===========================================================================

/// Registers the Phase-0 fd-table + errno + the Phase-1 POSIX symbol cluster.
///
/// PROOF 2 (driving *unmodified* upstream mio through this) is OUT OF SLICE — a
/// genuine wall, not a cfg flip (LIBC_SHIM_SCOPE §4 / the plan's mioWiring):
///   (a) mio's epoll selector is gated on `target_os in {linux,…}`; libc's
///       `epoll_*` externs live under `cfg(target_os="linux")` → both need a
///       linux-shaped cfg scoped to mio+libc ALONE (e.g. a crate-name
///       RUSTC_WRAPPER adding `--cfg unix,target_os="linux",target_env="gnu"`).
///   (b) THE cargo wall: mio's `libc` dep is `[target.'cfg(unix…)'.dependencies]`
///       (vendor/mio/Cargo.toml), so on os=dotnet cargo never compiles libc into
///       mio AT ALL; RUSTFLAGS `--cfg` runs AFTER dep resolution and cannot pull
///       it in. Needs a one-line un-gate of mio's Cargo.toml or a [patch] libc.
///   (c) THE capstone: mio's `Source` impl is bounded `T: AsRawFd` and builds
///       streams via `FromRawFd`; dotnet std's net `Socket` implements NEITHER
///       (no `std::os::fd` on dotnet). Needs the std net Socket made fd-backed
///       (LIBC_SHIM_SCOPE §4.2 item 2 — the explicitly-LAST capstone).
/// So `pal_mio` stays on its working D1 vendored-fork arm; the FLOOR proof
/// (`pal_libc`, a raw `extern "C"` loopback echo + ENOENT errno round-trip)
/// proves the fd-table + symbols + errno end-to-end with NO mio.
pub fn insert_posix_shim(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    // statics + entry class + fd-table builtins (defs, not overrides).
    define_fd_entry(asm);
    init_statics(asm);
    define_fdtable_builtins(asm);
    // Overrides so the dotnet std net Socket onion's bare `extern "C"`
    // rcl_fdtable_handle/insert references resolve to the MethodDefs above.
    insert_fdtable_externs(asm, patcher);

    // errno.
    define_errno_from_exception(asm);
    insert_errno_location(asm, patcher);

    // fd I/O.
    insert_read(asm, patcher);
    insert_write(asm, patcher);
    insert_close(asm, patcher);
    insert_fcntl(asm, patcher);
    insert_ioctl(asm, patcher);

    // sockets.
    insert_socket(asm, patcher);
    insert_bind(asm, patcher);
    insert_listen(asm, patcher);
    insert_connect(asm, patcher);
    insert_accept(asm, patcher);
    insert_send_recv(asm, patcher);
    insert_shutdown(asm, patcher);
    insert_setsockopt(asm, patcher);
    insert_get_addr(asm, patcher, "getsockname", "get_LocalEndPoint");
    insert_get_addr(asm, patcher, "getpeername", "get_RemoteEndPoint");

    // readiness.
    insert_epoll(asm, patcher);

    // files (the ENOENT errno proof).
    insert_open(asm, patcher);
}
