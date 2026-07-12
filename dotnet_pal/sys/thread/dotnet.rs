//! Threads for the .NET ("dotnet") platform.
//!
//! Backs `std::thread` (spawn / join / yield / sleep / available_parallelism)
//! with the .NET BCL (`System.Threading.Thread`, `System.Environment`) through a
//! small set of `extern "C"` hooks that the cilly linker maps to BCL calls — the
//! same `MissingMethodPatcher` mechanism the alloc / stdio / random / time arms
//! use. See `cilly/src/ir/builtins/dotnet.rs`.
//!
//! FIXED extern contract (the names must match EXACTLY on the linker side):
//!
//! * `rcl_dotnet_thread_spawn(entry, arg) -> *mut u8`
//!   => allocates a managed `System.Threading.Thread` whose body invokes the
//!      native function pointer `entry(arg)`, `Start()`s it, and returns an
//!      opaque handle (a `GCHandle` `IntPtr` pinning the managed `Thread`) for a
//!      later `rcl_dotnet_thread_join`. Null is returned if the runtime refuses
//!      to start the thread. This reuses the linker's existing
//!      `UnmanagedThreadStart` machinery (shared with the pthread mapping).
//! * `rcl_dotnet_thread_join(handle)`
//!   => recovers the `Thread` from `handle` and calls `Thread.Join()`, then frees
//!      the `GCHandle`.
//! * `rcl_dotnet_thread_yield()`        => `System.Threading.Thread.Yield()`.
//! * `rcl_dotnet_thread_sleep(millis)`  => `System.Threading.Thread.Sleep((int)millis)`.
//! * `rcl_dotnet_available_parallelism() -> usize`
//!   => `System.Environment.ProcessorCount`.
//!
//! These are REAL preemptive OS threads (`System.Threading.Thread`), with REAL
//! per-thread TLS (each `thread_local!` key is a `ThreadLocal<IntPtr>` — see
//! `thread_local/dotnet.rs`), a REAL `Mutex` (`SemaphoreSlim`), and — as of the
//! Class-D threading slice — a REAL `Parker` (counting-`SemaphoreSlim`-backed)
//! that lets std's generic `Once`/`RwLock` and a `SemaphoreSlim` `Condvar` work
//! unmodified (see `sync/thread_parking/dotnet.rs` + `docs/THREADING_PAL_RESEARCH.md`).
//! This is sufficient to run contended multi-threaded code (e.g. rayon's
//! work-stealing pool).
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::ffi::CStr;
use crate::io;
use crate::num::NonZero;
use crate::ptr;
use crate::thread::ThreadInit;
use crate::time::Duration;

// FIXED extern contract — mapped to the .NET BCL by the cilly linker. Do not
// rename: the linker keys on these exact symbols.
unsafe extern "C" {
    /// Spawn a managed thread running `entry(arg)`; returns an opaque join handle
    /// (a `GCHandle` `IntPtr`), or null on failure.
    fn rcl_dotnet_thread_spawn(
        entry: unsafe extern "C" fn(*mut u8) -> *mut u8,
        arg: *mut u8,
    ) -> *mut u8;
    /// Join the thread identified by `handle` (returned by `rcl_dotnet_thread_spawn`).
    fn rcl_dotnet_thread_join(handle: *mut u8);
    /// `System.Threading.Thread.Yield()`.
    fn rcl_dotnet_thread_yield();
    /// `System.Threading.Thread.Sleep((int)millis)`.
    fn rcl_dotnet_thread_sleep(millis: u64);
    /// `System.Environment.ProcessorCount`.
    fn rcl_dotnet_available_parallelism() -> usize;
}

pub const DEFAULT_MIN_STACK_SIZE: usize = 2 * 1024 * 1024;

pub struct Thread {
    /// Opaque managed join handle from `rcl_dotnet_thread_spawn`. Non-null for a
    /// live, joinable thread.
    handle: *mut u8,
}

// SAFETY: the handle is an opaque managed `GCHandle` `IntPtr`; moving it between
// threads is sound (it identifies a managed `Thread`, not thread-affine state).
unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
    // unsafe: see thread::Builder::spawn_unchecked for safety requirements
    pub unsafe fn new(_stack: usize, init: Box<ThreadInit>) -> io::Result<Thread> {
        // Leak the init box to a raw pointer handed to the managed thread; the
        // trampoline reclaims it. (The managed side cannot honour a custom stack
        // size, so `_stack` is ignored — same as several other PAL arms.)
        let arg = Box::into_raw(init) as *mut u8;

        // The C-ABI entry the managed thread invokes. It reconstructs the
        // `ThreadInit`, runs the Rust start routine, and returns null (the
        // managed machinery expects an `fn(void*) -> void*`, mirroring pthread).
        unsafe extern "C" fn thread_start(arg: *mut u8) -> *mut u8 {
            // SAFETY: `arg` is exactly the `Box<ThreadInit>` leaked in `new`,
            // handed back to us once, on the new thread.
            let init = unsafe { Box::from_raw(arg as *mut ThreadInit) };
            init.init_dotnet();
            ptr::null_mut()
        }

        // SAFETY: `thread_start` is a valid C-ABI fn pointer and `arg` is the
        // freshly leaked init box; the linker-provided binding spawns a managed
        // thread that calls `thread_start(arg)` exactly once.
        let handle = unsafe { rcl_dotnet_thread_spawn(thread_start, arg) };
        if handle.is_null() {
            // Spawn failed: the managed side did not take ownership of `arg`, so
            // reclaim the box to avoid a leak, then report the error.
            // SAFETY: on the null (failure) path the trampoline never ran, so the
            // box is still ours and untouched.
            drop(unsafe { Box::from_raw(arg as *mut ThreadInit) });
            return Err(io::const_error!(
                io::ErrorKind::Uncategorized,
                "failed to spawn .NET thread"
            ));
        }
        Ok(Thread { handle })
    }

    pub fn join(self) {
        // SAFETY: `self.handle` came from a successful `rcl_dotnet_thread_spawn`
        // and is joined exactly once (`self` is consumed). The binding frees the
        // underlying `GCHandle`.
        unsafe { rcl_dotnet_thread_join(self.handle) }
    }

    /// DOTNET PAL ARM (Package A/B) — `os::unix::thread::JoinHandleExt`'s
    /// `as_pthread_t` casts `self.as_inner().id() as RawPthread`. There is no
    /// `pthread_t` on .NET; surface the opaque managed-thread `GCHandle` `IntPtr`
    /// as a stable, unique-per-live-thread token (cast to `usize` = `RawPthread`).
    /// **LEAKY:** it is NOT a real `pthread_t` and must not be passed to any libc
    /// pthread fn — none exist on this PAL.
    pub fn id(&self) -> usize {
        self.handle as usize
    }

    /// DOTNET PAL ARM (Package A/B) — `JoinHandleExt::into_pthread_t` consumes the
    /// handle (`self.into_inner().into_id() as RawPthread`). Returns the same
    /// opaque token; ownership of the join is the caller's concern (same leaky
    /// semantics as `id`).
    pub fn into_id(self) -> usize {
        // `Thread` has no Drop (the managed join handle is only freed by an
        // explicit `join`), so consuming `self` here simply forgets the handle —
        // matching the pthread `into_pthread_t` contract where the JoinHandle no
        // longer owns the thread.
        self.handle as usize
    }
}

pub fn available_parallelism() -> io::Result<NonZero<usize>> {
    // SAFETY: argumentless BCL property read (`Environment.ProcessorCount`).
    let count = unsafe { rcl_dotnet_available_parallelism() };
    // ProcessorCount is documented as >= 1, but clamp defensively so we never
    // hand back a zero `NonZero`.
    Ok(NonZero::new(count).unwrap_or(NonZero::<usize>::MIN))
}

pub fn current_os_id() -> Option<u64> {
    // The managed thread id is available (`Thread.CurrentThread.ManagedThreadId`)
    // but not currently surfaced through a hook; std only uses this for naming /
    // diagnostics, so reporting "unknown" is safe.
    None
}

pub fn yield_now() {
    // SAFETY: argumentless BCL call (`Thread.Yield()`).
    unsafe { rcl_dotnet_thread_yield() }
}

pub fn set_name(_name: &CStr) {
    // Naming the managed thread is not wired up yet; this is a no-op (the SGX /
    // several unix arms make the same choice). Safe: names are diagnostic only.
}

pub fn sleep(dur: Duration) {
    // `Thread.Sleep` takes a 32-bit millisecond count; saturate so a very long
    // requested sleep clamps to the max rather than wrapping. Loop so the total
    // requested duration is honoured even past the per-call ceiling.
    let mut millis = dur.as_millis();
    while millis > 0 {
        let chunk = millis.min(i32::MAX as u128);
        // SAFETY: single-argument BCL call; `chunk <= i32::MAX` so the
        // linker-side truncation to `int` is lossless.
        unsafe { rcl_dotnet_thread_sleep(chunk as u64) }
        millis -= chunk;
    }
}
