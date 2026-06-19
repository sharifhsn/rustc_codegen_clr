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
//! See also `dotnet_pal/sys/args/dotnet.rs` and `dotnet_pal/sys/env/dotnet.rs`.
//!
//! `realloc` is handled std-side via `realloc_fallback` (alloc+copy+free) and
//! `alloc_zeroed` via `rcl_dotnet_alloc` + zeroing, so those do not need their
//! own binding.

use super::UNMANAGED_THREAD_START;
use crate::cilnode::{ExtendKind, MethodKind, PtrCastRes};
use crate::ir::asm::MissingMethodPatcher;
use crate::ir::cilroot::BranchCond;
use crate::ir::{
    BasicBlock, CILNode, CILRoot, ClassRef, Const, Int, Interned, MethodImpl, MethodRef, Type,
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
///   => `System.Diagnostics.Stopwatch.Frequency` (the `get_Frequency` getter).
///
/// Ticks per second for the counter returned by `rcl_dotnet_instant_ticks`.
/// `Frequency` is a `public static readonly long`, surfaced in CIL as the
/// auto-generated `get_Frequency()` static getter, so this is a plain static
/// call — no static-field load needed.
fn insert_dotnet_instant_freq(asm: &mut Assembly, patcher: &mut MissingMethodPatcher) {
    let name = asm.alloc_string("rcl_dotnet_instant_freq");
    let generator = move |_, asm: &mut Assembly| {
        let stopwatch = ClassRef::stopwatch(asm);
        let get_freq = MethodRef::new(
            stopwatch,
            asm.alloc_string("get_Frequency"),
            asm.sig([], Type::Int(Int::I64)),
            MethodKind::Static,
            [].into(),
        );
        let get_freq = asm.alloc_methodref(get_freq);
        let freq = asm.alloc_node(CILNode::call(get_freq, []));
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
