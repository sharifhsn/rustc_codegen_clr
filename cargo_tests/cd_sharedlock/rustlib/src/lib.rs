//! Task 3 — the load-bearing cross-language proof that `mycorrhiza::sync::SharedLock` gives Rust and
//! C# genuine, shared mutual exclusion over the *same* managed `SemaphoreSlim` object.
//!
//! This is deliberately NOT a pure-Rust test (that's `cargo_tests/cd_sync`, check #7): the whole point
//! here is that one side of the critical section is driven by a real C# thread calling `Wait()`/
//! `Release()` directly on the managed handle, while the other side is a genuine Rust OS thread going
//! through [`mycorrhiza::sync::SharedLock::lock`] — and both must serialize on one shared counter with
//! zero lost updates.
//!
//! This crate demonstrates TWO deliberately different scenarios, side by side, so the contrast is
//! concrete rather than asserted:
//!
//! - **Scenario (a), below: bare [`SharedLock`] + a raw Rust-owned static.** C# itself calls into Rust
//!   (`sharedlock_bump_counter_unlocked`) to perform the increment, timed by its own direct
//!   `Wait()`/`Release()` on the shared handle. C# is a genuine co-mutator of the *same* logical counter
//!   — not merely gating access to some separate C#-owned resource. There is no way to hand a
//!   `SharedMutexGuard` (a Rust-only RAII type, produced by a Rust `&self` call and tied to a Rust
//!   lifetime) across the FFI boundary, so `SharedMutex<T>` has no path to let C# perform this
//!   increment itself. The `unsafe` `static mut` access here is therefore irreducible in this design —
//!   not a laziness gap — precisely because C# needs to mutate the *same* data, not just coordinate
//!   timing around Rust's own access to it.
//! - **Scenario (b): [`SharedMutex<T>`], fully safe, no unsafe anywhere.** See
//!   `sharedmutex_new`/`sharedmutex_*` below and the corresponding C# section in `Program.cs`. Here,
//!   ALL increments happen on the Rust side (two real Rust OS threads via
//!   `sharedmutex_spawn_two_workers`); C# only calls in to kick off the work and read back the final
//!   value — it never touches the protected `i64` itself, and never could, because `T` lives inside a
//!   private `UnsafeCell<T>` that this Rust value owns and never exposes across the boundary. This is
//!   exactly [`SharedMutex`]'s documented sweet spot: Rust owns and mutates the data; C# is a caller,
//!   not a co-mutator.
//!
//! No entrypoint: this is a `cdylib`. `#[no_mangle]` (hand-written here, since the exported functions
//! need to pass/return a raw managed `SemaphoreSlim` handle — a shape `#[dotnet_export]`'s marshalling
//! table does not (yet) cover; see the crate-level dotnet_macros doc comment) roots each export against
//! dead-code elimination.

use std::sync::atomic::{AtomicI64, Ordering};
use std::thread;

use mycorrhiza::bindings::System::Threading::SemaphoreSlim;
use mycorrhiza::sync::{SharedLock, SharedMutex};

/// The shared, NON-atomic-protected-by-itself counter both languages increment under the same lock.
/// (It's an `AtomicI64` only so Rust's own aliasing rules are happy about a `static mut`-shaped access
/// pattern from multiple threads; the atomicity of the increment is deliberately irrelevant to the
/// proof — it's a plain `load` + `store` pair below, exactly the read-modify-write shape that would
/// lose updates under real contention if `SharedLock` did NOT provide genuine mutual exclusion.)
static COUNTER: AtomicI64 = AtomicI64::new(0);

/// Non-atomic-looking increment: read, then write back +1. Under a real lock, invoked from two
/// directions (Rust thread + C# thread) alternately, this can never lose an update. Without real
/// cross-language exclusion, it would.
fn bump_counter() {
    let v = COUNTER.load(Ordering::Relaxed);
    // Give a concurrent, non-excluded caller a wide window to interleave (deliberately racy shape).
    let v = v + 1;
    COUNTER.store(v, Ordering::Relaxed);
}

/// Create a new `SharedLock` (a `SemaphoreSlim(1, 1)`) and hand its raw managed handle to C#. This is a
/// genuine typed managed reference — no P/Invoke, no serialization: the returned `SemaphoreSlim` IS the
/// same object `SharedLock::from_raw` will wrap on the Rust side.
#[no_mangle]
pub extern "C" fn sharedlock_new() -> SemaphoreSlim {
    SharedLock::new().raw()
}

/// Reset the shared counter to zero. Call this before starting a round of concurrent increments.
#[no_mangle]
pub extern "C" fn sharedlock_reset_counter() {
    COUNTER.store(0, Ordering::Relaxed);
}

/// Read the shared counter's current value.
#[no_mangle]
pub extern "C" fn sharedlock_get_counter() -> i64 {
    COUNTER.load(Ordering::Relaxed)
}

/// The bare, UNLOCKED read/increment/write — deliberately takes no lock of its own. C# calls this
/// exactly once per iteration from *inside* its own `sem.Wait() ... sem.Release()` pair (see
/// `csharp/Program.cs`), so the mutual exclusion for this call is provided entirely by C#'s own direct
/// use of the shared `SemaphoreSlim`, not by any Rust-side locking. This is what makes the C# side of
/// the proof genuine: if C#'s `Wait()`/`Release()` were not actually excluding the concurrent Rust
/// worker (i.e. if the handle sharing were fake), interleaved calls to this unlocked bump would lose
/// updates.
#[no_mangle]
pub extern "C" fn sharedlock_bump_counter_unlocked() {
    bump_counter();
}

/// Spawn a REAL Rust OS thread (`std::thread::spawn`, independent of whatever thread C# calls this
/// from) that acquires `handle` via [`SharedLock`] and increments the shared counter `iters` times,
/// then blocks the CALLING thread until that Rust thread finishes. This is the Rust side of the
/// concurrent proof: while this call is blocked, a C# thread is expected to be concurrently running its
/// own `Wait()`/increment/`Release()` loop against the *same* handle.
///
/// # Safety
/// `handle` must be a valid `SemaphoreSlim` reference (e.g. one produced by [`sharedlock_new`]).
#[no_mangle]
pub extern "C" fn sharedlock_spawn_rust_worker(handle: SemaphoreSlim, iters: i64) {
    let lock = SharedLock::from_raw(handle);
    let worker = thread::spawn(move || {
        for _ in 0..iters {
            let _g = lock.lock();
            bump_counter();
        }
    });
    worker.join().expect("cd_sharedlock: Rust worker thread panicked");
}

// =================================================================================================
// Scenario (b) -- SharedMutex<T>: fully safe, all-Rust-mutates / C#-only-coordinates contrast case.
//
// Unlike scenario (a) above, C# never mutates this counter itself and never could: `T` (an `i64`) lives
// inside `SharedMutex<T>`'s private `UnsafeCell`, which this crate never exposes across the FFI
// boundary. C# only calls in to start the work and read back the final value -- exactly the
// "Rust owns and mutates, C# only coordinates/observes" scenario `SharedMutex<T>`'s docs describe as
// its correct use case. No `unsafe` appears anywhere in this section.
// =================================================================================================

/// The `SharedMutex<i64>` behind scenario (b). Boxed and leaked into a raw handle so it can be passed
/// back and forth across the FFI boundary as an opaque `isize` -- there is no managed object to hand to
/// C# here (unlike scenario (a)'s `SemaphoreSlim`), because the whole point is that C# gets NO handle
/// onto the protected data, only an opaque token it passes back to Rust-side entry points.
#[no_mangle]
pub extern "C" fn sharedmutex_new(initial: i64) -> isize {
    let boxed = Box::new(SharedMutex::new(initial));
    Box::into_raw(boxed) as isize
}

/// Read the current value. Still goes through `.lock()` on the Rust side -- C# never touches the `i64`
/// directly, it only ever gets a copy handed back across the FFI boundary by value.
///
/// # Safety
/// `handle` must be a live pointer produced by `sharedmutex_new` and not yet freed.
#[no_mangle]
pub extern "C" fn sharedmutex_get(handle: isize) -> i64 {
    let mutex = unsafe { &*(handle as *const SharedMutex<i64>) };
    *mutex.lock()
}

/// Spawn TWO real Rust OS threads, each incrementing the mutex-protected counter `iters` times via
/// `SharedMutex::lock()` -- zero `unsafe` inside this function body itself (the pointer dereference to
/// reach the shared `SharedMutex<i64>` from the opaque FFI handle is the only unsafe, isolated to the
/// FFI seam, not to any data-race-prone access -- exactly the same shape every `#[no_mangle] extern "C"`
/// entry point in this file already needs to cross the boundary at all). Blocks the calling (C#) thread
/// until both Rust workers finish.
///
/// # Safety
/// `handle` must be a live pointer produced by `sharedmutex_new` and not yet freed.
#[no_mangle]
pub extern "C" fn sharedmutex_spawn_two_workers(handle: isize, iters: i64) {
    let mutex = unsafe { &*(handle as *const SharedMutex<i64>) };
    thread::scope(|s| {
        for _ in 0..2 {
            s.spawn(|| {
                for _ in 0..iters {
                    let mut guard = mutex.lock();
                    *guard += 1;
                }
            });
        }
    });
}

/// Free a `SharedMutex<i64>` created by `sharedmutex_new`.
///
/// # Safety
/// `handle` must be a live pointer produced by `sharedmutex_new`, not yet freed, and not used again
/// afterward.
#[no_mangle]
pub extern "C" fn sharedmutex_free(handle: isize) {
    unsafe {
        drop(Box::from_raw(handle as *mut SharedMutex<i64>));
    }
}
