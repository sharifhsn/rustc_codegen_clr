//! **Cross-thread / cross-language synchronization primitives** — idiomatic Rust wrappers over the
//! .NET `System.Threading` synchronization surface (`SemaphoreSlim`, `ManualResetEventSlim`,
//! `CountdownEvent`, `Barrier`), plus [`SharedLock`]: a mutex-shaped `SemaphoreSlim` meant to be handed
//! to C# as a genuine shared managed reference so Rust and C# can take turns inside the *same* critical
//! section.
//!
//! This mirrors [`crate::task`]'s conventions: thin, `#[inline]` wrappers over the raw generated
//! bindings in [`crate::bindings`], RAII guards for the acquire/release pairs, and (where a wait can be
//! `.await`ed) composition with the [`crate::task`] Task↔Future bridge rather than a second bridge.
//!
//! ```ignore
//! use mycorrhiza::sync::Semaphore;
//!
//! let sem = Semaphore::new(1); // a binary semaphore, i.e. a non-reentrant mutex
//! {
//!     let _permit = sem.acquire(); // blocks until available; released on drop
//!     // ... critical section ...
//! }
//! ```
//!
//! ## Why these and not `std::sync`
//!
//! `std::sync::{Mutex, Condvar, ...}` already work correctly on the dotnet PAL for *pure-Rust*
//! synchronization (see the foreign-thread TLS/Mutex/Parker research this module follows from) — this
//! module exists for the cases `std::sync` cannot cover: waiting on a **.NET-native** primitive
//! directly (so a `WaitAsync()` composes with [`crate::task`], or so a C# caller can see the exact same
//! managed wait object), and — via [`SharedLock`] — genuine cross-language mutual exclusion where a
//! *managed* lock object is shared by reference between a Rust side and a C# side.

use crate::bindings::{
    System::Threading::{Barrier as RawBarrier, CountdownEvent as RawCountdownEvent,
        ManualResetEventSlim as RawManualResetEventSlim, SemaphoreSlim as RawSemaphoreSlim},
};
use crate::task::{await_unit, Task};
use core::future::Future;

// =================================================================================================
// Semaphore
// =================================================================================================

/// A counting semaphore — `System.Threading.SemaphoreSlim` under a Rust-idiomatic, RAII-friendly
/// surface. A `Semaphore::new(1)` (one permit) behaves like a non-reentrant mutex; higher counts allow
/// up to `initial_count` concurrent holders.
///
/// Unlike `std::sync::Mutex`, a semaphore's "unlock" ([`release`](Semaphore::release)) is **not**
/// tied to the acquiring thread — any holder of the handle can release it, from any thread. This is
/// exactly the property that makes [`SharedLock`] (below) meaningful across the Rust/C# boundary.
#[derive(Clone, Copy)]
pub struct Semaphore {
    h: RawSemaphoreSlim,
}

impl Semaphore {
    /// `new SemaphoreSlim(initialCount)` — a semaphore starting with `initial_count` available
    /// permits (and no explicit maximum, i.e. `Release()` may grow the count without bound, matching
    /// the one-argument .NET constructor).
    #[inline]
    pub fn new(initial_count: i32) -> Self {
        Self { h: RawSemaphoreSlim::new(initial_count) }
    }

    /// Wrap a raw managed `SemaphoreSlim` handle (e.g. one received from C#, or produced by
    /// [`SharedLock::raw`]).
    #[inline]
    pub fn from_raw(h: RawSemaphoreSlim) -> Self {
        Self { h }
    }

    /// The raw managed handle, for handing the semaphore to a .NET API (or to C#) expecting
    /// `System.Threading.SemaphoreSlim`.
    #[inline]
    pub fn raw(self) -> RawSemaphoreSlim {
        self.h
    }

    /// `SemaphoreSlim.Wait()` — block the calling thread until a permit is available, then take one.
    /// Prefer [`acquire`](Semaphore::acquire) for the RAII form (guarantees the matching release).
    #[inline]
    pub fn wait(self) {
        self.h.wait();
    }

    /// `SemaphoreSlim.Release()` — return one permit, waking a waiter if any. Returns the semaphore's
    /// previous count (the count just before this release), matching the .NET API. Prefer
    /// [`acquire`](Semaphore::acquire)'s RAII guard over calling this directly.
    #[inline]
    pub fn release(self) -> i32 {
        self.h.release()
    }

    /// `SemaphoreSlim.CurrentCount` — the number of permits currently available (a snapshot; may be
    /// stale the instant it's read under contention).
    #[inline]
    pub fn current_count(self) -> i32 {
        self.h.get_current_count()
    }

    /// Blocking acquire — waits for a permit, then returns a [`SemaphorePermit`] RAII guard that
    /// releases it automatically on drop. This is the recommended entry point (mirrors
    /// `std::sync::Mutex::lock`'s guard shape).
    #[inline]
    pub fn acquire(self) -> SemaphorePermit {
        self.h.wait();
        SemaphorePermit { sem: self }
    }

    /// `SemaphoreSlim.WaitAsync()` `.await`-adapted — composes with [`crate::task`]'s Task↔Future
    /// bridge exactly as sketched: `WaitAsync()` returns a `Task`, fed through
    /// [`crate::task::await_unit`]. Returns a [`SemaphorePermit`] once the wait resolves, so the
    /// release is still RAII even on the async path.
    ///
    /// Same caveat as [`crate::task::TaskFuture`]: do not hold this future's `Task` handle across an
    /// `.await` *inside* a suspending `async fn` state machine — drive it with
    /// [`crate::task::block_on`] instead.
    #[inline]
    pub fn acquire_async(self) -> impl Future<Output = SemaphorePermit> {
        let wait_task = Task::from_raw(self.h.wait_async());
        async move {
            await_unit(wait_task).await;
            SemaphorePermit { sem: self }
        }
    }
}

/// RAII guard returned by [`Semaphore::acquire`] / [`Semaphore::acquire_async`] — releases its permit
/// back to the semaphore when dropped, exactly once.
pub struct SemaphorePermit {
    sem: Semaphore,
}

impl Drop for SemaphorePermit {
    #[inline]
    fn drop(&mut self) {
        self.sem.release();
    }
}

// =================================================================================================
// Signal (ManualResetEventSlim)
// =================================================================================================

/// A manual-reset signal — `System.Threading.ManualResetEventSlim`. Unlike a semaphore, setting it
/// wakes **every** current and future waiter (until [`reset`](Signal::reset) clears it again) rather
/// than handing out one permit per release — the .NET analog of `std::sync::Condvar` combined with a
/// sticky boolean.
#[derive(Clone, Copy)]
pub struct Signal {
    h: RawManualResetEventSlim,
}

impl Signal {
    /// `new ManualResetEventSlim()` — constructed unset (i.e. [`wait`](Signal::wait) blocks until the
    /// first [`set`](Signal::set)).
    #[inline]
    pub fn new() -> Self {
        Self { h: RawManualResetEventSlim::new() }
    }

    /// Wrap a raw managed `ManualResetEventSlim` handle.
    #[inline]
    pub fn from_raw(h: RawManualResetEventSlim) -> Self {
        Self { h }
    }

    /// The raw managed handle.
    #[inline]
    pub fn raw(self) -> RawManualResetEventSlim {
        self.h
    }

    /// `ManualResetEventSlim.Set()` — put the signal into the set state, releasing all current and
    /// future waiters until the next [`reset`](Signal::reset).
    #[inline]
    pub fn set(self) {
        self.h.set();
    }

    /// `ManualResetEventSlim.Wait()` — block the calling thread until the signal is set. Returns
    /// immediately if it is already set.
    #[inline]
    pub fn wait(self) {
        self.h.wait();
    }

    /// `ManualResetEventSlim.Reset()` — put the signal back into the unset state.
    #[inline]
    pub fn reset(self) {
        self.h.reset();
    }

    /// `ManualResetEventSlim.IsSet` — `true` if the signal is currently set.
    #[inline]
    pub fn is_set(self) -> bool {
        self.h.get_is_set()
    }
}

impl Default for Signal {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

// =================================================================================================
// CountdownEvent
// =================================================================================================

/// A one-shot countdown latch — `System.Threading.CountdownEvent`. Starts at `initial_count` and
/// counts down to zero via repeated [`signal`](CountdownEvent::signal) calls; [`wait`](CountdownEvent::wait)
/// blocks until it reaches zero. The .NET analog of a `WaitGroup`.
#[derive(Clone, Copy)]
pub struct CountdownEvent {
    h: RawCountdownEvent,
}

impl CountdownEvent {
    /// `new CountdownEvent(initialCount)`.
    #[inline]
    pub fn new(initial_count: i32) -> Self {
        Self { h: RawCountdownEvent::new(initial_count) }
    }

    /// Wrap a raw managed `CountdownEvent` handle.
    #[inline]
    pub fn from_raw(h: RawCountdownEvent) -> Self {
        Self { h }
    }

    /// The raw managed handle.
    #[inline]
    pub fn raw(self) -> RawCountdownEvent {
        self.h
    }

    /// `CountdownEvent.Signal()` — decrement the count by one. Returns `true` if this call is the one
    /// that brought the count to zero (releasing all waiters); `false` otherwise.
    #[inline]
    pub fn signal(self) -> bool {
        self.h.signal()
    }

    /// `CountdownEvent.Wait()` — block the calling thread until the count reaches zero. Returns
    /// immediately if it is already at zero.
    #[inline]
    pub fn wait(self) {
        self.h.wait();
    }

    /// `CountdownEvent.IsSet` — `true` once the count has reached zero.
    #[inline]
    pub fn is_set(self) -> bool {
        self.h.get_is_set()
    }

    /// `CountdownEvent.CurrentCount` — the number of signals still needed before the count reaches
    /// zero (a snapshot).
    #[inline]
    pub fn current_count(self) -> i32 {
        self.h.get_current_count()
    }
}

// =================================================================================================
// Barrier
// =================================================================================================

/// A reusable cyclic barrier — `System.Threading.Barrier`. `participant_count` threads each call
/// [`signal_and_wait`](Barrier::signal_and_wait); none proceeds past it until all have arrived, after
/// which the barrier resets for its next phase (unlike [`CountdownEvent`], which is one-shot).
#[derive(Clone, Copy)]
pub struct Barrier {
    h: RawBarrier,
}

impl Barrier {
    /// `new Barrier(participantCount)`.
    #[inline]
    pub fn new(participant_count: i32) -> Self {
        Self { h: RawBarrier::new(participant_count) }
    }

    /// Wrap a raw managed `Barrier` handle.
    #[inline]
    pub fn from_raw(h: RawBarrier) -> Self {
        Self { h }
    }

    /// The raw managed handle.
    #[inline]
    pub fn raw(self) -> RawBarrier {
        self.h
    }

    /// `Barrier.SignalAndWait()` — signal arrival at the barrier and block until every participant has
    /// also arrived, then proceed (the barrier resets for the next phase).
    #[inline]
    pub fn signal_and_wait(self) {
        self.h.signal_and_wait();
    }

    /// `Barrier.AddParticipant()` — register one additional participant (must happen between phases).
    /// Returns the phase number the new participant will first wait on.
    #[inline]
    pub fn add_participant(self) -> i64 {
        self.h.add_participant()
    }

    /// `Barrier.AddParticipants(n)` — register `n` additional participants at once.
    #[inline]
    pub fn add_participants(self, n: i32) -> i64 {
        self.h.add_participants(n)
    }

    /// `Barrier.RemoveParticipant()` — unregister one participant.
    #[inline]
    pub fn remove_participant(self) {
        self.h.remove_participant();
    }

    /// `Barrier.RemoveParticipants(n)` — unregister `n` participants at once.
    #[inline]
    pub fn remove_participants(self, n: i32) {
        self.h.remove_participants(n);
    }

    /// `Barrier.ParticipantCount` — the total number of registered participants.
    #[inline]
    pub fn participant_count(self) -> i32 {
        self.h.get_participant_count()
    }

    /// `Barrier.ParticipantsRemaining` — how many participants have yet to arrive at the current
    /// phase (a snapshot).
    #[inline]
    pub fn participants_remaining(self) -> i32 {
        self.h.get_participants_remaining()
    }
}

// =================================================================================================
// SharedLock — the cross-language piece
// =================================================================================================

/// A mutex-shaped lock — a `SemaphoreSlim(1, 1)` — meant to be **shared by reference with C#**, not
/// just used from Rust alone. `lock()` gives a Rust-side RAII guard; [`raw`](SharedLock::raw) hands the
/// same underlying managed object to a C# caller, which can `Wait()`/`Release()` it directly.
///
/// # What this does and does not prove
///
/// **What crosses the boundary and is real:** the underlying `SemaphoreSlim` is one managed .NET
/// object. `Wait()`/`Release()` are genuine CLR-level atomic operations on that *same* object no matter
/// which side (Rust via this wrapper, or C# via the raw handle) calls them — this has been verified
/// under real concurrent load (400,000 increments from each side landing exactly, no lost updates; see
/// [`cargo_tests/cd_sharedlock`](../../cargo_tests/cd_sharedlock) for the load-bearing proof). So the
/// *mutual exclusion itself* is real, shared, OS/CLR-backed exclusion — not an illusion.
///
/// **What does NOT cross the boundary: Rust's compile-time guarantees.** In ordinary Rust,
/// `std::sync::Mutex<T>` couples the lock to the data it protects — the borrow checker makes it
/// *impossible* to touch `T` without holding the guard. `SharedLock` has no such coupling: it is a bare
/// signal, not a `Mutex<T>`. The moment the raw `SemaphoreSlim` handle is handed to C#, correctness
/// becomes **discipline**, not proof — exactly the same as any hand-rolled C# locking convention where
/// callers must remember to `Wait()` before touching shared state and `Release()` after. Rust's type
/// system cannot see across the interop boundary to enforce this; nothing here changes that. Treat
/// `SharedLock` as "a real OS-level mutex you must use correctly," not as a magic cross-language
/// `Mutex<T>`.
///
/// # A specific C#-side footgun
///
/// `SemaphoreSlim.Release()` — unlike `Monitor.Exit` — **does not throw** if called without a matching
/// `Wait()` (it just increments the count, up to `SemaphoreFullException` if that would exceed the
/// configured maximum). This means an unbalanced `Release()` in C# fails *silently* at the call site:
/// it doesn't crash there, it just corrupts the invariant "at most one holder," so a second caller's
/// concurrent `Wait()` can now also succeed, defeating exclusion until the counts happen to realign.
/// There is no compiler or runtime check protecting against this — code review and discipline are the
/// only defenses, on both the Rust and the C# side of a shared `SharedLock`.
#[derive(Clone, Copy)]
pub struct SharedLock {
    h: RawSemaphoreSlim,
}

impl SharedLock {
    /// `new SemaphoreSlim(1, 1)` — a binary semaphore (one permit, capped at one), i.e. a real mutex
    /// shape: exactly one holder at a time, and `Release()` beyond the single permit throws
    /// `SemaphoreFullException` rather than silently growing the count (the two-argument constructor's
    /// max-count cap is what gives this extra safety net over a bare [`Semaphore::new`]`(1)`).
    #[inline]
    pub fn new() -> Self {
        Self { h: RawSemaphoreSlim::new_with_max(1, 1) }
    }

    /// Wrap a raw managed `SemaphoreSlim` handle — e.g. one received from C#, so Rust can join a lock
    /// C# created. There is no way to verify from Rust that the handle is actually shaped `(1, 1)`;
    /// that is on the caller (see the safety notes on [`SharedLock`] itself).
    #[inline]
    pub fn from_raw(h: RawSemaphoreSlim) -> Self {
        Self { h }
    }

    /// The raw managed `SemaphoreSlim` handle — hand this to a `#[dotnet_export]` return type (or a
    /// field on an exported class) to give C# genuine, typed, by-reference access to the same lock
    /// object. See the [`SharedLock`] docs for exactly what this does and doesn't guarantee once C#
    /// holds it.
    #[inline]
    pub fn raw(self) -> RawSemaphoreSlim {
        self.h
    }

    /// Blocking acquire — `Wait()` then return a [`SharedLockGuard`] that calls `Release()` on drop.
    /// Blocks the calling thread (Rust-native or one .NET handed control to) until the lock is free.
    #[inline]
    pub fn lock(self) -> SharedLockGuard {
        self.h.wait();
        SharedLockGuard { lock: self }
    }

    /// `SemaphoreSlim.WaitAsync()` `.await`-adapted, same shape as [`Semaphore::acquire_async`] — waits
    /// for the lock without blocking the calling thread, returning a [`SharedLockGuard`] once
    /// acquired.
    #[inline]
    pub fn lock_async(self) -> impl Future<Output = SharedLockGuard> {
        let wait_task = Task::from_raw(self.h.wait_async());
        async move {
            await_unit(wait_task).await;
            SharedLockGuard { lock: self }
        }
    }
}

impl Default for SharedLock {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard returned by [`SharedLock::lock`] / [`SharedLock::lock_async`] — calls `Release()` on the
/// underlying `SemaphoreSlim` when dropped, exactly once. See [`SharedLock`]'s docs for the honest
/// safety story: this guard protects Rust call sites correctly, but cannot protect a C# caller holding
/// the same raw handle from an unbalanced `Release()` — that remains the C# side's responsibility.
pub struct SharedLockGuard {
    lock: SharedLock,
}

impl Drop for SharedLockGuard {
    #[inline]
    fn drop(&mut self) {
        self.lock.h.release();
    }
}
