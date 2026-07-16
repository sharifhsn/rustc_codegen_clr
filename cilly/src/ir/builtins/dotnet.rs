//! BCL bindings for the .NET ("dotnet") PAL.
//!
//! The std-side PAL (under `dotnet_pal/`) declares a small set of `extern "C"`
//! symbols and routes its allocator / stdio through them. This module
//! implements those symbols as [`MissingMethodPatcher`] builtins that emit CIL
//! calling the .NET Base Class Library. The names below **must match exactly**
//! the symbols the PAL declares — see `dotnet_pal/sys/alloc/dotnet.rs` and
//! `dotnet_pal/sys/stdio/dotnet.rs`.
//!
//! FIXED extern contract:
//! * `rcl_dotnet_alloc(size, align) -> *mut u8`
//!   => `System.Runtime.InteropServices.NativeMemory.AlignedAlloc((nuint)size, (nuint)align)`
//! * `rcl_dotnet_free(ptr, size, align)`
//!   => `System.Runtime.InteropServices.NativeMemory.AlignedFree((void*)ptr)`
//! * `rcl_dotnet_write(fd, ptr, len) -> isize`
//!   => writes `len` UTF-8 bytes from `ptr` to `System.Console`'s stdout (fd 1)
//!      or stderr (fd 2); returns bytes written (-1 on error — never taken here,
//!      a managed exception unwinds instead).
//! * `rcl_dotnet_random_fill(ptr, len)`
//!   => fills `len` bytes at `ptr` with cryptographically-secure random data via
//!      `System.Security.Cryptography.RandomNumberGenerator.Fill(Span<byte>)`.
//! * `rcl_dotnet_thread_spawn(entry, arg) -> *mut u8`
//!   => spawns a `System.Threading.Thread` running `entry(arg)` (reusing the
//!      shared `UnmanagedThreadStart` machinery) and returns an opaque
//!      `GCHandle` join handle.
//! * `rcl_dotnet_thread_join(handle)` => `Thread.Join()` on the handle's thread.
//! * `rcl_dotnet_thread_yield()`       => `System.Threading.Thread.Yield()`.
//! * `rcl_dotnet_thread_sleep(millis)` => `System.Threading.Thread.Sleep((int)millis)`.
//! * `rcl_dotnet_tls_create() -> *mut u8`
//!   => `new ThreadLocal<nint>()`, `GCHandle`-pinned; the returned `IntPtr` is an
//!      opaque per-thread TLS "key" (its `.Value` is per-thread by construction).
//! * `rcl_dotnet_tls_get(key) -> *mut u8` => `((ThreadLocal<nint>)key).Value`.
//! * `rcl_dotnet_tls_set(key, val)`       => `((ThreadLocal<nint>)key).Value = (nint)val`.
//! * `rcl_dotnet_available_parallelism() -> usize`
//!   => `System.Environment.ProcessorCount`.
//! * `rcl_dotnet_args_count() -> usize`
//!   => `System.Environment.GetCommandLineArgs().Length`.
//! * `rcl_dotnet_arg(idx) -> *mut u8`
//!   => `Marshal.StringToCoTaskMemUTF8(Environment.GetCommandLineArgs()[idx])`
//!      (a NUL-terminated UTF-8 buffer the caller frees).
//! * `rcl_dotnet_getenv(key_ptr, key_len) -> *mut u8`
//!   => `Environment.GetEnvironmentVariable(<key>)` marshalled to a
//!      NUL-terminated UTF-8 buffer, or null if unset.
//! * `rcl_dotnet_setenv(key_ptr, key_len, val_ptr, val_len)`
//!   => `Environment.SetEnvironmentVariable(<key>, <val>)`.
//! * `rcl_dotnet_unsetenv(key_ptr, key_len)`
//!   => `Environment.SetEnvironmentVariable(<key>, null)`.
//! * `rcl_dotnet_cotaskmem_free(ptr)` => `Marshal.FreeCoTaskMem((IntPtr)ptr)`
//!   (frees buffers returned by `rcl_dotnet_arg` / `rcl_dotnet_getenv`).
//!
//! Filesystem (`sys/fs/dotnet.rs`), backed by `System.IO`. Paths arrive as
//! `(ptr, len)` UTF-8 buffers; open files / dir snapshots are opaque `GCHandle`
//! `IntPtr`s (never managed objects through the ABI):
//! * `rcl_dotnet_fs_open(path_ptr, path_len, mode, access, append) -> *mut u8`
//!   => `new FileStream(path, (FileMode)mode, (FileAccess)access)`, `GCHandle`-pinned.
//! * `rcl_dotnet_fs_read(handle, buf_ptr, len) -> isize`
//!   => `FileStream.Read(new Span<byte>(buf_ptr, (int)len))`.
//! * `rcl_dotnet_fs_write(handle, buf_ptr, len) -> isize`
//!   => `FileStream.Write(new ReadOnlySpan<byte>(buf_ptr, (int)len))`; returns `len`.
//! * `rcl_dotnet_fs_seek(handle, offset, origin) -> i64`
//!   => `FileStream.Seek(offset, (SeekOrigin)origin)`.
//! * `rcl_dotnet_fs_flush(handle)`  => `FileStream.Flush()`.
//! * `rcl_dotnet_fs_close(handle)`  => `FileStream.Dispose()` + free the `GCHandle`.
//! * `rcl_dotnet_fs_len(handle) -> i64` => `FileStream.get_Length`.
//! * `rcl_dotnet_fs_stat(path_ptr, path_len, out_size, out_is_dir) -> i32`
//!   => `Directory.Exists` ? (size 0, dir) : `File.Exists` ? (FileInfo.Length, file)
//!      : `-1` (NotFound). Fills `out_size`/`out_is_dir` via `StInd`.
//! * `rcl_dotnet_fs_exists(path_ptr, path_len) -> i32`
//!   => `(File.Exists || Directory.Exists) ? 1 : 0` (never errno-based).
//! * `rcl_dotnet_fs_mkdir(path_ptr, path_len) -> i32`  => `Directory.CreateDirectory`.
//! * `rcl_dotnet_fs_rmdir(path_ptr, path_len) -> i32`  => `Directory.Delete(path, false)`.
//! * `rcl_dotnet_fs_unlink(path_ptr, path_len) -> i32` => `File.Delete(path)`.
//! * `rcl_dotnet_fs_rename(old_ptr, old_len, new_ptr, new_len) -> i32`
//!   => `File.Move(old, new, true)`.
//! * `rcl_dotnet_fs_readdir_open(path_ptr, path_len) -> *mut u8`
//!   => `Directory.GetFileSystemEntries(path)` (a `string[]`), `GCHandle`-pinned.
//! * `rcl_dotnet_fs_readdir_count(handle) -> usize`     => `array.Length`.
//! * `rcl_dotnet_fs_readdir_get(handle, idx) -> *mut u8`
//!   => `Marshal.StringToCoTaskMemUTF8(array[idx])` (caller frees with
//!      `rcl_dotnet_cotaskmem_free`).
//! * `rcl_dotnet_fs_readdir_close(handle)`              => free the `GCHandle`.
//!
//! Networking (`sys/net/connection/dotnet.rs`), backed by `System.Net.Sockets`.
//! An open socket is an opaque `GCHandle` `IntPtr` pinning a managed
//! `System.Net.Sockets.Socket`; a `SocketAddr` crosses the ABI decomposed into
//! `(family: i32 [4=v4/6=v6], ip_ptr, ip_len, port: u16)` (network-order octets),
//! and addresses come back through caller `out_*` pointers. No managed `Socket` /
//! `IPEndPoint` / `IPAddress` is ever passed through a Rust signature:
//! * `rcl_dotnet_net_tcp_connect(family, ip_ptr, ip_len, port) -> *mut u8`
//!   => `var s = new Socket(ep.AddressFamily, Stream, Tcp); s.Connect(ep);` handle.
//! * `rcl_dotnet_net_bind(family, ip_ptr, ip_len, port, sock_type, backlog) -> *mut u8`
//!   => `var s = new Socket(ep.AddressFamily, (SocketType)t, (ProtocolType)p);
//!      s.Bind(ep); if (backlog >= 0) s.Listen(backlog);` handle.
//! * `rcl_dotnet_net_accept(handle, out_family, out_ip, out_port) -> *mut u8`
//!   => `var c = s.Accept(); write(c.RemoteEndPoint, out_*);` new handle.
//! * `rcl_dotnet_net_recv(handle, buf_ptr, len) -> isize`
//!   => `s.Receive(new Span<byte>(buf_ptr, len))` (0 == orderly shutdown / EOF).
//! * `rcl_dotnet_net_send(handle, buf_ptr, len) -> isize`
//!   => `s.Send(new ReadOnlySpan<byte>(buf_ptr, len))` (count sent).
//! * `rcl_dotnet_net_recv_from(handle, buf_ptr, len, out_family, out_ip, out_port) -> isize`
//!   => `EndPoint ep = new IPEndPoint(IPAddress.IPv6Any, 0);
//!      int n = s.ReceiveFrom(new Span<byte>(buf_ptr, len), ref ep);
//!      write((IPEndPoint)ep, out_*);` (the `ref EndPoint` overload, so an
//!      unconnected UDP socket reports the real sender).
//! * `rcl_dotnet_net_send_to(handle, buf_ptr, len, family, ip_ptr, ip_len, port) -> isize`
//!   => `s.SendTo(new ReadOnlySpan<byte>(buf_ptr, len), ep)` (count sent).
//! * `rcl_dotnet_net_local_addr(handle, out_family, out_ip, out_port) -> i32`
//!   => `write(s.LocalEndPoint, out_*); return 0;`
//! * `rcl_dotnet_net_peer_addr(handle, out_family, out_ip, out_port) -> i32`
//!   => `write(s.RemoteEndPoint, out_*); return 0;`
//! * `rcl_dotnet_net_udp_connect(handle, family, ip_ptr, ip_len, port) -> i32`
//!   => `s.Connect(ep); return 0;`
//! * `rcl_dotnet_net_shutdown(handle, how) -> i32` => `s.Shutdown((SocketShutdown)how);`
//! * `rcl_dotnet_net_set_nonblocking(handle, nb) -> i32` => `s.Blocking = (nb == 0);`
//! * `rcl_dotnet_net_set_nodelay(handle, on) -> i32` => `s.NoDelay = (on != 0);`
//! * `rcl_dotnet_net_nodelay(handle) -> i32` => `return s.NoDelay ? 1 : 0;`
//! * `rcl_dotnet_net_close(handle)` => `s.Dispose()` + free the `GCHandle`.
//! * `rcl_dotnet_socket_poll(handle, micros, mode) -> i32`
//!   => `return s.Poll((int)micros, (SelectMode)mode) ? 1 : 0;` — the readiness
//!   primitive behind the dotnet `mio` PAL arm's Selector (mode 0=read/1=write/
//!   2=error; negative micros = block forever).
//!
//! Address marshalling is inline CIL (no managed helper): inbound builds
//! `new IPAddress(ReadOnlySpan<byte>(ip_ptr, ip_len))` + `new IPEndPoint(addr,
//! port)` (the span length picks v4/v6 — no byte-swap; the port is host-order on
//! `IPEndPoint`). Outbound reads the endpoint's `Address.GetAddressBytes()` into
//! the `out_ip` buffer with `Marshal.Copy(byte[], 0, IntPtr, len)`, writes the
//! byte-array length to `out_family` (4 for v4 / 16 for v6 — NOTE: the std side
//! treats `>= 16` as v6, `== 4` as v4), and the port to `out_port` via `StInd`.
//! See also `dotnet_pal/sys/args/dotnet.rs`, `dotnet_pal/sys/env/dotnet.rs`,
//! `dotnet_pal/sys/fs/dotnet.rs`, and `dotnet_pal/sys/net/connection/dotnet.rs`.
//!
//! `realloc` is handled std-side via `realloc_fallback` (alloc+copy+free) and
//! `alloc_zeroed` via `rcl_dotnet_alloc` + zeroing, so those do not need their
//! own binding.

use super::UNMANAGED_THREAD_START;
use crate::Assembly;
use crate::cilnode::{ExtendKind, MethodKind, PtrCastRes};
use crate::ir::asm::MissingMethodPatcher;
use crate::ir::cilroot::BranchCond;
use crate::ir::tpe::GenericKind;
use crate::ir::{
    BasicBlock, BinOp, CILNode, CILRoot, ClassRef, Const, Int, Interned, MethodImpl, MethodRef,
    StaticFieldDesc, Type,
};
use std::num::NonZeroU8;

/// Declarative registration of a `MissingMethodPatcher` builtin.
///
/// Every `rcl_dotnet_*` hook shares the same envelope: intern the symbol name,
/// box a `move |_, asm| -> MethodImpl` generator closure, and `patcher.insert`
/// it. The hook bodies vary wildly (single static calls, multi-block branches,
/// try/catch), but the wrapper is mechanical and was hand-repeated ~99 times.
/// This macro centralises *just that wrapper* so the bodies stay 1:1 with their
/// hand-written CIL.
///
/// **The symbol string is a contract** — the backend/linker matches missing
/// methods against these exact `rcl_dotnet_*` names — so it is always spelled
/// literally at the invocation, never computed.
///
/// Forms:
///
/// - **Generic body** — wraps an arbitrary generator body (a block that ends in
///   a `MethodImpl`). `$asm` binds the `&mut Assembly` inside the body, exactly
///   like the original `move |_, asm: &mut Assembly| { … }` closures:
///   ```ignore
///   dotnet_hook!(asm, patcher, "rcl_dotnet_foo", |asm| { /* … */ });
///   ```
///
/// - **`static_getter`** — the simplest archetype: a no-argument static BCL
///   getter `CLASS::METHOD() -> RET` whose result is returned verbatim (single
///   block, no locals, no cast). `CLASS` is a `ClassRef` constructor path
///   (e.g. `ClassRef::stopwatch`).
///   ```ignore
///   dotnet_hook!(asm, patcher, "rcl_dotnet_instant_ticks",
///       static_getter ClassRef::stopwatch, "GetTimestamp" -> Type::Int(Int::I64));
///   ```
macro_rules! dotnet_hook {
    // ---- generic body with access to the original method reference ----
    ($asm:expr, $patcher:expr, $sym:literal, |$body_mref:ident, $body_asm:ident| $body:block) => {{
        let name = $asm.alloc_string($sym);
        let generator = move |$body_mref, $body_asm: &mut $crate::Assembly| $body;
        $patcher.insert(name, Box::new(generator));
    }};
    // ---- generic body: an arbitrary generator closure body ----
    ($asm:expr, $patcher:expr, $sym:literal, |$body_asm:ident| $body:block) => {{
        let name = $asm.alloc_string($sym);
        let generator = move |_, $body_asm: &mut $crate::Assembly| $body;
        $patcher.insert(name, Box::new(generator));
    }};

    // ---- archetype: a no-arg static BCL getter, result returned verbatim ----
    ($asm:expr, $patcher:expr, $sym:literal,
        static_getter $class:path, $method:literal -> $ret:expr) => {
        dotnet_hook!($asm, $patcher, $sym, |asm| {
            let class = $class(asm);
            let method_name = asm.alloc_string($method);
            let method = asm
                .class_ref(class)
                .clone()
                .static_mref(&[], $ret, method_name, asm);
            let value = asm.alloc_node(CILNode::call(method, []));
            let ret = asm.alloc_root(CILRoot::Ret(value));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        });
    };
}

/// Retag an ABI `i32` as its exact int-backed BCL enum value type. The bits are unchanged, but
/// ECMA-335 method signatures distinguish `int32` from `SeekOrigin`/`SocketType`/etc.; making the
/// reinterpret explicit keeps both cilly's verifier and CoreCLR's method resolution honest.
fn i32_to_bcl_enum(
    value: Interned<CILNode>,
    enum_type: Type,
    asm: &mut Assembly,
) -> Interned<CILNode> {
    debug_assert!(matches!(enum_type, Type::ClassRef(_)));
    asm.transmute_on_stack(Type::Int(Int::I32), enum_type, value)
}

pub(super) fn bcl_enum_to_i32(
    value: Interned<CILNode>,
    enum_type: Type,
    asm: &mut Assembly,
) -> Interned<CILNode> {
    debug_assert!(matches!(enum_type, Type::ClassRef(_)));
    asm.transmute_on_stack(enum_type, Type::Int(Int::I32), value)
}

#[cfg(test)]
mod enum_boundary_tests {
    use super::*;

    #[test]
    fn int_backed_bcl_enum_boundary_is_explicitly_retyped_both_ways() {
        let mut asm = Assembly::default();
        let seek_origin = Type::ClassRef(ClassRef::seek_origin(&mut asm));
        let value = asm.alloc_node(Const::I32(2));
        let adapted = i32_to_bcl_enum(value, seek_origin, &mut asm);
        let CILNode::Call(call) = &asm[adapted] else {
            panic!("BCL enum boundary did not use a typed reinterpret");
        };
        let sig = &asm[asm[call.0].sig()];
        assert_eq!(sig.inputs(), &[Type::Int(Int::I32)]);
        assert_eq!(*sig.output(), seek_origin);

        let round_trip = bcl_enum_to_i32(adapted, seek_origin, &mut asm);
        let CILNode::Call(call) = &asm[round_trip] else {
            panic!("BCL enum return did not use a typed reinterpret");
        };
        let sig = &asm[asm[call.0].sig()];
        assert_eq!(sig.inputs(), &[seek_origin]);
        assert_eq!(*sig.output(), Type::Int(Int::I32));
    }

    #[test]
    fn fs_stat_symlink_flag_is_a_verifier_clean_i32() {
        let mut asm = Assembly::default();
        let path_ty = asm.alloc_type(Type::PlatformString);
        let locals = vec![(None, path_ty)];
        let sig = asm.sig([], Type::Void);
        let flag = dotnet_path_is_symlink_i32(&mut asm, 0);

        assert_eq!(
            asm[flag].clone().typecheck(sig, &locals, &mut asm).unwrap(),
            Type::Int(Int::I32),
        );
    }
}

/// Registers all `rcl_dotnet_*` BCL bindings in `patcher`.
pub fn insert_dotnet_pal(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    use_pool_alloc: bool,
    unity_netstandard: bool,
) {
    if use_pool_alloc {
        super::pool_alloc::insert_pool_helpers(asm);
    }
    insert_dotnet_alloc(asm, patcher, use_pool_alloc, unity_netstandard);
    insert_dotnet_free(asm, patcher, use_pool_alloc, unity_netstandard);
    insert_dotnet_write(asm, patcher);
    insert_dotnet_random_fill(asm, patcher);
    insert_dotnet_instant_ticks(asm, patcher);
    insert_dotnet_instant_freq(asm, patcher);
    insert_dotnet_unix_ticks(asm, patcher);
    insert_dotnet_thread_spawn(asm, patcher);
    insert_dotnet_thread_join(asm, patcher);
    insert_dotnet_thread_yield(asm, patcher);
    insert_dotnet_thread_sleep(asm, patcher);
    insert_dotnet_mutex_new(asm, patcher);
    insert_dotnet_mutex_lock(asm, patcher);
    insert_dotnet_mutex_unlock(asm, patcher);
    insert_dotnet_mutex_trylock(asm, patcher);
    insert_dotnet_park_new(asm, patcher);
    insert_dotnet_park_wait(asm, patcher);
    insert_dotnet_park_wait_timeout(asm, patcher);
    insert_dotnet_park_release(asm, patcher);
    insert_dotnet_condvar_new(asm, patcher);
    insert_dotnet_condvar_wait(asm, patcher);
    insert_dotnet_condvar_wait_timeout(asm, patcher);
    insert_dotnet_condvar_release(asm, patcher);
    insert_dotnet_tls_create(asm, patcher);
    insert_dotnet_tls_get(asm, patcher);
    insert_dotnet_tls_set(asm, patcher);
    insert_dotnet_available_parallelism(asm, patcher);
    insert_dotnet_getpid(asm, patcher);
    insert_dotnet_exit(asm, patcher);
    insert_dotnet_hostname(asm, patcher);
    insert_dotnet_paths(asm, patcher);
    insert_dotnet_cotaskmem_free(asm, patcher);
    insert_dotnet_args(asm, patcher);
    insert_dotnet_env(asm, patcher);
    insert_dotnet_fs(asm, patcher);
    insert_dotnet_net(asm, patcher);
    insert_dotnet_process(asm, patcher);
}

/// Process-spawn hooks for the dotnet `process` PAL arm — a `System.Diagnostics.Process` bridge.
/// The Rust PAL builds a `ProcessStartInfo` (handle), sets FileName/Arguments/cwd, optionally
/// requests stdout/stderr capture, starts it (→ a `Process` handle), and waits. Each hook is
/// loop-free; the orchestration (arg pasting, etc.) is on the Rust side. Handles are `GCHandle`
/// `IntPtr`s (same convention as fs `FileStream` / net `Socket`).
fn insert_dotnet_process(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let void_ptr = |asm: &mut Assembly| {
        let void = asm.alloc_type(Type::Void);
        (void, Type::Ptr(void))
    };

    // rcl_dotnet_proc_psi_new(prog_ptr, prog_len) -> *mut u8
    //   => psi = new ProcessStartInfo(); psi.FileName = prog; psi.UseShellExecute = false;
    //      return (void*)GCHandle.Alloc(psi).
    {
        let name = asm.alloc_string("rcl_dotnet_proc_psi_new");
        let generator = move |_, asm: &mut Assembly| {
            let psi_cr = ClassRef::process_start_info(asm);
            let psi_ty = Type::ClassRef(psi_cr);
            let ctor = asm.class_ref(psi_cr).clone().ctor(&[], asm);
            let psi = asm.alloc_node(CILNode::call(ctor, []));
            let store = asm.alloc_root(CILRoot::StLoc(0, psi));
            // psi.FileName = decode(prog)
            let prog = decode_utf8(asm, 0, 1);
            let set_fn = asm.alloc_string("set_FileName");
            let set_file_name = asm.class_ref(psi_cr).clone().instance(
                &[Type::PlatformString],
                Type::Void,
                set_fn,
                asm,
            );
            let psi0 = asm.alloc_node(CILNode::LdLoc(0));
            let r_fn = asm.alloc_root(CILRoot::call(set_file_name, [psi0, prog]));
            // psi.UseShellExecute = false
            let set_use = asm.alloc_string("set_UseShellExecute");
            let set_use_shell =
                asm.class_ref(psi_cr)
                    .clone()
                    .instance(&[Type::Bool], Type::Void, set_use, asm);
            let psi1 = asm.alloc_node(CILNode::LdLoc(0));
            let false_c = asm.alloc_node(false);
            let r_use = asm.alloc_root(CILRoot::call(set_use_shell, [psi1, false_c]));
            // return (void*)GCHandle.Alloc(psi)
            let handle = CILNode::LdLoc(0).ref_to_handle(asm);
            let handle = asm.alloc_node(handle);
            let (void, _) = void_ptr(asm);
            let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
            let ret = asm.alloc_root(CILRoot::Ret(handle));
            let psi_local = asm.alloc_type(psi_ty);
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![store, r_fn, r_use, ret], 0, None)],
                locals: vec![(Some(asm.alloc_string("psi")), psi_local)],
            }
        };
        patcher.insert(name, Box::new(generator));
    }

    // setter hooks taking (psi_handle, str_ptr, str_len): set a string property on the PSI.
    let mut str_setter = |asm: &mut Assembly, hook: &str, setter: &str| {
        let name = asm.alloc_string(hook);
        let setter = setter.to_string();
        let generator = move |_, asm: &mut Assembly| {
            let psi_cr = ClassRef::process_start_info(asm);
            let psi = handle_to_class(asm, 0, psi_cr);
            let val = decode_utf8(asm, 1, 2);
            let sname = asm.alloc_string(setter.as_str());
            let set = asm.class_ref(psi_cr).clone().instance(
                &[Type::PlatformString],
                Type::Void,
                sname,
                asm,
            );
            let call = asm.alloc_root(CILRoot::call(set, [psi, val]));
            let ret = asm.alloc_root(CILRoot::VoidRet);
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![call, ret], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    };
    str_setter(asm, "rcl_dotnet_proc_psi_args", "set_Arguments");
    str_setter(asm, "rcl_dotnet_proc_psi_cwd", "set_WorkingDirectory");

    // rcl_dotnet_proc_psi_capture(psi_handle): psi.RedirectStandardOutput = true; ...Error = true.
    {
        let name = asm.alloc_string("rcl_dotnet_proc_psi_capture");
        let generator = move |_, asm: &mut Assembly| {
            let psi_cr = ClassRef::process_start_info(asm);
            let mut roots = Vec::new();
            for setter in ["set_RedirectStandardOutput", "set_RedirectStandardError"] {
                let psi = handle_to_class(asm, 0, psi_cr);
                let sname = asm.alloc_string(setter);
                let set =
                    asm.class_ref(psi_cr)
                        .clone()
                        .instance(&[Type::Bool], Type::Void, sname, asm);
                let true_c = asm.alloc_node(true);
                roots.push(asm.alloc_root(CILRoot::call(set, [psi, true_c])));
            }
            roots.push(asm.alloc_root(CILRoot::VoidRet));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(roots, 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    }

    // rcl_dotnet_proc_start(psi_handle) -> *mut u8
    //   => p = Process.Start(psi); free the psi GCHandle; return (void*)GCHandle.Alloc(p).
    {
        let name = asm.alloc_string("rcl_dotnet_proc_start");
        let generator = move |_, asm: &mut Assembly| {
            let psi_cr = ClassRef::process_start_info(asm);
            let proc_cr = ClassRef::process(asm);
            let psi = handle_to_class(asm, 0, psi_cr);
            let start_name = asm.alloc_string("Start");
            let start = asm.class_ref(proc_cr).clone().static_mref(
                &[Type::ClassRef(psi_cr)],
                Type::ClassRef(proc_cr),
                start_name,
                asm,
            );
            let p = asm.alloc_node(CILNode::call(start, [psi]));
            let store_p = asm.alloc_root(CILRoot::StLoc(0, p));
            // free the psi GCHandle (arg 0); it is consumed.
            let (store_gch, free, gc_handle_ty) = free_handle_roots(asm, 0, 1);
            // return (void*)GCHandle.Alloc(p)
            let handle = CILNode::LdLoc(0).ref_to_handle(asm);
            let handle = asm.alloc_node(handle);
            let (void, _) = void_ptr(asm);
            let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
            let ret = asm.alloc_root(CILRoot::Ret(handle));
            let proc_local = asm.alloc_type(Type::ClassRef(proc_cr));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(
                    vec![store_p, store_gch, free, ret],
                    0,
                    None,
                )],
                locals: vec![
                    (Some(asm.alloc_string("p")), proc_local),
                    (Some(asm.alloc_string("gch")), gc_handle_ty),
                ],
            }
        };
        patcher.insert(name, Box::new(generator));
    }

    // rcl_dotnet_proc_wait(proc_handle) -> i32 => p.WaitForExit(); return p.ExitCode.
    {
        let name = asm.alloc_string("rcl_dotnet_proc_wait");
        let generator = move |_, asm: &mut Assembly| {
            let proc_cr = ClassRef::process(asm);
            let p = handle_to_class(asm, 0, proc_cr);
            let wfe_name = asm.alloc_string("WaitForExit");
            let wfe = asm
                .class_ref(proc_cr)
                .clone()
                .instance(&[], Type::Void, wfe_name, asm);
            let r_wait = asm.alloc_root(CILRoot::call(wfe, [p]));
            let p2 = handle_to_class(asm, 0, proc_cr);
            let ec_name = asm.alloc_string("get_ExitCode");
            let get_ec =
                asm.class_ref(proc_cr)
                    .clone()
                    .instance(&[], Type::Int(Int::I32), ec_name, asm);
            let code = asm.alloc_node(CILNode::call(get_ec, [p2]));
            let ret = asm.alloc_root(CILRoot::Ret(code));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![r_wait, ret], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    }

    // simple instance-getter hooks: rcl_dotnet_proc_id -> u32 (Id), rcl_dotnet_proc_has_exited -> i32.
    let mut int_getter = |asm: &mut Assembly, hook: &str, getter: &str, ret: Int| {
        let name = asm.alloc_string(hook);
        let getter = getter.to_string();
        let generator = move |_, asm: &mut Assembly| {
            let proc_cr = ClassRef::process(asm);
            let p = handle_to_class(asm, 0, proc_cr);
            let gname = asm.alloc_string(getter.as_str());
            let get = asm
                .class_ref(proc_cr)
                .clone()
                .instance(&[], Type::Int(ret), gname, asm);
            let v = asm.alloc_node(CILNode::call(get, [p]));
            let ret_root = asm.alloc_root(CILRoot::Ret(v));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret_root], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    };
    int_getter(asm, "rcl_dotnet_proc_id", "get_Id", Int::I32);

    // rcl_dotnet_proc_has_exited(proc_handle) -> i32 (1 if exited else 0).
    {
        let name = asm.alloc_string("rcl_dotnet_proc_has_exited");
        let generator = move |_, asm: &mut Assembly| {
            let proc_cr = ClassRef::process(asm);
            let p = handle_to_class(asm, 0, proc_cr);
            let he_name = asm.alloc_string("get_HasExited");
            let get = asm
                .class_ref(proc_cr)
                .clone()
                .instance(&[], Type::Bool, he_name, asm);
            let b = asm.alloc_node(CILNode::call(get, [p]));
            let v = asm.int_cast(b, Int::I32, ExtendKind::ZeroExtend);
            let ret_root = asm.alloc_root(CILRoot::Ret(v));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret_root], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    }

    // rcl_dotnet_proc_kill(proc_handle) => p.Kill().
    {
        let name = asm.alloc_string("rcl_dotnet_proc_kill");
        let generator = move |_, asm: &mut Assembly| {
            let proc_cr = ClassRef::process(asm);
            let p = handle_to_class(asm, 0, proc_cr);
            let kill_name = asm.alloc_string("Kill");
            let kill = asm
                .class_ref(proc_cr)
                .clone()
                .instance(&[], Type::Void, kill_name, asm);
            let r_kill = asm.alloc_root(CILRoot::call(kill, [p]));
            let ret = asm.alloc_root(CILRoot::VoidRet);
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![r_kill, ret], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    }

    // child-stream getters: rcl_dotnet_proc_{stdout,stderr,stdin}(proc_handle) -> *mut u8
    //   => GCHandle to Process.Standard{Output,Error}.BaseStream (a reader) /
    //      Process.StandardInput.BaseStream (a writer) — the raw byte Stream for capture.
    let mut child_stream = |asm: &mut Assembly, hook: &str, getter: &str, reader: bool| {
        let name = asm.alloc_string(hook);
        let getter = getter.to_string();
        let generator = move |_, asm: &mut Assembly| {
            let proc_cr = ClassRef::process(asm);
            let rw_cr = if reader {
                ClassRef::stream_reader(asm)
            } else {
                ClassRef::stream_writer(asm)
            };
            let rw_ty = Type::ClassRef(rw_cr);
            let stream_cr = ClassRef::stream(asm);
            let stream_ty = Type::ClassRef(stream_cr);
            let p = handle_to_class(asm, 0, proc_cr);
            let gname = asm.alloc_string(getter.as_str());
            let get_std = asm
                .class_ref(proc_cr)
                .clone()
                .instance(&[], rw_ty, gname, asm);
            let rw = asm.alloc_node(CILNode::call(get_std, [p]));
            let store_rw = asm.alloc_root(CILRoot::StLoc(0, rw));
            let bs_name = asm.alloc_string("get_BaseStream");
            let get_bs = asm
                .class_ref(rw_cr)
                .clone()
                .instance(&[], stream_ty, bs_name, asm);
            let rw_load = asm.alloc_node(CILNode::LdLoc(0));
            let stream = asm.alloc_node(CILNode::call(get_bs, [rw_load]));
            let store_s = asm.alloc_root(CILRoot::StLoc(1, stream));
            let handle = CILNode::LdLoc(1).ref_to_handle(asm);
            let handle = asm.alloc_node(handle);
            let void = asm.alloc_type(Type::Void);
            let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
            let ret = asm.alloc_root(CILRoot::Ret(handle));
            let rw_local = asm.alloc_type(rw_ty);
            let s_local = asm.alloc_type(stream_ty);
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![store_rw, store_s, ret], 0, None)],
                locals: vec![
                    (Some(asm.alloc_string("rw")), rw_local),
                    (Some(asm.alloc_string("s")), s_local),
                ],
            }
        };
        patcher.insert(name, Box::new(generator));
    };
    child_stream(asm, "rcl_dotnet_proc_stdout", "get_StandardOutput", true);
    child_stream(asm, "rcl_dotnet_proc_stderr", "get_StandardError", true);
    child_stream(asm, "rcl_dotnet_proc_stdin", "get_StandardInput", false);

    // rcl_dotnet_stream_read(stream_handle, buf_ptr, len) -> i32 => Stream.Read(Span<byte>) (0 at EOF).
    {
        let name = asm.alloc_string("rcl_dotnet_stream_read");
        let generator = move |_, asm: &mut Assembly| {
            let stream_cr = ClassRef::stream(asm);
            let s = handle_to_class(asm, 0, stream_cr);
            let (span, span_ty) = build_byte_span(asm, 1, 2, false);
            let read_name = asm.alloc_string("Read");
            let read = asm.class_ref(stream_cr).clone().instance(
                &[span_ty],
                Type::Int(Int::I32),
                read_name,
                asm,
            );
            let n = asm.alloc_node(CILNode::call(read, [s, span]));
            let ret = asm.alloc_root(CILRoot::Ret(n));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    }

    // rcl_dotnet_stream_write(stream_handle, buf_ptr, len) -> i32 => Stream.Write(ROSpan<byte>); ret len.
    {
        let name = asm.alloc_string("rcl_dotnet_stream_write");
        let generator = move |_, asm: &mut Assembly| {
            let stream_cr = ClassRef::stream(asm);
            let s = handle_to_class(asm, 0, stream_cr);
            let (span, span_ty) = build_byte_span(asm, 1, 2, true);
            let write_name = asm.alloc_string("Write");
            let write =
                asm.class_ref(stream_cr)
                    .clone()
                    .instance(&[span_ty], Type::Void, write_name, asm);
            let r_write = asm.alloc_root(CILRoot::call(write, [s, span]));
            let len = asm.alloc_node(CILNode::LdArg(2));
            let len = asm.int_cast(len, Int::I32, ExtendKind::ZeroExtend);
            let ret = asm.alloc_root(CILRoot::Ret(len));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![r_write, ret], 0, None)],
                locals: vec![],
            }
        };
        patcher.insert(name, Box::new(generator));
    }

    // rcl_dotnet_stream_close(stream_handle) => Stream.Dispose() + free the GCHandle.
    {
        let name = asm.alloc_string("rcl_dotnet_stream_close");
        let generator = move |_, asm: &mut Assembly| {
            let stream_cr = ClassRef::stream(asm);
            let s = handle_to_class(asm, 0, stream_cr);
            let dispose_name = asm.alloc_string("Dispose");
            let dispose =
                asm.class_ref(stream_cr)
                    .clone()
                    .instance(&[], Type::Void, dispose_name, asm);
            let r_disp = asm.alloc_root(CILRoot::call(dispose, [s]));
            let (store_gch, free, gc_handle_ty) = free_handle_roots(asm, 0, 0);
            let ret = asm.alloc_root(CILRoot::VoidRet);
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![r_disp, store_gch, free, ret], 0, None)],
                locals: vec![(Some(asm.alloc_string("gch")), gc_handle_ty)],
            }
        };
        patcher.insert(name, Box::new(generator));
    }

    // rcl_dotnet_proc_free(proc_handle) => p.Dispose() + free the GCHandle.
    {
        let name = asm.alloc_string("rcl_dotnet_proc_free");
        let generator = move |_, asm: &mut Assembly| {
            let proc_cr = ClassRef::process(asm);
            let p = handle_to_class(asm, 0, proc_cr);
            let dispose_name = asm.alloc_string("Dispose");
            let dispose =
                asm.class_ref(proc_cr)
                    .clone()
                    .instance(&[], Type::Void, dispose_name, asm);
            let r_disp = asm.alloc_root(CILRoot::call(dispose, [p]));
            let (store_gch, free, gc_handle_ty) = free_handle_roots(asm, 0, 0);
            let ret = asm.alloc_root(CILRoot::VoidRet);
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![r_disp, store_gch, free, ret], 0, None)],
                locals: vec![(Some(asm.alloc_string("gch")), gc_handle_ty)],
            }
        };
        patcher.insert(name, Box::new(generator));
    }
}

/// `rcl_dotnet_instant_ticks() -> i64`
///   => `System.Diagnostics.Stopwatch.GetTimestamp()`.
///
/// A static method returning the current value of the platform's monotonic
/// high-resolution counter (QPC on Windows, `clock_gettime(CLOCK_MONOTONIC)` on
/// unix). Its zero is arbitrary; only differences matter — exactly the `Instant`
/// contract. Paired with `rcl_dotnet_instant_freq` to convert ticks to seconds
/// std-side. Backs `std::time::Instant::now` on the .NET PAL.
fn insert_dotnet_instant_ticks(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_instant_ticks",
        static_getter ClassRef::stopwatch, "GetTimestamp" -> Type::Int(Int::I64));
}

/// `rcl_dotnet_instant_freq() -> i64`
///   => `System.Diagnostics.Stopwatch.Frequency` (the static `Frequency` field, via `ldsfld`).
///
/// Ticks per second for the counter returned by `rcl_dotnet_instant_ticks`.
/// `Frequency` is a `public static readonly long` FIELD on `Stopwatch` — CoreCLR
/// exposes it directly, with no `get_Frequency()` getter — so this loads it with
/// `ldsfld` rather than issuing a static call.
fn insert_dotnet_instant_freq(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_instant_freq", |asm| {
        let stopwatch = ClassRef::stopwatch(asm);
        let freq_name = asm.alloc_string("Frequency");
        let freq_fld = asm.alloc_sfld(StaticFieldDesc::new(
            stopwatch,
            freq_name,
            Type::Int(Int::I64),
        ));
        let freq = asm.alloc_node(CILNode::LdStaticField(freq_fld));
        let ret = asm.alloc_root(CILRoot::Ret(freq));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_unix_ticks() -> i64`
///   => `System.DateTime.UtcNow.Ticks` (100-ns intervals since `0001-01-01Z`).
///
/// `DateTime.UtcNow` is a static getter returning a `DateTime` **struct**;
/// `.Ticks` is an instance getter on it. Calling an instance method on a value
/// type needs a managed `this` pointer, so the struct is stowed in a local and
/// its address (`ldloca`) is fed as the receiver. The PAL rebases the result
/// onto the Unix epoch in Rust (subtracting a constant), so the binding stays a
/// pair of property reads with no static-field load. Backs
/// `std::time::SystemTime::now` on the .NET PAL.
fn insert_dotnet_unix_ticks(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_unix_ticks", |asm| {
        let datetime = ClassRef::datetime(asm);
        let datetime_ty = Type::ClassRef(datetime);
        // `this` for the instance `get_Ticks` is a managed pointer to DateTime.
        let datetime_ref = asm.nref(datetime_ty);

        // UtcNow -> DateTime (static getter), stowed into local 0.
        let get_utcnow = MethodRef::new(
            datetime,
            asm.alloc_string("get_UtcNow"),
            asm.sig([], datetime_ty),
            MethodKind::Static,
            [].into(),
        );
        let get_utcnow = asm.alloc_methodref(get_utcnow);
        let now = asm.alloc_node(CILNode::call(get_utcnow, []));
        let store_now = asm.alloc_root(CILRoot::StLoc(0, now));

        // (&local0).Ticks -> int64. Receiver type (the managed ref) is sig
        // input[0]; the IL exporter drops it from the printed instance signature.
        let dt_addr = asm.alloc_node(CILNode::LdLocA(0));
        let get_ticks = MethodRef::new(
            datetime,
            asm.alloc_string("get_Ticks"),
            asm.sig([datetime_ref], Type::Int(Int::I64)),
            MethodKind::Instance,
            [].into(),
        );
        let get_ticks = asm.alloc_methodref(get_ticks);
        let ticks = asm.alloc_node(CILNode::call(get_ticks, [dt_addr]));
        let ret = asm.alloc_root(CILRoot::Ret(ticks));

        let datetime_local_ty = asm.alloc_type(datetime_ty);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store_now, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("utc_now")), datetime_local_ty)],
        }
    });
}

/// `rcl_dotnet_alloc(size: usize, align: usize) -> *mut u8`
///   => `NativeMemory.AlignedAlloc((nuint)size, (nuint)align)`.
///
/// Models the existing `__rust_alloc` builtin: forward straight to
/// `AlignedAlloc`. Recent rustc wraps allocator-shim scalars in transparent
/// value types, but this symbol comes from our own PAL's `extern "C"` decl with
/// plain `usize` arguments, so the args are loaded directly.
fn insert_dotnet_alloc(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    use_pool_alloc: bool,
    unity_netstandard: bool,
) {
    if use_pool_alloc {
        dotnet_hook!(asm, patcher, "rcl_dotnet_alloc", |mref, asm| {
            let size = asm.alloc_node(CILNode::LdArg(0));
            let align = asm.alloc_node(CILNode::LdArg(1));
            let alloc_mref = super::pool_alloc::pool_alloc_mref(asm);
            let alloc = asm.alloc_node(CILNode::call(alloc_mref, [size, align]));
            let void_ptr = asm.nptr(Type::Void);
            let alloc = super::adapt_runtime_result(mref, alloc, void_ptr, asm);
            let ret = asm.alloc_root(CILRoot::Ret(alloc));
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![ret], 0, None)],
                locals: vec![],
            }
        });
        return;
    }
    let allocator = super::aligned_allocator_refs(asm, unity_netstandard);
    dotnet_hook!(asm, patcher, "rcl_dotnet_alloc", |mref, asm| {
        let size = asm.alloc_node(CILNode::LdArg(0));
        let align = asm.alloc_node(CILNode::LdArg(1));
        let void_ptr = asm.nptr(Type::Void);
        let alloc = asm.alloc_node(CILNode::call(allocator.alloc, [size, align]));
        let alloc = super::adapt_runtime_result(mref, alloc, void_ptr, asm);
        let ret = asm.alloc_root(CILRoot::Ret(alloc));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_free(ptr: *mut u8, size: usize, align: usize)`
///   => `NativeMemory.AlignedFree((void*)ptr)`.
///
/// Models the non-libc `__rust_dealloc` builtin. In direct mode, `size` and
/// `align` are unused (`AlignedFree` takes only the pointer); pooled mode uses
/// `size`/`align` to select the free-list class.
fn insert_dotnet_free(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    use_pool_alloc: bool,
    unity_netstandard: bool,
) {
    if use_pool_alloc {
        dotnet_hook!(asm, patcher, "rcl_dotnet_free", |asm| {
            let ptr = asm.alloc_node(CILNode::LdArg(0));
            let void_ptr = asm.nptr(Type::Void);
            let ptr = asm.cast_ptr(ptr, void_ptr);
            let size = asm.alloc_node(CILNode::LdArg(1));
            let align = asm.alloc_node(CILNode::LdArg(2));
            let free_mref = super::pool_alloc::pool_free_mref(asm);
            let free = asm.alloc_root(CILRoot::call(free_mref, [ptr, size, align]));
            let ret = asm.alloc_root(CILRoot::VoidRet);
            MethodImpl::MethodBody {
                blocks: vec![BasicBlock::new(vec![free, ret], 0, None)],
                locals: vec![],
            }
        });
        return;
    }
    let allocator = super::aligned_allocator_refs(asm, unity_netstandard);
    dotnet_hook!(asm, patcher, "rcl_dotnet_free", |asm| {
        let ptr = asm.alloc_node(CILNode::LdArg(0));
        let void_ptr = asm.nptr(Type::Void);
        // Reinterpret *mut u8 as void* for the AlignedFree signature.
        let ptr = asm.cast_ptr(ptr, void_ptr);
        let free = asm.alloc_root(CILRoot::call(allocator.free, [ptr]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![free, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_write(fd: i32, ptr: *const u8, len: usize) -> isize`
///   => write `len` UTF-8 bytes from `ptr` to `System.Console`'s stdout (fd 1)
///      or stderr (fd 2); returns bytes written.
///
/// The `(ptr, len)` pair is turned into a managed `string` via
/// `System.Text.Encoding.UTF8.GetString(byte*, int)` (the overload that takes a
/// raw pointer, so no managed `byte[]` needs to be materialised), then written
/// with `System.Console.Out.Write(string)` / `Console.Error.Write(string)`
/// (a virtual `System.IO.TextWriter.Write`). Returns the input `len` (the bytes
/// consumed); on a managed I/O fault a .NET exception unwinds rather than the
/// `-1` path being taken.
fn insert_dotnet_write(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_write");
    let generator = move |_, asm: &mut Assembly| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));

        // ---- BCL class refs ----
        let console = ClassRef::console(asm);
        let encoding = {
            let name = asm.alloc_string("System.Text.Encoding");
            let asm_name = Some(asm.alloc_string("System.Runtime"));
            asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
        };
        let text_writer = {
            let name = asm.alloc_string("System.IO.TextWriter");
            let asm_name = Some(asm.alloc_string("System.Runtime"));
            asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
        };
        let encoding_ty = Type::ClassRef(encoding);
        let text_writer_ty = Type::ClassRef(text_writer);

        // ---- decode the (ptr, len) buffer into a managed string ----
        // Encoding.UTF8 -> Encoding   (static property getter)
        let get_utf8 = MethodRef::new(
            encoding,
            asm.alloc_string("get_UTF8"),
            asm.sig([], encoding_ty),
            MethodKind::Static,
            [].into(),
        );
        let get_utf8 = asm.alloc_methodref(get_utf8);
        let utf8 = asm.alloc_node(CILNode::call(get_utf8, []));

        // Encoding.GetString(byte* bytes, int byteCount) -> string  (instance)
        let len = asm.alloc_node(CILNode::LdArg(2));
        // usize -> int32 (`conv.u4`); truncates the 64-bit length to the int the
        // GetString(byte*, int) overload expects.
        let len_i32 = asm.int_cast(len, Int::I32, ExtendKind::ZeroExtend);
        let ptr = asm.alloc_node(CILNode::LdArg(1));
        let ptr = asm.cast_ptr(ptr, u8_ptr);
        let get_string = MethodRef::new(
            encoding,
            asm.alloc_string("GetString"),
            asm.sig(
                [encoding_ty, u8_ptr, Type::Int(Int::I32)],
                Type::PlatformString,
            ),
            MethodKind::Instance,
            [].into(),
        );
        let get_string = asm.alloc_methodref(get_string);
        let managed = asm.alloc_node(CILNode::call(get_string, [utf8, ptr, len_i32]));
        let store_str = asm.alloc_root(CILRoot::StLoc(0, managed));

        // ---- select stdout/stderr by fd, then Write(string) ----
        // get_Out / get_Error -> TextWriter (static), then virtual Write(string).
        let make_writer = |asm: &mut Assembly, getter: &str| {
            let getter = asm.alloc_string(getter);
            let mref = MethodRef::new(
                console,
                getter,
                asm.sig([], text_writer_ty),
                MethodKind::Static,
                [].into(),
            );
            asm.alloc_methodref(mref)
        };
        let get_out = make_writer(asm, "get_Out");
        let get_error = make_writer(asm, "get_Error");
        let write = {
            let mref = MethodRef::new(
                text_writer,
                asm.alloc_string("Write"),
                asm.sig([text_writer_ty, Type::PlatformString], Type::Void),
                MethodKind::Virtual,
                [].into(),
            );
            asm.alloc_methodref(mref)
        };

        // Block 0: if (fd == 1) goto stdout(1) else goto stderr(2)
        let fd = asm.alloc_node(CILNode::LdArg(0));
        let one = asm.alloc_node(crate::Const::I32(1));
        let branch_stdout = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(fd, one)),
        ))));
        let goto_stderr = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // Block 1 (stdout): Console.Out.Write(str); goto ret(3)
        let writer_out = asm.alloc_node(CILNode::call(get_out, []));
        let str_out = asm.alloc_node(CILNode::LdLoc(0));
        let write_out = asm.alloc_root(CILRoot::call(write, [writer_out, str_out]));
        let out_to_ret = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // Block 2 (stderr): Console.Error.Write(str); goto ret(3)
        let writer_err = asm.alloc_node(CILNode::call(get_error, []));
        let str_err = asm.alloc_node(CILNode::LdLoc(0));
        let write_err = asm.alloc_root(CILRoot::call(write, [writer_err, str_err]));
        let err_to_ret = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // Block 3 (ret): return (isize)len
        let len_ret = asm.alloc_node(CILNode::LdArg(2));
        let len_ret = asm.int_cast(len_ret, Int::ISize, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(len_ret));

        let string_ty = asm.alloc_type(Type::PlatformString);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_str, branch_stdout, goto_stderr], 0, None),
                BasicBlock::new(vec![write_out, out_to_ret], 1, None),
                BasicBlock::new(vec![write_err, err_to_ret], 2, None),
                BasicBlock::new(vec![ret], 3, None),
            ],
            locals: vec![(Some(asm.alloc_string("managed_str")), string_ty)],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_random_fill(ptr: *mut u8, len: usize)`
///   => `RandomNumberGenerator.Fill(new Span<byte>((void*)ptr, (int)len))`.
///
/// Wraps the raw `(ptr, len)` buffer in a `System.Span<byte>` (constructed in
/// place from its `(void*, int32)` ctor — no managed `byte[]` is allocated) and
/// hands it to the static, cryptographically-secure
/// `System.Security.Cryptography.RandomNumberGenerator.Fill(Span<byte>)`, which
/// fills the entire span. This is what backs std's `sys::random::fill_bytes` on
/// the .NET PAL, so `HashMap`'s `RandomState` / SipHash keys get real entropy
/// (replacing the deterministic SplitMix64 placeholder).
///
/// The `Span<byte>` is built with a `newobj` whose value is consumed directly by
/// `Fill` — the exporter emits the ctor first (leaving the span on the stack),
/// then the call, which is the correct CIL ordering for a by-value `Span` arg.
fn insert_dotnet_random_fill(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_random_fill", |asm| {
        let void_ptr = asm.nptr(Type::Void);
        let byte_ty = Type::Int(Int::U8);

        // ---- Span<byte> type + its (void*, int32) constructor ----
        let span_byte = ClassRef::span(asm, byte_ty);
        let span_ty = Type::ClassRef(span_byte);
        // Constructor sig: this(Span<byte>), void* pointer, int32 length -> void.
        // For `newobj` the exporter drops the implicit `this` (input[0]); the
        // explicit args are `void*` then `int32`.
        let ctor_sig = asm.sig([span_ty, void_ptr, Type::Int(Int::I32)], Type::Void);
        let ctor_name = asm.alloc_string(".ctor");
        let ctor = asm.alloc_methodref(MethodRef::new(
            span_byte,
            ctor_name,
            ctor_sig,
            MethodKind::Constructor,
            [].into(),
        ));

        // ---- build the span from (ptr, len) ----
        // (void*)ptr
        let ptr = asm.alloc_node(CILNode::LdArg(0));
        let ptr = asm.cast_ptr(ptr, void_ptr);
        // (int32)len — truncate the 64-bit usize to the int the ctor expects.
        let len = asm.alloc_node(CILNode::LdArg(1));
        let len_i32 = asm.int_cast(len, Int::I32, ExtendKind::ZeroExtend);
        let span = asm.alloc_node(CILNode::call(ctor, [ptr, len_i32]));

        // ---- RandomNumberGenerator.Fill(span) (static) ----
        let rng = ClassRef::random_number_generator(asm);
        let fill_sig = asm.sig([span_ty], Type::Void);
        let fill_name = asm.alloc_string("Fill");
        let fill = asm.alloc_methodref(MethodRef::new(
            rng,
            fill_name,
            fill_sig,
            MethodKind::Static,
            [].into(),
        ));
        let fill = asm.alloc_root(CILRoot::call(fill, [span]));
        let ret = asm.alloc_root(CILRoot::VoidRet);

        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![fill, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_thread_spawn(entry: fn(*mut u8) -> *mut u8, arg: *mut u8) -> *mut u8`
///   => spawn a `System.Threading.Thread` that runs `entry(arg)`; return a
///      `GCHandle` `IntPtr` pinning the managed `Thread` for a later join.
///
/// Reuses the shared `UnmanagedThreadStart` helper class (registered by
/// `builtins::instert_threading`, which runs in the same .NET link pass): it
/// wraps a native `fn(void*) -> void*` start routine plus a `void* data`, and
/// exposes a managed `Start()` method that `calli`s the native pointer. We:
///   1. construct an `UnmanagedThreadStart(entry, arg)`,
///   2. take `ldftn UnmanagedThreadStart::Start` and build a `ThreadStart`
///      delegate `(target_obj, ftn)`,
///   3. construct a `Thread(ThreadStart)` and `Start()` it,
///   4. pin the `Thread` in a `GCHandle` and return that handle as an `IntPtr`.
/// This is the exact dance `pthread_create` performs, minus the `pthread_t`
/// out-parameter and the arg-transmute (our `entry`/`arg` arrive already typed),
/// returning the handle by value instead of through a pointer.
fn insert_dotnet_thread_spawn(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_thread_spawn");
    let generator = move |_, asm: &mut Assembly| {
        let void_ptr = asm.nptr(Type::Void);
        // The native start routine: fn(void*) -> void*, matching UnmanagedThreadStart.
        let start_sig = asm.sig([void_ptr], void_ptr);

        // entry (arg0) is the fn pointer; arg (arg1) is the data pointer.
        let entry = asm.alloc_node(CILNode::LdArg(0));
        let entry = asm.alloc_node(CILNode::PtrCast(
            entry,
            Box::new(PtrCastRes::FnPtr(start_sig)),
        ));
        let void = asm.alloc_type(Type::Void);
        let arg = asm.alloc_node(CILNode::LdArg(1));
        let arg = asm.alloc_node(CILNode::PtrCast(arg, Box::new(PtrCastRes::Ptr(void))));

        // ---- UnmanagedThreadStart(entry, arg) ----
        let uts_name = asm.alloc_string(UNMANAGED_THREAD_START);
        let uts = ClassRef::new(uts_name, None, false, [].into());
        let uts_ctor = uts.ctor(&[Type::FnPtr(start_sig), void_ptr], asm);
        let uts_obj = asm.alloc_node(CILNode::call(uts_ctor, [entry, arg]));

        // ldftn UnmanagedThreadStart::Start  -> native int
        let start = asm.alloc_string("Start");
        let uts_start = uts.virtual_mref(&[], Type::Void, start, asm);
        let uts_start_fn = asm.alloc_node(CILNode::LdFtn(uts_start));
        let uts_start_fn =
            asm.alloc_node(CILNode::PtrCast(uts_start_fn, Box::new(PtrCastRes::ISize)));

        // ---- new ThreadStart(uts_obj, ftn) ----
        let thread_start = ClassRef::thread_start(asm);
        let thread_start_ctor = asm
            .class_ref(thread_start)
            .clone()
            .ctor(&[Type::PlatformObject, Type::Int(Int::ISize)], asm);
        let thread_start_obj =
            asm.alloc_node(CILNode::call(thread_start_ctor, [uts_obj, uts_start_fn]));
        let store_ts = asm.alloc_root(CILRoot::StLoc(1, thread_start_obj));
        let thread_start_obj = asm.alloc_node(CILNode::LdLoc(1));
        let thread_start_ty = asm.alloc_type(Type::ClassRef(thread_start));
        let thread_start_obj =
            asm.alloc_node(CILNode::CheckedCast(thread_start_obj, thread_start_ty));

        // ---- new Thread(ThreadStart); Thread.Start() ----
        let thread = ClassRef::thread(asm);
        let thread_ctor = asm
            .class_ref(thread)
            .clone()
            .ctor(&[Type::ClassRef(thread_start)], asm);
        let thread_obj = asm.alloc_node(CILNode::call(thread_ctor, [thread_start_obj]));
        let store_thread = asm.alloc_root(CILRoot::StLoc(0, thread_obj));
        // Mark the thread BACKGROUND before starting it, to match Rust process-exit semantics:
        // when `main` returns the process exits regardless of still-running threads. A foreground
        // .NET thread (the default) instead keeps the process alive at exit, hanging any program
        // with unjoined long-lived threads — e.g. rayon's global work-stealing pool (whose workers
        // park forever and are never joined). Joined threads (pal_threads) are unaffected: `Join`
        // still waits for a background thread.
        let set_is_background = asm.alloc_string("set_IsBackground");
        let set_bg_mref = asm.class_ref(thread).clone().virtual_mref(
            &[Type::Bool],
            Type::Void,
            set_is_background,
            asm,
        );
        let ld_thread_bg = asm.alloc_node(CILNode::LdLoc(0));
        let true_const = asm.alloc_node(Const::Bool(true));
        let set_bg = asm.alloc_root(CILRoot::call(set_bg_mref, [ld_thread_bg, true_const]));
        let thread_start_call =
            asm.class_ref(thread)
                .clone()
                .virtual_mref(&[], Type::Void, start, asm);
        let ld_thread = asm.alloc_node(CILNode::LdLoc(0));
        let start_thread = asm.alloc_root(CILRoot::call(thread_start_call, [ld_thread]));

        // ---- return (IntPtr)GCHandle.Alloc(thread) ----
        let handle = CILNode::LdLoc(0).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        // ref_to_handle yields a native int (isize); the symbol returns *mut u8.
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let ret = asm.alloc_root(CILRoot::Ret(handle));

        let thread_ty = asm.alloc_type(Type::ClassRef(thread));
        let thread_start_local_ty = asm.alloc_type(Type::ClassRef(thread_start));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![store_ts, store_thread, set_bg, start_thread, ret],
                0,
                None,
            )],
            locals: vec![
                (Some(asm.alloc_string("thread")), thread_ty),
                (
                    Some(asm.alloc_string("thread_start")),
                    thread_start_local_ty,
                ),
            ],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_thread_join(handle: *mut u8)`
///   => recover the `Thread` from `handle` (a `GCHandle` `IntPtr`) and
///      `Thread.Join()`, then free the `GCHandle`.
///
/// Mirrors `pthread_join`'s handle round-trip — `handle_to_obj` turns the
/// `IntPtr` back into the pinned `object`, which is cast to `Thread` and joined —
/// but additionally frees the `GCHandle` afterwards (pthread leaves detaching to
/// the std side), and takes no result-out-pointer (`std`'s `Thread::join`
/// discards the return value).
fn insert_dotnet_thread_join(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_thread_join", |asm| {
        // handle (arg0, *mut u8) -> isize -> object (via shared `handle_to_obj`).
        let arg0 = asm.alloc_node(CILNode::LdArg(0));
        let handle_isize = asm.alloc_node(CILNode::PtrCast(arg0, Box::new(PtrCastRes::ISize)));
        let handle_to_obj = asm.alloc_string("handle_to_obj");
        let main_module = asm.main_module();
        let handle_to_obj = asm.class_ref(*main_module).clone().static_mref(
            &[Type::Int(Int::ISize)],
            Type::PlatformObject,
            handle_to_obj,
            asm,
        );
        let obj = asm.alloc_node(CILNode::call(handle_to_obj, [handle_isize]));

        // (Thread)obj, stowed in local 0.
        let thread = ClassRef::thread(asm);
        let thread_ty = asm.alloc_type(Type::ClassRef(thread));
        let obj = asm.alloc_node(CILNode::CheckedCast(obj, thread_ty));
        let store_thread = asm.alloc_root(CILRoot::StLoc(0, obj));

        // thread.Join()
        let join = asm.alloc_string("Join");
        let join = asm
            .class_ref(thread)
            .clone()
            .virtual_mref(&[], Type::Void, join, asm);
        let ld_thread = asm.alloc_node(CILNode::LdLoc(0));
        let join = asm.alloc_root(CILRoot::call(join, [ld_thread]));

        // Free the GCHandle so the pinned Thread can be collected:
        //   GCHandle.FromIntPtr((nint)handle).Free();
        let gc_handle = ClassRef::gc_handle(asm);
        let from_int_ptr = asm.alloc_string("FromIntPtr");
        let from_int_ptr = asm.class_ref(gc_handle).clone().static_mref(
            &[Type::Int(Int::ISize)],
            Type::ClassRef(gc_handle),
            from_int_ptr,
            asm,
        );
        let arg0_again = asm.alloc_node(CILNode::LdArg(0));
        let handle_isize2 =
            asm.alloc_node(CILNode::PtrCast(arg0_again, Box::new(PtrCastRes::ISize)));
        let gch = asm.alloc_node(CILNode::call(from_int_ptr, [handle_isize2]));
        let store_gch = asm.alloc_root(CILRoot::StLoc(1, gch));
        let free = asm.alloc_string("Free");
        let free = asm
            .class_ref(gc_handle)
            .clone()
            .instance(&[], Type::Void, free, asm);
        let gch_addr = asm.alloc_node(CILNode::LdLocA(1));
        let free = asm.alloc_root(CILRoot::call(free, [gch_addr]));

        let ret = asm.alloc_root(CILRoot::VoidRet);
        let gc_handle_ty = asm.alloc_type(Type::ClassRef(gc_handle));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![store_thread, join, store_gch, free, ret],
                0,
                None,
            )],
            locals: vec![
                (Some(asm.alloc_string("thread")), thread_ty),
                (Some(asm.alloc_string("gch")), gc_handle_ty),
            ],
        }
    });
}

/// `rcl_dotnet_thread_yield()` => `System.Threading.Thread.Yield()`.
///
/// `Thread.Yield` is a static `bool` (whether the OS switched to another thread);
/// `std`'s `yield_now` ignores the result, so we pop it.
fn insert_dotnet_thread_yield(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_thread_yield", |asm| {
        let thread = ClassRef::thread(asm);
        let yield_now = asm.alloc_string("Yield");
        let yield_now = asm
            .class_ref(thread)
            .clone()
            .static_mref(&[], Type::Bool, yield_now, asm);
        let yielded = asm.alloc_node(CILNode::call(yield_now, []));
        let pop = asm.alloc_root(CILRoot::Pop(yielded));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![pop, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_thread_sleep(millis: u64)` => `System.Threading.Thread.Sleep((int)millis)`.
///
/// The std side already chunks long sleeps to `<= i32::MAX` ms, so the truncation
/// to the `int` `Sleep` overload is lossless.
fn insert_dotnet_thread_sleep(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_thread_sleep", |asm| {
        let thread = ClassRef::thread(asm);
        let millis = asm.alloc_node(CILNode::LdArg(0));
        let millis_i32 = asm.int_cast(millis, Int::I32, ExtendKind::ZeroExtend);
        let sleep = asm.alloc_string("Sleep");
        let sleep = asm.class_ref(thread).clone().static_mref(
            &[Type::Int(Int::I32)],
            Type::Void,
            sleep,
            asm,
        );
        let sleep = asm.alloc_root(CILRoot::call(sleep, [millis_i32]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![sleep, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_mutex_new() -> *mut u8`
///   => `new SemaphoreSlim(1, 1)`; return a `GCHandle` `IntPtr` pinning it.
///
/// The dotnet PAL `Mutex` is a single-permit counting semaphore: `SemaphoreSlim`
/// with `initialCount = 1` (available immediately) and `maxCount = 1` (a single
/// release at a time — non-reentrant, exactly the std `sys::sync::Mutex`
/// contract). The managed object is pinned in a `GCHandle` and the handle's
/// `IntPtr` is returned as `*mut u8`, mirroring `rcl_dotnet_thread_spawn`'s
/// handle round-trip (`ref_to_handle` => `GCHandle.Alloc`). The std side
/// CAS-installs this handle on first lock and never frees it (one semaphore per
/// live `Mutex`, freed implicitly at process exit).
fn insert_dotnet_mutex_new(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_mutex_new", |asm| {
        // new SemaphoreSlim(1, 1)
        let sem = ClassRef::semaphore_slim(asm);
        let ctor = asm
            .class_ref(sem)
            .clone()
            .ctor(&[Type::Int(Int::I32), Type::Int(Int::I32)], asm);
        let one_a = asm.alloc_node(Const::I32(1));
        let one_b = asm.alloc_node(Const::I32(1));
        let sem_obj = asm.alloc_node(CILNode::call(ctor, [one_a, one_b]));
        let store = asm.alloc_root(CILRoot::StLoc(0, sem_obj));

        // return (void*)GCHandle.Alloc(sem)
        let void = asm.alloc_type(Type::Void);
        let handle = CILNode::LdLoc(0).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let ret = asm.alloc_root(CILRoot::Ret(handle));

        let sem_ty = asm.alloc_type(Type::ClassRef(sem));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("sem")), sem_ty)],
        }
    });
}

/// Recover the `SemaphoreSlim` from a `GCHandle` `IntPtr` (arg0, `*mut u8`):
/// `handle_to_obj((nint)h)` then `(SemaphoreSlim)obj`. Mirrors the
/// `rcl_dotnet_thread_join` recovery dance. Returns the casted node.
fn recover_semaphore(asm: &mut Assembly) -> Interned<CILNode> {
    let arg0 = asm.alloc_node(CILNode::LdArg(0));
    let handle_isize = asm.alloc_node(CILNode::PtrCast(arg0, Box::new(PtrCastRes::ISize)));
    let handle_to_obj = asm.alloc_string("handle_to_obj");
    let main_module = asm.main_module();
    let handle_to_obj = asm.class_ref(*main_module).clone().static_mref(
        &[Type::Int(Int::ISize)],
        Type::PlatformObject,
        handle_to_obj,
        asm,
    );
    let obj = asm.alloc_node(CILNode::call(handle_to_obj, [handle_isize]));
    let sem = ClassRef::semaphore_slim(asm);
    let sem_ty = asm.alloc_type(Type::ClassRef(sem));
    asm.alloc_node(CILNode::CheckedCast(obj, sem_ty))
}

/// `rcl_dotnet_mutex_lock(h: *mut u8)`
///   => recover the `SemaphoreSlim` from `h` and `Wait()` (block until acquired).
///
/// `SemaphoreSlim.Wait()` (no args) is the blocking acquire — it returns void and
/// decrements the single permit. Backs `sys::sync::Mutex::lock`.
fn insert_dotnet_mutex_lock(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_mutex_lock", |asm| {
        let sem = recover_semaphore(asm);
        let sem_class = ClassRef::semaphore_slim(asm);
        let wait = asm.alloc_string("Wait");
        let wait = asm
            .class_ref(sem_class)
            .clone()
            .instance(&[], Type::Void, wait, asm);
        let wait = asm.alloc_root(CILRoot::call(wait, [sem]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![wait, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_mutex_unlock(h: *mut u8)`
///   => recover the `SemaphoreSlim` from `h` and `Release()` (return the permit).
///
/// `SemaphoreSlim.Release()` returns the previous count (an `int`); the std
/// `unlock` contract is void, so the result is popped. Backs
/// `sys::sync::Mutex::unlock`.
fn insert_dotnet_mutex_unlock(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_mutex_unlock", |asm| {
        let sem = recover_semaphore(asm);
        let sem_class = ClassRef::semaphore_slim(asm);
        let release = asm.alloc_string("Release");
        let release =
            asm.class_ref(sem_class)
                .clone()
                .instance(&[], Type::Int(Int::I32), release, asm);
        let prev = asm.alloc_node(CILNode::call(release, [sem]));
        let pop = asm.alloc_root(CILRoot::Pop(prev));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![pop, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_mutex_trylock(h: *mut u8) -> bool`
///   => recover the `SemaphoreSlim` from `h` and `Wait(0)` (non-blocking acquire).
///
/// `SemaphoreSlim.Wait(int millisecondsTimeout)` with `0` polls without blocking,
/// returning `true` iff the permit was taken — exactly the
/// `sys::sync::Mutex::try_lock` contract. Backs `sys::sync::Mutex::try_lock`.
fn insert_dotnet_mutex_trylock(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_mutex_trylock", |asm| {
        let sem = recover_semaphore(asm);
        let sem_class = ClassRef::semaphore_slim(asm);
        let wait = asm.alloc_string("Wait");
        let wait = asm.class_ref(sem_class).clone().instance(
            &[Type::Int(Int::I32)],
            Type::Bool,
            wait,
            asm,
        );
        let zero = asm.alloc_node(Const::I32(0));
        let entered = asm.alloc_node(CILNode::call(wait, [sem, zero]));
        let ret = asm.alloc_root(CILRoot::Ret(entered));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

// ===========================================================================
// Parker — the Class-D threading keystone (research: docs/THREADING_PAL_RESEARCH.md).
//
// One COUNTING `SemaphoreSlim(0, int.MaxValue)` per `Parker`, reached through an
// opaque `*mut u8` GCHandle, exactly mirroring the `rcl_dotnet_mutex_*` handle
// round-trip. A counting semaphore is the token-not-lost primitive *for free*
// (research §2): `unpark` Releases a permit, `park` Waits for one. A `Release`
// that happens BEFORE the matching `Wait` is not lost — the permit persists in
// the count, so the next `Wait` returns immediately. This needs NO manual reset
// and NO event-level Set/Reset race (the earlier ManualResetEventSlim variant
// deadlocked rayon: resetting a level-triggered event after consuming a wakeup
// raced a concurrent unpark and lost it). The std-side `Parker` (dotnet arm)
// keeps the EMPTY/PARKED/NOTIFIED atom only for the FAST PATH (an unpark-before-
// park is consumed by the atom without touching the semaphore at all), and
// touches the semaphore exactly once per real block/wake — so the permit count
// stays balanced and any rare extra permit is just a permitted spurious wakeup.
// ===========================================================================

/// `rcl_dotnet_park_new() -> *mut u8`
///   => `new SemaphoreSlim(0, int.MaxValue)`; return a `GCHandle` `IntPtr`.
///
/// `initialCount = 0` => a fresh `Parker` has no token, so the first `park()`
/// blocks until an `unpark()` Releases a permit. `maxCount = int.MaxValue` so a
/// burst of unparks never throws. The managed object is pinned in a `GCHandle`
/// and the handle's `IntPtr` is returned as `*mut u8`, mirroring
/// `rcl_dotnet_mutex_new`. The std side CAS-installs this handle on first use and
/// never frees it (one semaphore per live `Parker`).
fn insert_dotnet_park_new(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_park_new", |asm| {
        // new SemaphoreSlim(0, int.MaxValue)
        let sem = ClassRef::semaphore_slim(asm);
        let ctor = asm
            .class_ref(sem)
            .clone()
            .ctor(&[Type::Int(Int::I32), Type::Int(Int::I32)], asm);
        let zero = asm.alloc_node(Const::I32(0));
        let max = asm.alloc_node(Const::I32(i32::MAX));
        let sem_obj = asm.alloc_node(CILNode::call(ctor, [zero, max]));
        let store = asm.alloc_root(CILRoot::StLoc(0, sem_obj));

        // return (void*)GCHandle.Alloc(sem)
        let void = asm.alloc_type(Type::Void);
        let handle = CILNode::LdLoc(0).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let ret = asm.alloc_root(CILRoot::Ret(handle));

        let sem_ty = asm.alloc_type(Type::ClassRef(sem));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("sem")), sem_ty)],
        }
    });
}

/// `rcl_dotnet_park_wait(h: *mut u8)`
///   => recover the `SemaphoreSlim` from `h` and `Wait()` (block for a permit).
///
/// `SemaphoreSlim.Wait()` blocks until a permit is available, consuming it. A
/// permit deposited by an earlier `unpark` releases immediately (token-not-lost).
/// Backs the dotnet `Parker::park` blocking step.
fn insert_dotnet_park_wait(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_park_wait", |asm| {
        let sem = recover_semaphore(asm);
        let sem_class = ClassRef::semaphore_slim(asm);
        let wait = asm.alloc_string("Wait");
        let wait = asm
            .class_ref(sem_class)
            .clone()
            .instance(&[], Type::Void, wait, asm);
        let wait = asm.alloc_root(CILRoot::call(wait, [sem]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![wait, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_park_wait_timeout(h: *mut u8, millis: usize) -> bool`
///   => recover the semaphore from `h` and `Wait((int)millis)` (block up to ms).
///
/// `SemaphoreSlim.Wait(int millisecondsTimeout)` blocks up to the timeout,
/// returning `true` iff a permit was taken (vs. timing out). Backs the dotnet
/// `Parker::park_timeout`. The std side clamps long timeouts to `<= i32::MAX` ms,
/// so the truncation to the `int` overload is lossless.
fn insert_dotnet_park_wait_timeout(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_park_wait_timeout", |asm| {
        let sem = recover_semaphore(asm);
        let sem_class = ClassRef::semaphore_slim(asm);
        let millis = asm.alloc_node(CILNode::LdArg(1));
        let millis_i32 = asm.int_cast(millis, Int::I32, ExtendKind::ZeroExtend);
        let wait = asm.alloc_string("Wait");
        let wait = asm.class_ref(sem_class).clone().instance(
            &[Type::Int(Int::I32)],
            Type::Bool,
            wait,
            asm,
        );
        let taken = asm.alloc_node(CILNode::call(wait, [sem, millis_i32]));
        let ret = asm.alloc_root(CILRoot::Ret(taken));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_park_release(h: *mut u8)`
///   => recover the semaphore from `h` and `Release()` (deposit one wakeup permit).
///
/// `SemaphoreSlim.Release()` adds one permit, waking a blocked `Wait` or persisting
/// for a future one (this is what makes an `unpark` before `park` not lose the
/// token). The returned previous-count is popped. Backs the dotnet `Parker::unpark`.
fn insert_dotnet_park_release(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_park_release", |asm| {
        let sem = recover_semaphore(asm);
        let sem_class = ClassRef::semaphore_slim(asm);
        let release = asm.alloc_string("Release");
        let release =
            asm.class_ref(sem_class)
                .clone()
                .instance(&[], Type::Int(Int::I32), release, asm);
        let prev = asm.alloc_node(CILNode::call(release, [sem]));
        let pop = asm.alloc_root(CILRoot::Pop(prev));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![pop, ret], 0, None)],
            locals: vec![],
        }
    });
}

// ===========================================================================
// Condvar — a counting-semaphore condition variable (Class-D).
//
// `std::sync::Condvar` has no generic Parker-only arm in std (the generic ones
// are `futex` and `pthread`, both unavailable here). The dotnet `Condvar`
// (`dotnet_pal/sys/sync/condvar/dotnet.rs`) is a small bespoke arm built on a
// `System.Threading.SemaphoreSlim(0, int.MaxValue)` as a wakeup counter — the
// textbook semaphore condvar that composes correctly across multiple waiters
// (unlike a single shared ManualResetEventSlim, whose manual Reset races between
// waiters). `notify_*` Release N permits; `wait` Acquires one. A Release BEFORE
// the matching Wait is not lost (SemaphoreSlim counts permits), and an extra
// permit only ever causes a permitted spurious wakeup.
// ===========================================================================

/// `rcl_dotnet_condvar_new() -> *mut u8`
///   => `new SemaphoreSlim(0, int.MaxValue)`; return a `GCHandle` `IntPtr`.
///
/// `initialCount = 0` (no waiter may proceed until a notify), `maxCount =
/// int.MaxValue` (unbounded outstanding notifications). Mirrors
/// `rcl_dotnet_mutex_new`'s handle round-trip.
fn insert_dotnet_condvar_new(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_condvar_new", |asm| {
        // new SemaphoreSlim(0, int.MaxValue)
        let sem = ClassRef::semaphore_slim(asm);
        let ctor = asm
            .class_ref(sem)
            .clone()
            .ctor(&[Type::Int(Int::I32), Type::Int(Int::I32)], asm);
        let zero = asm.alloc_node(Const::I32(0));
        let max = asm.alloc_node(Const::I32(i32::MAX));
        let sem_obj = asm.alloc_node(CILNode::call(ctor, [zero, max]));
        let store = asm.alloc_root(CILRoot::StLoc(0, sem_obj));

        // return (void*)GCHandle.Alloc(sem)
        let void = asm.alloc_type(Type::Void);
        let handle = CILNode::LdLoc(0).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let ret = asm.alloc_root(CILRoot::Ret(handle));

        let sem_ty = asm.alloc_type(Type::ClassRef(sem));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("sem")), sem_ty)],
        }
    });
}

/// `rcl_dotnet_condvar_wait(h: *mut u8)`
///   => recover the `SemaphoreSlim` from `h` and `Wait()` (block for a permit).
///
/// Backs `Condvar::wait`'s blocking step (the std side unlocks the mutex first,
/// blocks here, then relocks). A permit deposited by an earlier `notify` releases
/// immediately (token-not-lost).
fn insert_dotnet_condvar_wait(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_condvar_wait", |asm| {
        let sem = recover_semaphore(asm);
        let sem_class = ClassRef::semaphore_slim(asm);
        let wait = asm.alloc_string("Wait");
        let wait = asm
            .class_ref(sem_class)
            .clone()
            .instance(&[], Type::Void, wait, asm);
        let wait = asm.alloc_root(CILRoot::call(wait, [sem]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![wait, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_condvar_wait_timeout(h: *mut u8, millis: usize) -> bool`
///   => recover the `SemaphoreSlim` and `Wait((int)millis)` (block up to ms).
///
/// Returns `true` iff a permit was taken (vs. timing out). Backs
/// `Condvar::wait_timeout`. The std side clamps long timeouts, so truncation to
/// the `int` overload is lossless.
fn insert_dotnet_condvar_wait_timeout(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_condvar_wait_timeout", |asm| {
        let sem = recover_semaphore(asm);
        let sem_class = ClassRef::semaphore_slim(asm);
        let millis = asm.alloc_node(CILNode::LdArg(1));
        let millis_i32 = asm.int_cast(millis, Int::I32, ExtendKind::ZeroExtend);
        let wait = asm.alloc_string("Wait");
        let wait = asm.class_ref(sem_class).clone().instance(
            &[Type::Int(Int::I32)],
            Type::Bool,
            wait,
            asm,
        );
        let taken = asm.alloc_node(CILNode::call(wait, [sem, millis_i32]));
        let ret = asm.alloc_root(CILRoot::Ret(taken));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_condvar_release(h: *mut u8, n: usize)`
///   => recover the `SemaphoreSlim` and `Release((int)n)` if `n > 0` (deposit
///      `n` wakeup permits).
///
/// Backs `Condvar::notify_one` (n=1) and `notify_all` (n=#waiters).
/// `SemaphoreSlim.Release(int)` throws on `0`, so the `n == 0` case is skipped
/// (no waiter to wake). The returned previous-count is popped.
fn insert_dotnet_condvar_release(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_condvar_release", |asm| {
        // Block 0: if ((int)n == 0) goto ret(2) else goto release(1).
        let n = asm.alloc_node(CILNode::LdArg(1));
        let n_i32 = asm.int_cast(n, Int::I32, ExtendKind::ZeroExtend);
        let zero = asm.alloc_node(Const::I32(0));
        let br_ret = asm.alloc_root(CILRoot::Branch(Box::new((
            2,
            0,
            Some(BranchCond::Eq(n_i32, zero)),
        ))));
        let goto_release = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));
        let entry = BasicBlock::new(vec![br_ret, goto_release], 0, None);

        // Block 1: sem.Release((int)n); pop; goto ret(2).
        let sem = recover_semaphore(asm);
        let sem_class = ClassRef::semaphore_slim(asm);
        let n2 = asm.alloc_node(CILNode::LdArg(1));
        let n2_i32 = asm.int_cast(n2, Int::I32, ExtendKind::ZeroExtend);
        let release = asm.alloc_string("Release");
        let release = asm.class_ref(sem_class).clone().instance(
            &[Type::Int(Int::I32)],
            Type::Int(Int::I32),
            release,
            asm,
        );
        let prev = asm.alloc_node(CILNode::call(release, [sem, n2_i32]));
        let pop = asm.alloc_root(CILRoot::Pop(prev));
        let goto_ret = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));
        let rel_blk = BasicBlock::new(vec![pop, goto_ret], 1, None);

        // Block 2: ret.
        let ret = asm.alloc_root(CILRoot::VoidRet);
        let ret_blk = BasicBlock::new(vec![ret], 2, None);

        MethodImpl::MethodBody {
            blocks: vec![entry, rel_blk, ret_blk],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_tls_create() -> *mut u8`
///   => `new ThreadLocal<nint>()`; return a `GCHandle` `IntPtr` pinning it.
///
/// Slice 2 — REAL per-thread thread-local storage. Each `thread_local!` TLS key
/// is one managed `System.Threading.ThreadLocal<IntPtr>` whose `.Value` is
/// per-thread BY CONSTRUCTION (no `ManagedThreadId` composite key needed). The
/// object is pinned in a `GCHandle` and the handle's `IntPtr` is returned as
/// `*mut u8` — the opaque "key" the std side stores. Mirrors
/// `rcl_dotnet_mutex_new`'s handle round-trip (`ref_to_handle` => `GCHandle.Alloc`).
/// The key lives for the program's lifetime (no Free binding in this slice; one
/// `ThreadLocal` per live TLS key, collected at process exit).
fn insert_dotnet_tls_create(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_tls_create", |asm| {
        // new ThreadLocal<nint>()  (the parameterless ctor; default Value is 0)
        let tl = ClassRef::thread_local(asm, Type::Int(Int::ISize));
        let ctor = asm.class_ref(tl).clone().ctor(&[], asm);
        let tl_obj = asm.alloc_node(CILNode::call(ctor, []));
        let store = asm.alloc_root(CILRoot::StLoc(0, tl_obj));

        // return (void*)GCHandle.Alloc(threadLocal)
        let void = asm.alloc_type(Type::Void);
        let handle = CILNode::LdLoc(0).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let ret = asm.alloc_root(CILRoot::Ret(handle));

        let tl_ty = asm.alloc_type(Type::ClassRef(tl));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("tl")), tl_ty)],
        }
    });
}

/// Recover the `ThreadLocal<nint>` from a `GCHandle` `IntPtr` (arg0, `*mut u8`):
/// `handle_to_obj((nint)h)` then `(ThreadLocal<nint>)obj`. Mirrors
/// `recover_semaphore`. Returns the casted node.
fn recover_thread_local(asm: &mut Assembly) -> Interned<CILNode> {
    let arg0 = asm.alloc_node(CILNode::LdArg(0));
    let handle_isize = asm.alloc_node(CILNode::PtrCast(arg0, Box::new(PtrCastRes::ISize)));
    let handle_to_obj = asm.alloc_string("handle_to_obj");
    let main_module = asm.main_module();
    let handle_to_obj = asm.class_ref(*main_module).clone().static_mref(
        &[Type::Int(Int::ISize)],
        Type::PlatformObject,
        handle_to_obj,
        asm,
    );
    let obj = asm.alloc_node(CILNode::call(handle_to_obj, [handle_isize]));
    let tl = ClassRef::thread_local(asm, Type::Int(Int::ISize));
    let tl_ty = asm.alloc_type(Type::ClassRef(tl));
    asm.alloc_node(CILNode::CheckedCast(obj, tl_ty))
}

/// `rcl_dotnet_tls_get(key: *mut u8) -> *mut u8`
///   => recover the `ThreadLocal<nint>` from `key` and return `key.Value` (an
///      `nint`) reinterpreted as `*mut u8`.
///
/// `ThreadLocal<T>.get_Value()` is the instance property getter; its result is
/// the CALLING THREAD's slot (per-thread by construction). Backs the dotnet PAL
/// `key::get`.
fn insert_dotnet_tls_get(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_tls_get", |mref, asm| {
        let tl = recover_thread_local(asm);
        let tl_class = ClassRef::thread_local(asm, Type::Int(Int::ISize));
        let get_value = asm.alloc_string("get_Value");
        // `ThreadLocal<T>.get_Value()` returns the CLASS generic `!0` (here `nint`),
        // NOT a concrete `IntPtr` — the CLR matches the property's exact generic
        // signature, so a literal `IntPtr` return yields a runtime
        // MissingMethodException. Use `PlatformGeneric(0)` (the class generic),
        // mirroring how `ConcurrentDictionary<K,V>.get_Item` is referenced.
        let get_value = asm.class_ref(tl_class).clone().instance(
            &[],
            Type::PlatformGeneric(0, GenericKind::TypeGeneric),
            get_value,
            asm,
        );
        let val = asm.alloc_node(CILNode::call(get_value, [tl]));
        // Materialize the bound class generic into its concrete `nint` local before converting it
        // to a native pointer. A PtrCast directly from the open `!0` marker is ill-typed even though
        // this ThreadLocal instantiation binds `!0 = nint`.
        let store = asm.alloc_root(CILRoot::StLoc(0, val));
        let val = asm.alloc_node(CILNode::LdLoc(0));
        let val = super::adapt_runtime_result(mref, val, Type::Int(Int::ISize), asm);
        let ret = asm.alloc_root(CILRoot::Ret(val));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store, ret], 0, None)],
            locals: vec![(
                Some(asm.alloc_string("value")),
                asm.alloc_type(Type::Int(Int::ISize)),
            )],
        }
    });
}

/// `rcl_dotnet_tls_set(key: *mut u8, val: *mut u8)`
///   => recover the `ThreadLocal<nint>` from `key` and `key.Value = (nint)val`.
///
/// `ThreadLocal<T>.set_Value(T)` is the instance property setter; it writes the
/// CALLING THREAD's slot only. Backs the dotnet PAL `key::set`.
fn insert_dotnet_tls_set(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_tls_set", |asm| {
        let tl = recover_thread_local(asm);
        let tl_class = ClassRef::thread_local(asm, Type::Int(Int::ISize));
        let set_value = asm.alloc_string("set_Value");
        // `ThreadLocal<T>.set_Value(T)` takes the CLASS generic `!0` (here `nint`),
        // not a concrete `IntPtr` — same exact-signature-match reason as the getter.
        let set_value = asm.class_ref(tl_class).clone().instance(
            &[Type::PlatformGeneric(0, GenericKind::TypeGeneric)],
            Type::Void,
            set_value,
            asm,
        );
        // (nint)val  (arg1 is the *mut u8 value to store)
        let val = asm.alloc_node(CILNode::LdArg(1));
        let val = asm.alloc_node(CILNode::PtrCast(val, Box::new(PtrCastRes::ISize)));
        let set = asm.alloc_root(CILRoot::call(set_value, [tl, val]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![set, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_available_parallelism() -> usize` => `System.Environment.ProcessorCount`.
///
/// `ProcessorCount` is an `int` static getter (`get_ProcessorCount`); we
/// zero-extend it to `usize` for the symbol's return type. The std side wraps
/// the result in a `NonZero<usize>`, clamping a (spec-impossible) zero to 1.
fn insert_dotnet_available_parallelism(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_available_parallelism", |asm| {
        let env = ClassRef::enviroment(asm);
        let get_count = asm.alloc_string("get_ProcessorCount");
        let get_count =
            asm.class_ref(env)
                .clone()
                .static_mref(&[], Type::Int(Int::I32), get_count, asm);
        let count = asm.alloc_node(CILNode::call(get_count, []));
        let count = asm.int_cast(count, Int::USize, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(count));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_getpid() -> u32` => `System.Environment.ProcessId`.
///
/// Backs `sys::process::getpid` on the dotnet PAL (a genuine process id, unlike
/// `spawn`'s synthetic-pid wall). `ProcessId` is an `int` static getter
/// (`get_ProcessId`); the symbol's return type is `u32`, so the BCL boundary must
/// explicitly retag the same-width stack value for cilly's signedness-aware IR.
fn dotnet_process_id_u32(asm: &mut Assembly) -> Interned<CILNode> {
    let env = ClassRef::enviroment(asm);
    let get_process_id_name = asm.alloc_string("get_ProcessId");
    let get_process_id =
        asm.class_ref(env)
            .clone()
            .static_mref(&[], Type::Int(Int::I32), get_process_id_name, asm);
    let process_id = asm.alloc_node(CILNode::call(get_process_id, []));
    asm.int_cast(process_id, Int::U32, ExtendKind::ZeroExtend)
}

fn insert_dotnet_getpid(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_getpid", |asm| {
        let process_id = dotnet_process_id_u32(asm);
        let ret = asm.alloc_root(CILRoot::Ret(process_id));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

#[cfg(test)]
mod process_boundary_tests {
    use super::*;

    #[test]
    fn process_id_is_retyped_to_the_rust_u32_contract() {
        let mut asm = Assembly::default();
        let process_id = dotnet_process_id_u32(&mut asm);
        let sig = asm.sig([], Type::Void);

        assert_eq!(
            asm[process_id]
                .clone()
                .typecheck(sig, &[], &mut asm)
                .unwrap(),
            Type::Int(Int::U32),
        );
    }
}

/// `exit(code: c_int) -> !` => `System.Environment.Exit((int)code)`.
///
/// B2 Piece 5 — closes the host-libc `exit` P/Invoke leak (L9). std's
/// `sys::exit::exit` calls `libc::exit(code)` on the `target_family="unix"` arm;
/// the bare symbol `exit` is in `LIBC_FNS`, so absent an override it resolves to
/// a host-libc P/Invoke (green on a Linux test host, but a LEAK on a shipped
/// non-Linux .NET host). Registering an override under the demangled last
/// `::`-segment **`exit`** (this name, NOT `rcl_dotnet_exit`) intercepts it at
/// link time: `patch_missing_methods` looks up `override_methods` BEFORE the
/// LIBC externs fallback, so the override wins. `Environment.Exit(int)`
/// terminates the process; since libc `exit` is `-> !` (noreturn) the CIL body
/// must still typecheck, so we emit an unreachable `Ret` of an uninitialised
/// value of the method's declared return type after the (never-returning) call.
fn insert_dotnet_exit(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    // The generator body is shared by two override keys (below). It emits
    // `System.Environment.Exit((int)arg0)` then an unreachable terminator typed
    // to the call site's declared return.
    fn gen_exit(mref: Interned<MethodRef>, asm: &mut Assembly) -> MethodImpl {
        // Environment.Exit((int)arg0) — static void Exit(int).
        let env = ClassRef::enviroment(asm);
        let exit_name = asm.alloc_string("Exit");
        let exit = asm.class_ref(env).clone().static_mref(
            &[Type::Int(Int::I32)],
            Type::Void,
            exit_name,
            asm,
        );
        let code = asm.alloc_node(CILNode::LdArg(0));
        let code = asm.int_cast(code, Int::I32, ExtendKind::SignExtend);
        let call = asm.alloc_root(CILRoot::call(exit, [code]));
        // Unreachable terminator after the (never-returning) Environment.Exit.
        // libc `exit` is `-> !`; rustc lowers the never type to either `Void`
        // (a unit-like never callee — most call sites, incl. a raw `extern "C" fn
        // exit(_) -> !`) or a concrete scalar. A void-typed `Ret(value)` is
        // INVALID CIL (the JIT rejects the whole method with
        // InvalidProgramException), so emit `VoidRet` for Void and `Ret(uninit)`
        // otherwise.
        let ret_ty = *asm[asm[mref].sig()].output();
        let ret = if ret_ty == Type::Void {
            asm.alloc_root(CILRoot::VoidRet)
        } else {
            let unreachable = asm.uninit_val(ret_ty);
            asm.alloc_root(CILRoot::Ret(unreachable))
        };
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![call, ret], 0, None)],
            locals: vec![],
        }
    }
    // Key 1: bare `exit` — intercepts the host-libc `exit` P/Invoke on the
    // `target_family="unix"` arm of `sys::process` / non-dotnet exit paths.
    let exit = asm.alloc_string("exit");
    patcher.insert(exit, Box::new(gen_exit));
    // Key 2: `rcl_dotnet_exit` — the dedicated symbol the std dotnet `sys::exit`
    // arm calls. std's in-tree `libc` shim does NOT declare `exit`, so the dotnet
    // arm cannot call `libc::exit` (E0425); instead it declares + calls
    // `rcl_dotnet_exit(code)`, which this override maps to `Environment.Exit(code)`
    // for a CLEAN process-exit WITH the code (matching native rustc's `exit(7)`).
    // P2-S2 differential-oracle fix: the arm previously dropped the code and called
    // `intrinsics::abort()` ("Called abort!", exit 134) — a real behavioral
    // divergence. Verified byte-identical vs native by cargo_tests/pal_exit_code.
    let rcl = asm.alloc_string("rcl_dotnet_exit");
    patcher.insert(rcl, Box::new(gen_exit));
}

/// `rcl_dotnet_hostname() -> *mut u8` =>
///   `Marshal.StringToCoTaskMemUTF8(System.Environment.MachineName)`.
///
/// Backs `sys::net::hostname` on the dotnet PAL. `MachineName` is a `string`
/// static getter (`get_MachineName`); we marshal it to a freshly-allocated,
/// NUL-terminated UTF-8 C string (COM-task-memory heap) the std side reads with
/// `CStr` and frees with `rcl_dotnet_cotaskmem_free` — mirroring the env getter.
fn insert_dotnet_hostname(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_hostname", |asm| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        // s = Environment.MachineName (a non-null string).
        let env = ClassRef::enviroment(asm);
        let get_name = asm.alloc_string("get_MachineName");
        let get_name =
            asm.class_ref(env)
                .clone()
                .static_mref(&[], Type::PlatformString, get_name, asm);
        let s = asm.alloc_node(CILNode::call(get_name, []));
        // buf = (u8*)StringToCoTaskMemUTF8(s).
        let to_utf8 = string_to_utf8(asm);
        let buf = asm.alloc_node(CILNode::call(to_utf8, [s]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret = asm.alloc_root(CILRoot::Ret(buf));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// PACKAGE A — the four `sys::paths` hooks the `target-family="unix"` flip
/// requires (`dotnet_pal/sys/paths/dotnet.rs`). Each `string`-returning hook
/// marshals via `Marshal.StringToCoTaskMemUTF8` (NUL-terminated UTF-8, freed by
/// `rcl_dotnet_cotaskmem_free`), exactly like `hostname`/`getenv`:
/// * `rcl_dotnet_paths_getcwd()       -> *mut u8`  => `Directory.GetCurrentDirectory()`
/// * `rcl_dotnet_paths_current_exe()  -> *mut u8`  => `Environment.ProcessPath` (may be null)
/// * `rcl_dotnet_paths_chdir(ptr,len) -> i32`      => `Directory.SetCurrentDirectory(s)` (0 ok)
/// * `rcl_dotnet_paths_temp_dir()     -> *mut u8`  => `Path.GetTempPath()`
fn insert_dotnet_paths(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    // Helper: build a `() -> *mut u8` body that marshals a static string getter.
    // `class`/`getter_name`/`sig_ret` describe the BCL static `string` getter.

    // ---- rcl_dotnet_paths_getcwd() -> *mut u8 (Directory.GetCurrentDirectory) ----
    let getcwd_name = asm.alloc_string("rcl_dotnet_paths_getcwd");
    let getcwd_gen = move |_, asm: &mut Assembly| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        let dir = ClassRef::directory(asm);
        let get_cwd = asm.alloc_string("GetCurrentDirectory");
        let get_cwd =
            asm.class_ref(dir)
                .clone()
                .static_mref(&[], Type::PlatformString, get_cwd, asm);
        let s = asm.alloc_node(CILNode::call(get_cwd, []));
        let to_utf8 = string_to_utf8(asm);
        let buf = asm.alloc_node(CILNode::call(to_utf8, [s]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret = asm.alloc_root(CILRoot::Ret(buf));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(getcwd_name, Box::new(getcwd_gen));

    // ---- rcl_dotnet_paths_temp_dir() -> *mut u8 (Path.GetTempPath) ----
    let temp_name = asm.alloc_string("rcl_dotnet_paths_temp_dir");
    let temp_gen = move |_, asm: &mut Assembly| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        let path = ClassRef::path_io(asm);
        let get_temp = asm.alloc_string("GetTempPath");
        let get_temp =
            asm.class_ref(path)
                .clone()
                .static_mref(&[], Type::PlatformString, get_temp, asm);
        let s = asm.alloc_node(CILNode::call(get_temp, []));
        let to_utf8 = string_to_utf8(asm);
        let buf = asm.alloc_node(CILNode::call(to_utf8, [s]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret = asm.alloc_root(CILRoot::Ret(buf));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(temp_name, Box::new(temp_gen));

    // ---- rcl_dotnet_paths_current_exe() -> *mut u8 (Environment.ProcessPath) ----
    // `ProcessPath` may be null; mirror getenv's null->(u8*)0 path.
    let exe_name = asm.alloc_string("rcl_dotnet_paths_current_exe");
    let exe_gen = move |_, asm: &mut Assembly| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        let string_class = ClassRef::string(asm);
        let env = ClassRef::enviroment(asm);
        let get_path = asm.alloc_string("get_ProcessPath");
        let get_path =
            asm.class_ref(env)
                .clone()
                .static_mref(&[], Type::PlatformString, get_path, asm);
        let s = asm.alloc_node(CILNode::call(get_path, []));
        let store_s = asm.alloc_root(CILRoot::StLoc(0, s));
        // Block 0: if (s == null) goto 1 else goto 2.
        let s_load = asm.alloc_node(CILNode::LdLoc(0));
        let null_str = asm.alloc_node(CILNode::Const(Box::new(Const::Null(string_class))));
        let br_null = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(s_load, null_str)),
        ))));
        let goto_marshal = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));
        // Block 1: return (u8*)0.
        let zero = asm.alloc_node(0_i32);
        let zero = asm.int_cast(zero, Int::ISize, ExtendKind::ZeroExtend);
        let null_ptr = asm.cast_ptr(zero, u8_ptr);
        let ret_null = asm.alloc_root(CILRoot::Ret(null_ptr));
        // Block 2: return (u8*)StringToCoTaskMemUTF8(s).
        let to_utf8 = string_to_utf8(asm);
        let s_load2 = asm.alloc_node(CILNode::LdLoc(0));
        let buf = asm.alloc_node(CILNode::call(to_utf8, [s_load2]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret_buf = asm.alloc_root(CILRoot::Ret(buf));
        let string_ty = asm.alloc_type(Type::PlatformString);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_s, br_null, goto_marshal], 0, None),
                BasicBlock::new(vec![ret_null], 1, None),
                BasicBlock::new(vec![ret_buf], 2, None),
            ],
            locals: vec![(Some(asm.alloc_string("exe_path")), string_ty)],
        }
    };
    patcher.insert(exe_name, Box::new(exe_gen));

    // ---- rcl_dotnet_paths_chdir(ptr, len) -> i32 (Directory.SetCurrentDirectory) ----
    // Returns 0 (the BCL method throws on failure; for compile-floor correctness a
    // plain 0-return after the call is sufficient — std maps any uncaught managed
    // exception to its own error path; runtime hardening is deferred per scope).
    let chdir_name = asm.alloc_string("rcl_dotnet_paths_chdir");
    let chdir_gen = move |_, asm: &mut Assembly| {
        let path = decode_utf8(asm, 0, 1);
        let dir = ClassRef::directory(asm);
        let set_cwd = asm.alloc_string("SetCurrentDirectory");
        let set_cwd_sig = asm.sig([Type::PlatformString], Type::Void);
        let set_cwd = asm.alloc_methodref(MethodRef::new(
            dir,
            set_cwd,
            set_cwd_sig,
            MethodKind::Static,
            [].into(),
        ));
        let call = asm.alloc_root(CILRoot::call(set_cwd, [path]));
        let zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![call, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(chdir_name, Box::new(chdir_gen));
}

// ===========================================================================
// args + env (`sys/args/dotnet.rs`, `sys/env/dotnet.rs`)
//
// The PAL's args/env arms marshal managed `string`s by asking the BCL to hand
// back NUL-terminated UTF-8 C strings via `Marshal.StringToCoTaskMemUTF8`, then
// reading them with `CStr` and freeing them with `rcl_dotnet_cotaskmem_free`.
// This mirrors the C-entry-shim `argc_argv_init`/`get_environ` builtins (which
// build a `char**` the unix std reads), but exposes per-value getters the
// non-unix dotnet PAL consumes directly — so no `target_os="dotnet"` arm is
// needed in std's `args/mod.rs`/`env/mod.rs` `common` gating.
// ===========================================================================

/// Returns a methodref to `System.Environment.GetCommandLineArgs() -> string[]`.
fn get_command_line_args(asm: &mut Assembly) -> Interned<MethodRef> {
    let string = asm.alloc_type(Type::PlatformString);
    let string_arr = Type::PlatformArray {
        elem: string,
        dims: NonZeroU8::new(1).unwrap(),
    };
    let env = ClassRef::enviroment(asm);
    let name = asm.alloc_string("GetCommandLineArgs");
    let sig = asm.sig([], string_arr);
    asm.alloc_methodref(MethodRef::new(
        env,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ))
}

/// Returns a methodref to `Marshal.StringToCoTaskMemUTF8(string) -> IntPtr`.
///
/// The `IntPtr` it returns points at a freshly-allocated, NUL-terminated UTF-8
/// buffer (the COM-task-memory heap); the caller frees it with `FreeCoTaskMem`
/// (our `rcl_dotnet_cotaskmem_free`). Same call as `utilis::mstring_to_utf8ptr`.
fn string_to_utf8(asm: &mut Assembly) -> Interned<MethodRef> {
    let marshal = ClassRef::marshal(asm);
    let name = asm.alloc_string("StringToCoTaskMemUTF8");
    let sig = asm.sig([Type::PlatformString], Type::Int(Int::ISize));
    asm.alloc_methodref(MethodRef::new(
        marshal,
        name,
        sig,
        MethodKind::Static,
        [].into(),
    ))
}

/// Decode a `(ptr, len)` UTF-8 byte buffer (at the given `LdArg` indices) into a
/// managed `string`, via `Encoding.UTF8.GetString(byte*, int)`. Returns the
/// node holding the decoded string. Mirrors the decode half of
/// `insert_dotnet_write`.
pub(crate) fn decode_utf8(asm: &mut Assembly, ptr_arg: u32, len_arg: u32) -> Interned<CILNode> {
    let u8_ptr = asm.nptr(Type::Int(Int::U8));
    let encoding = {
        let name = asm.alloc_string("System.Text.Encoding");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, false, [].into()))
    };
    let encoding_ty = Type::ClassRef(encoding);
    // Encoding.UTF8 -> Encoding (static property getter).
    let get_utf8_name = asm.alloc_string("get_UTF8");
    let get_utf8_sig = asm.sig([], encoding_ty);
    let get_utf8 = asm.alloc_methodref(MethodRef::new(
        encoding,
        get_utf8_name,
        get_utf8_sig,
        MethodKind::Static,
        [].into(),
    ));
    let utf8 = asm.alloc_node(CILNode::call(get_utf8, []));
    // (byte*)ptr, (int32)len.
    let ptr = asm.alloc_node(CILNode::LdArg(ptr_arg));
    let ptr = asm.cast_ptr(ptr, u8_ptr);
    let len = asm.alloc_node(CILNode::LdArg(len_arg));
    let len_i32 = asm.int_cast(len, Int::I32, ExtendKind::ZeroExtend);
    // Encoding.GetString(byte* bytes, int byteCount) -> string (instance).
    let get_string_name = asm.alloc_string("GetString");
    let get_string_sig = asm.sig(
        [encoding_ty, u8_ptr, Type::Int(Int::I32)],
        Type::PlatformString,
    );
    let get_string = asm.alloc_methodref(MethodRef::new(
        encoding,
        get_string_name,
        get_string_sig,
        MethodKind::Instance,
        [].into(),
    ));
    asm.alloc_node(CILNode::call(get_string, [utf8, ptr, len_i32]))
}

/// `rcl_dotnet_cotaskmem_free(ptr: *mut u8)`
///   => `Marshal.FreeCoTaskMem((IntPtr)ptr)`.
///
/// Frees a buffer produced by `StringToCoTaskMemUTF8` (i.e. one returned from
/// `rcl_dotnet_arg` / `rcl_dotnet_getenv`). `FreeCoTaskMem` takes an `IntPtr`,
/// so the `*mut u8` argument is reinterpreted as an `isize` (native int).
fn insert_dotnet_cotaskmem_free(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_cotaskmem_free", |asm| {
        // (IntPtr)ptr — a pointer and a native int share a representation.
        let ptr = asm.alloc_node(CILNode::LdArg(0));
        let ptr = asm.int_cast(ptr, Int::ISize, ExtendKind::ZeroExtend);
        let marshal = ClassRef::marshal(asm);
        let free_name = asm.alloc_string("FreeCoTaskMem");
        let free_sig = asm.sig([Type::Int(Int::ISize)], Type::Void);
        let free = asm.alloc_methodref(MethodRef::new(
            marshal,
            free_name,
            free_sig,
            MethodKind::Static,
            [].into(),
        ));
        let call = asm.alloc_root(CILRoot::call(free, [ptr]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![call, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// Registers `rcl_dotnet_args_count` and `rcl_dotnet_arg`.
///
/// * `rcl_dotnet_args_count() -> usize`
///   => `(usize)Environment.GetCommandLineArgs().Length`.
/// * `rcl_dotnet_arg(idx: usize) -> *mut u8`
///   => `Marshal.StringToCoTaskMemUTF8(Environment.GetCommandLineArgs()[idx])`.
///
/// Two calls to `GetCommandLineArgs()` (the BCL returns a fresh copy each time);
/// `rcl_dotnet_arg` indexes element `idx` of that array with `ldelem.ref` and
/// marshals it to a NUL-terminated UTF-8 buffer. The std side reads the buffer
/// with `CStr` and frees it via `rcl_dotnet_cotaskmem_free`.
fn insert_dotnet_args(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    // ---- rcl_dotnet_args_count() -> usize ----
    let count_name = asm.alloc_string("rcl_dotnet_args_count");
    let count_gen = move |_, asm: &mut Assembly| {
        let get_args = get_command_line_args(asm);
        let args = asm.alloc_node(CILNode::call(get_args, []));
        // ldlen yields a native uint; widen to usize.
        let len = asm.ld_len(args);
        let len = asm.int_cast(len, Int::USize, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(len));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(count_name, Box::new(count_gen));

    // ---- rcl_dotnet_arg(idx: usize) -> *mut u8 ----
    let arg_name = asm.alloc_string("rcl_dotnet_arg");
    let arg_gen = move |_, asm: &mut Assembly| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        let get_args = get_command_line_args(asm);
        let args = asm.alloc_node(CILNode::call(get_args, []));
        // args[idx] (idx is usize; ldelem.ref accepts a native-int index).
        let idx = asm.alloc_node(CILNode::LdArg(0));
        let arg_str = asm.ld_elem_ref(args, idx);
        // Marshal.StringToCoTaskMemUTF8(arg) -> IntPtr; reinterpret as *mut u8.
        let to_utf8 = string_to_utf8(asm);
        let buf = asm.alloc_node(CILNode::call(to_utf8, [arg_str]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret = asm.alloc_root(CILRoot::Ret(buf));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(arg_name, Box::new(arg_gen));
}

/// Registers `rcl_dotnet_getenv`, `rcl_dotnet_setenv`, `rcl_dotnet_unsetenv`.
///
/// * `rcl_dotnet_getenv(key_ptr, key_len) -> *mut u8`
///   => `var s = Environment.GetEnvironmentVariable(<key>);`
///      `return s == null ? null : Marshal.StringToCoTaskMemUTF8(s);`
/// * `rcl_dotnet_setenv(key_ptr, key_len, val_ptr, val_len)`
///   => `Environment.SetEnvironmentVariable(<key>, <val>)`.
/// * `rcl_dotnet_unsetenv(key_ptr, key_len)`
///   => `Environment.SetEnvironmentVariable(<key>, null)`.
///
/// Keys/values arrive as `(ptr, len)` UTF-8 buffers, decoded to managed strings
/// with `Encoding.UTF8.GetString` ([`decode_utf8`]). A `getenv` miss returns a
/// null `*mut u8` (the managed `GetEnvironmentVariable` returns a null
/// `string`), which the std side maps to `None`.
fn insert_dotnet_env(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    // ---- rcl_dotnet_getenv(key_ptr, key_len) -> *mut u8 ----
    let getenv_name = asm.alloc_string("rcl_dotnet_getenv");
    let getenv_gen = move |_, asm: &mut Assembly| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        let string_class = ClassRef::string(asm);
        // key = Encoding.UTF8.GetString(key_ptr, key_len)
        let key = decode_utf8(asm, 0, 1);
        // s = Environment.GetEnvironmentVariable(key)
        let env = ClassRef::enviroment(asm);
        let get_var_name = asm.alloc_string("GetEnvironmentVariable");
        let get_var_sig = asm.sig([Type::PlatformString], Type::PlatformString);
        let get_var = asm.alloc_methodref(MethodRef::new(
            env,
            get_var_name,
            get_var_sig,
            MethodKind::Static,
            [].into(),
        ));
        let s = asm.alloc_node(CILNode::call(get_var, [key]));
        let store_s = asm.alloc_root(CILRoot::StLoc(0, s));

        // Block 0: if (s == null) goto null_ret(1) else goto marshal(2).
        let s_load = asm.alloc_node(CILNode::LdLoc(0));
        let null_str = asm.alloc_node(CILNode::Const(Box::new(Const::Null(string_class))));
        let br_null = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(s_load, null_str)),
        ))));
        let goto_marshal = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // Block 1 (null_ret): return (u8*)0.
        let zero = asm.alloc_node(0_i32);
        let zero = asm.int_cast(zero, Int::ISize, ExtendKind::ZeroExtend);
        let null_ptr = asm.cast_ptr(zero, u8_ptr);
        let ret_null = asm.alloc_root(CILRoot::Ret(null_ptr));

        // Block 2 (marshal): return (u8*)StringToCoTaskMemUTF8(s).
        let to_utf8 = string_to_utf8(asm);
        let s_load2 = asm.alloc_node(CILNode::LdLoc(0));
        let buf = asm.alloc_node(CILNode::call(to_utf8, [s_load2]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret_buf = asm.alloc_root(CILRoot::Ret(buf));

        let string_ty = asm.alloc_type(Type::PlatformString);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_s, br_null, goto_marshal], 0, None),
                BasicBlock::new(vec![ret_null], 1, None),
                BasicBlock::new(vec![ret_buf], 2, None),
            ],
            locals: vec![(Some(asm.alloc_string("env_val")), string_ty)],
        }
    };
    patcher.insert(getenv_name, Box::new(getenv_gen));

    // ---- rcl_dotnet_setenv(key_ptr, key_len, val_ptr, val_len) ----
    let setenv_name = asm.alloc_string("rcl_dotnet_setenv");
    let setenv_gen = move |_, asm: &mut Assembly| {
        let key = decode_utf8(asm, 0, 1);
        let val = decode_utf8(asm, 2, 3);
        let env = ClassRef::enviroment(asm);
        let set_var_name = asm.alloc_string("SetEnvironmentVariable");
        let set_var_sig = asm.sig([Type::PlatformString, Type::PlatformString], Type::Void);
        let set_var = asm.alloc_methodref(MethodRef::new(
            env,
            set_var_name,
            set_var_sig,
            MethodKind::Static,
            [].into(),
        ));
        let call = asm.alloc_root(CILRoot::call(set_var, [key, val]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![call, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(setenv_name, Box::new(setenv_gen));

    // ---- rcl_dotnet_unsetenv(key_ptr, key_len) ----
    let unsetenv_name = asm.alloc_string("rcl_dotnet_unsetenv");
    let unsetenv_gen = move |_, asm: &mut Assembly| {
        let string_class = ClassRef::string(asm);
        let key = decode_utf8(asm, 0, 1);
        // SetEnvironmentVariable(key, null) deletes the variable.
        let null_str = asm.alloc_node(CILNode::Const(Box::new(Const::Null(string_class))));
        let env = ClassRef::enviroment(asm);
        let set_var_name = asm.alloc_string("SetEnvironmentVariable");
        let set_var_sig = asm.sig([Type::PlatformString, Type::PlatformString], Type::Void);
        let set_var = asm.alloc_methodref(MethodRef::new(
            env,
            set_var_name,
            set_var_sig,
            MethodKind::Static,
            [].into(),
        ));
        let call = asm.alloc_root(CILRoot::call(set_var, [key, null_str]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![call, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(unsetenv_name, Box::new(unsetenv_gen));

    // ---- rcl_dotnet_environ() -> *mut u8 (Environment.GetEnvironmentVariables) ----
    // Enumerate the whole process environment into a freshly-allocated,
    // NUL-terminated UTF-8 buffer of `KEY=VALUE\n` lines (the caller copies it out
    // and frees with `rcl_dotnet_cotaskmem_free`). Backs `std::env::vars()`/`vars_os()`
    // on the .NET PAL, which previously fell through to the panicking `unsupported`
    // arm. Iterates the `IDictionary` returned by `Environment.GetEnvironmentVariables`
    // with the same `IDictionaryEnumerator` walk used by the epoll shim
    // (`posix_epoll.rs`). NOTE: lines are `\n`-separated; an environment *value*
    // containing a literal newline would split wrong (keys cannot contain `=`/newlines,
    // and no value can contain `\0` since the OS environ is NUL-separated). Newlines in
    // values are pathological and never produced by real environments.
    let environ_name = asm.alloc_string("rcl_dotnet_environ");
    let environ_gen = move |_, asm: &mut Assembly| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        let i_dictionary = ClassRef::i_dictionary(asm);
        let i_enumerator = ClassRef::i_enumerator(asm);
        let dict_iter = ClassRef::dictionary_iterator(asm);
        let dict_entry = ClassRef::dictionary_entry(asm);
        let env = ClassRef::enviroment(asm);
        let string_cls = ClassRef::string(asm);
        let string_ty = asm.alloc_type(Type::PlatformString);
        let iter_ty = asm.alloc_type(Type::ClassRef(dict_iter));
        let entry_ty = asm.alloc_type(Type::ClassRef(dict_entry));

        // Method references.
        let get_env_vars = {
            let n = asm.alloc_string("GetEnvironmentVariables");
            asm.class_ref(env)
                .clone()
                .static_mref(&[], Type::ClassRef(i_dictionary), n, asm)
        };
        let get_enum = {
            let n = asm.alloc_string("GetEnumerator");
            asm.class_ref(i_dictionary)
                .clone()
                .virtual_mref(&[], Type::ClassRef(dict_iter), n, asm)
        };
        let move_next = {
            let n = asm.alloc_string("MoveNext");
            asm.class_ref(i_enumerator)
                .clone()
                .virtual_mref(&[], Type::Bool, n, asm)
        };
        let get_current = {
            let n = asm.alloc_string("get_Current");
            asm.class_ref(i_enumerator)
                .clone()
                .virtual_mref(&[], Type::PlatformObject, n, asm)
        };
        let entry_ref = asm.nref(Type::ClassRef(dict_entry));
        let get_key = {
            let n = asm.alloc_string("get_Key");
            let sig = asm.sig([entry_ref], Type::PlatformObject);
            asm.alloc_methodref(MethodRef::new(
                dict_entry,
                n,
                sig,
                MethodKind::Instance,
                [].into(),
            ))
        };
        let get_value = {
            let n = asm.alloc_string("get_Value");
            let sig = asm.sig([entry_ref], Type::PlatformObject);
            asm.alloc_methodref(MethodRef::new(
                dict_entry,
                n,
                sig,
                MethodKind::Instance,
                [].into(),
            ))
        };
        let concat2 = {
            let n = asm.alloc_string("Concat");
            asm.class_ref(string_cls).clone().static_mref(
                &[Type::PlatformString, Type::PlatformString],
                Type::PlatformString,
                n,
                asm,
            )
        };
        let concat4 = {
            let n = asm.alloc_string("Concat");
            asm.class_ref(string_cls).clone().static_mref(
                &[
                    Type::PlatformString,
                    Type::PlatformString,
                    Type::PlatformString,
                    Type::PlatformString,
                ],
                Type::PlatformString,
                n,
                asm,
            )
        };
        let to_utf8 = string_to_utf8(asm);
        let empty = {
            let s = asm.ldstr("");
            asm.alloc_node(s)
        };
        let eq_str = {
            let s = asm.ldstr("=");
            asm.alloc_node(s)
        };
        let nl_str = {
            let s = asm.ldstr("\n");
            asm.alloc_node(s)
        };

        const L_ACC: u32 = 0;
        const L_ITER: u32 = 1;
        const L_ENTRY: u32 = 2;

        // Block 0: acc = ""; iter = GetEnvironmentVariables().GetEnumerator(); goto 1.
        let store_acc0 = asm.alloc_root(CILRoot::StLoc(L_ACC, empty));
        let dict = asm.alloc_node(CILNode::call(get_env_vars, []));
        let iter = asm.alloc_node(CILNode::call(get_enum, [dict]));
        let store_iter = asm.alloc_root(CILRoot::StLoc(L_ITER, iter));
        let goto1 = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

        // Block 1 (loop head): if !iter.MoveNext() goto 3 else goto 2.
        let iter_l1 = asm.alloc_node(CILNode::LdLoc(L_ITER));
        let has_next = asm.alloc_node(CILNode::call(move_next, [iter_l1]));
        let br_end = asm.alloc_root(CILRoot::Branch(Box::new((
            3,
            0,
            Some(BranchCond::False(has_next)),
        ))));
        let goto2 = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // Block 2 (body): entry = (DictionaryEntry)iter.get_Current;
        //   key=(string)entry.get_Key; val=(string)entry.get_Value;
        //   acc = Concat(acc, key, "=", Concat(val, "\n")); goto 1.
        let iter_l2 = asm.alloc_node(CILNode::LdLoc(L_ITER));
        let cur = asm.alloc_node(CILNode::call(get_current, [iter_l2]));
        let entry = asm.unbox_any(cur, entry_ty);
        let store_entry = asm.alloc_root(CILRoot::StLoc(L_ENTRY, entry));
        let ek = asm.alloc_node(CILNode::LdLocA(L_ENTRY));
        let key_obj = asm.alloc_node(CILNode::call(get_key, [ek]));
        let key = asm.alloc_node(CILNode::CheckedCast(key_obj, string_ty));
        let ev = asm.alloc_node(CILNode::LdLocA(L_ENTRY));
        let val_obj = asm.alloc_node(CILNode::call(get_value, [ev]));
        let val = asm.alloc_node(CILNode::CheckedCast(val_obj, string_ty));
        let val_nl = asm.alloc_node(CILNode::call(concat2, [val, nl_str]));
        let acc_l = asm.alloc_node(CILNode::LdLoc(L_ACC));
        let new_acc = asm.alloc_node(CILNode::call(concat4, [acc_l, key, eq_str, val_nl]));
        let store_acc2 = asm.alloc_root(CILRoot::StLoc(L_ACC, new_acc));
        let goto1b = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

        // Block 3 (return): return (u8*)StringToCoTaskMemUTF8(acc).
        let acc_final = asm.alloc_node(CILNode::LdLoc(L_ACC));
        let buf = asm.alloc_node(CILNode::call(to_utf8, [acc_final]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret = asm.alloc_root(CILRoot::Ret(buf));

        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_acc0, store_iter, goto1], 0, None),
                BasicBlock::new(vec![br_end, goto2], 1, None),
                BasicBlock::new(vec![store_entry, store_acc2, goto1b], 2, None),
                BasicBlock::new(vec![ret], 3, None),
            ],
            locals: vec![
                (Some(asm.alloc_string("acc")), string_ty),
                (Some(asm.alloc_string("iter")), iter_ty),
                (Some(asm.alloc_string("entry")), entry_ty),
            ],
        }
    };
    patcher.insert(environ_name, Box::new(environ_gen));
}

// ===========================================================================
// fs (`sys/fs/dotnet.rs`)
//
// Backs std::fs with System.IO. An open file is a `GCHandle` to a `FileStream`;
// a `read_dir` snapshot is a `GCHandle` to a `string[]`. Paths arrive as
// `(ptr, len)` UTF-8 buffers (the same `decode_utf8` mechanism args/env use).
// All handles cross the Rust ABI as opaque `IntPtr`/`*mut u8` — no managed
// object is ever passed through a Rust signature.
// ===========================================================================

/// Recover the managed object pinned by an `IntPtr`/`*mut u8` handle (the value
/// `ref_to_handle` produced) and `castclass` it to `class`, leaving the typed
/// object node on the stack. Mirrors the handle round-trip in
/// `insert_dotnet_thread_join`.
pub(crate) fn handle_to_class(
    asm: &mut Assembly,
    handle_arg: u32,
    class: Interned<ClassRef>,
) -> Interned<CILNode> {
    let arg = asm.alloc_node(CILNode::LdArg(handle_arg));
    let handle_isize = asm.alloc_node(CILNode::PtrCast(arg, Box::new(PtrCastRes::ISize)));
    let handle_to_obj = asm.alloc_string("handle_to_obj");
    let main_module = asm.main_module();
    let handle_to_obj = asm.class_ref(*main_module).clone().static_mref(
        &[Type::Int(Int::ISize)],
        Type::PlatformObject,
        handle_to_obj,
        asm,
    );
    let obj = asm.alloc_node(CILNode::call(handle_to_obj, [handle_isize]));
    let class_ty = asm.alloc_type(Type::ClassRef(class));
    asm.alloc_node(CILNode::CheckedCast(obj, class_ty))
}

/// Build the roots that free the `GCHandle` whose `IntPtr` is `LdArg(handle_arg)`:
/// `GCHandle.FromIntPtr((nint)handle).Free();`, stowing the handle in local
/// `gch_local`. Mirrors the free dance in `insert_dotnet_thread_join`.
pub(crate) fn free_handle_roots(
    asm: &mut Assembly,
    handle_arg: u32,
    gch_local: u32,
) -> (Interned<CILRoot>, Interned<CILRoot>, Interned<Type>) {
    let gc_handle = ClassRef::gc_handle(asm);
    let from_int_ptr = asm.alloc_string("FromIntPtr");
    let from_int_ptr = asm.class_ref(gc_handle).clone().static_mref(
        &[Type::Int(Int::ISize)],
        Type::ClassRef(gc_handle),
        from_int_ptr,
        asm,
    );
    let arg = asm.alloc_node(CILNode::LdArg(handle_arg));
    let handle_isize = asm.alloc_node(CILNode::PtrCast(arg, Box::new(PtrCastRes::ISize)));
    let gch = asm.alloc_node(CILNode::call(from_int_ptr, [handle_isize]));
    let store_gch = asm.alloc_root(CILRoot::StLoc(gch_local, gch));
    let free = asm.alloc_string("Free");
    let free = asm
        .class_ref(gc_handle)
        .clone()
        .instance(&[], Type::Void, free, asm);
    let gch_addr = asm.alloc_node(CILNode::LdLocA(gch_local));
    let free = asm.alloc_root(CILRoot::call(free, [gch_addr]));
    let gc_handle_ty = asm.alloc_type(Type::ClassRef(gc_handle));
    (store_gch, free, gc_handle_ty)
}

/// Build a `System.Span<byte>` (or `ReadOnlySpan<byte>` when `read_only`) from
/// the `(buf_ptr, len)` pair at `LdArg(ptr_arg)` / `LdArg(len_arg)`, via the
/// span's `(void*, int32)` ctor. Mirrors `insert_dotnet_random_fill`.
pub(crate) fn build_byte_span(
    asm: &mut Assembly,
    ptr_arg: u32,
    len_arg: u32,
    read_only: bool,
) -> (Interned<CILNode>, Type) {
    let void_ptr = asm.nptr(Type::Void);
    let byte_ty = Type::Int(Int::U8);
    let span = if read_only {
        ClassRef::read_only_span(asm, byte_ty)
    } else {
        ClassRef::span(asm, byte_ty)
    };
    let span_ty = Type::ClassRef(span);
    let ctor_sig = asm.sig([span_ty, void_ptr, Type::Int(Int::I32)], Type::Void);
    let ctor_name = asm.alloc_string(".ctor");
    let ctor = asm.alloc_methodref(MethodRef::new(
        span,
        ctor_name,
        ctor_sig,
        MethodKind::Constructor,
        [].into(),
    ));
    let ptr = asm.alloc_node(CILNode::LdArg(ptr_arg));
    let ptr = asm.cast_ptr(ptr, void_ptr);
    let len = asm.alloc_node(CILNode::LdArg(len_arg));
    let len_i32 = asm.int_cast(len, Int::I32, ExtendKind::ZeroExtend);
    let span_node = asm.alloc_node(CILNode::call(ctor, [ptr, len_i32]));
    (span_node, span_ty)
}

/// Registers all `rcl_dotnet_fs_*` BCL bindings (System.IO) in `patcher`.
fn insert_dotnet_fs(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    insert_dotnet_fs_open(asm, patcher);
    insert_dotnet_fs_read(asm, patcher);
    insert_dotnet_fs_write(asm, patcher);
    insert_dotnet_fs_read_at(asm, patcher);
    insert_dotnet_fs_write_at(asm, patcher);
    insert_dotnet_fs_symlink(asm, patcher);
    insert_dotnet_fs_readlink(asm, patcher);
    insert_dotnet_fs_seek(asm, patcher);
    insert_dotnet_fs_flush(asm, patcher);
    insert_dotnet_fs_close(asm, patcher);
    insert_dotnet_fs_len(asm, patcher);
    insert_dotnet_fs_set_len(asm, patcher);
    insert_dotnet_fs_canonicalize(asm, patcher);
    insert_dotnet_fs_set_readonly(asm, patcher);
    insert_dotnet_fs_stat(asm, patcher);
    insert_dotnet_fs_exists(asm, patcher);
    insert_dotnet_fs_mkdir(asm, patcher);
    insert_dotnet_fs_rmdir(asm, patcher);
    insert_dotnet_fs_unlink(asm, patcher);
    insert_dotnet_fs_rename(asm, patcher);
    insert_dotnet_fs_readdir_open(asm, patcher);
    insert_dotnet_fs_readdir_count(asm, patcher);
    insert_dotnet_fs_readdir_get(asm, patcher);
    insert_dotnet_fs_readdir_close(asm, patcher);
}

/// `rcl_dotnet_fs_open(path_ptr, path_len, mode, access, append) -> *mut u8`
///   => `new FileStream(path, (FileMode)mode, (FileAccess)access)`, returned as
///      an opaque `GCHandle` `IntPtr`.
///
/// `FileMode`/`FileAccess` are int-backed enums; the std side computes the int
/// values, so we pass `mode`/`access` straight to the ctor. The `append` arg is
/// unused here: std maps an append open to `FileMode.Append`, which positions
/// the stream at end-of-file itself.
///
/// PAL-fidelity: on a managed I/O fault the body catches the exception, maps it
/// to a POSIX `errno` via `rcl_errno_from_exception` (FileNotFound→ENOENT,
/// UnauthorizedAccess→EACCES, …) and returns **null**. The std `File::open` arm
/// then reports the precise `ErrorKind` via `io::Error::last_os_error()`. This
/// does NOT use `super::posix::errno_wrapped` because that helper returns `-1`
/// (a non-null pointer), which would defeat the std-side `handle.is_null()`
/// check — the open hook must return a null pointer on failure.
fn insert_dotnet_fs_open(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_open", |asm| {
        let path = decode_utf8(asm, 0, 1);
        let mode = asm.alloc_node(CILNode::LdArg(2));
        let access = asm.alloc_node(CILNode::LdArg(3));
        // new FileStream(string, FileMode, FileAccess). FileMode/FileAccess are
        // int-backed enum value types; an `int32` on the stack is binary
        // compatible, so the args feed straight in, but the ctor *signature*
        // must name the enum types or method resolution fails (the BCL has no
        // `(string, int, int)` ctor).
        let file_stream = ClassRef::file_stream(asm);
        let file_mode = Type::ClassRef(ClassRef::file_mode(asm));
        let file_access = Type::ClassRef(ClassRef::file_access(asm));
        let mode = i32_to_bcl_enum(mode, file_mode, asm);
        let access = i32_to_bcl_enum(access, file_access, asm);
        let ctor = asm
            .class_ref(file_stream)
            .clone()
            .ctor(&[Type::PlatformString, file_mode, file_access], asm);
        let stream = asm.alloc_node(CILNode::call(ctor, [path, mode, access]));
        let store = asm.alloc_root(CILRoot::StLoc(0, stream));
        // result(local 1) = (void*)GCHandle.Alloc(stream).
        let handle = CILNode::LdLoc(0).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let void = asm.alloc_type(Type::Void);
        let void_ptr = Type::Ptr(void);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let store_ok = asm.alloc_root(CILRoot::StLoc(1, handle));
        // try { build stream; result = handle; leave -> 2 }
        let leave_ok = asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 2,
            source: 0,
        });
        // catch (block 1): errno = rcl_errno_from_exception(GetException);
        //                  result = null; leave -> 2.
        let get_exn = asm.alloc_node(CILNode::GetException);
        let mapper = super::posix::main_static(
            asm,
            "rcl_errno_from_exception",
            &[Type::PlatformObject],
            Type::Int(Int::I32),
        );
        let mapped = asm.alloc_node(CILNode::call(mapper, [get_exn]));
        let set_errno = super::posix::set_errno_node(asm, mapped);
        // null = (void*)(isize)0 — the established null-pointer idiom.
        let zero_addr = asm.alloc_node(0_i32);
        let zero_addr = asm.int_cast(zero_addr, Int::ISize, ExtendKind::ZeroExtend);
        let nullp = asm.cast_ptr(zero_addr, void_ptr);
        let store_null = asm.alloc_root(CILRoot::StLoc(1, nullp));
        let leave_catch = asm.alloc_root(CILRoot::ExitSpecialRegion {
            target: 2,
            source: 1,
        });
        // block 2: ret result.
        let ld_result = asm.alloc_node(CILNode::LdLoc(1));
        let ret = asm.alloc_root(CILRoot::Ret(ld_result));

        let stream_ty = asm.alloc_type(Type::ClassRef(file_stream));
        let void_ptr_ty = asm.alloc_type(void_ptr);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(
                    vec![store, store_ok, leave_ok],
                    0,
                    Some(vec![BasicBlock::new(
                        vec![set_errno, store_null, leave_catch],
                        1,
                        None,
                    )]),
                ),
                BasicBlock::new(vec![ret], 2, None),
            ],
            locals: vec![
                (Some(asm.alloc_string("stream")), stream_ty),
                (Some(asm.alloc_string("result")), void_ptr_ty),
            ],
        }
    });
}

/// `rcl_dotnet_fs_read(handle, buf_ptr, len) -> isize`
///   => `FileStream.Read(new Span<byte>(buf_ptr, (int)len))` (count read).
fn insert_dotnet_fs_read(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_read", |asm| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let (span, span_ty) = build_byte_span(asm, 1, 2, false);
        let read_name = asm.alloc_string("Read");
        let read = asm.class_ref(file_stream).clone().instance(
            &[span_ty],
            Type::Int(Int::I32),
            read_name,
            asm,
        );
        let count = asm.alloc_node(CILNode::call(read, [stream, span]));
        let count = asm.int_cast(count, Int::ISize, ExtendKind::SignExtend);
        let ret = asm.alloc_root(CILRoot::Ret(count));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_fs_write(handle, buf_ptr, len) -> isize`
///   => `FileStream.Write(new ReadOnlySpan<byte>(buf_ptr, (int)len))`; returns
///      `len` (the BCL `Write(ReadOnlySpan<byte>)` overload writes all of it or
///      throws).
fn insert_dotnet_fs_write(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_write", |asm| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let (span, span_ty) = build_byte_span(asm, 1, 2, true);
        let write_name = asm.alloc_string("Write");
        let write =
            asm.class_ref(file_stream)
                .clone()
                .instance(&[span_ty], Type::Void, write_name, asm);
        let write = asm.alloc_root(CILRoot::call(write, [stream, span]));
        let len = asm.alloc_node(CILNode::LdArg(2));
        let len = asm.int_cast(len, Int::ISize, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(len));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![write, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// Resolve a `FileStream` GCHandle (`LdArg(handle_arg)`) to its
/// `SafeFileHandle` via the `FileStream.SafeFileHandle` instance getter — the
/// `this` `System.IO.RandomAccess.{Read,Write}` need. Returns the SFH node and
/// its type. Shared by the `read_at`/`write_at` hooks (B2 Piece 3).
fn filestream_safe_handle(asm: &mut Assembly, handle_arg: u32) -> (Interned<CILNode>, Type) {
    let file_stream = ClassRef::file_stream(asm);
    let safe_handle = ClassRef::safe_file_handle(asm);
    let sfh_ty = Type::ClassRef(safe_handle);
    let stream = handle_to_class(asm, handle_arg, file_stream);
    let get_sfh_name = asm.alloc_string("get_SafeFileHandle");
    let get_sfh = asm
        .class_ref(file_stream)
        .clone()
        .instance(&[], sfh_ty, get_sfh_name, asm);
    let sfh = asm.alloc_node(CILNode::call(get_sfh, [stream]));
    (sfh, sfh_ty)
}

/// `rcl_dotnet_fs_read_at(handle, buf_ptr, len, offset: i64) -> isize`
///   => `RandomAccess.Read(FileStream.SafeFileHandle, new Span<byte>(buf_ptr,
///      (int)len), offset)` (count read; does NOT move the stream position).
///
/// B2 Piece 3 — backs `os::unix::fs::FileExt::read_at` (pread) on the dotnet PAL
/// via `System.IO.RandomAccess`, the managed offset-relative I/O API.
fn insert_dotnet_fs_read_at(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_read_at", |asm| {
        let random_access = ClassRef::random_access(asm);
        let (sfh, sfh_ty) = filestream_safe_handle(asm, 0);
        let (span, span_ty) = build_byte_span(asm, 1, 2, false);
        let offset = asm.alloc_node(CILNode::LdArg(3));
        let read_name = asm.alloc_string("Read");
        let read = asm.class_ref(random_access).clone().static_mref(
            &[sfh_ty, span_ty, Type::Int(Int::I64)],
            Type::Int(Int::I32),
            read_name,
            asm,
        );
        let count = asm.alloc_node(CILNode::call(read, [sfh, span, offset]));
        let count = asm.int_cast(count, Int::ISize, ExtendKind::SignExtend);
        let ret = asm.alloc_root(CILRoot::Ret(count));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_fs_write_at(handle, buf_ptr, len, offset: i64) -> isize`
///   => `RandomAccess.Write(FileStream.SafeFileHandle, new
///      ReadOnlySpan<byte>(buf_ptr, (int)len), offset)`; returns `len` (Write
///      writes all of it or throws). Does NOT move the stream position.
///
/// B2 Piece 3 — backs `os::unix::fs::FileExt::write_at` (pwrite).
fn insert_dotnet_fs_write_at(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_write_at", |asm| {
        let random_access = ClassRef::random_access(asm);
        let (sfh, sfh_ty) = filestream_safe_handle(asm, 0);
        let (span, span_ty) = build_byte_span(asm, 1, 2, true);
        let offset = asm.alloc_node(CILNode::LdArg(3));
        let write_name = asm.alloc_string("Write");
        let write = asm.class_ref(random_access).clone().static_mref(
            &[sfh_ty, span_ty, Type::Int(Int::I64)],
            Type::Void,
            write_name,
            asm,
        );
        let write = asm.alloc_root(CILRoot::call(write, [sfh, span, offset]));
        let len = asm.alloc_node(CILNode::LdArg(2));
        let len = asm.int_cast(len, Int::ISize, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(len));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![write, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_fs_symlink(link_ptr, link_len, target_ptr, target_len) -> i32`
///   => `File.CreateSymbolicLink(link, target)` (returns 0; the BCL throws on
///      failure, which unwinds to std's error path).
///
/// B2 Piece 4 — backs `sys::fs::symlink`. `.NET` names the args
/// `CreateSymbolicLink(string path, string pathToTarget)`: `path` is the symlink
/// location (= std `link`), `pathToTarget` is where it points (= std `original`/
/// `target`). The result `FileSystemInfo` is popped (discarded).
fn insert_dotnet_fs_symlink(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_symlink", |asm| {
        let file = ClassRef::file(asm);
        let fsi = ClassRef::file_system_info(asm);
        let fsi_ty = Type::ClassRef(fsi);
        let link = decode_utf8(asm, 0, 1);
        let store_link = asm.alloc_root(CILRoot::StLoc(0, link));
        let target = decode_utf8(asm, 2, 3);
        let store_target = asm.alloc_root(CILRoot::StLoc(1, target));
        let create_name = asm.alloc_string("CreateSymbolicLink");
        let create = asm.class_ref(file).clone().static_mref(
            &[Type::PlatformString, Type::PlatformString],
            fsi_ty,
            create_name,
            asm,
        );
        let link2 = asm.alloc_node(CILNode::LdLoc(0));
        let target2 = asm.alloc_node(CILNode::LdLoc(1));
        // `CreateSymbolicLink` returns a FileSystemInfo, so the result must be
        // popped — CILRoot::call is for void methods and leaving a non-void
        // result on the stack yields an InvalidProgramException at JIT time.
        let created = asm.alloc_node(CILNode::call(create, [link2, target2]));
        let call = asm.alloc_root(CILRoot::Pop(created));
        let zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        let string_ty = asm.alloc_type(Type::PlatformString);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![store_link, store_target, call, ret],
                0,
                None,
            )],
            locals: vec![
                (Some(asm.alloc_string("link")), string_ty),
                (Some(asm.alloc_string("target")), string_ty),
            ],
        }
    });
}

/// `rcl_dotnet_fs_readlink(path_ptr, path_len) -> *mut u8`
///   => `File.ResolveLinkTarget(path, returnFinalTarget: false)`; on a non-null
///      `FileSystemInfo` marshals its `FullName` to a NUL-terminated UTF-8 C
///      string (freed std-side by `rcl_dotnet_cotaskmem_free`); on null (the
///      path is not a link / does not exist) returns `(u8*)0` so std maps it to
///      `NotFound`.
///
/// B2 Piece 4 — backs `sys::fs::readlink`. Models the null-path on
/// `rcl_dotnet_paths_current_exe`.
fn insert_dotnet_fs_readlink(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_readlink", |asm| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        let file = ClassRef::file(asm);
        let fsi = ClassRef::file_system_info(asm);
        let fsi_ty = Type::ClassRef(fsi);

        // info = File.ResolveLinkTarget(path, false) -> FileSystemInfo? (local 1).
        let path = decode_utf8(asm, 0, 1);
        let store_path = asm.alloc_root(CILRoot::StLoc(0, path));
        let resolve_name = asm.alloc_string("ResolveLinkTarget");
        let resolve = asm.class_ref(file).clone().static_mref(
            &[Type::PlatformString, Type::Bool],
            fsi_ty,
            resolve_name,
            asm,
        );
        let path2 = asm.alloc_node(CILNode::LdLoc(0));
        let false_c = asm.alloc_node(false);
        let info = asm.alloc_node(CILNode::call(resolve, [path2, false_c]));
        let store_info = asm.alloc_root(CILRoot::StLoc(1, info));

        // Block 0: if (info == null) goto 1 else goto 2.
        let info_load = asm.alloc_node(CILNode::LdLoc(1));
        let null_fsi = asm.alloc_node(CILNode::Const(Box::new(Const::Null(fsi))));
        let br_null = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(info_load, null_fsi)),
        ))));
        let goto_marshal = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // Block 1: return (u8*)0.
        let zero = asm.alloc_node(0_i32);
        let zero = asm.int_cast(zero, Int::ISize, ExtendKind::ZeroExtend);
        let null_ptr = asm.cast_ptr(zero, u8_ptr);
        let ret_null = asm.alloc_root(CILRoot::Ret(null_ptr));

        // Block 2: s = info.FullName; return (u8*)StringToCoTaskMemUTF8(s).
        let get_full_name = asm.alloc_string("get_FullName");
        let get_full =
            asm.class_ref(fsi)
                .clone()
                .instance(&[], Type::PlatformString, get_full_name, asm);
        let info_load2 = asm.alloc_node(CILNode::LdLoc(1));
        let s = asm.alloc_node(CILNode::call(get_full, [info_load2]));
        let to_utf8 = string_to_utf8(asm);
        let buf = asm.alloc_node(CILNode::call(to_utf8, [s]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret_buf = asm.alloc_root(CILRoot::Ret(buf));

        let string_ty = asm.alloc_type(Type::PlatformString);
        let fsi_local_ty = asm.alloc_type(fsi_ty);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_path, store_info, br_null, goto_marshal], 0, None),
                BasicBlock::new(vec![ret_null], 1, None),
                BasicBlock::new(vec![ret_buf], 2, None),
            ],
            locals: vec![
                (Some(asm.alloc_string("path")), string_ty),
                (Some(asm.alloc_string("info")), fsi_local_ty),
            ],
        }
    });
}

/// `rcl_dotnet_fs_seek(handle, offset: i64, origin: i32) -> i64`
///   => `FileStream.Seek(offset, (SeekOrigin)origin)` (new absolute position).
///
/// `offset` is a signed 64-bit value (it may be negative for `SeekFrom::End` /
/// `Current`), so it is loaded as-is — never zero-extended.
fn insert_dotnet_fs_seek(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_seek", |asm| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let offset = asm.alloc_node(CILNode::LdArg(1));
        let origin = asm.alloc_node(CILNode::LdArg(2));
        let seek_name = asm.alloc_string("Seek");
        // FileStream.Seek(long, SeekOrigin) — the second param is the int-backed
        // SeekOrigin enum value type (an int32 is binary compatible on the stack).
        let seek_origin = Type::ClassRef(ClassRef::seek_origin(asm));
        let origin = i32_to_bcl_enum(origin, seek_origin, asm);
        let seek = asm.class_ref(file_stream).clone().instance(
            &[Type::Int(Int::I64), seek_origin],
            Type::Int(Int::I64),
            seek_name,
            asm,
        );
        let pos = asm.alloc_node(CILNode::call(seek, [stream, offset, origin]));
        let ret = asm.alloc_root(CILRoot::Ret(pos));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_fs_flush(handle)` => `FileStream.Flush()`.
fn insert_dotnet_fs_flush(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_flush", |asm| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let flush_name = asm.alloc_string("Flush");
        let flush = asm
            .class_ref(file_stream)
            .clone()
            .instance(&[], Type::Void, flush_name, asm);
        let flush = asm.alloc_root(CILRoot::call(flush, [stream]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![flush, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_fs_close(handle)` => `FileStream.Dispose()` then free the
/// `GCHandle` so the stream can be collected.
fn insert_dotnet_fs_close(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_close", |asm| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let dispose_name = asm.alloc_string("Dispose");
        let dispose =
            asm.class_ref(file_stream)
                .clone()
                .instance(&[], Type::Void, dispose_name, asm);
        let dispose = asm.alloc_root(CILRoot::call(dispose, [stream]));
        let (store_gch, free, gc_handle_ty) = free_handle_roots(asm, 0, 0);
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![dispose, store_gch, free, ret],
                0,
                None,
            )],
            locals: vec![(Some(asm.alloc_string("gch")), gc_handle_ty)],
        }
    });
}

/// `rcl_dotnet_fs_len(handle) -> i64` => `FileStream.get_Length`.
fn insert_dotnet_fs_len(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_len", |asm| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let get_len_name = asm.alloc_string("get_Length");
        let get_len = asm.class_ref(file_stream).clone().instance(
            &[],
            Type::Int(Int::I64),
            get_len_name,
            asm,
        );
        let len = asm.alloc_node(CILNode::call(get_len, [stream]));
        let ret = asm.alloc_root(CILRoot::Ret(len));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_fs_set_len(handle, len: i64) -> i32` => `FileStream.SetLength(len)`; returns 0.
///
/// Backs `File::truncate` (`std::fs::File::set_len`) on the dotnet PAL. `SetLength` truncates or
/// zero-grows the file to `len`; it throws on failure (mapped upstream like the other fs hooks), so
/// the success path is always 0.
fn insert_dotnet_fs_set_len(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_set_len", |asm| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let len = asm.alloc_node(CILNode::LdArg(1));
        let set_len_name = asm.alloc_string("SetLength");
        let set_len = asm.class_ref(file_stream).clone().instance(
            &[Type::Int(Int::I64)],
            Type::Void,
            set_len_name,
            asm,
        );
        let call = asm.alloc_root(CILRoot::call(set_len, [stream, len]));
        let zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![call, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_fs_canonicalize(path_ptr, path_len) -> *mut u8`
///   => if neither `File.Exists(path)` nor `Directory.Exists(path)`, returns `(u8*)0` (std maps to
///      `NotFound`, matching `canonicalize`'s require-exists contract); else marshals
///      `Path.GetFullPath(path)` (absolute, `.`/`..`-normalized) to a NUL-terminated UTF-8 C string
///      (freed std-side by `rcl_dotnet_cotaskmem_free`).
///
/// Backs `sys::fs::canonicalize`. NOTE: `GetFullPath` does not resolve symlinks in the path; on the
/// common (no-symlink) path this equals Rust's `canonicalize`.
fn insert_dotnet_fs_canonicalize(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_canonicalize", |asm| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        let file = ClassRef::file(asm);
        let directory = ClassRef::directory(asm);
        let path_io = ClassRef::path_io(asm);
        let exists_name = asm.alloc_string("Exists");

        // Block 0: path = decode_utf8; if File.Exists(path) goto 2 else goto 1.
        let path = decode_utf8(asm, 0, 1);
        let store_path = asm.alloc_root(CILRoot::StLoc(0, path));
        let file_exists = asm.class_ref(file).clone().static_mref(
            &[Type::PlatformString],
            Type::Bool,
            exists_name,
            asm,
        );
        let p1 = asm.alloc_node(CILNode::LdLoc(0));
        let fe = asm.alloc_node(CILNode::call(file_exists, [p1]));
        let true_c = asm.alloc_node(true);
        let br_fe = asm.alloc_root(CILRoot::Branch(Box::new((
            2,
            0,
            Some(BranchCond::Eq(fe, true_c)),
        ))));
        let goto_dir = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

        // Block 1: if Directory.Exists(path) goto 2 else goto 3 (null).
        let dir_exists = asm.class_ref(directory).clone().static_mref(
            &[Type::PlatformString],
            Type::Bool,
            exists_name,
            asm,
        );
        let p2 = asm.alloc_node(CILNode::LdLoc(0));
        let de = asm.alloc_node(CILNode::call(dir_exists, [p2]));
        let true_c2 = asm.alloc_node(true);
        let br_de = asm.alloc_root(CILRoot::Branch(Box::new((
            2,
            0,
            Some(BranchCond::Eq(de, true_c2)),
        ))));
        let goto_null = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // Block 2: return (u8*)StringToCoTaskMemUTF8(Path.GetFullPath(path)).
        let gfp_name = asm.alloc_string("GetFullPath");
        let gfp = asm.class_ref(path_io).clone().static_mref(
            &[Type::PlatformString],
            Type::PlatformString,
            gfp_name,
            asm,
        );
        let p3 = asm.alloc_node(CILNode::LdLoc(0));
        let full = asm.alloc_node(CILNode::call(gfp, [p3]));
        let to_utf8 = string_to_utf8(asm);
        let buf = asm.alloc_node(CILNode::call(to_utf8, [full]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret_buf = asm.alloc_root(CILRoot::Ret(buf));

        // Block 3: return (u8*)0.
        let zero = asm.alloc_node(0_i32);
        let zero = asm.int_cast(zero, Int::ISize, ExtendKind::ZeroExtend);
        let null_ptr = asm.cast_ptr(zero, u8_ptr);
        let ret_null = asm.alloc_root(CILRoot::Ret(null_ptr));

        let string_ty = asm.alloc_type(Type::PlatformString);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_path, br_fe, goto_dir], 0, None),
                BasicBlock::new(vec![br_de, goto_null], 1, None),
                BasicBlock::new(vec![ret_buf], 2, None),
                BasicBlock::new(vec![ret_null], 3, None),
            ],
            locals: vec![(Some(asm.alloc_string("path")), string_ty)],
        }
    });
}

/// `rcl_dotnet_fs_set_readonly(path_ptr, path_len, readonly: i32) -> i32` (returns 0)
///   => `File.SetAttributes(path, readonly ? FileAttributes.ReadOnly : FileAttributes.Normal)`.
///
/// Backs `sys::fs::set_perm` on the dotnet PAL. .NET has no Unix mode model, so a `FilePermissions`
/// carries only the read-only bit (`FilePermissions::readonly`); this toggles `FileAttributes.ReadOnly`
/// (`= 1`) vs `FileAttributes.Normal` (`= 128`). It throws on failure (mapped upstream), so 0 = ok.
fn insert_dotnet_fs_set_readonly(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_set_readonly", |asm| {
        let file = ClassRef::file(asm);
        let fattr = ClassRef::file_attributes(asm);
        let fattr_ty = Type::ClassRef(fattr);
        let sa_name = asm.alloc_string("SetAttributes");
        let set_attrs = asm.class_ref(file).clone().static_mref(
            &[Type::PlatformString, fattr_ty],
            Type::Void,
            sa_name,
            asm,
        );

        // Block 0: path = decode_utf8; if readonly == 0 goto 2 (Normal) else goto 1 (ReadOnly).
        let path = decode_utf8(asm, 0, 1);
        let store_path = asm.alloc_root(CILRoot::StLoc(0, path));
        let ro = asm.alloc_node(CILNode::LdArg(2));
        let zero_arg = asm.alloc_node(0_i32);
        let br = asm.alloc_root(CILRoot::Branch(Box::new((
            2,
            0,
            Some(BranchCond::Eq(ro, zero_arg)),
        ))));
        let goto_ro = asm.alloc_root(CILRoot::Branch(Box::new((1, 0, None))));

        // Block 1 (readonly): SetAttributes(path, (FileAttributes)1); return 0.
        let p1 = asm.alloc_node(CILNode::LdLoc(0));
        let one = asm.alloc_node(1_i32);
        let one_fa = i32_to_bcl_enum(one, fattr_ty, asm);
        let call1 = asm.alloc_root(CILRoot::call(set_attrs, [p1, one_fa]));
        let z1 = asm.alloc_node(0_i32);
        let ret1 = asm.alloc_root(CILRoot::Ret(z1));

        // Block 2 (not readonly): SetAttributes(path, (FileAttributes)128 /*Normal*/); return 0.
        let p2 = asm.alloc_node(CILNode::LdLoc(0));
        let n128 = asm.alloc_node(128_i32);
        let n128_fa = i32_to_bcl_enum(n128, fattr_ty, asm);
        let call2 = asm.alloc_root(CILRoot::call(set_attrs, [p2, n128_fa]));
        let z2 = asm.alloc_node(0_i32);
        let ret2 = asm.alloc_root(CILRoot::Ret(z2));

        let string_ty = asm.alloc_type(Type::PlatformString);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_path, br, goto_ro], 0, None),
                BasicBlock::new(vec![call1, ret1], 1, None),
                BasicBlock::new(vec![call2, ret2], 2, None),
            ],
            locals: vec![(Some(asm.alloc_string("path")), string_ty)],
        }
    });
}

/// `rcl_dotnet_fs_stat(path_ptr, path_len, out_size: *mut u64, out_is_dir: *mut i32,
///                     out_mtime: *mut i64, out_atime: *mut i64, out_ctime: *mut i64,
///                     out_is_symlink: *mut i32) -> i32`
///   => `Directory.Exists(path)` ? write (size 0, is_dir 1) + times + symlink, return 0
///      : `File.Exists(path)`    ? write (FileInfo.Length, is_dir 0) + times + symlink, return 0
///      : return -1 (NotFound — the std side maps -1 to `ErrorKind::NotFound`).
///
/// B2 Piece 2/4 extended this from the original 4-arg `(size, is_dir)` shape to
/// the full 8-arg shape: mtime/atime via `File.GetLast{Write,Access}TimeUtc`,
/// ctime via `GetCreationTimeUtc` (a documented semantic mismatch: this is the
/// .NET *creation* time, NOT the POSIX inode-change time), is_symlink via the
/// `FileAttributes.ReparsePoint` bit. The `-1` return is reserved for the
/// genuinely-not-found case (both `Exists` checks false) — a managed throw
/// (e.g. an ACL denial) still unwinds here; the not-found `-1` is correctly
/// `ErrorKind::NotFound` on the std side.
/// .NET ticks (100-ns intervals since `0001-01-01Z`) at the Unix epoch
/// (`1970-01-01Z`). Subtract this from a `DateTime.Ticks` and divide by
/// `10_000_000` to get whole Unix seconds. Same constant the std `SystemTime`
/// PAL uses to rebase `rcl_dotnet_unix_ticks`.
const DOTNET_UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
const DOTNET_TICKS_PER_SEC: i64 = 10_000_000;

/// Build a node producing the Unix-seconds timestamp for `path_local`
/// (`LdLoc(path_local)`) via the static getter `getter` on `System.IO.File`
/// (e.g. `GetLastWriteTimeUtc`), which returns a `DateTime` struct. The struct
/// is stowed in `dt_local` so its `.Ticks` instance getter can take a managed
/// `this` (`LdLocA`); the result is rebased onto the Unix epoch. Returns the
/// (StLoc-root, secs-node) pair — the caller schedules the StLoc then uses the
/// node. Used by `rcl_dotnet_fs_stat` (B2 Piece 2) for mtime/atime/ctime.
fn dotnet_path_time_unix_secs(
    asm: &mut Assembly,
    path_local: u32,
    getter: &str,
    dt_local: u32,
) -> (Interned<CILRoot>, Interned<CILNode>) {
    let file = ClassRef::file(asm);
    let datetime = ClassRef::datetime(asm);
    let datetime_ty = Type::ClassRef(datetime);
    let datetime_ref = asm.nref(datetime_ty);

    // dt = File.<getter>(path) -> DateTime (static), stowed into dt_local.
    let getter_name = asm.alloc_string(getter);
    let get_time = asm.class_ref(file).clone().static_mref(
        &[Type::PlatformString],
        datetime_ty,
        getter_name,
        asm,
    );
    let path = asm.alloc_node(CILNode::LdLoc(path_local));
    let dt = asm.alloc_node(CILNode::call(get_time, [path]));
    let store_dt = asm.alloc_root(CILRoot::StLoc(dt_local, dt));

    // (&dt_local).Ticks -> int64.
    let get_ticks_name = asm.alloc_string("get_Ticks");
    let get_ticks = MethodRef::new(
        datetime,
        get_ticks_name,
        asm.sig([datetime_ref], Type::Int(Int::I64)),
        MethodKind::Instance,
        [].into(),
    );
    let get_ticks = asm.alloc_methodref(get_ticks);
    let dt_addr = asm.alloc_node(CILNode::LdLocA(dt_local));
    let ticks = asm.alloc_node(CILNode::call(get_ticks, [dt_addr]));

    // secs = (ticks - DOTNET_UNIX_EPOCH_TICKS) / DOTNET_TICKS_PER_SEC.
    let epoch = asm.alloc_node(DOTNET_UNIX_EPOCH_TICKS);
    let rebased = asm.alloc_node(CILNode::BinOp(ticks, epoch, BinOp::Sub));
    let per_sec = asm.alloc_node(DOTNET_TICKS_PER_SEC);
    let secs = asm.alloc_node(CILNode::BinOp(rebased, per_sec, BinOp::Div));
    (store_dt, secs)
}

/// `FileAttributes.ReparsePoint` flag (a symlink/junction is a reparse point on
/// every .NET-supported filesystem). `File.GetAttributes` returns the
/// `[Flags] enum : int FileAttributes`.
const DOTNET_FILE_ATTR_REPARSE_POINT: i32 = 0x400;

/// Build an i32 node that is 1 if `path_local` (`LdLoc`) is a symlink (the
/// `FileAttributes.ReparsePoint` bit is set), else 0. `File.GetAttributes`
/// returns the int-backed `FileAttributes` enum; we mask the reparse bit and
/// compare to non-zero. Used by `rcl_dotnet_fs_stat` (B2 Piece 2/4 is_symlink).
fn dotnet_path_is_symlink_i32(asm: &mut Assembly, path_local: u32) -> Interned<CILNode> {
    let file = ClassRef::file(asm);
    let file_attributes = {
        let name = asm.alloc_string("System.IO.FileAttributes");
        let asm_name = Some(asm.alloc_string("System.Runtime"));
        asm.alloc_class_ref(ClassRef::new(name, asm_name, true, [].into()))
    };
    let fa_ty = Type::ClassRef(file_attributes);
    let get_attrs_name = asm.alloc_string("GetAttributes");
    let get_attrs = asm.class_ref(file).clone().static_mref(
        &[Type::PlatformString],
        fa_ty,
        get_attrs_name,
        asm,
    );
    let path = asm.alloc_node(CILNode::LdLoc(path_local));
    let attrs = asm.alloc_node(CILNode::call(get_attrs, [path]));
    // (int)attrs & ReparsePoint  — the enum is int-backed, so the `and` is on i32.
    let attrs_i = bcl_enum_to_i32(attrs, fa_ty, asm);
    let reparse = asm.alloc_node(DOTNET_FILE_ATTR_REPARSE_POINT);
    let masked = asm.alloc_node(CILNode::BinOp(attrs_i, reparse, BinOp::And));
    // (masked != 0) as i32 — `Eq` yields 1/0; negate via `1 - (masked == 0)`.
    let zero = asm.alloc_node(0_i32);
    let is_zero = asm.alloc_node(CILNode::BinOp(masked, zero, BinOp::Eq));
    let is_zero = asm.int_cast(is_zero, Int::I32, ExtendKind::ZeroExtend);
    let one = asm.alloc_node(1_i32);
    asm.alloc_node(CILNode::BinOp(one, is_zero, BinOp::Sub))
}

fn insert_dotnet_fs_stat(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_stat");
    let generator = move |_, asm: &mut Assembly| {
        let directory = ClassRef::directory(asm);
        let file = ClassRef::file(asm);
        let file_info = ClassRef::file_info(asm);

        // Decode the path once into local 0 (it is re-read in two blocks).
        let path = decode_utf8(asm, 0, 1);
        let store_path = asm.alloc_root(CILRoot::StLoc(0, path));

        // Block 0: if Directory.Exists(path) goto dir(1) else goto file_check(2).
        let dir_exists_name = asm.alloc_string("Exists");
        let dir_exists = asm.class_ref(directory).clone().static_mref(
            &[Type::PlatformString],
            Type::Bool,
            dir_exists_name,
            asm,
        );
        let path0 = asm.alloc_node(CILNode::LdLoc(0));
        let is_dir = asm.alloc_node(CILNode::call(dir_exists, [path0]));
        let truec = asm.alloc_node(true);
        let br_dir = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(is_dir, truec)),
        ))));
        let goto_file = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // Block 1 (dir): *out_size = 0; *out_is_dir = 1; timestamps + is_symlink;
        //                return 0.
        let out_size1 = asm.alloc_node(CILNode::LdArg(2));
        let zero_u64 = asm.alloc_node(0_u64);
        let st_size1 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_size1,
            zero_u64,
            Type::Int(Int::U64),
            false,
        ))));
        let out_isdir1 = asm.alloc_node(CILNode::LdArg(3));
        let one_i32 = asm.alloc_node(1_i32);
        let st_isdir1 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_isdir1,
            one_i32,
            Type::Int(Int::I32),
            false,
        ))));
        // *out_mtime/atime/ctime = File.GetLast{Write,Access}TimeUtc /
        // GetCreationTimeUtc(path), rebased to Unix seconds. The static File
        // getters work for directories too (no FileInfo/DirectoryInfo split).
        let (store_mt1, mt1) = dotnet_path_time_unix_secs(asm, 0, "GetLastWriteTimeUtc", 2);
        let out_mt1 = asm.alloc_node(CILNode::LdArg(4));
        let st_mt1 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_mt1,
            mt1,
            Type::Int(Int::I64),
            false,
        ))));
        let (store_at1, at1) = dotnet_path_time_unix_secs(asm, 0, "GetLastAccessTimeUtc", 3);
        let out_at1 = asm.alloc_node(CILNode::LdArg(5));
        let st_at1 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_at1,
            at1,
            Type::Int(Int::I64),
            false,
        ))));
        let (store_ct1, ct1) = dotnet_path_time_unix_secs(asm, 0, "GetCreationTimeUtc", 4);
        let out_ct1 = asm.alloc_node(CILNode::LdArg(6));
        let st_ct1 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_ct1,
            ct1,
            Type::Int(Int::I64),
            false,
        ))));
        let sym1 = dotnet_path_is_symlink_i32(asm, 0);
        let out_sym1 = asm.alloc_node(CILNode::LdArg(7));
        let st_sym1 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_sym1,
            sym1,
            Type::Int(Int::I32),
            false,
        ))));
        let zero_ret1 = asm.alloc_node(0_i32);
        let ret_dir = asm.alloc_root(CILRoot::Ret(zero_ret1));

        // Block 2 (file_check): if File.Exists(path) goto file(3) else notfound(4).
        let file_exists_name = asm.alloc_string("Exists");
        let file_exists = asm.class_ref(file).clone().static_mref(
            &[Type::PlatformString],
            Type::Bool,
            file_exists_name,
            asm,
        );
        let path2 = asm.alloc_node(CILNode::LdLoc(0));
        let is_file = asm.alloc_node(CILNode::call(file_exists, [path2]));
        let truec2 = asm.alloc_node(true);
        let br_file = asm.alloc_root(CILRoot::Branch(Box::new((
            3,
            0,
            Some(BranchCond::Eq(is_file, truec2)),
        ))));
        let goto_notfound = asm.alloc_root(CILRoot::Branch(Box::new((4, 0, None))));

        // Block 3 (file): size = new FileInfo(path).Length; *out_size = (u64)size;
        //                 *out_is_dir = 0; return 0.
        let fi_ctor = asm
            .class_ref(file_info)
            .clone()
            .ctor(&[Type::PlatformString], asm);
        let path3 = asm.alloc_node(CILNode::LdLoc(0));
        let fi = asm.alloc_node(CILNode::call(fi_ctor, [path3]));
        let store_fi = asm.alloc_root(CILRoot::StLoc(1, fi));
        let get_len_name = asm.alloc_string("get_Length");
        let get_len =
            asm.class_ref(file_info)
                .clone()
                .instance(&[], Type::Int(Int::I64), get_len_name, asm);
        let ld_fi = asm.alloc_node(CILNode::LdLoc(1));
        let size_i64 = asm.alloc_node(CILNode::call(get_len, [ld_fi]));
        let size_u64 = asm.int_cast(size_i64, Int::U64, ExtendKind::ZeroExtend);
        let out_size3 = asm.alloc_node(CILNode::LdArg(2));
        let st_size3 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_size3,
            size_u64,
            Type::Int(Int::U64),
            false,
        ))));
        let out_isdir3 = asm.alloc_node(CILNode::LdArg(3));
        let zero_i32_3 = asm.alloc_node(0_i32);
        let st_isdir3 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_isdir3,
            zero_i32_3,
            Type::Int(Int::I32),
            false,
        ))));
        // *out_mtime/atime/ctime + *out_is_symlink (same as the dir block).
        let (store_mt3, mt3) = dotnet_path_time_unix_secs(asm, 0, "GetLastWriteTimeUtc", 2);
        let out_mt3 = asm.alloc_node(CILNode::LdArg(4));
        let st_mt3 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_mt3,
            mt3,
            Type::Int(Int::I64),
            false,
        ))));
        let (store_at3, at3) = dotnet_path_time_unix_secs(asm, 0, "GetLastAccessTimeUtc", 3);
        let out_at3 = asm.alloc_node(CILNode::LdArg(5));
        let st_at3 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_at3,
            at3,
            Type::Int(Int::I64),
            false,
        ))));
        let (store_ct3, ct3) = dotnet_path_time_unix_secs(asm, 0, "GetCreationTimeUtc", 4);
        let out_ct3 = asm.alloc_node(CILNode::LdArg(6));
        let st_ct3 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_ct3,
            ct3,
            Type::Int(Int::I64),
            false,
        ))));
        let sym3 = dotnet_path_is_symlink_i32(asm, 0);
        let out_sym3 = asm.alloc_node(CILNode::LdArg(7));
        let st_sym3 = asm.alloc_root(CILRoot::StInd(Box::new((
            out_sym3,
            sym3,
            Type::Int(Int::I32),
            false,
        ))));
        let zero_ret3 = asm.alloc_node(0_i32);
        let ret_file = asm.alloc_root(CILRoot::Ret(zero_ret3));

        // Block 4 (notfound): return -1.
        let neg1 = asm.alloc_node(-1_i32);
        let ret_nf = asm.alloc_root(CILRoot::Ret(neg1));

        let string_ty = asm.alloc_type(Type::PlatformString);
        let file_info_ty = asm.alloc_type(Type::ClassRef(file_info));
        let datetime_cref = ClassRef::datetime(asm);
        let datetime_ty = asm.alloc_type(Type::ClassRef(datetime_cref));
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_path, br_dir, goto_file], 0, None),
                BasicBlock::new(
                    vec![
                        st_size1, st_isdir1, store_mt1, st_mt1, store_at1, st_at1, store_ct1,
                        st_ct1, st_sym1, ret_dir,
                    ],
                    1,
                    None,
                ),
                BasicBlock::new(vec![br_file, goto_notfound], 2, None),
                BasicBlock::new(
                    vec![
                        store_fi, st_size3, st_isdir3, store_mt3, st_mt3, store_at3, st_at3,
                        store_ct3, st_ct3, st_sym3, ret_file,
                    ],
                    3,
                    None,
                ),
                BasicBlock::new(vec![ret_nf], 4, None),
            ],
            locals: vec![
                (Some(asm.alloc_string("path")), string_ty),
                (Some(asm.alloc_string("file_info")), file_info_ty),
                (Some(asm.alloc_string("dt_mtime")), datetime_ty),
                (Some(asm.alloc_string("dt_atime")), datetime_ty),
                (Some(asm.alloc_string("dt_ctime")), datetime_ty),
            ],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_exists(path_ptr, path_len) -> i32`
///   => `(File.Exists(path) || Directory.Exists(path)) ? 1 : 0`.
///
/// Never errno-based: the std side uses this for `Path::exists`, which must not
/// surface the `Uncategorized` io-error trap on a missing path.
fn insert_dotnet_fs_exists(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_exists", |asm| {
        let file = ClassRef::file(asm);
        let directory = ClassRef::directory(asm);
        let path = decode_utf8(asm, 0, 1);
        let store_path = asm.alloc_root(CILRoot::StLoc(0, path));

        // Block 0: if File.Exists(path) goto yes(1) else goto dir_check(2).
        let file_exists_name = asm.alloc_string("Exists");
        let file_exists = asm.class_ref(file).clone().static_mref(
            &[Type::PlatformString],
            Type::Bool,
            file_exists_name,
            asm,
        );
        let p0 = asm.alloc_node(CILNode::LdLoc(0));
        let is_file = asm.alloc_node(CILNode::call(file_exists, [p0]));
        let truec = asm.alloc_node(true);
        let br_yes = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(is_file, truec)),
        ))));
        let goto_dir = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // Block 1 (yes): return 1.
        let one = asm.alloc_node(1_i32);
        let ret_yes = asm.alloc_root(CILRoot::Ret(one));

        // Block 2 (dir_check): if Directory.Exists(path) goto yes2(3) else no(4).
        let dir_exists_name = asm.alloc_string("Exists");
        let dir_exists = asm.class_ref(directory).clone().static_mref(
            &[Type::PlatformString],
            Type::Bool,
            dir_exists_name,
            asm,
        );
        let p2 = asm.alloc_node(CILNode::LdLoc(0));
        let is_dir = asm.alloc_node(CILNode::call(dir_exists, [p2]));
        let truec2 = asm.alloc_node(true);
        let br_yes2 = asm.alloc_root(CILRoot::Branch(Box::new((
            3,
            0,
            Some(BranchCond::Eq(is_dir, truec2)),
        ))));
        let goto_no = asm.alloc_root(CILRoot::Branch(Box::new((4, 0, None))));

        // Block 3 (yes2): return 1.
        let one2 = asm.alloc_node(1_i32);
        let ret_yes2 = asm.alloc_root(CILRoot::Ret(one2));

        // Block 4 (no): return 0.
        let zero = asm.alloc_node(0_i32);
        let ret_no = asm.alloc_root(CILRoot::Ret(zero));

        let string_ty = asm.alloc_type(Type::PlatformString);
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_path, br_yes, goto_dir], 0, None),
                BasicBlock::new(vec![ret_yes], 1, None),
                BasicBlock::new(vec![br_yes2, goto_no], 2, None),
                BasicBlock::new(vec![ret_yes2], 3, None),
                BasicBlock::new(vec![ret_no], 4, None),
            ],
            locals: vec![(Some(asm.alloc_string("path")), string_ty)],
        }
    });
}

/// Shared shape for the path-in/`i32`-out fs hooks (`mkdir`/`rmdir`/`unlink`):
/// decode the path, issue one static System.IO call (consuming any result), and
/// return 0 on success. PAL-fidelity: the body is wrapped in
/// `super::posix::errno_wrapped`, so a managed fault sets the thread-local
/// `errno` (via `rcl_errno_from_exception`: FileNotFound→ENOENT,
/// UnauthorizedAccess→EACCES, …) and returns -1 instead of unwinding. The std fs
/// arm then reports the precise `ErrorKind` via `io::Error::last_os_error()`.
/// The wrapper owns blocks 1/2 and local 0, so the body uses ONLY block 0 and
/// stores nothing into local 0 (the success value 0 is filled by the wrapper's
/// `leave`; on a fault the wrapper stores -1).
fn insert_path_to_rc(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    symbol: &str,
    build_call: impl Fn(&mut Assembly, Interned<CILNode>) -> Interned<CILRoot> + 'static,
) {
    let name = asm.alloc_string(symbol);
    let generator = move |_, asm: &mut Assembly| {
        let path = decode_utf8(asm, 0, 1);
        let call = build_call(asm, path);
        // On success, local 0 (the wrapper's `result`) must hold 0.
        let zero = asm.alloc_node(0_i32);
        let store_ok = asm.alloc_root(CILRoot::StLoc(0, zero));
        super::posix::errno_wrapped(asm, vec![call, store_ok], Type::Int(Int::I32), vec![])
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_mkdir(path_ptr, path_len) -> i32`
///   => `Directory.CreateDirectory(path)` (result `DirectoryInfo` popped); 0.
fn insert_dotnet_fs_mkdir(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    insert_path_to_rc(asm, patcher, "rcl_dotnet_fs_mkdir", |asm, path| {
        let directory = ClassRef::directory(asm);
        // CreateDirectory(string) -> DirectoryInfo (a reference type).
        let dir_info = {
            let n = asm.alloc_string("System.IO.DirectoryInfo");
            let a = Some(asm.alloc_string("System.Runtime"));
            asm.alloc_class_ref(ClassRef::new(n, a, false, [].into()))
        };
        let create_name = asm.alloc_string("CreateDirectory");
        let create = asm.class_ref(directory).clone().static_mref(
            &[Type::PlatformString],
            Type::ClassRef(dir_info),
            create_name,
            asm,
        );
        let info = asm.alloc_node(CILNode::call(create, [path]));
        asm.alloc_root(CILRoot::Pop(info))
    });
}

/// `rcl_dotnet_fs_rmdir(path_ptr, path_len) -> i32`
///   => `Directory.Delete(path, recursive: false)`; 0.
fn insert_dotnet_fs_rmdir(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    insert_path_to_rc(asm, patcher, "rcl_dotnet_fs_rmdir", |asm, path| {
        let directory = ClassRef::directory(asm);
        let delete_name = asm.alloc_string("Delete");
        let delete = asm.class_ref(directory).clone().static_mref(
            &[Type::PlatformString, Type::Bool],
            Type::Void,
            delete_name,
            asm,
        );
        let falsec = asm.alloc_node(false);
        asm.alloc_root(CILRoot::call(delete, [path, falsec]))
    });
}

/// `rcl_dotnet_fs_unlink(path_ptr, path_len) -> i32` => `File.Delete(path)`; 0.
fn insert_dotnet_fs_unlink(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    insert_path_to_rc(asm, patcher, "rcl_dotnet_fs_unlink", |asm, path| {
        let file = ClassRef::file(asm);
        let delete_name = asm.alloc_string("Delete");
        let delete = asm.class_ref(file).clone().static_mref(
            &[Type::PlatformString],
            Type::Void,
            delete_name,
            asm,
        );
        asm.alloc_root(CILRoot::call(delete, [path]))
    });
}

/// `rcl_dotnet_fs_rename(old_ptr, old_len, new_ptr, new_len) -> i32`
///   => `File.Move(old, new, overwrite: true)`; 0 on success. PAL-fidelity:
/// wrapped in `errno_wrapped` (a missing source path throws FileNotFound→ENOENT,
/// an ACL denial throws UnauthorizedAccess→EACCES), so a fault sets `errno` and
/// returns -1 rather than unwinding. Body uses only block 0; local 0 is the
/// wrapper's `result`.
fn insert_dotnet_fs_rename(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_rename", |asm| {
        let old = decode_utf8(asm, 0, 1);
        let new = decode_utf8(asm, 2, 3);
        let file = ClassRef::file(asm);
        let move_name = asm.alloc_string("Move");
        let mv = asm.class_ref(file).clone().static_mref(
            &[Type::PlatformString, Type::PlatformString, Type::Bool],
            Type::Void,
            move_name,
            asm,
        );
        let truec = asm.alloc_node(true);
        let call = asm.alloc_root(CILRoot::call(mv, [old, new, truec]));
        let zero = asm.alloc_node(0_i32);
        let store_ok = asm.alloc_root(CILRoot::StLoc(0, zero));
        super::posix::errno_wrapped(asm, vec![call, store_ok], Type::Int(Int::I32), vec![])
    });
}

/// `rcl_dotnet_fs_readdir_open(path_ptr, path_len) -> *mut u8`
///   => `Directory.GetFileSystemEntries(path)` (a `string[]`), returned as an
///      opaque `GCHandle` `IntPtr`.
fn insert_dotnet_fs_readdir_open(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_readdir_open", |asm| {
        let string = asm.alloc_type(Type::PlatformString);
        let string_arr = Type::PlatformArray {
            elem: string,
            dims: NonZeroU8::new(1).unwrap(),
        };
        let path = decode_utf8(asm, 0, 1);
        let directory = ClassRef::directory(asm);
        let entries_name = asm.alloc_string("GetFileSystemEntries");
        let entries = asm.class_ref(directory).clone().static_mref(
            &[Type::PlatformString],
            string_arr,
            entries_name,
            asm,
        );
        let arr = asm.alloc_node(CILNode::call(entries, [path]));
        let store = asm.alloc_root(CILRoot::StLoc(0, arr));
        let handle = CILNode::LdLoc(0).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let void = asm.alloc_type(Type::Void);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let ret = asm.alloc_root(CILRoot::Ret(handle));
        let arr_ty = asm.alloc_type(string_arr);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("entries")), arr_ty)],
        }
    });
}

/// `rcl_dotnet_fs_readdir_count(handle) -> usize` => `string[].Length`.
fn insert_dotnet_fs_readdir_count(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_readdir_count", |asm| {
        let string = asm.alloc_type(Type::PlatformString);
        let string_arr = Type::PlatformArray {
            elem: string,
            dims: NonZeroU8::new(1).unwrap(),
        };
        // handle -> object -> (string[]) via the array's value-type-less castclass.
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let handle_isize = asm.alloc_node(CILNode::PtrCast(arg, Box::new(PtrCastRes::ISize)));
        let handle_to_obj = asm.alloc_string("handle_to_obj");
        let main_module = asm.main_module();
        let handle_to_obj = asm.class_ref(*main_module).clone().static_mref(
            &[Type::Int(Int::ISize)],
            Type::PlatformObject,
            handle_to_obj,
            asm,
        );
        let obj = asm.alloc_node(CILNode::call(handle_to_obj, [handle_isize]));
        let arr_ty = asm.alloc_type(string_arr);
        let arr = asm.alloc_node(CILNode::CheckedCast(obj, arr_ty));
        let len = asm.ld_len(arr);
        let len = asm.int_cast(len, Int::USize, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(len));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_fs_readdir_get(handle, idx) -> *mut u8`
///   => `Marshal.StringToCoTaskMemUTF8(string[][idx])` (caller frees via
///      `rcl_dotnet_cotaskmem_free`).
fn insert_dotnet_fs_readdir_get(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_readdir_get", |asm| {
        let u8_ptr = asm.nptr(Type::Int(Int::U8));
        let string = asm.alloc_type(Type::PlatformString);
        let string_arr = Type::PlatformArray {
            elem: string,
            dims: NonZeroU8::new(1).unwrap(),
        };
        let arg = asm.alloc_node(CILNode::LdArg(0));
        let handle_isize = asm.alloc_node(CILNode::PtrCast(arg, Box::new(PtrCastRes::ISize)));
        let handle_to_obj = asm.alloc_string("handle_to_obj");
        let main_module = asm.main_module();
        let handle_to_obj = asm.class_ref(*main_module).clone().static_mref(
            &[Type::Int(Int::ISize)],
            Type::PlatformObject,
            handle_to_obj,
            asm,
        );
        let obj = asm.alloc_node(CILNode::call(handle_to_obj, [handle_isize]));
        let arr_ty = asm.alloc_type(string_arr);
        let arr = asm.alloc_node(CILNode::CheckedCast(obj, arr_ty));
        let idx = asm.alloc_node(CILNode::LdArg(1));
        let entry = asm.ld_elem_ref(arr, idx);
        let to_utf8 = string_to_utf8(asm);
        let buf = asm.alloc_node(CILNode::call(to_utf8, [entry]));
        let buf = asm.cast_ptr(buf, u8_ptr);
        let ret = asm.alloc_root(CILRoot::Ret(buf));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_fs_readdir_close(handle)` => free the `string[]` `GCHandle`.
fn insert_dotnet_fs_readdir_close(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_fs_readdir_close", |asm| {
        let (store_gch, free, gc_handle_ty) = free_handle_roots(asm, 0, 0);
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store_gch, free, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("gch")), gc_handle_ty)],
        }
    });
}

// ===========================================================================
// net (`sys/net/connection/dotnet.rs`)
//
// An open socket is a `GCHandle` to a `System.Net.Sockets.Socket`. A SocketAddr
// crosses the ABI as `(family, ip_ptr, ip_len, port)` (network-order octets);
// addresses return through caller out-pointers. IPEndPoint/IPAddress are built
// and read entirely BCL-side (inline CIL) — no managed object crosses a Rust
// signature. See the module-doc net contract above and `insert_dotnet_fs`.
// ===========================================================================

/// `.NET SocketType` int-backed enum: Stream=1, Dgram=2 (these happen to equal
/// our ABI `SOCK_STREAM`/`SOCK_DGRAM`, so the std side's value passes straight in).
const NET_SOCKTYPE_STREAM: i32 = 1;
/// `.NET ProtocolType`: Tcp=6, Udp=17.
const NET_PROTO_TCP: i32 = 6;
const NET_PROTO_UDP: i32 = 17;

/// Build a `System.Net.IPEndPoint` from `(ip_ptr, ip_len, port)` at the given
/// `LdArg` indices, inline:
///   `new IPEndPoint(new IPAddress(new ReadOnlySpan<byte>(ip_ptr, (int)ip_len)),
///                   (int)port)`.
/// The span length picks v4/v6; the octets are network order (no byte-swap), and
/// `IPEndPoint`'s port is host-order (no `to_be`).
pub(crate) fn build_endpoint(
    asm: &mut Assembly,
    ip_ptr_arg: u32,
    ip_len_arg: u32,
    port_arg: u32,
) -> Interned<CILNode> {
    // new IPAddress(ReadOnlySpan<byte>(ip_ptr, ip_len))
    let (span, span_ty) = build_byte_span(asm, ip_ptr_arg, ip_len_arg, true);
    let ip_address = ClassRef::ip_address(asm);
    let ip_ctor = asm.class_ref(ip_address).clone().ctor(&[span_ty], asm);
    let addr = asm.alloc_node(CILNode::call(ip_ctor, [span]));
    // new IPEndPoint(IPAddress, (int)port)
    let port = asm.alloc_node(CILNode::LdArg(port_arg));
    let port = asm.int_cast(port, Int::I32, ExtendKind::ZeroExtend);
    let ip_endpoint = ClassRef::ip_endpoint(asm);
    let ep_ctor = asm
        .class_ref(ip_endpoint)
        .clone()
        .ctor(&[Type::ClassRef(ip_address), Type::Int(Int::I32)], asm);
    asm.alloc_node(CILNode::call(ep_ctor, [addr, port]))
}

/// Upcast an `IPEndPoint` node to the base `System.Net.EndPoint` (a `castclass`,
/// always sound for an upcast) — the declared param type of `Socket.Bind` /
/// `Connect` / `SendTo`. Needed because the cilly typechecker does not model
/// class inheritance, so an `IPEndPoint`-typed arg would not match an `EndPoint`
/// param, and the BCL only exposes those methods on the `EndPoint` base.
pub(crate) fn endpoint_as_base(
    asm: &mut Assembly,
    ip_endpoint_node: Interned<CILNode>,
) -> Interned<CILNode> {
    let endpoint_base = ClassRef::endpoint(asm);
    let base_ty = asm.alloc_type(Type::ClassRef(endpoint_base));
    asm.alloc_node(CILNode::CheckedCast(ip_endpoint_node, base_ty))
}

/// Build `new Socket(endpoint.AddressFamily, (SocketType)sock_type,
/// (ProtocolType)proto)` for an already-built `IPEndPoint` node. The address
/// family is read off the endpoint (so no family-int translation is needed); the
/// `sock_type`/`proto` int nodes feed the int-backed enum params directly (the
/// ctor signature must still name the enum types or BCL method resolution fails).
pub(crate) fn build_socket(
    asm: &mut Assembly,
    endpoint: Interned<CILNode>,
    ep_local: u32,
    sock_type: Interned<CILNode>,
    proto: Interned<CILNode>,
) -> (Interned<CILRoot>, Interned<CILNode>, Interned<Type>) {
    let ip_endpoint = ClassRef::ip_endpoint(asm);
    let store_ep = asm.alloc_root(CILRoot::StLoc(ep_local, endpoint));
    // (AddressFamily)endpoint.AddressFamily — inherited getter on IPEndPoint.
    let address_family = ClassRef::address_family(asm);
    let get_af_name = asm.alloc_string("get_AddressFamily");
    let get_af = asm.class_ref(ip_endpoint).clone().instance(
        &[],
        Type::ClassRef(address_family),
        get_af_name,
        asm,
    );
    let ep0 = asm.alloc_node(CILNode::LdLoc(ep_local));
    let af = asm.alloc_node(CILNode::call(get_af, [ep0]));
    // new Socket(AddressFamily, SocketType, ProtocolType)
    let socket = ClassRef::socket(asm);
    let socket_type = Type::ClassRef(ClassRef::socket_type(asm));
    let protocol_type = Type::ClassRef(ClassRef::protocol_type(asm));
    let sock_type = i32_to_bcl_enum(sock_type, socket_type, asm);
    let proto = i32_to_bcl_enum(proto, protocol_type, asm);
    let sock_ctor = asm.class_ref(socket).clone().ctor(
        &[Type::ClassRef(address_family), socket_type, protocol_type],
        asm,
    );
    let sock = asm.alloc_node(CILNode::call(sock_ctor, [af, sock_type, proto]));
    let ep_ty = asm.alloc_type(Type::ClassRef(ip_endpoint));
    (store_ep, sock, ep_ty)
}

/// Recover the `Socket` pinned by the handle at `LdArg(handle_arg)`. Thin wrapper
/// over `handle_to_class` for readability.
pub(crate) fn handle_to_socket(asm: &mut Assembly, handle_arg: u32) -> Interned<CILNode> {
    let socket = ClassRef::socket(asm);
    handle_to_class(asm, handle_arg, socket)
}

/// `(IntPtr)GCHandle.Alloc(socket_in_local)` as a `*mut u8`, ready to return as
/// an opaque handle. Mirrors the `ref_to_handle` tail of `insert_dotnet_fs_open`.
pub(crate) fn socket_local_to_handle(asm: &mut Assembly, sock_local: u32) -> Interned<CILNode> {
    let handle = CILNode::LdLoc(sock_local).ref_to_handle(asm);
    let handle = asm.alloc_node(handle);
    let void = asm.alloc_type(Type::Void);
    asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))))
}

/// Build the roots that marshal an `IPEndPoint` (held in `ep_local`) out into the
/// caller's `(out_family, out_ip, out_port)` pointers at the given `LdArg`
/// indices:
///   `byte[] b = ((IPEndPoint)ep).Address.GetAddressBytes();`
///   `*out_family = b.Length; Marshal.Copy(b, 0, (IntPtr)out_ip, b.Length);`
///   `*out_port = (ushort)ep.Port;`
/// `out_family` receives the IP byte length (4 v4 / 16 v6); the std side maps it.
/// `bytes_local` must be a `byte[]` local.
pub(crate) fn write_endpoint_out(
    asm: &mut Assembly,
    ep_local: u32,
    bytes_local: u32,
    out_family_arg: u32,
    out_ip_arg: u32,
    out_port_arg: u32,
) -> Vec<Interned<CILRoot>> {
    let ip_endpoint = ClassRef::ip_endpoint(asm);
    let ip_address = ClassRef::ip_address(asm);
    let byte_ty = asm.alloc_type(Type::Int(Int::U8));
    let byte_arr = Type::PlatformArray {
        elem: byte_ty,
        dims: NonZeroU8::new(1).unwrap(),
    };

    // addr = ep.get_Address(); b = addr.GetAddressBytes(); store b.
    let get_addr_name = asm.alloc_string("get_Address");
    let get_addr = asm.class_ref(ip_endpoint).clone().instance(
        &[],
        Type::ClassRef(ip_address),
        get_addr_name,
        asm,
    );
    let ep0 = asm.alloc_node(CILNode::LdLoc(ep_local));
    let addr = asm.alloc_node(CILNode::call(get_addr, [ep0]));
    let get_bytes_name = asm.alloc_string("GetAddressBytes");
    let get_bytes = asm
        .class_ref(ip_address)
        .clone()
        .instance(&[], byte_arr, get_bytes_name, asm);
    let bytes = asm.alloc_node(CILNode::call(get_bytes, [addr]));
    let store_bytes = asm.alloc_root(CILRoot::StLoc(bytes_local, bytes));

    // len = b.Length (i32); *out_family = len (4 v4 / 16 v6).
    let b0 = asm.alloc_node(CILNode::LdLoc(bytes_local));
    let len = asm.ld_len(b0);
    let len_i32 = asm.int_cast(len, Int::I32, ExtendKind::ZeroExtend);
    let out_family = asm.alloc_node(CILNode::LdArg(out_family_arg));
    let st_family = asm.alloc_root(CILRoot::StInd(Box::new((
        out_family,
        len_i32,
        Type::Int(Int::I32),
        false,
    ))));

    // Marshal.Copy(byte[] src, int 0, IntPtr dst, int len).
    let marshal = ClassRef::marshal(asm);
    let copy_name = asm.alloc_string("Copy");
    let copy = asm.class_ref(marshal).clone().static_mref(
        &[
            byte_arr,
            Type::Int(Int::I32),
            Type::Int(Int::ISize),
            Type::Int(Int::I32),
        ],
        Type::Void,
        copy_name,
        asm,
    );
    let b1 = asm.alloc_node(CILNode::LdLoc(bytes_local));
    let zero = asm.alloc_node(0_i32);
    let out_ip = asm.alloc_node(CILNode::LdArg(out_ip_arg));
    let out_ip_isize = asm.int_cast(out_ip, Int::ISize, ExtendKind::ZeroExtend);
    let b2 = asm.alloc_node(CILNode::LdLoc(bytes_local));
    let len2 = asm.ld_len(b2);
    let len2_i32 = asm.int_cast(len2, Int::I32, ExtendKind::ZeroExtend);
    let copy = asm.alloc_root(CILRoot::call(copy, [b1, zero, out_ip_isize, len2_i32]));

    // *out_port = (ushort)ep.get_Port().
    let get_port_name = asm.alloc_string("get_Port");
    let get_port =
        asm.class_ref(ip_endpoint)
            .clone()
            .instance(&[], Type::Int(Int::I32), get_port_name, asm);
    let ep1 = asm.alloc_node(CILNode::LdLoc(ep_local));
    let port = asm.alloc_node(CILNode::call(get_port, [ep1]));
    let port_u16 = asm.int_cast(port, Int::U16, ExtendKind::ZeroExtend);
    let out_port = asm.alloc_node(CILNode::LdArg(out_port_arg));
    let st_port = asm.alloc_root(CILRoot::StInd(Box::new((
        out_port,
        port_u16,
        Type::Int(Int::U16),
        false,
    ))));

    vec![store_bytes, st_family, copy, st_port]
}

/// Registers all `rcl_dotnet_net_*` BCL bindings (System.Net.Sockets).
fn insert_dotnet_net(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    insert_dotnet_net_tcp_connect(asm, patcher);
    insert_dotnet_net_socket(asm, patcher);
    insert_dotnet_net_bind(asm, patcher);
    insert_dotnet_net_accept(asm, patcher);
    insert_dotnet_net_recv(asm, patcher);
    insert_dotnet_net_send(asm, patcher);
    insert_dotnet_net_recv_from(asm, patcher);
    insert_dotnet_net_send_to(asm, patcher);
    insert_dotnet_net_addr(
        asm,
        patcher,
        "rcl_dotnet_net_local_addr",
        "get_LocalEndPoint",
    );
    insert_dotnet_net_addr(
        asm,
        patcher,
        "rcl_dotnet_net_peer_addr",
        "get_RemoteEndPoint",
    );
    insert_dotnet_net_udp_connect(asm, patcher);
    insert_dotnet_net_shutdown(asm, patcher);
    insert_dotnet_net_set_nonblocking(asm, patcher);
    insert_dotnet_net_set_nodelay(asm, patcher);
    insert_dotnet_net_nodelay(asm, patcher);
    insert_dotnet_net_close(asm, patcher);
    insert_dotnet_socket_poll(asm, patcher);
    insert_dotnet_eventfd(asm, patcher);
    insert_dotnet_pipe_pair(asm, patcher);
}

/// `rcl_dotnet_net_tcp_connect(family, ip_ptr, ip_len, port) -> *mut u8`
///   => `var s = new Socket(ep.AddressFamily, Stream, Tcp); s.Connect(ep);`
///      return the `GCHandle` `IntPtr`.
/// (Args: 0=family [unused — the family is read off the endpoint], 1=ip_ptr,
/// 2=ip_len, 3=port.)
fn insert_dotnet_net_tcp_connect(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_tcp_connect", |asm| {
        let socket = ClassRef::socket(asm);
        let endpoint_base = ClassRef::endpoint(asm);
        let endpoint = build_endpoint(asm, 1, 2, 3);
        let stream = asm.alloc_node(NET_SOCKTYPE_STREAM);
        let tcp = asm.alloc_node(NET_PROTO_TCP);
        // local 0 = endpoint, local 1 = socket.
        let (store_ep, sock, ep_ty) = build_socket(asm, endpoint, 0, stream, tcp);
        let store_sock = asm.alloc_root(CILRoot::StLoc(1, sock));
        // s.Connect(EndPoint) — Connect is declared on the EndPoint base.
        let connect_name = asm.alloc_string("Connect");
        let connect = asm.class_ref(socket).clone().instance(
            &[Type::ClassRef(endpoint_base)],
            Type::Void,
            connect_name,
            asm,
        );
        let sock0 = asm.alloc_node(CILNode::LdLoc(1));
        let ep1 = asm.alloc_node(CILNode::LdLoc(0));
        let ep1 = endpoint_as_base(asm, ep1);
        let do_connect = asm.alloc_root(CILRoot::call(connect, [sock0, ep1]));
        // return handle.
        let handle = socket_local_to_handle(asm, 1);
        let ret = asm.alloc_root(CILRoot::Ret(handle));
        let sock_ty = asm.alloc_type(Type::ClassRef(socket));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![store_ep, store_sock, do_connect, ret],
                0,
                None,
            )],
            locals: vec![
                (Some(asm.alloc_string("endpoint")), ep_ty),
                (Some(asm.alloc_string("socket")), sock_ty),
            ],
        }
    });
}

/// `rcl_dotnet_net_socket(af_dotnet, sock_type, proto) -> *mut u8`
///   => `var s = new Socket((AddressFamily)af_dotnet, (SocketType)sock_type,
///      (ProtocolType)proto);` return the `GCHandle` `IntPtr`.
///
/// The endpoint-free `socket()` constructor (POSIX `socket(2)` creates an unbound
/// socket; `build_socket` always wants an endpoint to read the family off). The
/// caller (the POSIX `socket` wrapper in `posix.rs`) passes the *already-.NET*
/// `AddressFamily` value (AF_INET 2 → InterNetwork 2; AF_INET6 10 → InterNetworkV6
/// 23), the `SocketType` int (Stream 1 / Dgram 2) and `ProtocolType` int (Tcp 6 /
/// Udp 17) straight through — the int-backed enum value-types are stack-compatible.
fn insert_dotnet_net_socket(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_socket", |asm| {
        let socket = ClassRef::socket(asm);
        let address_family = ClassRef::address_family(asm);
        let socket_type = Type::ClassRef(ClassRef::socket_type(asm));
        let protocol_type = Type::ClassRef(ClassRef::protocol_type(asm));
        let af = asm.alloc_node(CILNode::LdArg(0));
        let st = asm.alloc_node(CILNode::LdArg(1));
        let proto = asm.alloc_node(CILNode::LdArg(2));
        let af = i32_to_bcl_enum(af, Type::ClassRef(address_family), asm);
        let st = i32_to_bcl_enum(st, socket_type, asm);
        let proto = i32_to_bcl_enum(proto, protocol_type, asm);
        let sock_ctor = asm.class_ref(socket).clone().ctor(
            &[Type::ClassRef(address_family), socket_type, protocol_type],
            asm,
        );
        let sock = asm.alloc_node(CILNode::call(sock_ctor, [af, st, proto]));
        let store_sock = asm.alloc_root(CILRoot::StLoc(0, sock));
        let handle = socket_local_to_handle(asm, 0);
        let ret = asm.alloc_root(CILRoot::Ret(handle));
        let sock_ty = asm.alloc_type(Type::ClassRef(socket));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store_sock, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("socket")), sock_ty)],
        }
    });
}

/// `rcl_dotnet_net_bind(family, ip_ptr, ip_len, port, sock_type, backlog) -> *mut u8`
///   => `var s = new Socket(ep.AddressFamily, (SocketType)sock_type,
///      sock_type==Stream?Tcp:Udp); s.Bind(ep); if (backlog >= 0) s.Listen(backlog);`
///      return handle. (Args: 1=ip_ptr,2=ip_len,3=port,4=sock_type,5=backlog.)
fn insert_dotnet_net_bind(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_bind", |asm| {
        let socket = ClassRef::socket(asm);
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let sock_ty = asm.alloc_type(Type::ClassRef(socket));
        let ep_ty_t = asm.alloc_type(Type::ClassRef(ip_endpoint));

        // Block 0: choose proto by sock_type (4): Stream => Tcp(2), else Udp(1).
        // We branch the *whole* construction by proto into blocks 1 (tcp) / 2 (udp),
        // then converge to block 3 for Bind/[Listen]/return.
        let sock_type0 = asm.alloc_node(CILNode::LdArg(4));
        let stream_c = asm.alloc_node(NET_SOCKTYPE_STREAM);
        let br_tcp = asm.alloc_root(CILRoot::Branch(Box::new((
            1,
            0,
            Some(BranchCond::Eq(sock_type0, stream_c)),
        ))));
        let goto_udp = asm.alloc_root(CILRoot::Branch(Box::new((2, 0, None))));

        // Block 1 (tcp): build socket with (Stream, Tcp), store in local 1, goto 3.
        let ep_tcp = build_endpoint(asm, 1, 2, 3);
        let stream1 = asm.alloc_node(NET_SOCKTYPE_STREAM);
        let tcp1 = asm.alloc_node(NET_PROTO_TCP);
        let (store_ep_tcp, sock_tcp, _) = build_socket(asm, ep_tcp, 0, stream1, tcp1);
        let store_sock_tcp = asm.alloc_root(CILRoot::StLoc(1, sock_tcp));
        let goto_bind_tcp = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // Block 2 (udp): build socket with (Dgram, Udp), store in local 1, goto 3.
        let ep_udp = build_endpoint(asm, 1, 2, 3);
        let dgram2 = asm.alloc_node(CILNode::LdArg(4)); // sock_type == Dgram(2) here
        let udp2 = asm.alloc_node(NET_PROTO_UDP);
        let (store_ep_udp, sock_udp, _) = build_socket(asm, ep_udp, 0, dgram2, udp2);
        let store_sock_udp = asm.alloc_root(CILRoot::StLoc(1, sock_udp));
        let goto_bind_udp = asm.alloc_root(CILRoot::Branch(Box::new((3, 0, None))));

        // Block 3 (bind): s.Bind(ep); if (backlog >= 0) goto listen(4) else done(5).
        // Bind is declared on the EndPoint base; upcast the IPEndPoint local.
        let endpoint_base = ClassRef::endpoint(asm);
        let bind_name = asm.alloc_string("Bind");
        let bind = asm.class_ref(socket).clone().instance(
            &[Type::ClassRef(endpoint_base)],
            Type::Void,
            bind_name,
            asm,
        );
        let sock3 = asm.alloc_node(CILNode::LdLoc(1));
        let ep3 = asm.alloc_node(CILNode::LdLoc(0));
        let ep3 = endpoint_as_base(asm, ep3);
        let do_bind = asm.alloc_root(CILRoot::call(bind, [sock3, ep3]));
        let backlog3 = asm.alloc_node(CILNode::LdArg(5));
        let zero3 = asm.alloc_node(0_i32);
        // backlog < 0  => skip listen (UDP). Branch to done(5) when backlog < 0.
        let br_done = asm.alloc_root(CILRoot::Branch(Box::new((
            5,
            0,
            Some(BranchCond::Lt(
                backlog3,
                zero3,
                crate::ir::cilroot::CmpKind::Signed,
            )),
        ))));
        let goto_listen = asm.alloc_root(CILRoot::Branch(Box::new((4, 0, None))));

        // Block 4 (listen): s.Listen(backlog); goto done(5).
        let listen_name = asm.alloc_string("Listen");
        let listen = asm.class_ref(socket).clone().instance(
            &[Type::Int(Int::I32)],
            Type::Void,
            listen_name,
            asm,
        );
        let sock4 = asm.alloc_node(CILNode::LdLoc(1));
        let backlog4 = asm.alloc_node(CILNode::LdArg(5));
        let do_listen = asm.alloc_root(CILRoot::call(listen, [sock4, backlog4]));
        let goto_done = asm.alloc_root(CILRoot::Branch(Box::new((5, 0, None))));

        // Block 5 (done): return handle.
        let handle = socket_local_to_handle(asm, 1);
        let ret = asm.alloc_root(CILRoot::Ret(handle));

        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![br_tcp, goto_udp], 0, None),
                BasicBlock::new(vec![store_ep_tcp, store_sock_tcp, goto_bind_tcp], 1, None),
                BasicBlock::new(vec![store_ep_udp, store_sock_udp, goto_bind_udp], 2, None),
                BasicBlock::new(vec![do_bind, br_done, goto_listen], 3, None),
                BasicBlock::new(vec![do_listen, goto_done], 4, None),
                BasicBlock::new(vec![ret], 5, None),
            ],
            locals: vec![
                (Some(asm.alloc_string("endpoint")), ep_ty_t),
                (Some(asm.alloc_string("socket")), sock_ty),
            ],
        }
    });
}

/// `rcl_dotnet_net_accept(handle, out_family, out_ip, out_port) -> *mut u8`
///   => `var c = s.Accept(); write(c.RemoteEndPoint, out_*);` return c's handle.
/// (Args: 0=handle, 1=out_family, 2=out_ip, 3=out_port.)
fn insert_dotnet_net_accept(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_accept", |asm| {
        let socket = ClassRef::socket(asm);
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let endpoint_base = ClassRef::endpoint(asm);

        // c = s.Accept(); store c in local 0.
        let s = handle_to_socket(asm, 0);
        let accept_name = asm.alloc_string("Accept");
        let accept =
            asm.class_ref(socket)
                .clone()
                .instance(&[], Type::ClassRef(socket), accept_name, asm);
        let conn = asm.alloc_node(CILNode::call(accept, [s]));
        let store_conn = asm.alloc_root(CILRoot::StLoc(0, conn));

        // ep = (IPEndPoint)c.RemoteEndPoint; store in local 1.
        let get_rep_name = asm.alloc_string("get_RemoteEndPoint");
        let get_rep = asm.class_ref(socket).clone().instance(
            &[],
            Type::ClassRef(endpoint_base),
            get_rep_name,
            asm,
        );
        let conn0 = asm.alloc_node(CILNode::LdLoc(0));
        let ep_obj = asm.alloc_node(CILNode::call(get_rep, [conn0]));
        let ip_ep_ty = asm.alloc_type(Type::ClassRef(ip_endpoint));
        let ep = asm.alloc_node(CILNode::CheckedCast(ep_obj, ip_ep_ty));
        let store_ep = asm.alloc_root(CILRoot::StLoc(1, ep));

        // write peer addr out (local 1 = ep, local 2 = byte[] scratch).
        let mut roots = vec![store_conn, store_ep];
        roots.extend(write_endpoint_out(asm, 1, 2, 1, 2, 3));

        // return (void*)GCHandle.Alloc(conn).
        let handle = socket_local_to_handle(asm, 0);
        let ret = asm.alloc_root(CILRoot::Ret(handle));
        roots.push(ret);

        let sock_ty = asm.alloc_type(Type::ClassRef(socket));
        let ep_ty = asm.alloc_type(Type::ClassRef(ip_endpoint));
        let byte_ty = asm.alloc_type(Type::Int(Int::U8));
        let byte_arr_ty = asm.alloc_type(Type::PlatformArray {
            elem: byte_ty,
            dims: NonZeroU8::new(1).unwrap(),
        });
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals: vec![
                (Some(asm.alloc_string("conn")), sock_ty),
                (Some(asm.alloc_string("endpoint")), ep_ty),
                (Some(asm.alloc_string("bytes")), byte_arr_ty),
            ],
        }
    });
}

/// `rcl_dotnet_net_recv(handle, buf_ptr, len) -> isize`
///   => `s.Receive(new Span<byte>(buf_ptr, (int)len))` (0 == orderly shutdown).
fn insert_dotnet_net_recv(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_recv", |asm| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        let (span, span_ty) = build_byte_span(asm, 1, 2, false);
        let recv_name = asm.alloc_string("Receive");
        let recv =
            asm.class_ref(socket)
                .clone()
                .instance(&[span_ty], Type::Int(Int::I32), recv_name, asm);
        let count = asm.alloc_node(CILNode::call(recv, [s, span]));
        let count = asm.int_cast(count, Int::ISize, ExtendKind::SignExtend);
        // WouldBlock fix: a non-blocking Socket.Receive after Socket.Poll says
        // ready can still race and throw SocketException(WouldBlock/10035). Wrap
        // the body in the POSIX errno catch (result -> local 0) so that race
        // returns -1 / errno=EAGAIN cleanly instead of an uncaught managed
        // exception propagating up through std's recv. `errno_wrapped`'s default
        // mapper (`rcl_errno_from_exception`) maps WouldBlock(10035)->EAGAIN(11)
        // and delegates other SocketExceptions to their real errno.
        let store = asm.alloc_root(CILRoot::StLoc(0, count));
        super::posix::errno_wrapped(asm, vec![store], Type::Int(Int::ISize), vec![])
    });
}

/// `rcl_dotnet_net_send(handle, buf_ptr, len) -> isize`
///   => `s.Send(new ReadOnlySpan<byte>(buf_ptr, (int)len))` (count sent).
fn insert_dotnet_net_send(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_send", |asm| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        let (span, span_ty) = build_byte_span(asm, 1, 2, true);
        let send_name = asm.alloc_string("Send");
        let send =
            asm.class_ref(socket)
                .clone()
                .instance(&[span_ty], Type::Int(Int::I32), send_name, asm);
        let count = asm.alloc_node(CILNode::call(send, [s, span]));
        let count = asm.int_cast(count, Int::ISize, ExtendKind::SignExtend);
        let ret = asm.alloc_root(CILRoot::Ret(count));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_net_recv_from(handle, buf_ptr, len, out_family, out_ip, out_port) -> isize`
///   => `EndPoint ep = new IPEndPoint(0L, 0);   // 0.0.0.0:0 seed
///      int n = s.ReceiveFrom(new Span<byte>(buf_ptr, (int)len), ref ep);
///      write((IPEndPoint)ep, out_*); return n;`
/// The seed uses the `IPEndPoint(long, int)` ctor (an IPv4 0.0.0.0 placeholder)
/// rather than `IPAddress.Any` — `Any`/`IPv6Any` are static *fields*, not
/// property getters, and the long ctor avoids loading a static field. `ReceiveFrom`
/// overwrites `ep` with the real sender (v4 or v6) regardless of the seed family.
/// (Args: 0=handle,1=buf_ptr,2=len,3=out_family,4=out_ip,5=out_port.)
fn insert_dotnet_net_recv_from(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_recv_from", |asm| {
        let socket = ClassRef::socket(asm);
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let endpoint_base = ClassRef::endpoint(asm);

        // seed: EndPoint ep = new IPEndPoint(0L, 0); store in local 0.
        let zero_addr = asm.alloc_node(0_i64);
        let zero_port = asm.alloc_node(0_i32);
        let ep_ctor = asm
            .class_ref(ip_endpoint)
            .clone()
            .ctor(&[Type::Int(Int::I64), Type::Int(Int::I32)], asm);
        let seed = asm.alloc_node(CILNode::call(ep_ctor, [zero_addr, zero_port]));
        // ep local is typed as the base EndPoint (the `ref EndPoint` param type).
        let ep_base_ty = asm.alloc_type(Type::ClassRef(endpoint_base));
        let seed = asm.alloc_node(CILNode::CheckedCast(seed, ep_base_ty));
        // WouldBlock fix: wrap the body in the POSIX errno catch. errno_wrapped
        // reserves local 0 = result (isize) and appends our extra_locals AFTER
        // it, so every local index here is shifted up by one:
        //   result=0, endpoint=1, count=2, ip_endpoint=3, bytes=4.
        let store_seed = asm.alloc_root(CILRoot::StLoc(1, seed));

        // n = s.ReceiveFrom(Span<byte>(buf,len), ref ep).
        let s = handle_to_socket(asm, 0); // arg0 is the socket handle
        let (span, span_ty) = build_byte_span(asm, 1, 2, false);
        let ep_ref_ty = asm.nref(Type::ClassRef(endpoint_base));
        let recv_from_name = asm.alloc_string("ReceiveFrom");
        let recv_from = asm.class_ref(socket).clone().instance(
            &[span_ty, ep_ref_ty],
            Type::Int(Int::I32),
            recv_from_name,
            asm,
        );
        let ep_addr = asm.alloc_node(CILNode::LdLocA(1));
        let n = asm.alloc_node(CILNode::call(recv_from, [s, span, ep_addr]));
        let store_n = asm.alloc_root(CILRoot::StLoc(2, n));

        // ep2 = (IPEndPoint)ep; store in local 3; write addr out (bytes scratch = 4).
        let ep_obj = asm.alloc_node(CILNode::LdLoc(1));
        let ip_ep_ty = asm.alloc_type(Type::ClassRef(ip_endpoint));
        let ep2 = asm.alloc_node(CILNode::CheckedCast(ep_obj, ip_ep_ty));
        let store_ep2 = asm.alloc_root(CILRoot::StLoc(3, ep2));
        let mut roots = vec![store_seed, store_n, store_ep2];
        // out args (family=3, ip=4, port=5) are LdArg indices, unaffected by the
        // local shift; only the ep_local (3) and bytes_local (4) move up.
        roots.extend(write_endpoint_out(asm, 3, 4, 3, 4, 5));

        // store (isize)n in local 0 (result); errno_wrapped emits the ret.
        let n_load = asm.alloc_node(CILNode::LdLoc(2));
        let n_isize = asm.int_cast(n_load, Int::ISize, ExtendKind::SignExtend);
        let store_result = asm.alloc_root(CILRoot::StLoc(0, n_isize));
        roots.push(store_result);

        let ep_ty = asm.alloc_type(Type::ClassRef(ip_endpoint));
        let byte_ty = asm.alloc_type(Type::Int(Int::U8));
        let byte_arr_ty = asm.alloc_type(Type::PlatformArray {
            elem: byte_ty,
            dims: NonZeroU8::new(1).unwrap(),
        });
        let i32_ty = asm.alloc_type(Type::Int(Int::I32));
        let n_endpoint = asm.alloc_string("endpoint");
        let n_count = asm.alloc_string("count");
        let n_ip_endpoint = asm.alloc_string("ip_endpoint");
        let n_bytes = asm.alloc_string("bytes");
        let extra_locals = vec![
            (Some(n_endpoint), ep_base_ty),
            (Some(n_count), i32_ty),
            (Some(n_ip_endpoint), ep_ty),
            (Some(n_bytes), byte_arr_ty),
        ];
        super::posix::errno_wrapped(asm, roots, Type::Int(Int::ISize), extra_locals)
    });
}

/// `rcl_dotnet_net_send_to(handle, buf_ptr, len, family, ip_ptr, ip_len, port) -> isize`
///   => `s.SendTo(new ReadOnlySpan<byte>(buf_ptr, (int)len), ep)` (count sent).
/// (Args: 0=handle,1=buf_ptr,2=len,3=family,4=ip_ptr,5=ip_len,6=port.)
fn insert_dotnet_net_send_to(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_send_to", |asm| {
        let socket = ClassRef::socket(asm);
        let endpoint_base = ClassRef::endpoint(asm);
        let s = handle_to_socket(asm, 0);
        let (span, span_ty) = build_byte_span(asm, 1, 2, true);
        // ep = new IPEndPoint(IPAddress(span(ip_ptr,ip_len)), port). args 4/5/6.
        let endpoint = build_endpoint(asm, 4, 5, 6);
        // SendTo(ReadOnlySpan<byte>, EndPoint) -> int.
        let send_to_name = asm.alloc_string("SendTo");
        let send_to = asm.class_ref(socket).clone().instance(
            &[span_ty, Type::ClassRef(endpoint_base)],
            Type::Int(Int::I32),
            send_to_name,
            asm,
        );
        let endpoint = endpoint_as_base(asm, endpoint);
        let count = asm.alloc_node(CILNode::call(send_to, [s, span, endpoint]));
        let count = asm.int_cast(count, Int::ISize, ExtendKind::SignExtend);
        let ret = asm.alloc_root(CILRoot::Ret(count));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// Shared generator for `rcl_dotnet_net_local_addr` / `rcl_dotnet_net_peer_addr`
/// (`get_LocalEndPoint` / `get_RemoteEndPoint`):
///   `write((IPEndPoint)s.<EndPoint>, out_*); return 0;`
/// (Args: 0=handle, 1=out_family, 2=out_ip, 3=out_port.)
fn insert_dotnet_net_addr(
    asm: &mut Assembly,
    patcher: &mut MissingMethodPatcher,
    sym: &'static str,
    getter: &'static str,
) {
    let name = asm.alloc_string(sym);
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let endpoint_base = ClassRef::endpoint(asm);

        // ep = (IPEndPoint)s.<getter>(); store in local 0.
        let s = handle_to_socket(asm, 0);
        let getter_name = asm.alloc_string(getter);
        let get_ep = asm.class_ref(socket).clone().instance(
            &[],
            Type::ClassRef(endpoint_base),
            getter_name,
            asm,
        );
        let ep_obj = asm.alloc_node(CILNode::call(get_ep, [s]));
        let ip_ep_ty = asm.alloc_type(Type::ClassRef(ip_endpoint));
        let ep = asm.alloc_node(CILNode::CheckedCast(ep_obj, ip_ep_ty));
        let store_ep = asm.alloc_root(CILRoot::StLoc(0, ep));

        // write addr out (local 0 = ep, local 1 = byte[] scratch); return 0.
        let mut roots = vec![store_ep];
        roots.extend(write_endpoint_out(asm, 0, 1, 1, 2, 3));
        let zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        roots.push(ret);

        let ep_ty = asm.alloc_type(Type::ClassRef(ip_endpoint));
        let byte_ty = asm.alloc_type(Type::Int(Int::U8));
        let byte_arr_ty = asm.alloc_type(Type::PlatformArray {
            elem: byte_ty,
            dims: NonZeroU8::new(1).unwrap(),
        });
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(roots, 0, None)],
            locals: vec![
                (Some(asm.alloc_string("endpoint")), ep_ty),
                (Some(asm.alloc_string("bytes")), byte_arr_ty),
            ],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_udp_connect(handle, family, ip_ptr, ip_len, port) -> i32`
///   => `s.Connect(ep); return 0;` (Args: 0=handle,1=family,2=ip_ptr,3=ip_len,4=port.)
fn insert_dotnet_net_udp_connect(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_udp_connect", |asm| {
        let socket = ClassRef::socket(asm);
        let endpoint_base = ClassRef::endpoint(asm);
        let s = handle_to_socket(asm, 0);
        let endpoint = build_endpoint(asm, 2, 3, 4);
        let endpoint = endpoint_as_base(asm, endpoint);
        let connect_name = asm.alloc_string("Connect");
        let connect = asm.class_ref(socket).clone().instance(
            &[Type::ClassRef(endpoint_base)],
            Type::Void,
            connect_name,
            asm,
        );
        let do_connect = asm.alloc_root(CILRoot::call(connect, [s, endpoint]));
        let zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![do_connect, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_net_shutdown(handle, how) -> i32`
///   => `s.Shutdown((SocketShutdown)how); return 0;`
fn insert_dotnet_net_shutdown(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_shutdown", |asm| {
        let socket = ClassRef::socket(asm);
        let socket_shutdown = Type::ClassRef(ClassRef::socket_shutdown(asm));
        let s = handle_to_socket(asm, 0);
        let how = asm.alloc_node(CILNode::LdArg(1));
        let how = i32_to_bcl_enum(how, socket_shutdown, asm);
        let shutdown_name = asm.alloc_string("Shutdown");
        let shutdown = asm.class_ref(socket).clone().instance(
            &[socket_shutdown],
            Type::Void,
            shutdown_name,
            asm,
        );
        let do_shutdown = asm.alloc_root(CILRoot::call(shutdown, [s, how]));
        let zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![do_shutdown, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_net_set_nonblocking(handle, nonblocking) -> i32`
///   => `s.Blocking = (nonblocking == 0); return 0;` (Blocking is the inverse).
fn insert_dotnet_net_set_nonblocking(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_set_nonblocking", |asm| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        // blocking = (nonblocking == 0) : the BCL `Blocking` flag is the inverse of
        // std's `nonblocking`. `ceq` against 0 yields a `Type::Bool`.
        let nb = asm.alloc_node(CILNode::LdArg(1));
        let zero = asm.alloc_node(0_i32);
        let is_zero = asm.alloc_node(CILNode::BinOp(nb, zero, crate::cilnode::BinOp::Eq));
        let set_blocking_name = asm.alloc_string("set_Blocking");
        let set_blocking = asm.class_ref(socket).clone().instance(
            &[Type::Bool],
            Type::Void,
            set_blocking_name,
            asm,
        );
        let do_set = asm.alloc_root(CILRoot::call(set_blocking, [s, is_zero]));
        let ret_zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(ret_zero));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![do_set, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_net_set_nodelay(handle, on) -> i32`
///   => `s.NoDelay = (on != 0); return 0;`
fn insert_dotnet_net_set_nodelay(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_set_nodelay", |asm| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        // on != 0 -> bool : !(on == 0). `ceq` yields Bool, `UnOp::Not` flips it.
        let on = asm.alloc_node(CILNode::LdArg(1));
        let zero = asm.alloc_node(0_i32);
        let is_zero = asm.alloc_node(CILNode::BinOp(on, zero, crate::cilnode::BinOp::Eq));
        let on_bool = asm.alloc_node(CILNode::UnOp(is_zero, crate::cilnode::UnOp::Not));
        let set_nodelay_name = asm.alloc_string("set_NoDelay");
        let set_nodelay = asm.class_ref(socket).clone().instance(
            &[Type::Bool],
            Type::Void,
            set_nodelay_name,
            asm,
        );
        let do_set = asm.alloc_root(CILRoot::call(set_nodelay, [s, on_bool]));
        let ret_zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(ret_zero));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![do_set, ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_net_nodelay(handle) -> i32` => `return s.NoDelay ? 1 : 0;`
fn insert_dotnet_net_nodelay(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_nodelay", |asm| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        let get_nodelay_name = asm.alloc_string("get_NoDelay");
        let get_nodelay =
            asm.class_ref(socket)
                .clone()
                .instance(&[], Type::Bool, get_nodelay_name, asm);
        let v = asm.alloc_node(CILNode::call(get_nodelay, [s]));
        // bool -> i32 (0/1).
        let v = asm.int_cast(v, Int::I32, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(v));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_net_close(handle)` => `s.Dispose()` then free the `GCHandle`.
fn insert_dotnet_net_close(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_net_close", |asm| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        let dispose_name = asm.alloc_string("Dispose");
        let dispose = asm
            .class_ref(socket)
            .clone()
            .instance(&[], Type::Void, dispose_name, asm);
        let dispose = asm.alloc_root(CILRoot::call(dispose, [s]));
        let (store_gch, free, gc_handle_ty) = free_handle_roots(asm, 0, 0);
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![dispose, store_gch, free, ret],
                0,
                None,
            )],
            locals: vec![(Some(asm.alloc_string("gch")), gc_handle_ty)],
        }
    });
}

/// `rcl_dotnet_socket_poll(handle, micros, mode) -> i32`
///   => `return s.Poll((int)micros, (SelectMode)mode) ? 1 : 0;`
///
/// The readiness primitive behind the dotnet `mio` PAL arm's Selector. `mode` is
/// passed straight through as a `System.Net.Sockets.SelectMode` value-type int
/// (0=SelectRead, 1=SelectWrite, 2=SelectError) — exactly how `shutdown` feeds a
/// raw int as `SocketShutdown`. A negative `micros` means an infinite wait (BCL
/// `Socket.Poll` treats negative as block-forever). Returns 1 if the socket is
/// ready in the requested mode, else 0.
fn insert_dotnet_socket_poll(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_socket_poll", |asm| {
        let socket = ClassRef::socket(asm);
        let select_mode = Type::ClassRef(ClassRef::select_mode(asm));
        let s = handle_to_socket(asm, 0);
        let micros = asm.alloc_node(CILNode::LdArg(1));
        let mode = asm.alloc_node(CILNode::LdArg(2));
        let mode = i32_to_bcl_enum(mode, select_mode, asm);
        let poll_name = asm.alloc_string("Poll");
        let poll = asm.class_ref(socket).clone().instance(
            &[Type::Int(Int::I32), select_mode],
            Type::Bool,
            poll_name,
            asm,
        );
        let v = asm.alloc_node(CILNode::call(poll, [s, micros, mode]));
        // bool -> i32 (0/1).
        let v = asm.int_cast(v, Int::I32, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(v));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    });
}

/// `rcl_dotnet_eventfd() -> *mut u8` — the mio Waker primitive, realised as a
/// SELF-READABLE loopback UDP socket (NOT a kernel eventfd, which CoreCLR has no
/// equivalent of). Returns the `GCHandle` of a `Socket` that is connected to its
/// OWN bound endpoint, so:
///   * `write(fd, &counter)` -> `rcl_dotnet_net_send` -> `s.Send` (datagram to
///     self) -> the socket's own receive buffer fills -> it becomes READABLE;
///   * `epoll_wait`'s per-fd `Socket.Poll(SelectRead)` sweep then fires;
///   * `read(fd, buf)` -> `rcl_dotnet_net_recv` -> `s.Receive` drains it.
/// The 8-byte eventfd counter degrades gracefully to a readiness EDGE: mio's
/// waker only cares that the read end becomes pollable, never the exact value.
/// The fd is registered FD_KIND_SOCKET by `insert_eventfd`, so read/write/close/
/// poll all kind-dispatch through the existing net path with NO new plumbing.
///
/// Body (single block):
///   `var s = new Socket(InterNetwork, Dgram, Udp);
///    s.Bind(new IPEndPoint(0x0100007F /*127.0.0.1*/, 0));
///    s.Connect((EndPoint)s.LocalEndPoint);   // self-connect (UDP)
///    s.Blocking = false;                      // EFD_NONBLOCK
///    return GCHandle(s);`
/// InterNetwork(2)/Dgram(2)/Udp(17) are int-backed enum values passed straight
/// in (the ctor signature still names the enum types for BCL resolution).
fn insert_dotnet_eventfd(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_eventfd", |asm| {
        let socket = ClassRef::socket(asm);
        let address_family = ClassRef::address_family(asm);
        let socket_type = Type::ClassRef(ClassRef::socket_type(asm));
        let protocol_type = Type::ClassRef(ClassRef::protocol_type(asm));
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let endpoint_base = ClassRef::endpoint(asm);

        // var s = new Socket(InterNetwork=2, Dgram=2, Udp=17); local 0.
        let af = asm.alloc_node(2_i32); // AddressFamily.InterNetwork
        let st = asm.alloc_node(2_i32); // SocketType.Dgram
        let proto = asm.alloc_node(NET_PROTO_UDP);
        let af = i32_to_bcl_enum(af, Type::ClassRef(address_family), asm);
        let st = i32_to_bcl_enum(st, socket_type, asm);
        let proto = i32_to_bcl_enum(proto, protocol_type, asm);
        let sock_ctor = asm.class_ref(socket).clone().ctor(
            &[Type::ClassRef(address_family), socket_type, protocol_type],
            asm,
        );
        let sock = asm.alloc_node(CILNode::call(sock_ctor, [af, st, proto]));
        let store_sock = asm.alloc_root(CILRoot::StLoc(0, sock));

        // s.Bind(new IPEndPoint(0x0100007F /*127.0.0.1*/, 0)).
        let loopback = asm.alloc_node(0x0100_007F_i64); // 127.0.0.1 (IPAddress long)
        let zero_port = asm.alloc_node(0_i32);
        let ep_ctor = asm
            .class_ref(ip_endpoint)
            .clone()
            .ctor(&[Type::Int(Int::I64), Type::Int(Int::I32)], asm);
        let bind_ep = asm.alloc_node(CILNode::call(ep_ctor, [loopback, zero_port]));
        let bind_ep = endpoint_as_base(asm, bind_ep);
        let bind_name = asm.alloc_string("Bind");
        let bind = asm.class_ref(socket).clone().instance(
            &[Type::ClassRef(endpoint_base)],
            Type::Void,
            bind_name,
            asm,
        );
        let s_bind = asm.alloc_node(CILNode::LdLoc(0));
        let do_bind = asm.alloc_root(CILRoot::call(bind, [s_bind, bind_ep]));

        // ep = s.LocalEndPoint (the kernel-assigned port); self-connect to it.
        let get_local_name = asm.alloc_string("get_LocalEndPoint");
        let get_local = asm.class_ref(socket).clone().instance(
            &[],
            Type::ClassRef(endpoint_base),
            get_local_name,
            asm,
        );
        let s_get = asm.alloc_node(CILNode::LdLoc(0));
        let local_ep = asm.alloc_node(CILNode::call(get_local, [s_get]));
        // s.Connect(localEp) — UDP connect just fixes the default peer = self.
        let connect_name = asm.alloc_string("Connect");
        let connect = asm.class_ref(socket).clone().instance(
            &[Type::ClassRef(endpoint_base)],
            Type::Void,
            connect_name,
            asm,
        );
        let s_conn = asm.alloc_node(CILNode::LdLoc(0));
        let do_connect = asm.alloc_root(CILRoot::call(connect, [s_conn, local_ep]));

        // s.Blocking = false (EFD_NONBLOCK).
        let set_blocking_name = asm.alloc_string("set_Blocking");
        let set_blocking = asm.class_ref(socket).clone().instance(
            &[Type::Bool],
            Type::Void,
            set_blocking_name,
            asm,
        );
        let s_blk = asm.alloc_node(CILNode::LdLoc(0));
        let false_v = asm.alloc_node(false);
        let do_blocking = asm.alloc_root(CILRoot::call(set_blocking, [s_blk, false_v]));

        let handle = socket_local_to_handle(asm, 0);
        let ret = asm.alloc_root(CILRoot::Ret(handle));
        let sock_ty = asm.alloc_type(Type::ClassRef(socket));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![store_sock, do_bind, do_connect, do_blocking, ret],
                0,
                None,
            )],
            locals: vec![(Some(asm.alloc_string("socket")), sock_ty)],
        }
    });
}

/// `rcl_dotnet_pipe_pair(out_read, out_write) -> i32` — construct two distinct
/// non-blocking UDP sockets connected to each other over loopback. This is the
/// managed transport behind POSIX `pipe`/`pipe2`: unlike duplicating one GCHandle,
/// each fd owns and closes its own Socket while writes on one endpoint become
/// readable on the other.
fn insert_dotnet_pipe_pair(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    dotnet_hook!(asm, patcher, "rcl_dotnet_pipe_pair", |asm| {
        let socket = ClassRef::socket(asm);
        let address_family = ClassRef::address_family(asm);
        let socket_type = Type::ClassRef(ClassRef::socket_type(asm));
        let protocol_type = Type::ClassRef(ClassRef::protocol_type(asm));
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let endpoint_base = ClassRef::endpoint(asm);
        let void_ptr = asm.nptr(Type::Void);

        let sock_ctor = asm.class_ref(socket).clone().ctor(
            &[Type::ClassRef(address_family), socket_type, protocol_type],
            asm,
        );
        let make_socket = |asm: &mut Assembly| {
            let af = asm.alloc_node(2_i32);
            let st = asm.alloc_node(2_i32);
            let proto = asm.alloc_node(NET_PROTO_UDP);
            let af = i32_to_bcl_enum(af, Type::ClassRef(address_family), asm);
            let st = i32_to_bcl_enum(st, socket_type, asm);
            let proto = i32_to_bcl_enum(proto, protocol_type, asm);
            asm.alloc_node(CILNode::call(sock_ctor, [af, st, proto]))
        };
        let first = make_socket(asm);
        let store_first = asm.alloc_root(CILRoot::StLoc(0, first));
        let second = make_socket(asm);
        let store_second = asm.alloc_root(CILRoot::StLoc(1, second));

        let ep_ctor = asm
            .class_ref(ip_endpoint)
            .clone()
            .ctor(&[Type::Int(Int::I64), Type::Int(Int::I32)], asm);
        let bind_name = asm.alloc_string("Bind");
        let bind = asm.class_ref(socket).clone().instance(
            &[Type::ClassRef(endpoint_base)],
            Type::Void,
            bind_name,
            asm,
        );
        let bind_socket = |asm: &mut Assembly, local| {
            let loopback = asm.alloc_node(0x0100_007F_i64);
            let zero_port = asm.alloc_node(0_i32);
            let endpoint = asm.alloc_node(CILNode::call(ep_ctor, [loopback, zero_port]));
            let endpoint = endpoint_as_base(asm, endpoint);
            let socket = asm.alloc_node(CILNode::LdLoc(local));
            asm.alloc_root(CILRoot::call(bind, [socket, endpoint]))
        };
        let bind_first = bind_socket(asm, 0);
        let bind_second = bind_socket(asm, 1);

        let get_local_name = asm.alloc_string("get_LocalEndPoint");
        let get_local = asm.class_ref(socket).clone().instance(
            &[],
            Type::ClassRef(endpoint_base),
            get_local_name,
            asm,
        );
        let connect_name = asm.alloc_string("Connect");
        let connect = asm.class_ref(socket).clone().instance(
            &[Type::ClassRef(endpoint_base)],
            Type::Void,
            connect_name,
            asm,
        );
        let connect_to = |asm: &mut Assembly, socket_local, peer_local| {
            let peer = asm.alloc_node(CILNode::LdLoc(peer_local));
            let peer_endpoint = asm.alloc_node(CILNode::call(get_local, [peer]));
            let socket = asm.alloc_node(CILNode::LdLoc(socket_local));
            asm.alloc_root(CILRoot::call(connect, [socket, peer_endpoint]))
        };
        let connect_first = connect_to(asm, 0, 1);
        let connect_second = connect_to(asm, 1, 0);

        let set_blocking_name = asm.alloc_string("set_Blocking");
        let set_blocking = asm.class_ref(socket).clone().instance(
            &[Type::Bool],
            Type::Void,
            set_blocking_name,
            asm,
        );
        let set_nonblocking = |asm: &mut Assembly, local| {
            let socket = asm.alloc_node(CILNode::LdLoc(local));
            let no = asm.alloc_node(false);
            asm.alloc_root(CILRoot::call(set_blocking, [socket, no]))
        };
        let nonblocking_first = set_nonblocking(asm, 0);
        let nonblocking_second = set_nonblocking(asm, 1);

        let out_type = asm.alloc_type(void_ptr);
        let out_first = asm.alloc_node(CILNode::LdArg(0));
        let out_first = asm.alloc_node(CILNode::PtrCast(
            out_first,
            Box::new(PtrCastRes::Ptr(out_type)),
        ));
        let first_handle = socket_local_to_handle(asm, 0);
        let write_first = asm.alloc_root(CILRoot::StInd(Box::new((
            out_first,
            first_handle,
            void_ptr,
            false,
        ))));
        let out_second = asm.alloc_node(CILNode::LdArg(1));
        let out_second = asm.alloc_node(CILNode::PtrCast(
            out_second,
            Box::new(PtrCastRes::Ptr(out_type)),
        ));
        let second_handle = socket_local_to_handle(asm, 1);
        let write_second = asm.alloc_root(CILRoot::StInd(Box::new((
            out_second,
            second_handle,
            void_ptr,
            false,
        ))));
        let zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        let socket_type = asm.alloc_type(Type::ClassRef(socket));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(
                vec![
                    store_first,
                    store_second,
                    bind_first,
                    bind_second,
                    connect_first,
                    connect_second,
                    nonblocking_first,
                    nonblocking_second,
                    write_first,
                    write_second,
                    ret,
                ],
                0,
                None,
            )],
            locals: vec![(None, socket_type), (None, socket_type)],
        }
    });
}

#[cfg(test)]
mod pipe_pair_tests {
    use super::*;
    use crate::{Access, MethodDef};

    #[test]
    fn connected_socket_pair_helper_is_verifier_clean() {
        let mut asm = Assembly::default();
        let mut patcher = MissingMethodPatcher::default();
        insert_dotnet_pipe_pair(&mut asm, &mut patcher);

        let name = asm.alloc_string("rcl_dotnet_pipe_pair");
        let void_ptr = asm.nptr(Type::Void);
        let sig = asm.sig([void_ptr, void_ptr], Type::Int(Int::I32));
        let main_module = *asm.main_module();
        let mref = asm.new_methodref(
            main_module,
            "rcl_dotnet_pipe_pair",
            sig,
            MethodKind::Static,
            vec![],
        );
        let body = patcher.get(&name).unwrap()(mref, &mut asm);
        asm.new_method(MethodDef::new(
            Access::Public,
            crate::class::ClassDefIdx(main_module),
            name,
            sig,
            MethodKind::Static,
            body,
            vec![None, None],
        ));

        assert_eq!(asm.typecheck(), 0);
    }
}
