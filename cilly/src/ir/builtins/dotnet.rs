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
//! See also `dotnet_pal/sys/args/dotnet.rs`, `dotnet_pal/sys/env/dotnet.rs`,
//! and `dotnet_pal/sys/fs/dotnet.rs`.
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
    insert_dotnet_cotaskmem_free(asm, patcher);
    insert_dotnet_args(asm, patcher);
    insert_dotnet_env(asm, patcher);
    insert_dotnet_fs(asm, patcher);
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
fn decode_utf8(asm: &mut Assembly, ptr_arg: u32, len_arg: u32) -> Interned<CILNode> {
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
fn handle_to_class(
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
fn free_handle_roots(
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
fn build_byte_span(
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
