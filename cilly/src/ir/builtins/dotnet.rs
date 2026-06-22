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
//! * `rcl_dotnet_free(ptr, align)`
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
use crate::cilnode::{ExtendKind, MethodKind, PtrCastRes};
use crate::ir::asm::MissingMethodPatcher;
use crate::ir::cilroot::BranchCond;
use crate::ir::{
    BasicBlock, CILNode, CILRoot, ClassRef, Const, Int, Interned, MethodImpl, MethodRef,
    StaticFieldDesc, Type,
};
use crate::Assembly;
use std::num::NonZeroU8;

/// Registers all `rcl_dotnet_*` BCL bindings in `patcher`.
pub fn insert_dotnet_pal(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    insert_dotnet_alloc(asm, patcher);
    insert_dotnet_free(asm, patcher);
    insert_dotnet_write(asm, patcher);
    insert_dotnet_random_fill(asm, patcher);
    insert_dotnet_instant_ticks(asm, patcher);
    insert_dotnet_instant_freq(asm, patcher);
    insert_dotnet_unix_ticks(asm, patcher);
    insert_dotnet_thread_spawn(asm, patcher);
    insert_dotnet_thread_join(asm, patcher);
    insert_dotnet_thread_yield(asm, patcher);
    insert_dotnet_thread_sleep(asm, patcher);
    insert_dotnet_available_parallelism(asm, patcher);
    insert_dotnet_getpid(asm, patcher);
    insert_dotnet_hostname(asm, patcher);
    insert_dotnet_cotaskmem_free(asm, patcher);
    insert_dotnet_args(asm, patcher);
    insert_dotnet_env(asm, patcher);
    insert_dotnet_fs(asm, patcher);
    insert_dotnet_net(asm, patcher);
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
    let name = asm.alloc_string("rcl_dotnet_instant_ticks");
    let generator = move |_, asm: &mut Assembly| {
        let stopwatch = ClassRef::stopwatch(asm);
        let get_timestamp = MethodRef::new(
            stopwatch,
            asm.alloc_string("GetTimestamp"),
            asm.sig([], Type::Int(Int::I64)),
            MethodKind::Static,
            [].into(),
        );
        let get_timestamp = asm.alloc_methodref(get_timestamp);
        let ticks = asm.alloc_node(CILNode::call(get_timestamp, []));
        let ret = asm.alloc_root(CILRoot::Ret(ticks));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_instant_freq() -> i64`
///   => `System.Diagnostics.Stopwatch.Frequency` (the static `Frequency` field, via `ldsfld`).
///
/// Ticks per second for the counter returned by `rcl_dotnet_instant_ticks`.
/// `Frequency` is a `public static readonly long` FIELD on `Stopwatch` — CoreCLR
/// exposes it directly, with no `get_Frequency()` getter — so this loads it with
/// `ldsfld` rather than issuing a static call.
fn insert_dotnet_instant_freq(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_instant_freq");
    let generator = move |_, asm: &mut Assembly| {
        let stopwatch = ClassRef::stopwatch(asm);
        let freq_name = asm.alloc_string("Frequency");
        let freq_fld = asm.alloc_sfld(StaticFieldDesc::new(stopwatch, freq_name, Type::Int(Int::I64)));
        let freq = asm.alloc_node(CILNode::LdStaticField(freq_fld));
        let ret = asm.alloc_root(CILRoot::Ret(freq));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
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
    let name = asm.alloc_string("rcl_dotnet_unix_ticks");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_alloc(size: usize, align: usize) -> *mut u8`
///   => `NativeMemory.AlignedAlloc((nuint)size, (nuint)align)`.
///
/// Models the existing `__rust_alloc` builtin: forward straight to
/// `AlignedAlloc`. Recent rustc wraps allocator-shim scalars in transparent
/// value types, but this symbol comes from our own PAL's `extern "C"` decl with
/// plain `usize` arguments, so the args are loaded directly.
fn insert_dotnet_alloc(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_alloc");
    let generator = move |_, asm: &mut Assembly| {
        let size = asm.alloc_node(CILNode::LdArg(0));
        let align = asm.alloc_node(CILNode::LdArg(1));
        let void_ptr = asm.nptr(Type::Void);
        let sig = asm.sig([Type::Int(Int::USize), Type::Int(Int::USize)], void_ptr);
        let aligned_alloc = asm.alloc_string("AlignedAlloc");
        let native_mem = ClassRef::native_mem(asm);
        let call_method = asm.alloc_methodref(MethodRef::new(
            native_mem,
            aligned_alloc,
            sig,
            MethodKind::Static,
            [].into(),
        ));
        let alloc = asm.alloc_node(CILNode::call(call_method, [size, align]));
        // Result type is *mut u8; AlignedAlloc returns void*, which is
        // pointer-compatible, so a plain return suffices.
        let ret = asm.alloc_root(CILRoot::Ret(alloc));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_free(ptr: *mut u8, align: usize)`
///   => `NativeMemory.AlignedFree((void*)ptr)`.
///
/// Models the non-libc `__rust_dealloc` builtin. The `align` argument is unused
/// (`AlignedFree` takes only the pointer); it is part of the contract so the
/// std side can stay symmetric with `AlignedAlloc`.
fn insert_dotnet_free(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_free");
    let generator = move |_, asm: &mut Assembly| {
        let ptr = asm.alloc_node(CILNode::LdArg(0));
        let void_ptr = asm.nptr(Type::Void);
        // Reinterpret *mut u8 as void* for the AlignedFree signature.
        let ptr = asm.cast_ptr(ptr, void_ptr);
        let sig = asm.sig([void_ptr], Type::Void);
        let aligned_free = asm.alloc_string("AlignedFree");
        let native_mem = ClassRef::native_mem(asm);
        let call_method = asm.alloc_methodref(MethodRef::new(
            native_mem,
            aligned_free,
            sig,
            MethodKind::Static,
            [].into(),
        ));
        let free = asm.alloc_root(CILRoot::call(call_method, [ptr]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![free, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
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
    let name = asm.alloc_string("rcl_dotnet_random_fill");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
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
                vec![store_ts, store_thread, start_thread, ret],
                0,
                None,
            )],
            locals: vec![
                (Some(asm.alloc_string("thread")), thread_ty),
                (Some(asm.alloc_string("thread_start")), thread_start_local_ty),
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
    let name = asm.alloc_string("rcl_dotnet_thread_join");
    let generator = move |_, asm: &mut Assembly| {
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
        let free = asm.class_ref(gc_handle).clone().instance(
            &[],
            Type::Void,
            free,
            asm,
        );
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_thread_yield()` => `System.Threading.Thread.Yield()`.
///
/// `Thread.Yield` is a static `bool` (whether the OS switched to another thread);
/// `std`'s `yield_now` ignores the result, so we pop it.
fn insert_dotnet_thread_yield(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_thread_yield");
    let generator = move |_, asm: &mut Assembly| {
        let thread = ClassRef::thread(asm);
        let yield_now = asm.alloc_string("Yield");
        let yield_now =
            asm.class_ref(thread)
                .clone()
                .static_mref(&[], Type::Bool, yield_now, asm);
        let yielded = asm.alloc_node(CILNode::call(yield_now, []));
        let pop = asm.alloc_root(CILRoot::Pop(yielded));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![pop, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_thread_sleep(millis: u64)` => `System.Threading.Thread.Sleep((int)millis)`.
///
/// The std side already chunks long sleeps to `<= i32::MAX` ms, so the truncation
/// to the `int` `Sleep` overload is lossless.
fn insert_dotnet_thread_sleep(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_thread_sleep");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_available_parallelism() -> usize` => `System.Environment.ProcessorCount`.
///
/// `ProcessorCount` is an `int` static getter (`get_ProcessorCount`); we
/// zero-extend it to `usize` for the symbol's return type. The std side wraps
/// the result in a `NonZero<usize>`, clamping a (spec-impossible) zero to 1.
fn insert_dotnet_available_parallelism(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_available_parallelism");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_getpid() -> u32` => `System.Environment.ProcessId`.
///
/// Backs `sys::process::getpid` on the dotnet PAL (a genuine process id, unlike
/// `spawn`'s synthetic-pid wall). `ProcessId` is an `int` static getter
/// (`get_ProcessId`); the symbol's return type is `u32`, and the value is always
/// non-negative, so a plain widen-free reinterpret suffices.
fn insert_dotnet_getpid(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_getpid");
    let generator = move |_, asm: &mut Assembly| {
        let env = ClassRef::enviroment(asm);
        let get_pid = asm.alloc_string("get_ProcessId");
        let get_pid =
            asm.class_ref(env)
                .clone()
                .static_mref(&[], Type::Int(Int::I32), get_pid, asm);
        let pid = asm.alloc_node(CILNode::call(get_pid, []));
        let ret = asm.alloc_root(CILRoot::Ret(pid));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_hostname() -> *mut u8` =>
///   `Marshal.StringToCoTaskMemUTF8(System.Environment.MachineName)`.
///
/// Backs `sys::net::hostname` on the dotnet PAL. `MachineName` is a `string`
/// static getter (`get_MachineName`); we marshal it to a freshly-allocated,
/// NUL-terminated UTF-8 C string (COM-task-memory heap) the std side reads with
/// `CStr` and frees with `rcl_dotnet_cotaskmem_free` — mirroring the env getter.
fn insert_dotnet_hostname(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_hostname");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
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
    let name = asm.alloc_string("rcl_dotnet_cotaskmem_free");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
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
    insert_dotnet_fs_seek(asm, patcher);
    insert_dotnet_fs_flush(asm, patcher);
    insert_dotnet_fs_close(asm, patcher);
    insert_dotnet_fs_len(asm, patcher);
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
/// the stream at end-of-file itself. On a managed I/O fault the exception
/// unwinds (the std side never sees null, mirroring the other handle hooks).
fn insert_dotnet_fs_open(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_open");
    let generator = move |_, asm: &mut Assembly| {
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
        let ctor = asm
            .class_ref(file_stream)
            .clone()
            .ctor(&[Type::PlatformString, file_mode, file_access], asm);
        let stream = asm.alloc_node(CILNode::call(ctor, [path, mode, access]));
        let store = asm.alloc_root(CILRoot::StLoc(0, stream));
        // return (void*)GCHandle.Alloc(stream)
        let handle = CILNode::LdLoc(0).ref_to_handle(asm);
        let handle = asm.alloc_node(handle);
        let void = asm.alloc_type(Type::Void);
        let handle = asm.alloc_node(CILNode::PtrCast(handle, Box::new(PtrCastRes::Ptr(void))));
        let ret = asm.alloc_root(CILRoot::Ret(handle));
        let stream_ty = asm.alloc_type(Type::ClassRef(file_stream));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("stream")), stream_ty)],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_read(handle, buf_ptr, len) -> isize`
///   => `FileStream.Read(new Span<byte>(buf_ptr, (int)len))` (count read).
fn insert_dotnet_fs_read(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_read");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_write(handle, buf_ptr, len) -> isize`
///   => `FileStream.Write(new ReadOnlySpan<byte>(buf_ptr, (int)len))`; returns
///      `len` (the BCL `Write(ReadOnlySpan<byte>)` overload writes all of it or
///      throws).
fn insert_dotnet_fs_write(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_write");
    let generator = move |_, asm: &mut Assembly| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let (span, span_ty) = build_byte_span(asm, 1, 2, true);
        let write_name = asm.alloc_string("Write");
        let write = asm.class_ref(file_stream).clone().instance(
            &[span_ty],
            Type::Void,
            write_name,
            asm,
        );
        let write = asm.alloc_root(CILRoot::call(write, [stream, span]));
        let len = asm.alloc_node(CILNode::LdArg(2));
        let len = asm.int_cast(len, Int::ISize, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(len));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![write, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_seek(handle, offset: i64, origin: i32) -> i64`
///   => `FileStream.Seek(offset, (SeekOrigin)origin)` (new absolute position).
///
/// `offset` is a signed 64-bit value (it may be negative for `SeekFrom::End` /
/// `Current`), so it is loaded as-is — never zero-extended.
fn insert_dotnet_fs_seek(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_seek");
    let generator = move |_, asm: &mut Assembly| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let offset = asm.alloc_node(CILNode::LdArg(1));
        let origin = asm.alloc_node(CILNode::LdArg(2));
        let seek_name = asm.alloc_string("Seek");
        // FileStream.Seek(long, SeekOrigin) — the second param is the int-backed
        // SeekOrigin enum value type (an int32 is binary compatible on the stack).
        let seek_origin = Type::ClassRef(ClassRef::seek_origin(asm));
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_flush(handle)` => `FileStream.Flush()`.
fn insert_dotnet_fs_flush(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_flush");
    let generator = move |_, asm: &mut Assembly| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let flush_name = asm.alloc_string("Flush");
        let flush = asm.class_ref(file_stream).clone().instance(
            &[],
            Type::Void,
            flush_name,
            asm,
        );
        let flush = asm.alloc_root(CILRoot::call(flush, [stream]));
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![flush, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_close(handle)` => `FileStream.Dispose()` then free the
/// `GCHandle` so the stream can be collected.
fn insert_dotnet_fs_close(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_close");
    let generator = move |_, asm: &mut Assembly| {
        let file_stream = ClassRef::file_stream(asm);
        let stream = handle_to_class(asm, 0, file_stream);
        let dispose_name = asm.alloc_string("Dispose");
        let dispose = asm.class_ref(file_stream).clone().instance(
            &[],
            Type::Void,
            dispose_name,
            asm,
        );
        let dispose = asm.alloc_root(CILRoot::call(dispose, [stream]));
        let (store_gch, free, gc_handle_ty) = free_handle_roots(asm, 0, 0);
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![dispose, store_gch, free, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("gch")), gc_handle_ty)],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_len(handle) -> i64` => `FileStream.get_Length`.
fn insert_dotnet_fs_len(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_len");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_stat(path_ptr, path_len, out_size: *mut u64, out_is_dir: *mut i32) -> i32`
///   => `Directory.Exists(path)` ? write (size 0, is_dir 1), return 0
///      : `File.Exists(path)`    ? write (FileInfo.Length, is_dir 0), return 0
///      : return -1 (NotFound — the std side maps -1 to `ErrorKind::NotFound`).
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

        // Block 1 (dir): *out_size = 0; *out_is_dir = 1; return 0.
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
        let get_len = asm.class_ref(file_info).clone().instance(
            &[],
            Type::Int(Int::I64),
            get_len_name,
            asm,
        );
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
        let zero_ret3 = asm.alloc_node(0_i32);
        let ret_file = asm.alloc_root(CILRoot::Ret(zero_ret3));

        // Block 4 (notfound): return -1.
        let neg1 = asm.alloc_node(-1_i32);
        let ret_nf = asm.alloc_root(CILRoot::Ret(neg1));

        let string_ty = asm.alloc_type(Type::PlatformString);
        let file_info_ty = asm.alloc_type(Type::ClassRef(file_info));
        MethodImpl::MethodBody {
            blocks: vec![
                BasicBlock::new(vec![store_path, br_dir, goto_file], 0, None),
                BasicBlock::new(vec![st_size1, st_isdir1, ret_dir], 1, None),
                BasicBlock::new(vec![br_file, goto_notfound], 2, None),
                BasicBlock::new(vec![store_fi, st_size3, st_isdir3, ret_file], 3, None),
                BasicBlock::new(vec![ret_nf], 4, None),
            ],
            locals: vec![
                (Some(asm.alloc_string("path")), string_ty),
                (Some(asm.alloc_string("file_info")), file_info_ty),
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
    let name = asm.alloc_string("rcl_dotnet_fs_exists");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// Shared shape for the path-in/`i32`-out fs hooks (`mkdir`/`rmdir`/`unlink`):
/// decode the path, issue one static System.IO call (consuming any result), and
/// return 0. A managed fault unwinds rather than returning a non-zero code.
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
        let zero = asm.alloc_node(0_i32);
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![call, ret], 0, None)],
            locals: vec![],
        }
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
///   => `File.Move(old, new, overwrite: true)`; 0.
fn insert_dotnet_fs_rename(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_rename");
    let generator = move |_, asm: &mut Assembly| {
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
        let ret = asm.alloc_root(CILRoot::Ret(zero));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![call, ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_readdir_open(path_ptr, path_len) -> *mut u8`
///   => `Directory.GetFileSystemEntries(path)` (a `string[]`), returned as an
///      opaque `GCHandle` `IntPtr`.
fn insert_dotnet_fs_readdir_open(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_readdir_open");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_readdir_count(handle) -> usize` => `string[].Length`.
fn insert_dotnet_fs_readdir_count(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_readdir_count");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_readdir_get(handle, idx) -> *mut u8`
///   => `Marshal.StringToCoTaskMemUTF8(string[][idx])` (caller frees via
///      `rcl_dotnet_cotaskmem_free`).
fn insert_dotnet_fs_readdir_get(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_readdir_get");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_fs_readdir_close(handle)` => free the `string[]` `GCHandle`.
fn insert_dotnet_fs_readdir_close(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_fs_readdir_close");
    let generator = move |_, asm: &mut Assembly| {
        let (store_gch, free, gc_handle_ty) = free_handle_roots(asm, 0, 0);
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![store_gch, free, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("gch")), gc_handle_ty)],
        }
    };
    patcher.insert(name, Box::new(generator));
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
    let ep_ctor = asm.class_ref(ip_endpoint).clone().ctor(
        &[Type::ClassRef(ip_address), Type::Int(Int::I32)],
        asm,
    );
    asm.alloc_node(CILNode::call(ep_ctor, [addr, port]))
}

/// Upcast an `IPEndPoint` node to the base `System.Net.EndPoint` (a `castclass`,
/// always sound for an upcast) — the declared param type of `Socket.Bind` /
/// `Connect` / `SendTo`. Needed because the cilly typechecker does not model
/// class inheritance, so an `IPEndPoint`-typed arg would not match an `EndPoint`
/// param, and the BCL only exposes those methods on the `EndPoint` base.
pub(crate) fn endpoint_as_base(asm: &mut Assembly, ip_endpoint_node: Interned<CILNode>) -> Interned<CILNode> {
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
    let get_bytes = asm.class_ref(ip_address).clone().instance(
        &[],
        byte_arr,
        get_bytes_name,
        asm,
    );
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
        &[byte_arr, Type::Int(Int::I32), Type::Int(Int::ISize), Type::Int(Int::I32)],
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
    let get_port = asm.class_ref(ip_endpoint).clone().instance(
        &[],
        Type::Int(Int::I32),
        get_port_name,
        asm,
    );
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
    insert_dotnet_net_addr(asm, patcher, "rcl_dotnet_net_local_addr", "get_LocalEndPoint");
    insert_dotnet_net_addr(asm, patcher, "rcl_dotnet_net_peer_addr", "get_RemoteEndPoint");
    insert_dotnet_net_udp_connect(asm, patcher);
    insert_dotnet_net_shutdown(asm, patcher);
    insert_dotnet_net_set_nonblocking(asm, patcher);
    insert_dotnet_net_set_nodelay(asm, patcher);
    insert_dotnet_net_nodelay(asm, patcher);
    insert_dotnet_net_close(asm, patcher);
    insert_dotnet_socket_poll(asm, patcher);
}

/// `rcl_dotnet_net_tcp_connect(family, ip_ptr, ip_len, port) -> *mut u8`
///   => `var s = new Socket(ep.AddressFamily, Stream, Tcp); s.Connect(ep);`
///      return the `GCHandle` `IntPtr`.
/// (Args: 0=family [unused — the family is read off the endpoint], 1=ip_ptr,
/// 2=ip_len, 3=port.)
fn insert_dotnet_net_tcp_connect(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_tcp_connect");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
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
    let name = asm.alloc_string("rcl_dotnet_net_socket");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let address_family = ClassRef::address_family(asm);
        let socket_type = Type::ClassRef(ClassRef::socket_type(asm));
        let protocol_type = Type::ClassRef(ClassRef::protocol_type(asm));
        let af = asm.alloc_node(CILNode::LdArg(0));
        let st = asm.alloc_node(CILNode::LdArg(1));
        let proto = asm.alloc_node(CILNode::LdArg(2));
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_bind(family, ip_ptr, ip_len, port, sock_type, backlog) -> *mut u8`
///   => `var s = new Socket(ep.AddressFamily, (SocketType)sock_type,
///      sock_type==Stream?Tcp:Udp); s.Bind(ep); if (backlog >= 0) s.Listen(backlog);`
///      return handle. (Args: 1=ip_ptr,2=ip_len,3=port,4=sock_type,5=backlog.)
fn insert_dotnet_net_bind(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_bind");
    let generator = move |_, asm: &mut Assembly| {
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
            Some(BranchCond::Lt(backlog3, zero3, crate::ir::cilroot::CmpKind::Signed)),
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_accept(handle, out_family, out_ip, out_port) -> *mut u8`
///   => `var c = s.Accept(); write(c.RemoteEndPoint, out_*);` return c's handle.
/// (Args: 0=handle, 1=out_family, 2=out_ip, 3=out_port.)
fn insert_dotnet_net_accept(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_accept");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let endpoint_base = ClassRef::endpoint(asm);

        // c = s.Accept(); store c in local 0.
        let s = handle_to_socket(asm, 0);
        let accept_name = asm.alloc_string("Accept");
        let accept = asm.class_ref(socket).clone().instance(
            &[],
            Type::ClassRef(socket),
            accept_name,
            asm,
        );
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_recv(handle, buf_ptr, len) -> isize`
///   => `s.Receive(new Span<byte>(buf_ptr, (int)len))` (0 == orderly shutdown).
fn insert_dotnet_net_recv(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_recv");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        let (span, span_ty) = build_byte_span(asm, 1, 2, false);
        let recv_name = asm.alloc_string("Receive");
        let recv = asm.class_ref(socket).clone().instance(
            &[span_ty],
            Type::Int(Int::I32),
            recv_name,
            asm,
        );
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_send(handle, buf_ptr, len) -> isize`
///   => `s.Send(new ReadOnlySpan<byte>(buf_ptr, (int)len))` (count sent).
fn insert_dotnet_net_send(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_send");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        let (span, span_ty) = build_byte_span(asm, 1, 2, true);
        let send_name = asm.alloc_string("Send");
        let send = asm.class_ref(socket).clone().instance(
            &[span_ty],
            Type::Int(Int::I32),
            send_name,
            asm,
        );
        let count = asm.alloc_node(CILNode::call(send, [s, span]));
        let count = asm.int_cast(count, Int::ISize, ExtendKind::SignExtend);
        let ret = asm.alloc_root(CILRoot::Ret(count));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
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
    let name = asm.alloc_string("rcl_dotnet_net_recv_from");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let ip_endpoint = ClassRef::ip_endpoint(asm);
        let endpoint_base = ClassRef::endpoint(asm);

        // seed: EndPoint ep = new IPEndPoint(0L, 0); store in local 0.
        let zero_addr = asm.alloc_node(0_i64);
        let zero_port = asm.alloc_node(0_i32);
        let ep_ctor = asm.class_ref(ip_endpoint).clone().ctor(
            &[Type::Int(Int::I64), Type::Int(Int::I32)],
            asm,
        );
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_send_to(handle, buf_ptr, len, family, ip_ptr, ip_len, port) -> isize`
///   => `s.SendTo(new ReadOnlySpan<byte>(buf_ptr, (int)len), ep)` (count sent).
/// (Args: 0=handle,1=buf_ptr,2=len,3=family,4=ip_ptr,5=ip_len,6=port.)
fn insert_dotnet_net_send_to(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_send_to");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
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
    let name = asm.alloc_string("rcl_dotnet_net_udp_connect");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_shutdown(handle, how) -> i32`
///   => `s.Shutdown((SocketShutdown)how); return 0;`
fn insert_dotnet_net_shutdown(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_shutdown");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let socket_shutdown = Type::ClassRef(ClassRef::socket_shutdown(asm));
        let s = handle_to_socket(asm, 0);
        let how = asm.alloc_node(CILNode::LdArg(1));
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_set_nonblocking(handle, nonblocking) -> i32`
///   => `s.Blocking = (nonblocking == 0); return 0;` (Blocking is the inverse).
fn insert_dotnet_net_set_nonblocking(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_set_nonblocking");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_set_nodelay(handle, on) -> i32`
///   => `s.NoDelay = (on != 0); return 0;`
fn insert_dotnet_net_set_nodelay(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_set_nodelay");
    let generator = move |_, asm: &mut Assembly| {
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
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_nodelay(handle) -> i32` => `return s.NoDelay ? 1 : 0;`
fn insert_dotnet_net_nodelay(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_nodelay");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        let get_nodelay_name = asm.alloc_string("get_NoDelay");
        let get_nodelay = asm.class_ref(socket).clone().instance(
            &[],
            Type::Bool,
            get_nodelay_name,
            asm,
        );
        let v = asm.alloc_node(CILNode::call(get_nodelay, [s]));
        // bool -> i32 (0/1).
        let v = asm.int_cast(v, Int::I32, ExtendKind::ZeroExtend);
        let ret = asm.alloc_root(CILRoot::Ret(v));
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![ret], 0, None)],
            locals: vec![],
        }
    };
    patcher.insert(name, Box::new(generator));
}

/// `rcl_dotnet_net_close(handle)` => `s.Dispose()` then free the `GCHandle`.
fn insert_dotnet_net_close(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_net_close");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let s = handle_to_socket(asm, 0);
        let dispose_name = asm.alloc_string("Dispose");
        let dispose = asm.class_ref(socket).clone().instance(
            &[],
            Type::Void,
            dispose_name,
            asm,
        );
        let dispose = asm.alloc_root(CILRoot::call(dispose, [s]));
        let (store_gch, free, gc_handle_ty) = free_handle_roots(asm, 0, 0);
        let ret = asm.alloc_root(CILRoot::VoidRet);
        MethodImpl::MethodBody {
            blocks: vec![BasicBlock::new(vec![dispose, store_gch, free, ret], 0, None)],
            locals: vec![(Some(asm.alloc_string("gch")), gc_handle_ty)],
        }
    };
    patcher.insert(name, Box::new(generator));
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
    let name = asm.alloc_string("rcl_dotnet_socket_poll");
    let generator = move |_, asm: &mut Assembly| {
        let socket = ClassRef::socket(asm);
        let select_mode = Type::ClassRef(ClassRef::select_mode(asm));
        let s = handle_to_socket(asm, 0);
        let micros = asm.alloc_node(CILNode::LdArg(1));
        let mode = asm.alloc_node(CILNode::LdArg(2));
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
    };
    patcher.insert(name, Box::new(generator));
}
