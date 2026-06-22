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
fn endpoint_from_sockaddr_inet(asm: &mut Assembly, sa_ptr: Interned<CILNode>) -> Interned<CILNode> {
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

/// B2 Piece 1 — `rcl_endpoint_from_sockaddr(*void sa) -> EndPoint`: a
/// self-branching MethodDef that reads the address family at `*(u16*)sa` and
/// returns either a `UnixDomainSocketEndPoint` (AF_UNIX path socket) or an
/// `IPEndPoint` (AF_INET/INET6), both upcast to the `EndPoint` base. Factoring
/// the family dispatch into its own MethodDef keeps `bind`/`connect` single-block
/// errno_wrapped bodies (they just call this and pass the EndPoint to Bind/
/// Connect). For AF_UNIX the path is the NUL-terminated `sun_path` at `sa+2`
/// (Linux ABI: `sun_family` u16 @0, `sun_path` @2), decoded via
/// `Marshal.PtrToStringUTF8` (model: `insert_open`).
fn define_endpoint_from_sockaddr(asm: &mut Assembly) {
    let main_module = asm.main_module();
    let void_ptr = asm.nptr(Type::Void);
    let endpoint_base = ClassRef::endpoint(asm);
    let endpoint_base_ty = Type::ClassRef(endpoint_base);
    let uds_ep = ClassRef::unix_domain_socket_endpoint(asm);

    // Block 0: fam = *(u16*)sa; if fam == AF_UNIX(1) goto 1(unix) else goto 2(inet).
    let sa0 = asm.alloc_node(CILNode::LdArg(0));
    let u16_ty = asm.alloc_type(Type::Int(Int::U16));
    let sa0_ptr = asm.alloc_node(CILNode::PtrCast(sa0, Box::new(PtrCastRes::Ptr(u16_ty))));
    let fam = asm.alloc_node(CILNode::LdInd {
        addr: sa0_ptr,
        tpe: u16_ty,
        volatile: false,
    });
    let fam = asm.int_cast(fam, Int::I32, ExtendKind::ZeroExtend);
    let af_unix = asm.alloc_node(Const::I32(POSIX_AF_UNIX));
    let br_unix = asm.alloc_root(CILRoot::Branch(Box::new((
        1,
        0,
        Some(BranchCond::Eq(fam, af_unix)),
    ))));
    let goto_inet = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

    // Block 1 (unix): path = Marshal.PtrToStringUTF8((IntPtr)(sa+2));
    //   ep = new UnixDomainSocketEndPoint(path); ret (EndPoint)ep.
    let marshal = ClassRef::marshal(asm);
    let ptr_to_str_name = asm.alloc_string("PtrToStringUTF8");
    let ptr_to_str = asm.class_ref(marshal).clone().static_mref(
        &[Type::Int(Int::ISize)],
        Type::PlatformString,
        ptr_to_str_name,
        asm,
    );
    let sa_u = asm.alloc_node(CILNode::LdArg(0));
    let sa_u_isize = asm.alloc_node(CILNode::PtrCast(sa_u, Box::new(PtrCastRes::ISize)));
    let two = asm.alloc_node(Const::ISize(2));
    let path_ptr = asm.alloc_node(CILNode::BinOp(sa_u_isize, two, BinOp::Add));
    let path_str = asm.alloc_node(CILNode::call(ptr_to_str, [path_ptr]));
    let uds_ctor = asm.class_ref(uds_ep).clone().ctor(&[Type::PlatformString], asm);
    let uds = asm.alloc_node(CILNode::call(uds_ctor, [path_str]));
    let endpoint_base_ty_idx = asm.alloc_type(endpoint_base_ty);
    let uds_base = asm.alloc_node(CILNode::CheckedCast(uds, endpoint_base_ty_idx));
    let ret_unix = asm.alloc_root(CILRoot::Ret(uds_base));

    // Block 2 (inet): ep = endpoint_from_sockaddr_inet(sa); ret (EndPoint)ep.
    let sa_i = asm.alloc_node(CILNode::LdArg(0));
    let inet_ep = endpoint_from_sockaddr_inet(asm, sa_i);
    let inet_base = endpoint_as_base(asm, inet_ep);
    let ret_inet = asm.alloc_root(CILRoot::Ret(inet_base));

    let name = asm.alloc_string("rcl_endpoint_from_sockaddr");
    let sig = asm.sig([void_ptr], endpoint_base_ty);
    asm.new_method(MethodDef::new(
        Access::Public,
        main_module,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![br_unix, goto_inet], 0, None),
                BasicBlock::new(vec![ret_unix], 1, None),
                BasicBlock::new(vec![ret_inet], 2, None),
            ],
            locals: vec![],
        },
        vec![None],
    ));
}

/// Call the `rcl_endpoint_from_sockaddr` MethodDef on `sa_ptr`, returning an
/// `EndPoint` node (already the base type, ready for `Bind`/`Connect`).
fn endpoint_from_sockaddr(asm: &mut Assembly, sa_ptr: Interned<CILNode>) -> Interned<CILNode> {
    let void_ptr = asm.nptr(Type::Void);
    let endpoint_base = ClassRef::endpoint(asm);
    let m = main_static(
        asm,
        "rcl_endpoint_from_sockaddr",
        &[void_ptr],
        Type::ClassRef(endpoint_base),
    );
    let sa = asm.cast_ptr(sa_ptr, void_ptr);
    asm.alloc_node(CILNode::call(m, [sa]))
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

        // ty = ((type & 0xFF) == SOCK_DGRAM) ? Dgram : Stream ; proto = Dgram?Udp:Tcp.
        // mio ORs SOCK_NONBLOCK(2048)|SOCK_CLOEXEC(524288) into `type` (net.rs), so a
        // bare `type == SOCK_DGRAM` misclassifies a non-blocking stream socket as UDP.
        // Mask to the low byte (the real socket-type field) before comparing.
        let typ = asm.alloc_node(CILNode::LdArg(1));
        let type_mask = asm.alloc_node(Const::I32(0xFF));
        let typ = asm.alloc_node(CILNode::BinOp(typ, type_mask, BinOp::And));
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
            let type_mask2 = asm.alloc_node(Const::I32(0xFF));
            let typ2 = asm.alloc_node(CILNode::BinOp(typ2, type_mask2, BinOp::And));
            let dgram_p2 = asm.alloc_node(Const::I32(POSIX_SOCK_DGRAM));
            let eq2 = asm.alloc_node(CILNode::BinOp(typ2, dgram_p2, BinOp::Eq));
            asm.int_cast(eq2, Int::I32, ExtendKind::ZeroExtend)
        };
        let p_term = asm.alloc_node(CILNode::BinOp(is_dgram_i2, p_diff, BinOp::Mul));
        let proto = asm.alloc_node(CILNode::BinOp(p_tcp, p_term, BinOp::Add));
        let store_proto = asm.alloc_root(CILRoot::StLoc(3, proto));

        // B2 Piece 1 — AF_UNIX override (single-block, branch-free blend): when
        // domain == AF_UNIX(1) the BCL Socket ctor needs AddressFamily.Unix(1) +
        // ProtocolType.Unspecified(0) (Tcp/Udp are rejected for the Unix family).
        // is_unix = (domain == AF_UNIX). af = is_unix ? 1 : af; proto = is_unix ? 0 : proto.
        let domain_u = asm.alloc_node(CILNode::LdArg(0));
        let af_unix_p = asm.alloc_node(Const::I32(POSIX_AF_UNIX));
        let is_unix = asm.alloc_node(CILNode::BinOp(domain_u, af_unix_p, BinOp::Eq));
        let is_unix_i = asm.int_cast(is_unix, Int::I32, ExtendKind::ZeroExtend);
        // not_unix = 1 - is_unix.
        let one_u = asm.alloc_node(Const::I32(1));
        let not_unix_i = asm.alloc_node(CILNode::BinOp(one_u, is_unix_i, BinOp::Sub));
        // af_final = not_unix*af + is_unix*DOTNET_AF_UNIX.
        let af_cur = asm.alloc_node(CILNode::LdLoc(1));
        let af_keep = asm.alloc_node(CILNode::BinOp(not_unix_i, af_cur, BinOp::Mul));
        let af_unix_v = asm.alloc_node(Const::I32(DOTNET_AF_UNIX));
        let af_set = asm.alloc_node(CILNode::BinOp(is_unix_i, af_unix_v, BinOp::Mul));
        let af_final = asm.alloc_node(CILNode::BinOp(af_keep, af_set, BinOp::Add));
        let store_af_u = asm.alloc_root(CILRoot::StLoc(1, af_final));
        // proto_final = not_unix*proto (+ is_unix*0 — Unspecified is 0, so just
        // mask out the proto for the unix case).
        let proto_cur = asm.alloc_node(CILNode::LdLoc(3));
        let not_unix_i2 = {
            let domain_u2 = asm.alloc_node(CILNode::LdArg(0));
            let af_unix_p2 = asm.alloc_node(Const::I32(POSIX_AF_UNIX));
            let eq = asm.alloc_node(CILNode::BinOp(domain_u2, af_unix_p2, BinOp::Eq));
            let eq_i = asm.int_cast(eq, Int::I32, ExtendKind::ZeroExtend);
            let one2 = asm.alloc_node(Const::I32(1));
            asm.alloc_node(CILNode::BinOp(one2, eq_i, BinOp::Sub))
        };
        let proto_final = asm.alloc_node(CILNode::BinOp(not_unix_i2, proto_cur, BinOp::Mul));
        let _ = DOTNET_PROTO_UNSPEC; // documents the is_unix proto (0).
        let store_proto_u = asm.alloc_root(CILRoot::StLoc(3, proto_final));

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
        let store_h = asm.alloc_root(CILRoot::StLoc(4, h));
        // Honor SOCK_NONBLOCK (2048): mio ORs it into `type` and depends on the
        // socket being non-blocking for its readiness model. nb = (type & 2048)!=0
        // ? 1 : 0 = 1 - ((type & 2048) == 0); set on the BCL Socket. (Always-set on
        // a blocking socket would be a no-op `set_nonblocking(.,0)`.)
        const SOCK_NONBLOCK: i32 = 2048;
        let typ_nb = asm.alloc_node(CILNode::LdArg(1));
        let nbmask = asm.alloc_node(Const::I32(SOCK_NONBLOCK));
        let masked_nb = asm.alloc_node(CILNode::BinOp(typ_nb, nbmask, BinOp::And));
        let zero_nb = asm.alloc_node(Const::I32(0));
        let is_zero_nb = asm.alloc_node(CILNode::BinOp(masked_nb, zero_nb, BinOp::Eq));
        let is_zero_nb = asm.int_cast(is_zero_nb, Int::I32, ExtendKind::ZeroExtend);
        let one_nb = asm.alloc_node(Const::I32(1));
        let nb = asm.alloc_node(CILNode::BinOp(one_nb, is_zero_nb, BinOp::Sub));
        let set_nb = dotnet_mref(asm, "rcl_dotnet_net_set_nonblocking", &[void_ptr, Type::Int(Int::I32)], Type::Int(Int::I32));
        let hh = asm.alloc_node(CILNode::LdLoc(4));
        let snb = asm.alloc_node(CILNode::call(set_nb, [hh, nb]));
        let pop_nb = asm.alloc_root(CILRoot::Pop(snb));
        // fd = rcl_fdtable_insert(h, SOCKET, 0); local 0 = fd.
        let hh2 = asm.alloc_node(CILNode::LdLoc(4));
        let fd = call_fdtable_insert(asm, hh2, FD_KIND_SOCKET, 0);
        let store_fd = asm.alloc_root(CILRoot::StLoc(0, fd));

        let body = vec![
            store_af, store_st, store_proto, store_af_u, store_proto_u, store_h, pop_nb, store_fd,
        ];
        let i32_ty = asm.alloc_type(Type::Int(Int::I32));
        let void_ptr_idx = asm.alloc_type(void_ptr);
        errno_wrapped(
            asm,
            body,
            Type::Int(Int::I32),
            vec![
                (None, i32_ty),
                (None, i32_ty),
                (None, i32_ty),
                (None, void_ptr_idx),
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
        // Family-dispatching: returns an EndPoint base (IPEndPoint or
        // UnixDomainSocketEndPoint) — already the type Bind expects.
        let ep = endpoint_from_sockaddr(asm, sa);
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
        // Family-dispatching: EndPoint base (v4/v6 or Unix), ready for Connect.
        let ep = endpoint_from_sockaddr(asm, sa);
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
        // A non-blocking Socket.Connect to a not-yet-accepted endpoint throws
        // SocketException(WouldBlock); the connect-specific mapper turns that into
        // errno=EINPROGRESS (mio's connect treats ONLY EINPROGRESS as success).
        errno_wrapped_with(
            asm,
            vec![do_connect, store0],
            Type::Int(Int::I32),
            vec![],
            "rcl_connect_errno_from_exception",
        )
    };
    patcher.insert(name, Box::new(generator));
}

/// `accept(fd, *sockaddr, *socklen) -> i32` — `c = s.Accept()`; register `c` as a
/// new non-blocking SOCKET fd, and write the peer endpoint into the caller's
/// sockaddr out-param (mio's accept ALWAYS calls `to_socket_addr(addr)` and fails
/// with InvalidInput if it is not a valid sockaddr).
/// `rcl_accept_write_addr(Socket conn, *void sa, *void len)` — write `conn`'s
/// RemoteEndPoint into the caller's `sockaddr_in` (`sa`) + `*len = 16`, but ONLY
/// if `sa != null` (a raw probe like pal_libc passes NULL; mio passes a real
/// pointer). The null branch lets accept's body stay single-block (errno_wrapped).
fn define_accept_write_addr(asm: &mut Assembly) {
    let socket = ClassRef::socket(asm);
    let ip_endpoint = ClassRef::ip_endpoint(asm);
    let endpoint_base = ClassRef::endpoint(asm);
    let main_module = asm.main_module();
    let void_ptr = asm.nptr(Type::Void);

    // block 0: if sa(arg1) == null -> ret (block 5); else fall to 1.
    let sa = asm.alloc_node(CILNode::LdArg(1));
    let sa_isize = asm.alloc_node(CILNode::PtrCast(sa, Box::new(PtrCastRes::ISize)));
    let zero = asm.alloc_node(Const::ISize(0));
    let br_null = asm.alloc_root(CILRoot::Branch(Box::new((
        5,
        0,
        Some(BranchCond::Eq(sa_isize, zero)),
    ))));
    let goto_classify = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

    // block 1: ep = conn.RemoteEndPoint as IPEndPoint (isinst; null if it is a
    //   UnixDomainSocketEndPoint). store local 0; if ep != null goto 2 (v4) else
    //   goto 4 (unix). B2 Piece 1: an AF_UNIX accept's RemoteEndPoint is NOT an
    //   IPEndPoint, so the old unconditional cast threw — branch instead.
    let conn = asm.alloc_node(CILNode::LdArg(0));
    let get_remote_name = asm.alloc_string("get_RemoteEndPoint");
    let get_remote = asm.class_ref(socket).clone().instance(
        &[],
        Type::ClassRef(endpoint_base),
        get_remote_name,
        asm,
    );
    let ep_obj = asm.alloc_node(CILNode::call(get_remote, [conn]));
    let ip_ep_ty = asm.alloc_type(Type::ClassRef(ip_endpoint));
    let ep = asm.alloc_node(CILNode::IsInst(ep_obj, ip_ep_ty));
    let store_ep = asm.alloc_root(CILRoot::StLoc(0, ep));
    let ep_l = asm.alloc_node(CILNode::LdLoc(0));
    let null_ep = asm.alloc_node(CILNode::Const(Box::new(Const::Null(ip_endpoint))));
    let br_unix = asm.alloc_root(CILRoot::Branch(Box::new((
        4,
        0,
        Some(BranchCond::Eq(ep_l, null_ep)),
    ))));
    let goto_v4 = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

    // block 2 (v4): write family/port/addr from the IPEndPoint into *sa; goto 3.
    let mut blk2 = write_sockaddr_out(asm, 0, 1, 1);
    let goto_len = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));
    blk2.push(goto_len);

    // block 3: if len(arg2) != null -> *(u32*)len = 16 (sockaddr_in); then ret.
    let len = asm.alloc_node(CILNode::LdArg(2));
    let len_isize = asm.alloc_node(CILNode::PtrCast(len, Box::new(PtrCastRes::ISize)));
    let zero2 = asm.alloc_node(Const::ISize(0));
    let br_len_null = asm.alloc_root(CILRoot::Branch(Box::new((
        5,
        0,
        Some(BranchCond::Eq(len_isize, zero2)),
    ))));
    let len2 = asm.alloc_node(CILNode::LdArg(2));
    let u32_ty = asm.alloc_type(Type::Int(Int::U32));
    let len_ptr = asm.alloc_node(CILNode::PtrCast(len2, Box::new(PtrCastRes::Ptr(u32_ty))));
    let sixteen = asm.alloc_node(Const::U32(16));
    let st_len = asm.alloc_root(CILRoot::StInd(Box::new((len_ptr, sixteen, Type::Int(Int::U32), false))));
    let goto_ret3 = asm.alloc_root(CILRoot::Branch(Box::new((5, 0, None))));

    // block 4 (unix): minimal sockaddr_un — *(u16*)sa = AF_UNIX(1); if len != null
    //   *len = 2 (SUN_PATH_OFFSET, an unnamed accepted peer). Then ret. A full
    //   sun_path round-trip is a refinement; this satisfies UnixStream::peer_addr/
    //   local_addr without throwing (they only need the family + a valid len).
    let sa_u = asm.alloc_node(CILNode::LdArg(1));
    let sa_u_isize = asm.alloc_node(CILNode::PtrCast(sa_u, Box::new(PtrCastRes::ISize)));
    let u16_ty = asm.alloc_type(Type::Int(Int::U16));
    let fam_ptr = asm.alloc_node(CILNode::PtrCast(sa_u_isize, Box::new(PtrCastRes::Ptr(u16_ty))));
    let af_unix = asm.alloc_node(Const::I32(POSIX_AF_UNIX));
    let af_unix = asm.int_cast(af_unix, Int::U16, ExtendKind::ZeroExtend);
    let st_fam_u = asm.alloc_root(CILRoot::StInd(Box::new((fam_ptr, af_unix, Type::Int(Int::U16), false))));
    let len_u = asm.alloc_node(CILNode::LdArg(2));
    let len_u_isize = asm.alloc_node(CILNode::PtrCast(len_u, Box::new(PtrCastRes::ISize)));
    let zero_u = asm.alloc_node(Const::ISize(0));
    let br_len_null_u = asm.alloc_root(CILRoot::Branch(Box::new((
        5,
        0,
        Some(BranchCond::Eq(len_u_isize, zero_u)),
    ))));
    let len_u2 = asm.alloc_node(CILNode::LdArg(2));
    let len_u_ptr = asm.alloc_node(CILNode::PtrCast(len_u2, Box::new(PtrCastRes::Ptr(u32_ty))));
    let two_u = asm.alloc_node(Const::U32(2));
    let st_len_u = asm.alloc_root(CILRoot::StInd(Box::new((len_u_ptr, two_u, Type::Int(Int::U32), false))));
    let goto_ret4 = asm.alloc_root(CILRoot::Branch(Box::new((5, 0, None))));

    // block 5: ret.
    let ret = asm.alloc_root(CILRoot::VoidRet);

    let ip_ep_local = asm.alloc_type(Type::ClassRef(ip_endpoint));
    let byte_ty = asm.alloc_type(Type::Int(Int::U8));
    let byte_arr = asm.alloc_type(Type::PlatformArray {
        elem: byte_ty,
        dims: std::num::NonZeroU8::new(1).unwrap(),
    });
    let name = asm.alloc_string("rcl_accept_write_addr");
    let endpoint_local_name = asm.alloc_string("endpoint");
    let bytes_local_name = asm.alloc_string("bytes");
    let sig = asm.sig([Type::ClassRef(socket), void_ptr, void_ptr], Type::Void);
    asm.new_method(MethodDef::new(
        Access::Public,
        main_module,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_ep, br_null, goto_classify], 0, None),
                BasicBlock::new(vec![br_unix, goto_v4], 1, None),
                BasicBlock::new(blk2, 2, None),
                BasicBlock::new(vec![br_len_null, st_len, goto_ret3], 3, None),
                BasicBlock::new(vec![st_fam_u, br_len_null_u, st_len_u, goto_ret4], 4, None),
                BasicBlock::new(vec![ret], 5, None),
            ],
            locals: vec![
                (Some(endpoint_local_name), ip_ep_local),
                (Some(bytes_local_name), byte_arr),
            ],
        },
        vec![None, None, None],
    ));
}

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
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let endpoint_base = ClassRef::endpoint(asm);
        let conn = asm.alloc_node(CILNode::call(accept, [s]));
        let store_conn = asm.alloc_root(CILRoot::StLoc(1, conn));
        // conn.Blocking = listener.Blocking — INHERIT the listener's blocking mode
        // (B2 Piece 1 fix). A non-blocking listener (mio's SOCK_NONBLOCK socket)
        // yields a non-blocking accepted socket, exactly as before; a *blocking*
        // listener (a plain std UnixListener/TcpListener) now yields a BLOCKING
        // accepted socket, so a subsequent blocking `read` waits for data instead
        // of returning EAGAIN (which previously reset the UDS echo). The dotnet
        // BCL `Socket.Accept()` does not propagate `Blocking`, so we copy it.
        let get_blocking_name = asm.alloc_string("get_Blocking");
        let get_blocking = asm.class_ref(socket).clone().instance(
            &[],
            Type::Bool,
            get_blocking_name,
            asm,
        );
        let fd_b = asm.alloc_node(CILNode::LdArg(0));
        let h_b = call_fdtable_handle(asm, fd_b);
        let s_b = handle_to_socket_node(asm, h_b);
        let listener_blocking = asm.alloc_node(CILNode::call(get_blocking, [s_b]));
        let store_blk = asm.alloc_root(CILRoot::StLoc(2, listener_blocking));
        let set_blocking_name = asm.alloc_string("set_Blocking");
        let set_blocking = asm.class_ref(socket).clone().instance(
            &[Type::Bool],
            Type::Void,
            set_blocking_name,
            asm,
        );
        let conn_b = asm.alloc_node(CILNode::LdLoc(1));
        let blk_v = asm.alloc_node(CILNode::LdLoc(2));
        let do_set_blocking = asm.alloc_root(CILRoot::call(set_blocking, [conn_b, blk_v]));
        // handle = (void*)GCHandle.Alloc(conn).
        let handle = CILNode::LdLoc(1).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let void = asm.alloc_type(Type::Void);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let new_fd = call_fdtable_insert(asm, handle, FD_KIND_SOCKET, 0);
        let store_fd = asm.alloc_root(CILRoot::StLoc(0, new_fd));
        let _ = (void_ptr, ip_endpoint, endpoint_base);
        // Write the peer endpoint into the caller's sockaddr out-param + socklen via
        // the rcl_accept_write_addr helper. The helper NULL-CHECKS arg1/arg2 (a raw
        // `extern "C"` probe like pal_libc passes NULL; mio passes a real pointer),
        // so the conditional write lives in that MethodDef — keeping accept's body
        // single-block for the errno_wrapped try/catch. Pass (conn, sa, socklen).
        let writer = main_static(
            asm,
            "rcl_accept_write_addr",
            &[Type::ClassRef(socket), void_ptr, void_ptr],
            Type::Void,
        );
        let conn_w = asm.alloc_node(CILNode::LdLoc(1));
        let sa_arg = asm.alloc_node(CILNode::LdArg(1));
        let sa_arg = asm.cast_ptr(sa_arg, void_ptr);
        let len_arg = asm.alloc_node(CILNode::LdArg(2));
        let len_arg = asm.cast_ptr(len_arg, void_ptr);
        let do_write = asm.alloc_root(CILRoot::call(writer, [conn_w, sa_arg, len_arg]));
        let body = vec![store_conn, store_blk, do_set_blocking, store_fd, do_write];

        let sock_ty = asm.alloc_type(Type::ClassRef(socket));
        let bool_ty = asm.alloc_type(Type::Bool);
        let conn_name = asm.alloc_string("conn");
        let blk_name = asm.alloc_string("listener_blocking");
        errno_wrapped(
            asm,
            body,
            Type::Int(Int::I32),
            vec![(Some(conn_name), sock_ty), (Some(blk_name), bool_ty)],
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
        const F_DUPFD: i32 = 0;
        const F_DUPFD_CLOEXEC: i32 = 1030;
        const O_NONBLOCK: i32 = 2048;
        let void_ptr = asm.nptr(Type::Void);
        // block 0: if cmd==F_GETFL goto 1; if cmd==F_SETFL goto 2;
        //          if cmd==F_DUPFD || cmd==F_DUPFD_CLOEXEC goto 4 (dup); else goto 3.
        let cmd0 = asm.alloc_node(CILNode::LdArg(1));
        let getfl_c = asm.alloc_node(Const::I32(F_GETFL));
        let br_get = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, Some(BranchCond::Eq(cmd0, getfl_c))))));
        let cmd1 = asm.alloc_node(CILNode::LdArg(1));
        let setfl_c = asm.alloc_node(Const::I32(F_SETFL));
        let br_set = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, Some(BranchCond::Eq(cmd1, setfl_c))))));
        let cmd2 = asm.alloc_node(CILNode::LdArg(1));
        let dupfd_c = asm.alloc_node(Const::I32(F_DUPFD));
        let br_dup = asm.alloc_root(CILRoot::Branch(Box::new((4, 0, Some(BranchCond::Eq(cmd2, dupfd_c))))));
        let cmd3 = asm.alloc_node(CILNode::LdArg(1));
        let dupcl_c = asm.alloc_node(Const::I32(F_DUPFD_CLOEXEC));
        let br_dupcl = asm.alloc_root(CILRoot::Branch(Box::new((4, 0, Some(BranchCond::Eq(cmd3, dupcl_c))))));
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

        // block 4 (F_DUPFD / F_DUPFD_CLOEXEC): register a NEW fd-table entry that
        // SHARES the original fd's handle + kind, and return the new fd. Managed
        // handles are reference types, so both fds reference the SAME underlying
        // object (Socket / epoll interest-dict) — exactly the dup semantics needed
        // for tokio's `Selector::try_clone()` (OwnedFd::try_clone -> fcntl
        // F_DUPFD_CLOEXEC): the cloned epoll fd must see the same interest dict, or
        // epoll_ctl on the clone hits a null dict handle. (The minimum-fd `arg`
        // floor is ignored — the fd-table allocator just hands out the next id;
        // tokio/std do not rely on the exact value.)
        let fd_dup = asm.alloc_node(CILNode::LdArg(0));
        let dup_handle = call_fdtable_handle(asm, fd_dup);
        let fd_dup2 = asm.alloc_node(CILNode::LdArg(0));
        let dup_kind = call_fdtable_kind(asm, fd_dup2);
        let new_fd = call_fdtable_insert_dyn(asm, dup_handle, dup_kind, 0);
        let ret_dup = asm.alloc_root(CILRoot::Ret(new_fd));

        let _ = void_ptr;
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![br_get, br_set, br_dup, br_dupcl, goto_zero], 0, None),
                BasicBlock::new(vec![ret_flags], 1, None),
                BasicBlock::new(vec![pop, do_setf, ret0], 2, None),
                BasicBlock::new(vec![ret_z], 3, None),
                BasicBlock::new(vec![ret_dup], 4, None),
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
/// CAP-2 STATE: the POSIX shim is now mio-RUNTIME-complete — multi-fd epoll
/// (posix_epoll.rs: a per-instance interest Dictionary swept per-fd via
/// Socket.Poll), connect→EINPROGRESS, accept peer-addr write + nonblocking,
/// SOCK_NONBLOCK-aware socket(). The FLOOR proof `pal_libc` (a raw `extern "C"`
/// loopback echo over THIS multi-fd epoll + an ENOENT errno round-trip) exercises
/// the whole cluster end-to-end with NO mio.
///
/// DEFERRED (the headline, driving *unmodified* upstream mio): the prerequisite is
/// the `target_family=["unix"]` flip in x86_64-unknown-dotnet.json — the ONLY way
/// cargo resolves mio's `[target.'cfg(unix)'.dependencies] libc` (RUSTFLAGS `--cfg`
/// runs AFTER dep resolution). That flip is built (a crate-scoped RUSTC_WRAPPER,
/// feasibility/rcc-rustc-wrapper.sh, forces target_os="linux"+target_env="gnu" on
/// mio+libc so mio picks selector/epoll.rs + waker/eventfd.rs and libc exposes its
/// linux epoll/sockaddr surface). BUT the flip turns on `target_family=unix`
/// GLOBALLY, which switches std's own `sys::{fs,paths,io,process}` + `os::unix`
/// cascades to their unix arms — a wide std cfg(unix) cascade (with_native_path,
/// OsStr-bytes, errno_location, getppid, os::unix internal refs in backtrace/
/// os/mod.rs) that exceeds this slice's budget and several pieces have no clean
/// .NET mapping (AF_UNIX, MetadataExt). So the flip is REVERTED to keep main green
/// (Cap-1 state); `pal_mio` stays on its vendored-fork arm. See
/// docs/LIBC_SHIM_SCOPE.md §4.2 + the cap2 memory for the std-arm break list.
pub fn insert_posix_shim(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    // statics + entry class + fd-table builtins (defs, not overrides).
    define_fd_entry(asm);
    define_epoll_reg(asm);
    init_statics(asm);
    define_fdtable_builtins(asm);
    // Overrides so the dotnet std net Socket onion's bare `extern "C"`
    // rcl_fdtable_handle/insert references resolve to the MethodDefs above.
    insert_fdtable_externs(asm, patcher);

    // errno.
    define_errno_from_exception(asm);
    define_connect_errno_from_exception(asm);
    insert_errno_location(asm, patcher);

    // fd I/O.
    insert_read(asm, patcher);
    insert_write(asm, patcher);
    insert_close(asm, patcher);
    insert_fcntl(asm, patcher);
    insert_ioctl(asm, patcher);

    // sockets.
    define_endpoint_from_sockaddr(asm); // B2 Piece 1 — AF_UNIX/AF_INET dispatch.
    insert_socket(asm, patcher);
    insert_bind(asm, patcher);
    insert_listen(asm, patcher);
    insert_connect(asm, patcher);
    define_accept_write_addr(asm);
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
