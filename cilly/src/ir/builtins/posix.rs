//! A POSIX/libc-over-.NET shim — the **proof slice** (Phase 0 infra + the
//! mio/tokio-net symbol cluster). See `docs/LIBC_SHIM_SCOPE.md` for the full
//! design; this implements §3 (fd-table + errno) and §5 Phase 0/1.
//!
//! The idea (LIBC_SHIM_SCOPE §1): the .NET PAL already implements the hard parts
//! (sockets, files, alloc, time) as `rcl_dotnet_*` CIL bodies over the BCL
//! ([`super::dotnet`]). A POSIX C-ABI caller (`mio`, `socket2`, a `-sys` crate,
//! or a raw `extern "C"` probe) wants *integer file descriptors* and *bare POSIX
//! symbol names* (`socket`, `read`, `epoll_wait`, …) plus a thread-local `errno`.
//! This module is the seam: it
//!   1. owns a process-global int-fd ⇄ `GCHandle` **fd-table** (the spine),
//!   2. owns a thread-local **errno** cell + an exception→errno translation, and
//!   3. registers the bare POSIX symbols as [`MissingMethodPatcher`] overrides,
//!      each threading its int fd through the fd-table to an *existing*
//!      `rcl_dotnet_*` body — it **re-packages** the shipped bodies, it does not
//!      re-implement the BCL logic.
//!
//! The patcher keys on the demangled symbol name's last `::` segment
//! (`Assembly::patch_missing_methods`, asm.rs ~1120). A bare `extern "C"` symbol
//! has no `::`, so `socket`/`read`/… match their override directly, and overrides
//! are tried *before* the `LIBC_FNS` host-libc externs fallback — so e.g.
//! `__errno_location` (in `LIBC_FNS`) is captured here, not routed to host libc.
//!
//! ## The honest leak (LIBC_SHIM_SCOPE §3.2)
//! The BCL signals I/O failure by **throwing**, not by an errno. Every leaky
//! wrapper wraps its `rcl_dotnet_*` call in a `try/catch` and, on a caught
//! exception, maps it to a POSIX errno + returns `-1`. The map is lossy: ~20
//! `SocketError` codes map cleanly (notably `WouldBlock`→`EAGAIN`, load-bearing
//! for non-blocking sockets), and the long `IOException`/HResult tail collapses
//! to `EIO`. `EINTR` never fires (fine: `is_interrupted` stays false). This is the
//! single biggest honesty caveat of the tier.
//!
//! ## Scope of THIS slice
//! Phase 0 (fd-table + errno + symbol registration) + the Phase-1 mio/tokio-net
//! cluster (fd I/O, sockets, epoll readiness). Genuinely-new BCL is only
//! `rcl_dotnet_net_socket` (create-without-endpoint, in [`super::dotnet`]) + the
//! `sockaddr`↔`(family,ip,port)` parse here + the epoll interest dict. Driving
//! *unmodified* upstream mio through this (proof 2) is out of slice — see the note
//! at the end of [`insert_posix_shim`] and LIBC_SHIM_SCOPE §4.

use super::dotnet::{bcl_enum_to_i32, endpoint_as_base};
use crate::cilnode::{ExtendKind, MethodKind, PtrCastRes};
use crate::ir::asm::MissingMethodPatcher;
use crate::ir::cilroot::BranchCond;
use crate::ir::tpe::GenericKind;
use crate::ir::{
    Access, BasicBlock, CILNode, CILRoot, ClassDef, ClassRef, Const, FieldDesc, Int, Interned,
    MethodDef, MethodImpl, MethodRef, StaticFieldDesc, Type,
};
use crate::{Assembly, BinOp};

// fd-kind tags (the entry's `kind`). STD 0/1/2 are pre-seeded.
const FD_KIND_STD: i32 = 0;
const FD_KIND_FILE: i32 = 1;
const FD_KIND_SOCKET: i32 = 2;
const FD_KIND_EPOLL: i32 = 3;
// FD_KIND_EVENTFD (4) is RETIRED: eventfd() now returns a real FD_KIND_SOCKET fd
// backed by a self-readable loopback UDP socket (posix_epoll.rs::insert_eventfd),
// so read/write/poll/close all kind-dispatch through the net path. Kept reserved
// (not reused) so the tag space stays stable across the docs.
#[allow(dead_code)]
const FD_KIND_EVENTFD: i32 = 4;

// POSIX errno values (Linux x86_64 numbering — the shim hardcodes the Linux ABI).
const ENOENT: i32 = 2;
const EIO: i32 = 5;
const EAGAIN: i32 = 11;
const EADDRINUSE: i32 = 98;
const EINPROGRESS: i32 = 115;
const ECONNRESET: i32 = 104;
const ETIMEDOUT: i32 = 110;
const ECONNREFUSED: i32 = 111;
// fs-side errno values added for the exception→errno enrichment (PAL-fidelity).
// Mapping is HOST-AGNOSTIC (the BCL throws the same exception types on Unix-host
// and Windows-host CoreCLR); see the per-arm caveats in the mapper.
#[allow(dead_code)]
const EPERM: i32 = 1;
const EACCES: i32 = 13;
#[allow(dead_code)]
const EBUSY: i32 = 16;
#[allow(dead_code)]
const EEXIST: i32 = 17;
#[allow(dead_code)]
const EPIPE: i32 = 32;
const ENAMETOOLONG: i32 = 36;
const ENOSYS: i32 = 38;

// .NET `System.Net.Sockets.SocketError` enum ints (the curated set).
const SE_WOULD_BLOCK: i32 = 10035;
const SE_CONN_RESET: i32 = 10054;
const SE_TIMED_OUT: i32 = 10060;
const SE_CONN_REFUSED: i32 = 10061;
const SE_ADDR_IN_USE: i32 = 10048;

// .NET enum ints reused below.
const DOTNET_AF_INET: i32 = 2; // AddressFamily.InterNetwork
const DOTNET_AF_INET6: i32 = 23; // AddressFamily.InterNetworkV6
const DOTNET_SOCKTYPE_STREAM: i32 = 1; // SocketType.Stream
const DOTNET_SOCKTYPE_DGRAM: i32 = 2; // SocketType.Dgram
const DOTNET_PROTO_TCP: i32 = 6; // ProtocolType.Tcp
const DOTNET_PROTO_UDP: i32 = 17; // ProtocolType.Udp
// B2 Piece 1 — AF_UNIX: AddressFamily.Unix == 1 == POSIX AF_UNIX (Linux ABI), so
// the dotnet enum value and the POSIX domain int coincide. ProtocolType for a
// Unix socket MUST be Unspecified(0); the BCL Socket ctor throws if Tcp(6) is
// paired with AddressFamily.Unix.
const DOTNET_AF_UNIX: i32 = 1; // AddressFamily.Unix
const DOTNET_PROTO_UNSPEC: i32 = 0; // ProtocolType.Unspecified

// POSIX constants the wrappers branch on.
const POSIX_AF_INET6: i32 = 10;
const POSIX_AF_UNIX: i32 = 1;
const POSIX_SOCK_DGRAM: i32 = 2;

// ===========================================================================
// statics + RclFdEntry class
// ===========================================================================

/// `System.Collections.Generic.Dictionary<i32, object>` — the fd-table value type
/// and the epoll interest-dict type.
fn fd_dict(asm: &mut Assembly) -> Interned<ClassRef> {
    ClassRef::dictionary(Type::Int(Int::I32), Type::PlatformObject, asm)
}
fn fd_table_sfld(asm: &mut Assembly) -> Interned<StaticFieldDesc> {
    let main_mod = *asm.main_module();
    let dict = fd_dict(asm);
    let name = asm.alloc_string("rcl_fd_table");
    asm.alloc_sfld(StaticFieldDesc::new(main_mod, name, Type::ClassRef(dict)))
}
fn fd_next_sfld(asm: &mut Assembly) -> Interned<StaticFieldDesc> {
    let main_mod = *asm.main_module();
    let name = asm.alloc_string("rcl_fd_next");
    asm.alloc_sfld(StaticFieldDesc::new(main_mod, name, Type::Int(Int::I32)))
}
fn errno_sfld(asm: &mut Assembly) -> Interned<StaticFieldDesc> {
    let main_mod = *asm.main_module();
    let name = asm.alloc_string("rcl_errno");
    asm.alloc_sfld(StaticFieldDesc::new(main_mod, name, Type::Int(Int::I32)))
}

/// The `RclFdEntry` managed class (boxed into `Dictionary<i32,object>`).
fn fd_entry_class(asm: &mut Assembly) -> Interned<ClassRef> {
    let name = asm.alloc_string("RclFdEntry");
    asm.alloc_class_ref(ClassRef::new(name, None, false, [].into()))
}
fn fd_entry_handle_field(asm: &mut Assembly) -> Interned<FieldDesc> {
    let entry = fd_entry_class(asm);
    let name = asm.alloc_string("handle");
    asm.alloc_field(FieldDesc::new(entry, name, Type::Int(Int::ISize)))
}
fn fd_entry_kind_field(asm: &mut Assembly) -> Interned<FieldDesc> {
    let entry = fd_entry_class(asm);
    let name = asm.alloc_string("kind");
    asm.alloc_field(FieldDesc::new(entry, name, Type::Int(Int::I32)))
}
fn fd_entry_flags_field(asm: &mut Assembly) -> Interned<FieldDesc> {
    let entry = fd_entry_class(asm);
    let name = asm.alloc_string("flags");
    asm.alloc_field(FieldDesc::new(entry, name, Type::Int(Int::I32)))
}

/// Define `RclFdEntry` (fields handle/kind/flags + a ctor). `Access::Extern`
/// survives DCE (the documented `UnmanagedThreadStart` workaround, thread.rs:432).
fn define_fd_entry(asm: &mut Assembly) {
    let name = asm.alloc_string("RclFdEntry");
    let object = ClassRef::object(asm);
    let handle = asm.alloc_string("handle");
    let kind = asm.alloc_string("kind");
    let flags = asm.alloc_string("flags");
    let entry = asm
        .class_def(ClassDef::new(
            name,
            false,
            0,
            Some(object),
            vec![
                (Type::Int(Int::ISize), handle, None),
                (Type::Int(Int::I32), kind, None),
                (Type::Int(Int::I32), flags, None),
            ],
            vec![],
            Access::Extern,
            None,
            None,
            true,
        ))
        .unwrap();

    let ctor_name = asm.alloc_string(".ctor");
    let this = asm.alloc_node(CILNode::LdArg(0));
    let ld_handle = asm.alloc_node(CILNode::LdArg(1));
    let ld_kind = asm.alloc_node(CILNode::LdArg(2));
    let ld_flags = asm.alloc_node(CILNode::LdArg(3));
    let handle_field = asm.alloc_field(FieldDesc::new(*entry, handle, Type::Int(Int::ISize)));
    let kind_field = asm.alloc_field(FieldDesc::new(*entry, kind, Type::Int(Int::I32)));
    let flags_field = asm.alloc_field(FieldDesc::new(*entry, flags, Type::Int(Int::I32)));
    let set_handle = asm.alloc_root(CILRoot::SetField(Box::new((handle_field, this, ld_handle))));
    let set_kind = asm.alloc_root(CILRoot::SetField(Box::new((kind_field, this, ld_kind))));
    let set_flags = asm.alloc_root(CILRoot::SetField(Box::new((flags_field, this, ld_flags))));
    let ret = asm.alloc_root(CILRoot::VoidRet);
    let ctor_sig = asm.sig(
        [
            Type::ClassRef(*entry),
            Type::Int(Int::ISize),
            Type::Int(Int::I32),
            Type::Int(Int::I32),
        ],
        Type::Void,
    );
    asm.new_method(MethodDef::new(
        Access::Public,
        entry,
        ctor_name,
        ctor_sig,
        MethodKind::Constructor,
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![set_handle, set_kind, set_flags, ret],
                0,
                None,
            )],
            locals: vec![],
        },
        vec![None, Some(handle), Some(kind), Some(flags)],
    ));
}

/// `new RclFdEntry(handle, kind, flags)`.
fn new_fd_entry(
    asm: &mut Assembly,
    handle: Interned<CILNode>,
    kind: Interned<CILNode>,
    flags: Interned<CILNode>,
) -> Interned<CILNode> {
    let entry = fd_entry_class(asm);
    let ctor = asm.class_ref(entry).clone().ctor(
        &[
            Type::Int(Int::ISize),
            Type::Int(Int::I32),
            Type::Int(Int::I32),
        ],
        asm,
    );
    asm.alloc_node(CILNode::call(ctor, [handle, kind, flags]))
}

// ===========================================================================
// RclEpollReg — the per-fd interest record stored in an epoll instance's
// interest dict (`Dictionary<i32, object>`, key = registered fd, value = boxed
// RclEpollReg). Multi-fd epoll: ONE epoll instance tracks a SET of registered
// fds, each carrying its `events` mask + epoll_data `token`. (Cap-2 upgrade from
// the Cap-1 single-fd-per-instance epoll.)
// ===========================================================================

/// The `RclEpollReg` managed class: `{ events: i32, token: i64 }` (a reference
/// type, like RclFdEntry).
fn epoll_reg_class(asm: &mut Assembly) -> Interned<ClassRef> {
    let name = asm.alloc_string("RclEpollReg");
    asm.alloc_class_ref(ClassRef::new(name, None, false, [].into()))
}
fn epoll_reg_events_field(asm: &mut Assembly) -> Interned<FieldDesc> {
    let cls = epoll_reg_class(asm);
    let name = asm.alloc_string("events");
    asm.alloc_field(FieldDesc::new(cls, name, Type::Int(Int::I32)))
}
fn epoll_reg_token_field(asm: &mut Assembly) -> Interned<FieldDesc> {
    let cls = epoll_reg_class(asm);
    let name = asm.alloc_string("token");
    asm.alloc_field(FieldDesc::new(cls, name, Type::Int(Int::I64)))
}
/// `last_ready: i32` — edge-trigger (EPOLLET) state. epoll_wait reports an
/// EPOLLET-flagged fd ONLY on a not-ready -> ready transition, and updates this
/// each sweep. WITHOUT this, a perpetually-readable EPOLLET fd (e.g. the tokio
/// eventfd waker, which is never drained because edge-triggered) re-fires every
/// sweep and busy-spins the reactor, starving the real I/O epoll.
fn epoll_reg_last_ready_field(asm: &mut Assembly) -> Interned<FieldDesc> {
    let cls = epoll_reg_class(asm);
    let name = asm.alloc_string("last_ready");
    asm.alloc_field(FieldDesc::new(cls, name, Type::Int(Int::I32)))
}

/// Define `RclEpollReg` (fields events/token + a ctor). `Access::Extern` survives
/// DCE (the documented `UnmanagedThreadStart` workaround, as for RclFdEntry).
fn define_epoll_reg(asm: &mut Assembly) {
    let name = asm.alloc_string("RclEpollReg");
    let object = ClassRef::object(asm);
    let events = asm.alloc_string("events");
    let token = asm.alloc_string("token");
    let last_ready = asm.alloc_string("last_ready");
    let cls = asm
        .class_def(ClassDef::new(
            name,
            false,
            0,
            Some(object),
            vec![
                (Type::Int(Int::I32), events, None),
                (Type::Int(Int::I64), token, None),
                (Type::Int(Int::I32), last_ready, None),
            ],
            vec![],
            Access::Extern,
            None,
            None,
            true,
        ))
        .unwrap();

    let ctor_name = asm.alloc_string(".ctor");
    let this = asm.alloc_node(CILNode::LdArg(0));
    let ld_events = asm.alloc_node(CILNode::LdArg(1));
    let ld_token = asm.alloc_node(CILNode::LdArg(2));
    let events_field = asm.alloc_field(FieldDesc::new(*cls, events, Type::Int(Int::I32)));
    let token_field = asm.alloc_field(FieldDesc::new(*cls, token, Type::Int(Int::I64)));
    let last_ready_field = asm.alloc_field(FieldDesc::new(*cls, last_ready, Type::Int(Int::I32)));
    let set_events = asm.alloc_root(CILRoot::SetField(Box::new((events_field, this, ld_events))));
    let set_token = asm.alloc_root(CILRoot::SetField(Box::new((token_field, this, ld_token))));
    // last_ready starts 0 (not-ready) so the first readiness is an edge.
    let this2 = asm.alloc_node(CILNode::LdArg(0));
    let zero_lr = asm.alloc_node(Const::I32(0));
    let set_lr = asm.alloc_root(CILRoot::SetField(Box::new((
        last_ready_field,
        this2,
        zero_lr,
    ))));
    let ret = asm.alloc_root(CILRoot::VoidRet);
    let ctor_sig = asm.sig(
        [
            Type::ClassRef(*cls),
            Type::Int(Int::I32),
            Type::Int(Int::I64),
        ],
        Type::Void,
    );
    asm.new_method(MethodDef::new(
        Access::Public,
        cls,
        ctor_name,
        ctor_sig,
        MethodKind::Constructor,
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![set_events, set_token, set_lr, ret],
                0,
                None,
            )],
            locals: vec![],
        },
        vec![None, Some(events), Some(token)],
    ));
}

/// `new RclEpollReg(events, token)`.
fn new_epoll_reg(
    asm: &mut Assembly,
    events: Interned<CILNode>,
    token: Interned<CILNode>,
) -> Interned<CILNode> {
    let cls = epoll_reg_class(asm);
    let ctor = asm
        .class_ref(cls)
        .clone()
        .ctor(&[Type::Int(Int::I32), Type::Int(Int::I64)], asm);
    asm.alloc_node(CILNode::call(ctor, [events, token]))
}

/// Allocate the fd-table statics + initialize them. The fd-table (the `rcl_fd_table`
/// Dictionary + the `rcl_fd_next` counter) is **process-global**, so its init goes
/// in the once-per-process `.cctor` — NOT the per-thread `.tcctor`. (A previous
/// version seeded it in the tcctor; that re-ran the seed on every new managed
/// thread, clobbering the shared table — a spawned thread would reset
/// `rcl_fd_next` to 3 and wipe the dict, so fds registered by other threads
/// vanished. The single-managed-thread floor proof masked this; the net Socket's
/// real worker threads exposed it.) `rcl_errno` IS thread-local ([ThreadStatic]),
/// so its zero-init is implicit per-thread and needs no cctor/tcctor seed.
fn init_statics(asm: &mut Assembly) {
    let main_mod = asm.main_module();
    let dict = fd_dict(asm);
    asm.add_static(
        Type::ClassRef(dict),
        "rcl_fd_table",
        false,
        main_mod,
        None,
        false,
    );
    asm.add_static(
        Type::Int(Int::I32),
        "rcl_fd_next",
        false,
        main_mod,
        None,
        false,
    );
    // errno: thread_local=TRUE -> the exporter emits [ThreadStatic].
    asm.add_static(
        Type::Int(Int::I32),
        "rcl_errno",
        true,
        main_mod,
        None,
        false,
    );

    // rcl_fd_table = new Dictionary<i32,object>();
    let table_sfld = fd_table_sfld(asm);
    let dict_ctor = asm[dict].clone().ctor(&[], asm);
    let new_dict = asm.alloc_node(CILNode::call(dict_ctor, []));
    let init_table = asm.alloc_root(CILRoot::SetStaticField {
        field: table_sfld,
        val: new_dict,
    });

    // Pre-seed fd 0/1/2 as STD sentinels (handle 0).
    let mut roots = vec![init_table];
    for fd in 0..3_i32 {
        let dict_node = asm.alloc_node(CILNode::LdStaticField(table_sfld));
        let fd_node = asm.alloc_node(Const::I32(fd));
        let zero_h = asm.alloc_node(Const::ISize(0));
        let std_kind = asm.alloc_node(Const::I32(FD_KIND_STD));
        let zero_f = asm.alloc_node(Const::I32(0));
        let entry = new_fd_entry(asm, zero_h, std_kind, zero_f);
        roots.push(dict_set_item(asm, dict_node, fd_node, entry));
    }

    // rcl_fd_next = 3.
    let next_sfld = fd_next_sfld(asm);
    let three = asm.alloc_node(Const::I32(3));
    let init_next = asm.alloc_root(CILRoot::SetStaticField {
        field: next_sfld,
        val: three,
    });
    roots.push(init_next);

    // Process-global init -> the .cctor (runs once per process), NOT .tcctor.
    asm.add_cctor(&roots);
}

// ===========================================================================
// fd-table builtins — MethodDefs on main_module (like handle_to_obj). The POSIX
// wrappers CALL them; the two-pass patcher resolves the forward refs.
// ===========================================================================

fn load_fd_table(asm: &mut Assembly) -> Interned<CILNode> {
    let sfld = fd_table_sfld(asm);
    asm.alloc_node(CILNode::LdStaticField(sfld))
}
fn dict_get_item(
    asm: &mut Assembly,
    dict: Interned<CILNode>,
    key: Interned<CILNode>,
) -> Interned<CILNode> {
    // On a generic `Dictionary<K,V>` the method ref must name the params as the
    // class generics (!0/!1), NOT the concrete instantiation — exactly how the
    // pthread_keys dict calls set_Item/Remove (thread.rs).
    let fd_dict = fd_dict(asm);
    let get_item = asm.alloc_string("get_Item");
    let get = asm.class_ref(fd_dict).clone().virtual_mref(
        &[Type::PlatformGeneric(0, GenericKind::TypeGeneric)],
        Type::PlatformGeneric(1, GenericKind::TypeGeneric),
        get_item,
        asm,
    );
    asm.alloc_node(CILNode::call(get, [dict, key]))
}
fn dict_set_item(
    asm: &mut Assembly,
    dict: Interned<CILNode>,
    key: Interned<CILNode>,
    val: Interned<CILNode>,
) -> Interned<CILRoot> {
    let fd_dict = fd_dict(asm);
    let set_item = asm.alloc_string("set_Item");
    let set = asm.class_ref(fd_dict).clone().virtual_mref(
        &[
            Type::PlatformGeneric(0, GenericKind::TypeGeneric),
            Type::PlatformGeneric(1, GenericKind::TypeGeneric),
        ],
        Type::Void,
        set_item,
        asm,
    );
    asm.alloc_root(CILRoot::call(set, [dict, key, val]))
}
/// `(RclFdEntry)rcl_fd_table.get_Item(LdArg(fd_arg))`.
fn entry_of_fd(asm: &mut Assembly, fd_arg: u32) -> Interned<CILNode> {
    let dict = load_fd_table(asm);
    let fd = asm.alloc_node(CILNode::LdArg(fd_arg));
    let obj = dict_get_item(asm, dict, fd);
    let entry = fd_entry_class(asm);
    let entry_ty = asm.alloc_type(Type::ClassRef(entry));
    asm.alloc_node(CILNode::CheckedCast(obj, entry_ty))
}

/// Helper to register a straight-line main-module static MethodDef.
fn define_main_method(
    asm: &mut Assembly,
    name: &str,
    inputs: &[Type],
    output: Type,
    roots: Vec<Interned<CILRoot>>,
    locals: Vec<(Option<Interned<crate::IString>>, Interned<Type>)>,
    arg_names: Vec<Option<Interned<crate::IString>>>,
) {
    let main_module = asm.main_module();
    let name = asm.alloc_string(name);
    let sig = asm.sig(inputs.to_vec(), output);
    asm.new_method(MethodDef::new(
        Access::Public,
        main_module,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals,
        },
        arg_names,
    ));
}

/// A `static_mref` to a main-module method (the call-seam for wrappers).
pub(super) fn main_static(
    asm: &mut Assembly,
    name: &str,
    inputs: &[Type],
    output: Type,
) -> Interned<MethodRef> {
    let main_module = asm.main_module();
    let fn_name = asm.alloc_string(name);
    asm.class_ref(*main_module)
        .clone()
        .static_mref(inputs, output, fn_name, asm)
}

fn define_fdtable_builtins(asm: &mut Assembly) {
    let void_ptr = asm.nptr(Type::Void);
    let i32_ty = asm.alloc_type(Type::Int(Int::I32));

    // rcl_fdtable_insert(handle: isize, kind: i32, flags: i32) -> i32
    {
        let next_sfld = fd_next_sfld(asm);
        let fd_val = asm.alloc_node(CILNode::LdStaticField(next_sfld));
        let store_fd = asm.alloc_root(CILRoot::StLoc(0, fd_val));
        let fd0 = asm.alloc_node(CILNode::LdLoc(0));
        let one = asm.alloc_node(Const::I32(1));
        let incd = asm.alloc_node(CILNode::BinOp(fd0, one, BinOp::Add));
        let store_next = asm.alloc_root(CILRoot::SetStaticField {
            field: next_sfld,
            val: incd,
        });
        let handle = asm.alloc_node(CILNode::LdArg(0));
        let kind = asm.alloc_node(CILNode::LdArg(1));
        let flags = asm.alloc_node(CILNode::LdArg(2));
        let entry = new_fd_entry(asm, handle, kind, flags);
        let dict = load_fd_table(asm);
        let fd1 = asm.alloc_node(CILNode::LdLoc(0));
        let set = dict_set_item(asm, dict, fd1, entry);
        let fd2 = asm.alloc_node(CILNode::LdLoc(0));
        let ret = asm.alloc_root(CILRoot::Ret(fd2));
        define_main_method(
            asm,
            "rcl_fdtable_insert",
            &[
                Type::Int(Int::ISize),
                Type::Int(Int::I32),
                Type::Int(Int::I32),
            ],
            Type::Int(Int::I32),
            vec![store_fd, store_next, set, ret],
            vec![(None, i32_ty)],
            vec![None, None, None],
        );
    }
    // rcl_fdtable_handle(fd) -> *mut u8
    {
        let entry = entry_of_fd(asm, 0);
        let f = fd_entry_handle_field(asm);
        let h = asm.alloc_node(CILNode::LdField {
            addr: entry,
            field: f,
        });
        let void = asm.alloc_type(Type::Void);
        let h = asm.alloc_node(CILNode::PtrCast(h, Box::new(PtrCastRes::Ptr(void))));
        let ret = asm.alloc_root(CILRoot::Ret(h));
        define_main_method(
            asm,
            "rcl_fdtable_handle",
            &[Type::Int(Int::I32)],
            void_ptr,
            vec![ret],
            vec![],
            vec![None],
        );
    }
    // rcl_fdtable_kind(fd) -> i32
    {
        let entry = entry_of_fd(asm, 0);
        let f = fd_entry_kind_field(asm);
        let k = asm.alloc_node(CILNode::LdField {
            addr: entry,
            field: f,
        });
        let ret = asm.alloc_root(CILRoot::Ret(k));
        define_main_method(
            asm,
            "rcl_fdtable_kind",
            &[Type::Int(Int::I32)],
            Type::Int(Int::I32),
            vec![ret],
            vec![],
            vec![None],
        );
    }
    // rcl_fdtable_get_flags(fd) -> i32
    {
        let entry = entry_of_fd(asm, 0);
        let f = fd_entry_flags_field(asm);
        let v = asm.alloc_node(CILNode::LdField {
            addr: entry,
            field: f,
        });
        let ret = asm.alloc_root(CILRoot::Ret(v));
        define_main_method(
            asm,
            "rcl_fdtable_get_flags",
            &[Type::Int(Int::I32)],
            Type::Int(Int::I32),
            vec![ret],
            vec![],
            vec![None],
        );
    }
    // rcl_fdtable_set_flags(fd, flags)
    {
        let entry = entry_of_fd(asm, 0);
        let f = fd_entry_flags_field(asm);
        let nf = asm.alloc_node(CILNode::LdArg(1));
        let set = asm.alloc_root(CILRoot::SetField(Box::new((f, entry, nf))));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        define_main_method(
            asm,
            "rcl_fdtable_set_flags",
            &[Type::Int(Int::I32), Type::Int(Int::I32)],
            Type::Void,
            vec![set, ret],
            vec![],
            vec![None, None],
        );
    }
    // rcl_fdtable_remove(fd)
    {
        let fd_dict = fd_dict(asm);
        let dict = load_fd_table(asm);
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let remove_name = asm.alloc_string("Remove");
        let remove = asm.class_ref(fd_dict).clone().virtual_mref(
            &[Type::PlatformGeneric(0, GenericKind::TypeGeneric)],
            Type::Bool,
            remove_name,
            asm,
        );
        let removed = asm.alloc_node(CILNode::call(remove, [dict, fd]));
        let pop = asm.alloc_root(CILRoot::Pop(removed));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        define_main_method(
            asm,
            "rcl_fdtable_remove",
            &[Type::Int(Int::I32)],
            Type::Void,
            vec![pop, ret],
            vec![],
            vec![None],
        );
    }
}

// Convenience calls to the fd-table MethodDefs from a wrapper body.
fn call_fdtable_handle(asm: &mut Assembly, fd: Interned<CILNode>) -> Interned<CILNode> {
    let void_ptr = asm.nptr(Type::Void);
    let m = main_static(asm, "rcl_fdtable_handle", &[Type::Int(Int::I32)], void_ptr);
    asm.alloc_node(CILNode::call(m, [fd]))
}
fn call_fdtable_kind(asm: &mut Assembly, fd: Interned<CILNode>) -> Interned<CILNode> {
    let m = main_static(
        asm,
        "rcl_fdtable_kind",
        &[Type::Int(Int::I32)],
        Type::Int(Int::I32),
    );
    asm.alloc_node(CILNode::call(m, [fd]))
}
fn call_fdtable_insert(
    asm: &mut Assembly,
    handle: Interned<CILNode>,
    kind: i32,
    flags: i32,
) -> Interned<CILNode> {
    let m = main_static(
        asm,
        "rcl_fdtable_insert",
        &[
            Type::Int(Int::ISize),
            Type::Int(Int::I32),
            Type::Int(Int::I32),
        ],
        Type::Int(Int::I32),
    );
    let handle_isize = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::ISize)));
    let kind = asm.alloc_node(Const::I32(kind));
    let flags = asm.alloc_node(Const::I32(flags));
    asm.alloc_node(CILNode::call(m, [handle_isize, kind, flags]))
}

fn call_fdtable_insert_with_flags(
    asm: &mut Assembly,
    handle: Interned<CILNode>,
    kind: i32,
    flags: Interned<CILNode>,
) -> Interned<CILNode> {
    let m = main_static(
        asm,
        "rcl_fdtable_insert",
        &[
            Type::Int(Int::ISize),
            Type::Int(Int::I32),
            Type::Int(Int::I32),
        ],
        Type::Int(Int::I32),
    );
    let handle_isize = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::ISize)));
    let kind = asm.alloc_node(Const::I32(kind));
    let flags = asm.int_cast(flags, Int::I32, ExtendKind::ZeroExtend);
    asm.alloc_node(CILNode::call(m, [handle_isize, kind, flags]))
}
/// Like `call_fdtable_insert` but with a DYNAMIC `kind` node (the original fd's
/// kind, for dup). `handle` is a `void*`, `kind` an i32 node.
fn call_fdtable_insert_dyn(
    asm: &mut Assembly,
    handle: Interned<CILNode>,
    kind: Interned<CILNode>,
    flags: i32,
) -> Interned<CILNode> {
    let m = main_static(
        asm,
        "rcl_fdtable_insert",
        &[
            Type::Int(Int::ISize),
            Type::Int(Int::I32),
            Type::Int(Int::I32),
        ],
        Type::Int(Int::I32),
    );
    let handle_isize = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::ISize)));
    let flags = asm.alloc_node(Const::I32(flags));
    asm.alloc_node(CILNode::call(m, [handle_isize, kind, flags]))
}
fn call_fdtable_remove(asm: &mut Assembly, fd: Interned<CILNode>) -> Interned<CILRoot> {
    let m = main_static(
        asm,
        "rcl_fdtable_remove",
        &[Type::Int(Int::I32)],
        Type::Void,
    );
    asm.alloc_root(CILRoot::call(m, [fd]))
}

// A `static_mref` to a `rcl_dotnet_*` body (resolved by the dotnet PAL patcher).
fn dotnet_mref(
    asm: &mut Assembly,
    name: &str,
    inputs: &[Type],
    output: Type,
) -> Interned<MethodRef> {
    main_static(asm, name, inputs, output)
}

/// Register patcher OVERRIDES for the fd-table builtins that an EXTERNAL crate
/// (the dotnet std net PAL, `sys/net/connection/dotnet.rs`) references as bare
/// `extern "C"` symbols. The builtins are defined as MethodDefs on main_module
/// (for the posix wrappers' same-module calls), but a foreign `extern "C"`
/// reference is a *missing method* the patcher must fill — so we also provide an
/// override body that simply forwards to the main_module MethodDef. Without this,
/// the std-side `rcl_fdtable_handle`/`rcl_fdtable_insert` calls JIT to
/// "missing method". (`rcl_fdtable_kind`/`get_flags`/`set_flags`/`remove` are
/// only ever called internally by the posix wrappers, so they need no override —
/// but the two the net Socket onion calls do.)
fn insert_fdtable_externs(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    // rcl_fdtable_handle(fd: i32) -> *mut u8  — forward to the MethodDef.
    let name = asm.alloc_string("rcl_fdtable_handle");
    let gen_handle = move |_, asm: &mut Assembly| {
        let void_ptr = asm.nptr(Type::Void);
        let m = main_static(asm, "rcl_fdtable_handle", &[Type::Int(Int::I32)], void_ptr);
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let r = asm.alloc_node(CILNode::call(m, [fd]));
        let ret = asm.alloc_root(CILRoot::Ret(r));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(gen_handle));

    // rcl_fdtable_insert(handle: isize, kind: i32, flags: i32) -> i32 — forward.
    let name = asm.alloc_string("rcl_fdtable_insert");
    let gen_insert = move |_, asm: &mut Assembly| {
        let m = main_static(
            asm,
            "rcl_fdtable_insert",
            &[
                Type::Int(Int::ISize),
                Type::Int(Int::I32),
                Type::Int(Int::I32),
            ],
            Type::Int(Int::I32),
        );
        let h = asm.alloc_node(CILNode::LdArg(0));
        let kind = asm.alloc_node(CILNode::LdArg(1));
        let flags = asm.alloc_node(CILNode::LdArg(2));
        let r = asm.alloc_node(CILNode::call(m, [h, kind, flags]));
        let ret = asm.alloc_root(CILRoot::Ret(r));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(gen_insert));
}

// ===========================================================================
// errno
// ===========================================================================

/// `__errno_location()` / `errno_location()` -> `*mut i32` = `&rcl_errno`.
///
/// Linux-targeted libc uses the underscored link name; crates whose cfg tables do not yet name
/// `target_os=dotnet` retain the Rust source identifier. Both are ABI aliases of the same TLS cell.
fn insert_errno_location(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    for symbol in ["__errno_location", "errno_location"] {
        let name = asm.alloc_string(symbol);
        let generator = move |_, asm: &mut Assembly| {
            let sfld = errno_sfld(asm);
            let addr = asm.alloc_node(CILNode::LdStaticFieldAddress(sfld));
            let i32_ty = asm.alloc_type(Type::Int(Int::I32));
            let addr = asm.alloc_node(CILNode::PtrCast(addr, Box::new(PtrCastRes::Ptr(i32_ty))));
            let ret = asm.alloc_root(CILRoot::Ret(addr));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    }
}

// Symmetric constant-valued counterpart to `set_errno_node` (sets `rcl_errno` to
// a fixed errno). Currently unused — `open_errno_wrapped` was switched from a
// blind `ENOENT` to the rich `rcl_errno_from_exception` mapper — but kept as the
// obvious helper for any future fixed-errno wrapper.
fn set_errno(asm: &mut Assembly, val: i32) -> Interned<CILRoot> {
    let sfld = errno_sfld(asm);
    let v = asm.alloc_node(Const::I32(val));
    asm.alloc_root(CILRoot::SetStaticField {
        field: sfld,
        val: v,
    })
}

/// Like `set_errno`, but stores an already-built i32 NODE into the thread-local
/// `rcl_errno` cell (e.g. the result of `rcl_errno_from_exception`). Used by the
/// pointer-returning `rcl_dotnet_fs_open` hook, which catches faults itself
/// (it must return null, not the `errno_wrapped` `-1`).
pub(super) fn set_errno_node(asm: &mut Assembly, val: Interned<CILNode>) -> Interned<CILRoot> {
    let sfld = errno_sfld(asm);
    asm.alloc_root(CILRoot::SetStaticField { field: sfld, val })
}

/// `rcl_errno_from_exception(System.Object exn) -> i32` — map a caught BCL
/// exception to a POSIX errno. This is a normal (non-handler) MethodDef so its
/// branch chain lives outside any `try`/`catch` region (keeping the per-wrapper
/// catch handler to a single block — a multi-block handler with an internal
/// switch tripped the IL exporter's label resolution). NON-discarding:
///   * `exn isinst SocketException` -> map exn.SocketErrorCode (curated switch);
///   * `exn isinst FileNotFoundException` / `DirectoryNotFoundException` -> ENOENT;
///   * `exn isinst UnauthorizedAccessException` -> EACCES;
///   * `exn isinst PathTooLongException` -> ENAMETOOLONG;
///   * else (general IOException tail + anything unknown) -> EIO.
///
/// ORDER MATTERS: `FileNotFoundException`/`DirectoryNotFoundException`/
/// `PathTooLongException` all derive from `IOException`, so they MUST be tested
/// before any general `IOException` arm (we keep none — the IOException tail just
/// falls through to the EIO default, which is the honest leak per
/// LIBC_SHIM_SCOPE §3.2). `UnauthorizedAccessException` derives from
/// `SystemException`, not `IOException`, so its position relative to the IO
/// subclasses is free; it precedes the EIO fallthrough. The whole fs map is
/// HOST-AGNOSTIC (the BCL throws the same exception types on Unix-host and
/// Windows-host CoreCLR); the *meaning* of EACCES is Unix-host-best-effort (a
/// Windows host has no rwx model and throws UnauthorizedAccess for ACL denials
/// too) — see `ClassRef::unauthorized_access_exception`.
fn define_errno_from_exception(asm: &mut Assembly) {
    let main_module = asm.main_module();
    let name = asm.alloc_string("rcl_errno_from_exception");
    let socket_exception = ClassRef::socket_exception(asm);
    let se_ty = asm.alloc_type(Type::ClassRef(socket_exception));
    let fnf = ClassRef::file_not_found_exception(asm);
    let fnf_ty = asm.alloc_type(Type::ClassRef(fnf));
    let dnf = ClassRef::directory_not_found_exception(asm);
    let dnf_ty = asm.alloc_type(Type::ClassRef(dnf));
    let uae = ClassRef::unauthorized_access_exception(asm);
    let uae_ty = asm.alloc_type(Type::ClassRef(uae));
    let ptl = ClassRef::path_too_long_exception(asm);
    let ptl_ty = asm.alloc_type(Type::ClassRef(ptl));

    // block 0: a chain of isinst tests. SocketException -> block 2 (socket
    // switch); the fs exceptions -> their small return blocks (20..23); the EIO
    // default -> block 1. Each arm `if isinst<T> goto <tgt>`; a final
    // unconditional `goto 1` is the EIO fallthrough.
    let exn = asm.alloc_node(CILNode::LdArg(0));
    let is_se = asm.alloc_node(CILNode::IsInst(exn, se_ty));
    let goto_se = asm.alloc_root(CILRoot::Branch(Box::new((
        2,
        0,
        Some(BranchCond::True(is_se)),
    ))));
    let exn_fnf = asm.alloc_node(CILNode::LdArg(0));
    let is_fnf = asm.alloc_node(CILNode::IsInst(exn_fnf, fnf_ty));
    let goto_fnf = asm.alloc_root(CILRoot::Branch(Box::new((
        20,
        0,
        Some(BranchCond::True(is_fnf)),
    ))));
    let exn_dnf = asm.alloc_node(CILNode::LdArg(0));
    let is_dnf = asm.alloc_node(CILNode::IsInst(exn_dnf, dnf_ty));
    let goto_dnf = asm.alloc_root(CILRoot::Branch(Box::new((
        20,
        0,
        Some(BranchCond::True(is_dnf)),
    ))));
    let exn_uae = asm.alloc_node(CILNode::LdArg(0));
    let is_uae = asm.alloc_node(CILNode::IsInst(exn_uae, uae_ty));
    let goto_uae = asm.alloc_root(CILRoot::Branch(Box::new((
        21,
        0,
        Some(BranchCond::True(is_uae)),
    ))));
    let exn_ptl = asm.alloc_node(CILNode::LdArg(0));
    let is_ptl = asm.alloc_node(CILNode::IsInst(exn_ptl, ptl_ty));
    let goto_ptl = asm.alloc_root(CILRoot::Branch(Box::new((
        22,
        0,
        Some(BranchCond::True(is_ptl)),
    ))));
    let goto_eio = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));
    // block 1: ret EIO.
    let eio = asm.alloc_node(Const::I32(EIO));
    let ret_eio = asm.alloc_root(CILRoot::Ret(eio));
    // blocks 20..22: ret the fs errno.
    let enoent = asm.alloc_node(Const::I32(ENOENT));
    let ret_enoent = asm.alloc_root(CILRoot::Ret(enoent));
    let eacces = asm.alloc_node(Const::I32(EACCES));
    let ret_eacces = asm.alloc_root(CILRoot::Ret(eacces));
    let enametoolong = asm.alloc_node(Const::I32(ENAMETOOLONG));
    let ret_enametoolong = asm.alloc_root(CILRoot::Ret(enametoolong));
    // block 2: code = ((SocketException)exn).SocketErrorCode; switch.
    let exn2 = asm.alloc_node(CILNode::LdArg(0));
    let se_cast = asm.alloc_node(CILNode::CheckedCast(exn2, se_ty));
    let get_code_name = asm.alloc_string("get_SocketErrorCode");
    // `SocketException.SocketErrorCode` returns the `SocketError` ENUM, not i32 —
    // the CLR matches the signature exactly, so declaring i32 yields a runtime
    // MissingMethodException. Declare the enum return, then explicitly reinterpret
    // the int-backed value as i32 at the managed/ABI boundary before branching.
    let socket_error = ClassRef::socket_error(asm);
    let socket_error_ty = Type::ClassRef(socket_error);
    let get_code =
        asm.class_ref(socket_exception)
            .clone()
            .instance(&[], socket_error_ty, get_code_name, asm);
    let code = asm.alloc_node(CILNode::call(get_code, [se_cast]));
    let code = bcl_enum_to_i32(code, socket_error_ty, asm);
    let store_code = asm.alloc_root(CILRoot::StLoc(0, code));
    // test chain: blocks 10..14 return mapped errno; default (15) returns EIO.
    let test = |asm: &mut Assembly, se: i32, tgt: u32| {
        let c = asm.alloc_node(CILNode::LdLoc(0));
        let sc = asm.alloc_node(Const::I32(se));
        asm.alloc_root(CILRoot::Branch(Box::new((
            tgt,
            0,
            Some(BranchCond::Eq(c, sc)),
        ))))
    };
    let t_wb = test(asm, SE_WOULD_BLOCK, 10);
    let t_to = test(asm, SE_TIMED_OUT, 11);
    let t_cref = test(asm, SE_CONN_REFUSED, 12);
    let t_crst = test(asm, SE_CONN_RESET, 13);
    let t_aiu = test(asm, SE_ADDR_IN_USE, 14);
    let goto_default = asm.alloc_root(CILRoot::Branch(Box::new((15, 0, None))));
    let mk_ret = |asm: &mut Assembly, errno: i32| {
        let v = asm.alloc_node(Const::I32(errno));
        asm.alloc_root(CILRoot::Ret(v))
    };
    let r_eagain = mk_ret(asm, EAGAIN);
    let r_etimedout = mk_ret(asm, ETIMEDOUT);
    let r_econnrefused = mk_ret(asm, ECONNREFUSED);
    let r_econnreset = mk_ret(asm, ECONNRESET);
    let r_eaddrinuse = mk_ret(asm, EADDRINUSE);
    let r_default = mk_ret(asm, EIO);

    let se_local_ty = asm.alloc_type(Type::Int(Int::I32));
    let sig = asm.sig([Type::PlatformObject], Type::Int(Int::I32));
    asm.new_method(MethodDef::new(
        Access::Public,
        main_module,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(
                    vec![goto_se, goto_fnf, goto_dnf, goto_uae, goto_ptl, goto_eio],
                    0,
                    None,
                ),
                BasicBlock::new(vec![ret_eio], 1, None),
                BasicBlock::new(
                    vec![store_code, t_wb, t_to, t_cref, t_crst, t_aiu, goto_default],
                    2,
                    None,
                ),
                BasicBlock::new(vec![r_eagain], 10, None),
                BasicBlock::new(vec![r_etimedout], 11, None),
                BasicBlock::new(vec![r_econnrefused], 12, None),
                BasicBlock::new(vec![r_econnreset], 13, None),
                BasicBlock::new(vec![r_eaddrinuse], 14, None),
                BasicBlock::new(vec![r_default], 15, None),
                // fs exception arms (ENOENT/EACCES/ENAMETOOLONG).
                BasicBlock::new(vec![ret_enoent], 20, None),
                BasicBlock::new(vec![ret_eacces], 21, None),
                BasicBlock::new(vec![ret_enametoolong], 22, None),
            ],
            locals: vec![(None, se_local_ty)],
        },
        vec![None],
    ));
}

/// `rcl_connect_errno_from_exception(System.Object exn) -> i32` — the connect()
/// errno map: a non-blocking `Socket.Connect` to a not-yet-accepted endpoint
/// throws `SocketException(WouldBlock)`, which POSIX `connect` reports as
/// **EINPROGRESS** (NOT EAGAIN). mio's `connect` (tcp.rs) treats ONLY EINPROGRESS
/// as success; the default map (WouldBlock→EAGAIN) would make mio fail the
/// connect. Everything else delegates to the general `rcl_errno_from_exception`.
fn define_connect_errno_from_exception(asm: &mut Assembly) {
    let main_module = asm.main_module();
    let name = asm.alloc_string("rcl_connect_errno_from_exception");
    let socket_exception = ClassRef::socket_exception(asm);
    let se_ty = asm.alloc_type(Type::ClassRef(socket_exception));

    // block 0: if !(exn isinst SocketException) goto 1 (delegate); else goto 2.
    let exn = asm.alloc_node(CILNode::LdArg(0));
    let is_se = asm.alloc_node(CILNode::IsInst(exn, se_ty));
    let goto_delegate = asm.alloc_root(CILRoot::Branch(Box::new((
        1,
        0,
        Some(BranchCond::False(is_se)),
    ))));
    let goto_se = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));
    // block 1: ret rcl_errno_from_exception(exn).
    let delegate = main_static(
        asm,
        "rcl_errno_from_exception",
        &[Type::PlatformObject],
        Type::Int(Int::I32),
    );
    let exn_d = asm.alloc_node(CILNode::LdArg(0));
    let mapped = asm.alloc_node(CILNode::call(delegate, [exn_d]));
    let ret_delegate = asm.alloc_root(CILRoot::Ret(mapped));
    // block 2: code = ((SocketException)exn).SocketErrorCode;
    //   if code == WouldBlock -> ret EINPROGRESS else delegate (goto 1).
    let exn2 = asm.alloc_node(CILNode::LdArg(0));
    let se_cast = asm.alloc_node(CILNode::CheckedCast(exn2, se_ty));
    let get_code_name = asm.alloc_string("get_SocketErrorCode");
    // `SocketErrorCode` returns the `SocketError` enum (see the general mapper);
    // declaring i32 would MissingMethodException at runtime. Explicitly reinterpret
    // the int-backed enum as i32 before comparing it to the POSIX mapping constants.
    let socket_error = ClassRef::socket_error(asm);
    let socket_error_ty = Type::ClassRef(socket_error);
    let get_code =
        asm.class_ref(socket_exception)
            .clone()
            .instance(&[], socket_error_ty, get_code_name, asm);
    let code = asm.alloc_node(CILNode::call(get_code, [se_cast]));
    let code = bcl_enum_to_i32(code, socket_error_ty, asm);
    let store_code = asm.alloc_root(CILRoot::StLoc(0, code));
    let code_l = asm.alloc_node(CILNode::LdLoc(0));
    let wb = asm.alloc_node(Const::I32(SE_WOULD_BLOCK));
    let br_wb = asm.alloc_root(CILRoot::Branch(Box::new((
        3,
        0,
        Some(BranchCond::Eq(code_l, wb)),
    ))));
    let goto_delegate2 = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));
    // block 3: ret EINPROGRESS.
    let einprog = asm.alloc_node(Const::I32(EINPROGRESS));
    let ret_einprog = asm.alloc_root(CILRoot::Ret(einprog));

    let se_local_ty = asm.alloc_type(Type::Int(Int::I32));
    let sig = asm.sig([Type::PlatformObject], Type::Int(Int::I32));
    asm.new_method(MethodDef::new(
        Access::Public,
        main_module,
        name,
        sig,
        MethodKind::Static,
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![goto_delegate, goto_se], 0, None),
                BasicBlock::new(vec![ret_delegate], 1, None),
                BasicBlock::new(vec![store_code, br_wb, goto_delegate2], 2, None),
                BasicBlock::new(vec![ret_einprog], 3, None),
            ],
            locals: vec![(None, se_local_ty)],
        },
        vec![None],
    ));
}

#[cfg(test)]
mod errno_mapper_tests {
    use super::*;

    #[test]
    fn socket_error_enum_is_retyped_before_errno_comparisons() {
        let mut asm = Assembly::default();
        define_errno_from_exception(&mut asm);
        define_connect_errno_from_exception(&mut asm);

        assert_eq!(asm.typecheck(), 0);
    }
}

// ===========================================================================
// the try/catch errno wrapper
//
// A leaky wrapper's `body` is a straight-line root vector that ends by storing
// the OK result in local 0 (`StLoc(0, result)`). `errno_wrapped` appends:
//   * `leave -> ok(2)` to the try,
//   * a SINGLE-BLOCK catch handler: `errno = rcl_errno_from_exception(GetException);
//     local0 = -1; leave -> 2` (the switch lives in the helper MethodDef, so the
//     handler stays one block — a multi-block handler with an internal switch
//     tripped the IL exporter's label resolution),
//   * block 2: `ret local0`.
// The body uses ONLY block 0, so the handler's fixed block ids (1, 2) don't
// collide. Modeled on insert_catch_unwind (mod.rs:613).
// ===========================================================================
pub(super) fn errno_wrapped(
    asm: &mut Assembly,
    body: Vec<Interned<CILRoot>>,
    ret_ty: Type,
    extra_locals: Vec<(Option<Interned<crate::IString>>, Interned<Type>)>,
) -> MethodImpl {
    errno_wrapped_with(asm, body, ret_ty, extra_locals, "rcl_errno_from_exception")
}

/// `errno_wrapped`, but the catch handler maps the exception via `mapper` (a
/// main-module `(object) -> i32` MethodDef). `connect` uses the
/// EINPROGRESS-aware `rcl_connect_errno_from_exception`.
fn errno_wrapped_with(
    asm: &mut Assembly,
    mut body: Vec<Interned<CILRoot>>,
    ret_ty: Type,
    extra_locals: Vec<(Option<Interned<crate::IString>>, Interned<Type>)>,
    mapper: &str,
) -> MethodImpl {
    let leave_ok = asm.alloc_root(CILRoot::ExitSpecialRegion {
        target: 2,
        source: 0,
    });
    body.push(leave_ok);

    // Block 1 (catch): errno = <mapper>(GetException); local0=-1; leave->2.
    let get_exn = asm.alloc_node(CILNode::GetException);
    let map = main_static(asm, mapper, &[Type::PlatformObject], Type::Int(Int::I32));
    let mapped = asm.alloc_node(CILNode::call(map, [get_exn]));
    let errno_field = errno_sfld(asm);
    let set_errno = asm.alloc_root(CILRoot::SetStaticField {
        field: errno_field,
        val: mapped,
    });
    let minus1 = asm.alloc_node(Const::I32(-1));
    let minus1 = asm.int_cast(minus1, ret_ty_int(ret_ty), ExtendKind::SignExtend);
    let store_minus1 = asm.alloc_root(CILRoot::StLoc(0, minus1));
    let leave_catch = asm.alloc_root(CILRoot::ExitSpecialRegion {
        target: 2,
        source: 1,
    });

    // Block 2: ret local0.
    let ld0 = asm.alloc_node(CILNode::LdLoc(0));
    let ret = asm.alloc_root(CILRoot::Ret(ld0));

    let ret_ty_idx = asm.alloc_type(ret_ty);
    let mut locals = vec![(Some(asm.alloc_string("result")), ret_ty_idx)];
    locals.extend(extra_locals);

    MethodImpl::MethodBody {
        blocks: vec![
            BasicBlock::new(
                body,
                0,
                Some(vec![BasicBlock::new(
                    vec![set_errno, store_minus1, leave_catch],
                    1,
                    None,
                )]),
            ),
            BasicBlock::new(vec![ret], 2, None),
        ],
        locals,
    }
}

/// The integer width to coerce `-1` to for a given return type.
fn ret_ty_int(ret_ty: Type) -> Int {
    match ret_ty {
        Type::Int(i) => i,
        Type::Ptr(_) => Int::ISize,
        _ => Int::ISize,
    }
}

include!("posix_symbols.rs");
