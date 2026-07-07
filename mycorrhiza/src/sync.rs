//! **Cross-thread / cross-language synchronization primitives** — idiomatic Rust wrappers over the
//! .NET `System.Threading` synchronization surface (`SemaphoreSlim`, `ManualResetEventSlim`,
//! `CountdownEvent`, `Barrier`), plus [`SharedLock`]: a mutex-shaped `SemaphoreSlim` meant to be handed
//! to C# as a genuine shared managed reference so Rust and C# can take turns inside the *same* critical
//! section.
//!
//! On top of `SharedLock`, this module also provides data-owning, fully-safe wrappers for the
//! all-Rust-side case: [`SharedMutex<T>`] (mutual exclusion, a `SharedLock` + `UnsafeCell<T>` in the
//! exact shape of `std::sync::Mutex<T>`), [`SharedRwLock<T>`] (reader/writer, over
//! `ReaderWriterLockSlim`), and [`SharedOnce<T>`] (lazy one-time initialization, the
//! `std::sync::OnceLock<T>` shape, over a `SharedLock`-guarded double-checked-lock). None of these
//! require `unsafe` in calling code — see each type's docs for precisely what safety they do (and do
//! not) extend to a C# caller that also holds the raw lock.
//!
//! Separately, [`channel`]/[`bounded_channel`] give a `std::sync::mpsc`-shaped [`Sender<T>`]/
//! [`Receiver<T>`] pair over `System.Threading.Channels` — genuinely multi-producer multi-consumer
//! (both handles are `Copy`), with blocking, non-blocking (`try_*`), and `.await`-able forms of
//! send/receive. See [`channel`]'s docs for its own cross-language nuance.
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
//! managed wait object), and — via [`SharedLock`]/[`SharedMutex`]/[`SharedRwLock`] — genuine
//! cross-language coordination where a *managed* lock object is shared by reference between a Rust
//! side and a C# side.

use crate::bindings::{
    System::Threading::{Barrier as RawBarrier, CountdownEvent as RawCountdownEvent,
        ManualResetEventSlim as RawManualResetEventSlim, ReaderWriterLockSlim as RawReaderWriterLockSlim,
        SemaphoreSlim as RawSemaphoreSlim},
};
use crate::intrinsics::{
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2, rustc_clr_interop_generic_call3,
    rustc_clr_interop_generic_method_call0, rustc_clr_interop_generic_method_call1,
    RustcCLRInteropManagedClass, RustcCLRInteropManagedGeneric,
    RustcCLRInteropManagedGenericStruct, RustcCLRInteropManagedStruct, RustcCLRInteropMethodGeneric,
    RustcCLRInteropTypeGeneric,
};
use crate::task::{await_task, await_unit, Task, TaskT};
use core::cell::UnsafeCell;
use core::future::Future;
use core::ops::{Deref, DerefMut};

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

// =================================================================================================
// SharedMutex<T> — a real, data-owning Mutex<T> built on SharedLock
// =================================================================================================

/// A data-owning mutex, shaped exactly like `std::sync::Mutex<T>`, built on top of [`SharedLock`]:
/// one [`SharedLock`] plus one [`UnsafeCell<T>`], with the `UnsafeCell` written to (and read from)
/// **only** while the lock is held — encapsulated once, correctly, in this module, and never exposed
/// to callers. Unlike [`SharedLock`] alone, `SharedMutex<T>` gives ordinary Rust code the full
/// `std::sync::Mutex<T>` deal: it is *impossible* to touch `T` without holding a
/// [`SharedMutexGuard`], because the only way to reach `&T`/`&mut T` is through the guard's `Deref`/
/// `DerefMut`, and the guard can only be produced by [`lock`](SharedMutex::lock) /
/// [`lock_async`](SharedMutex::lock_async). No `unsafe` is needed anywhere outside this file to use
/// it correctly — the same promise `std::sync::Mutex<T>` makes.
///
/// # What safety this does and does not extend to C#
///
/// **Fully safe, for pure-Rust use.** As long as *only Rust code* ever touches `value` through this
/// wrapper, the guarantee is real and complete: the borrow checker enforces that `T` is reachable
/// only through a live [`SharedMutexGuard`], exactly as with `std::sync::Mutex<T>`. There is no
/// `unsafe` in any calling code, ever.
///
/// **What changes if the raw lock is also shared with C# (via [`shared_lock`](SharedMutex::shared_lock)):**
/// the underlying `SemaphoreSlim` — and *only* the semaphore, not `T` — can be hitchhiked by a
/// `#[dotnet_export]` that hands `shared_lock().raw()` to a C# caller. This is meaningfully different
/// from the bare-[`SharedLock`] + raw-pointer pattern (see `cargo_tests/cd_sharedlock`): there,
/// **C# reads and writes the exact same memory Rust does**, via a raw pointer the two sides agree on
/// out of band, so unsafe on the Rust side is irreducible — the language boundary itself has no type
/// system to enforce anything.
///
/// Here, `T` lives inside a private `UnsafeCell<T>` **owned by this Rust value**. There is no existing
/// mechanism for C# to name that memory, cast into it, or read/write it directly — `T`'s field layout,
/// address, and lifetime are never exposed across the interop boundary by this type. So even if C#
/// holds the same `SemaphoreSlim` (via `shared_lock()`), it can only ever **coordinate timing** —
/// `Wait()`/`Release()` on the shared semaphore — it cannot itself observe or mutate `T`. Concretely,
/// that makes `SharedMutex<T>` the right tool when: **Rust owns and mutates `T`, and C# only needs to
/// gate its own access to some *different*, C#-owned resource using the same lock** (e.g. a native
/// buffer, a file handle, a piece of shared UI state) — not when C# needs typed access to `T` itself.
/// If C# needs to see or change the protected data too, that is exactly the [`SharedLock`] +
/// raw-pointer scenario, and it requires `unsafe`, honestly, on the Rust side.
///
/// Do not oversell this: handing out `shared_lock()` does not make `SharedMutex<T>` a safe
/// cross-language `Mutex<T>`. It remains a safe *Rust-side* `Mutex<T>` whose lock object happens to
/// also be nameable from C# for coordination purposes only.
pub struct SharedMutex<T> {
    lock: SharedLock,
    data: UnsafeCell<T>,
}

// SAFETY: mirrors `std::sync::Mutex<T>`'s own bound. `SharedMutex<T>` only ever exposes `&T`/`&mut T`
// through a `SharedMutexGuard` obtained while the underlying `SharedLock` is held, so concurrent
// access from multiple threads is serialized exactly as `std::sync::Mutex<T>` serializes it — the
// same reasoning that lets `std::sync::Mutex<T>: Sync` hold for any `T: Send` (not `T: Sync`: the
// mutex itself supplies the missing synchronization).
unsafe impl<T: Send> Sync for SharedMutex<T> {}

impl<T> SharedMutex<T> {
    /// Construct a fresh [`SharedLock`] (a new `SemaphoreSlim(1, 1)`) and wrap `value` behind it.
    #[inline]
    pub fn new(value: T) -> Self {
        Self { lock: SharedLock::new(), data: UnsafeCell::new(value) }
    }

    /// Wrap a C#-supplied `SemaphoreSlim` handle as the mutex's lock, alongside a Rust-owned `value`.
    /// Useful when C# has already created (or otherwise holds) the lock object and Rust is joining
    /// it — e.g. the lock was constructed on the C# side and handed in via a `#[dotnet_export]`
    /// parameter. As with [`SharedLock::from_raw`], there is no way to verify from Rust that
    /// `lock_handle` is actually shaped `(1, 1)`; that is on the caller.
    #[inline]
    pub fn from_raw(lock_handle: RawSemaphoreSlim, value: T) -> Self {
        Self { lock: SharedLock::from_raw(lock_handle), data: UnsafeCell::new(value) }
    }

    /// Expose *just* the raw lock — e.g. for a `#[dotnet_export]` to hand to C# so it can coordinate
    /// timing with Rust's access to `value`. See the [`SharedMutex`] docs for exactly what this does
    /// and does not let C# do: C# gets a real, shared `SemaphoreSlim` for `Wait()`/`Release()`
    /// coordination, but no path to `value`'s memory.
    #[inline]
    pub fn shared_lock(&self) -> SharedLock {
        self.lock
    }

    /// Blocking acquire — waits for the lock, then returns a [`SharedMutexGuard`] giving safe `&T`/
    /// `&mut T` access via `Deref`/`DerefMut`, released automatically on drop.
    #[inline]
    pub fn lock(&self) -> SharedMutexGuard<'_, T> {
        SharedMutexGuard { guard: self.lock.lock(), data: &self.data }
    }

    /// `.await`-adapted acquire, composing [`SharedLock::lock_async`] with the data guard — waits for
    /// the lock without blocking the calling thread. Same caveat as [`SharedLock::lock_async`]: do not
    /// hold the returned guard (or its underlying `Task`) across an `.await` *inside* a suspending
    /// `async fn` state machine.
    #[inline]
    pub fn lock_async(&self) -> impl Future<Output = SharedMutexGuard<'_, T>> {
        let data = &self.data;
        async move { SharedMutexGuard { guard: self.lock.lock_async().await, data } }
    }

    /// Safe, lock-free access: `&mut self` already statically proves exclusive access (no other
    /// reference to this `SharedMutex` can exist), so no acquire/release is needed at all — exactly
    /// `std::sync::Mutex::get_mut`'s reasoning.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }

    /// Consume the mutex, taking ownership of the protected value directly (by-value `self` already
    /// proves no other reference can exist).
    #[inline]
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

/// RAII guard returned by [`SharedMutex::lock`] / [`SharedMutex::lock_async`] — dereferences to `&T`/
/// `&mut T` and releases the underlying [`SharedLock`] when dropped (via the held
/// [`SharedLockGuard`]'s own `Drop`; no separate `Drop` impl is needed here). See [`SharedMutex`]'s
/// docs for what this guard's safety does and does not extend to a C# caller holding the same raw
/// lock handle.
pub struct SharedMutexGuard<'a, T> {
    // Never read directly -- held purely so its `Drop` (releasing the SharedLock) fires when this
    // guard is dropped.
    #[allow(dead_code)]
    guard: SharedLockGuard,
    data: &'a UnsafeCell<T>,
}

impl<T> Deref for SharedMutexGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: this guard is only constructed while `self.guard` (the SharedLock) is held, and the
        // lock is the sole gate through which `SharedMutex` ever hands out a reference to `data` — so
        // this is exactly as sound as `std::sync::MutexGuard`'s own `Deref`.
        unsafe { &*self.data.get() }
    }
}

impl<T> DerefMut for SharedMutexGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: see `Deref` above; `&mut self` here additionally proves no other live borrow of
        // this guard exists, matching `std::sync::MutexGuard`'s own `DerefMut` reasoning.
        unsafe { &mut *self.data.get() }
    }
}

// =================================================================================================
// SharedRwLock<T> — a real, data-owning RwLock<T> built on ReaderWriterLockSlim
// =================================================================================================

/// A data-owning reader/writer lock, shaped like `std::sync::RwLock<T>`, built directly on
/// `System.Threading.ReaderWriterLockSlim` (there is no bare `SharedLock`-style intermediate type for
/// the reader/writer case — `ReaderWriterLockSlim` is not usefully "shared as a bare handle" the way a
/// binary `SemaphoreSlim` is, so this wraps the raw BCL type directly, one
/// [`RawReaderWriterLockSlim`] plus one [`UnsafeCell<T>`], written to (and read from) only while the
/// appropriate lock is held.
///
/// # Why this and not `std::sync::RwLock`
///
/// This is **not** a redundant reimplementation of `std::sync::RwLock<T>` — `std::sync::RwLock`
/// already works correctly for pure-Rust reader/writer synchronization on the dotnet PAL. `SharedRwLock<T>`
/// exists for exactly the same reason [`SharedMutex<T>`] exists alongside `std::sync::Mutex`: the
/// underlying lock object is a **.NET-native** `ReaderWriterLockSlim`, reachable via
/// [`SharedRwLock`]-adjacent raw accessors for cross-language coordination, `WaitAsync`-free async
/// composition with the rest of this module's .NET-native primitives, or simply so a C# caller
/// inspecting the same process can see genuine BCL reader/writer state (`IsReadLockHeld`,
/// `CurrentReadCount`, etc. — exposed on [`RawReaderWriterLockSlim`] directly, since those are queries
/// on the raw handle rather than something `SharedRwLock<T>`'s safe surface needs to re-expose). Pick
/// `std::sync::RwLock<T>` for ordinary pure-Rust code; pick `SharedRwLock<T>` when a .NET-native
/// reader/writer object (or its cross-language coordination story) specifically matters.
///
/// # What safety this does and does not extend to C#
///
/// Exactly the same nuance as [`SharedMutex<T>`], restated for the reader/writer shape: **fully safe,
/// complete, and requires zero `unsafe` in calling code, for pure-Rust use.** The only way to reach
/// `&T` is through a live [`SharedRwLockReadGuard`], and the only way to reach `&mut T` is through a
/// live [`SharedRwLockWriteGuard`] — both producible only via [`read`](SharedRwLock::read) /
/// [`write`](SharedRwLock::write), which route through the real `ReaderWriterLockSlim`. If this type
/// is ever extended with a raw-handle accessor for C# to coordinate on, the same limit applies as
/// `SharedMutex::shared_lock`: C# could observe/drive the *lock's* state (enter/exit read or write),
/// never `T`'s memory, since `T` lives in a private `UnsafeCell<T>` this Rust value owns and never
/// exposes across the interop boundary.
pub struct SharedRwLock<T> {
    lock: RawReaderWriterLockSlim,
    data: UnsafeCell<T>,
}

// SAFETY: mirrors `std::sync::RwLock<T>`'s own bound — `Sync` additionally requires `T: Sync` (unlike
// `Mutex<T>`, which only needs `T: Send`), because `SharedRwLockReadGuard` allows *multiple* concurrent
// `&T` borrows (via separate `read()` calls, possibly from separate threads) to be live at once. `T:
// Send` is required because a value written from one thread may be read by another when the lock
// changes hands. `SharedRwLock<T>` only ever exposes `&T` through a read guard (behaviour: many
// concurrent) or `&mut T` through a write guard (behaviour: exclusive of everything else), so access
// is serialized/shared exactly as `ReaderWriterLockSlim` itself serializes/shares it.
unsafe impl<T: Send + Sync> Sync for SharedRwLock<T> {}

impl<T> SharedRwLock<T> {
    /// `new ReaderWriterLockSlim()` (default constructor — `LockRecursionPolicy.NoRecursion`) wrapping
    /// `value`.
    #[inline]
    pub fn new(value: T) -> Self {
        Self { lock: RawReaderWriterLockSlim::new(), data: UnsafeCell::new(value) }
    }

    /// Blocking acquire of a **read** (shared) lock — `EnterReadLock()`, then returns a
    /// [`SharedRwLockReadGuard`] giving safe `&T` access via `Deref`. Any number of readers may hold
    /// the lock concurrently, as long as no writer holds it.
    #[inline]
    pub fn read(&self) -> SharedRwLockReadGuard<'_, T> {
        self.lock.enter_read_lock();
        SharedRwLockReadGuard { lock: &self.lock, data: &self.data }
    }

    /// Blocking acquire of the **write** (exclusive) lock — `EnterWriteLock()`, then returns a
    /// [`SharedRwLockWriteGuard`] giving safe `&T`/`&mut T` access via `Deref`/`DerefMut`. Excludes
    /// every reader and every other writer until dropped.
    #[inline]
    pub fn write(&self) -> SharedRwLockWriteGuard<'_, T> {
        self.lock.enter_write_lock();
        SharedRwLockWriteGuard { lock: &self.lock, data: &self.data }
    }

    /// Safe, lock-free access: `&mut self` already statically proves exclusive access, so no
    /// enter/exit call is needed at all — exactly `std::sync::RwLock::get_mut`'s reasoning.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }

    /// Consume the lock, taking ownership of the protected value directly (by-value `self` already
    /// proves no other reference can exist).
    #[inline]
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

/// RAII guard returned by [`SharedRwLock::read`] — dereferences to `&T` and calls `ExitReadLock()` on
/// the underlying `ReaderWriterLockSlim` when dropped. See [`SharedRwLock`]'s docs for the honest
/// safety story relative to a C# caller.
pub struct SharedRwLockReadGuard<'a, T> {
    lock: &'a RawReaderWriterLockSlim,
    data: &'a UnsafeCell<T>,
}

impl<T> Deref for SharedRwLockReadGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: this guard is only constructed while `self.lock`'s read lock is held, and
        // `SharedRwLock` never hands out `&mut T` (via a write guard) while any read guard could be
        // live — `ReaderWriterLockSlim` itself guarantees a writer excludes all readers — so this is
        // exactly as sound as `std::sync::RwLockReadGuard`'s own `Deref`.
        unsafe { &*self.data.get() }
    }
}

impl<T> Drop for SharedRwLockReadGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.exit_read_lock();
    }
}

/// RAII guard returned by [`SharedRwLock::write`] — dereferences to `&T`/`&mut T` and calls
/// `ExitWriteLock()` on the underlying `ReaderWriterLockSlim` when dropped. See [`SharedRwLock`]'s
/// docs for the honest safety story relative to a C# caller.
pub struct SharedRwLockWriteGuard<'a, T> {
    lock: &'a RawReaderWriterLockSlim,
    data: &'a UnsafeCell<T>,
}

impl<T> Deref for SharedRwLockWriteGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: this guard is only constructed while `self.lock`'s write lock is held, which
        // `ReaderWriterLockSlim` guarantees is exclusive of every reader and every other writer.
        unsafe { &*self.data.get() }
    }
}

impl<T> DerefMut for SharedRwLockWriteGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: see `Deref` above; the write lock's exclusivity is exactly what makes handing out
        // `&mut T` sound here, matching `std::sync::RwLockWriteGuard`'s own `DerefMut` reasoning.
        unsafe { &mut *self.data.get() }
    }
}

impl<T> Drop for SharedRwLockWriteGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.exit_write_lock();
    }
}

// =================================================================================================
// SharedOnce<T> — a real, data-owning OnceLock<T> built on SharedLock
// =================================================================================================

/// A lazy one-time-initialization cell, shaped like `std::sync::OnceLock<T>`: built on top of
/// [`SharedLock`] guarding an `UnsafeCell<Option<T>>`, exactly the same "one lock + one cell,
/// encapsulated once, correctly, in this module" pattern as [`SharedMutex<T>`] and
/// [`SharedRwLock<T>`].
///
/// # Why a hand-rolled double-checked lock, not `System.Lazy<T>`
///
/// .NET's own `System.Lazy<T>` was the obvious first candidate, but it does not fit here: `Lazy<T>`
/// is *generic over `T`*, and a `T` living purely on the Rust side (a Rust struct, a non-`unsafe`
/// value type with Rust-only layout) has no way to be named as a CLR generic type argument through
/// this project's interop surface — the same generic-instantiation-of-Rust-types ceiling documented
/// throughout `mycorrhiza`. Building `SharedOnce<T>` directly on [`SharedLock`] (already a
/// non-generic, real, shared `SemaphoreSlim`) plus a private `UnsafeCell<Option<T>>` sidesteps that
/// entirely: the lock is .NET-native and (optionally) cross-language-shareable via
/// [`shared_lock`](SharedOnce::shared_lock), while `T` stays exactly where [`SharedMutex<T>`] keeps
/// it — owned by this Rust value, never named across the interop boundary.
///
/// The double-checked-lock pattern itself is the standard, correct shape: an uncontended
/// [`get`](SharedOnce::get) or already-initialized [`get_or_init`](SharedOnce::get_or_init) reads a
/// snapshot without ever touching the lock; only the *first* initializer actually acquires it, and
/// every other concurrent caller blocks on that same acquire, then observes the now-initialized
/// value once it lets go — so the closure passed to `get_or_init` runs **at most once**, no matter
/// how many threads (Rust or, via the raw lock, C#) call it concurrently.
///
/// # What safety this does and does not extend to C#
///
/// Exactly the same nuance as [`SharedMutex<T>`] and [`SharedRwLock<T>`]: **fully safe, complete,
/// and requires zero `unsafe` in calling code, for pure-Rust use.** The only way to reach `&T` is
/// through [`get`](SharedOnce::get) / [`get_or_init`](SharedOnce::get_or_init), both of which route
/// through the real `SemaphoreSlim`-backed [`SharedLock`]. If the raw lock is also shared with C# via
/// [`shared_lock`](SharedOnce::shared_lock), C# can coordinate timing on that same semaphore, but has
/// no path to `T`'s memory — `T` lives in a private `UnsafeCell<Option<T>>` this Rust value owns and
/// never exposes across the interop boundary.
pub struct SharedOnce<T> {
    lock: SharedLock,
    // `None` until initialized, `Some(_)` forever after -- read/written only while `lock` is held,
    // except for the lock-free fast-path snapshot read in `get`/`get_or_init` (see their docs).
    data: UnsafeCell<Option<T>>,
}

// SAFETY: mirrors `std::sync::OnceLock<T>`'s own bound. `SharedOnce<T>` only ever writes `data`
// once, under the `SharedLock`, and only ever hands out `&T` (read-only, after that single write) --
// so concurrent access from multiple threads is exactly as sound as `OnceLock<T>: Sync` requires
// `T: Send + Sync` for (the value may be produced on one thread and observed on another, and once
// initialized is read concurrently by any number of threads without further synchronization).
unsafe impl<T: Send + Sync> Sync for SharedOnce<T> {}

impl<T> SharedOnce<T> {
    /// An empty cell, not yet initialized — construct a fresh [`SharedLock`] (a new
    /// `SemaphoreSlim(1, 1)`) to guard the one-time write. Mirrors `OnceLock::new()`.
    #[inline]
    pub fn new() -> Self {
        Self { lock: SharedLock::new(), data: UnsafeCell::new(None) }
    }

    /// Expose *just* the raw lock — e.g. for a `#[dotnet_export]` to hand to C# so it can coordinate
    /// timing with Rust's initialization of the cell. See the [`SharedOnce`] docs for exactly what
    /// this does and does not let C# do: a real, shared `SemaphoreSlim` for `Wait()`/`Release()`
    /// coordination, but no path to `T`'s memory.
    #[inline]
    pub fn shared_lock(&self) -> SharedLock {
        self.lock
    }

    /// Returns `&T` if the cell has already been initialized, `None` otherwise. Never blocks and
    /// never touches the lock: this is a plain snapshot read of the `Option<T>`, which is sound
    /// because after the single initializing write (always lock-protected) the value only ever moves
    /// from `None` to a permanently-fixed `Some(_)` — a torn read is impossible since `data` itself
    /// is never mutated again once `Some`. Mirrors `OnceLock::get`.
    #[inline]
    pub fn get(&self) -> Option<&T> {
        // SAFETY: `data` is written at most once (inside `get_or_init`, under `self.lock`) and, once
        // `Some`, is never written again -- so an unsynchronized read here can only ever observe
        // either `None` or the final, fully-initialized `Some(_)`, never a partial write. This is the
        // same reasoning `std::sync::OnceLock::get` relies on for its fast, lock-free read path.
        unsafe { (*self.data.get()).as_ref() }
    }

    /// Returns `&T`, initializing it by calling `f` if the cell is currently empty. If multiple
    /// threads (Rust-side; see [`shared_lock`](SharedOnce::shared_lock) for the C#-coordination case)
    /// call `get_or_init` concurrently, **exactly one** call to `f` actually runs — every other
    /// caller blocks on the same [`SharedLock`], then observes the value the winner produced. Mirrors
    /// `OnceLock::get_or_init`.
    #[inline]
    pub fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
        // Fast path: already initialized -- no lock needed at all (sound per `get`'s safety note).
        if let Some(v) = self.get() {
            return v;
        }
        // Slow path: acquire the lock, then check AGAIN (double-checked locking) -- another thread
        // may have finished initializing between the fast-path check above and taking the lock.
        let _guard = self.lock.lock();
        // SAFETY: `self.lock` is held here, and it is the sole gate through which `data` is ever
        // written, so no other thread can be concurrently writing (or reading-while-writing) it.
        let slot = unsafe { &mut *self.data.get() };
        if slot.is_none() {
            *slot = Some(f());
        }
        // SAFETY: `slot` is now guaranteed `Some` (either already was, or just set above), and no
        // other writer can run concurrently while `_guard` is held.
        slot.as_ref().unwrap()
    }
}

impl<T> Default for SharedOnce<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

// =================================================================================================
// Channel<T> — an mpsc/mpmc queue over System.Threading.Channels, real from both sides
// =================================================================================================
//
// `System.Threading.Channels` is the .NET analog of `std::sync::mpsc`, but it is genuinely
// **multi-producer multi-consumer** (any number of `ChannelWriter<T>`/`ChannelReader<T>` handles may
// send/receive concurrently — [`Sender`]/[`Receiver`] here are `Copy`, unlike `std::sync::mpsc`'s
// single-consumer-only, non-`Clone` `Receiver`), and every operation has both a synchronous,
// non-blocking form (`TryWrite`/`TryRead`) and a `Task`-returning asynchronous form
// (`WriteAsync`/`ReadAsync`), which this module exposes as blocking and `.await`-able Rust APIs
// respectively via the existing [`crate::task`] Task↔Future bridge — no second bridge is built here.
//
// `Channel.CreateBounded<T>`/`CreateUnbounded<T>` are STATIC GENERIC METHODS on the non-generic
// `Channel` class, returning the class-generic `Channel<T>` (whose `.Reader`/`.Writer` properties are
// themselves generic-instance accesses) — this is the WF-9 generic-method-returning-nested-generic
// path `linq.rs`'s `Expression.Lambda<T>`/`Queryable.AsQueryable<T>` already exercises, applied here to
// a completely different BCL surface.

const CHANNELS_ASM: &str = "System.Threading.Channels";
const CHANNEL: &str = "System.Threading.Channels.Channel";
const CHANNEL_READER: &str = "System.Threading.Channels.ChannelReader";
const CHANNEL_WRITER: &str = "System.Threading.Channels.ChannelWriter";

/// A managed `ChannelReader<T>` handle.
type RawReader<T> = RustcCLRInteropManagedGeneric<CHANNELS_ASM, CHANNEL_READER, (T,)>;
/// A managed `ChannelWriter<T>` handle.
type RawWriter<T> = RustcCLRInteropManagedGeneric<CHANNELS_ASM, CHANNEL_WRITER, (T,)>;
/// The def-shape (`!0`) reader/writer handles a `Channel<T>` `.Reader`/`.Writer` getter returns,
/// before the class generic is bound to the caller's concrete `T`.
type ReaderMG = RustcCLRInteropManagedGeneric<CHANNELS_ASM, CHANNEL_READER, (RustcCLRInteropTypeGeneric<0>,)>;
type WriterMG = RustcCLRInteropManagedGeneric<CHANNELS_ASM, CHANNEL_WRITER, (RustcCLRInteropTypeGeneric<0>,)>;
/// A managed `Channel<T>` handle (the pair-holding object `.Reader`/`.Writer` are read off).
type RawChannel<T> = RustcCLRInteropManagedGeneric<CHANNELS_ASM, CHANNEL, (T,)>;
/// The def-shape (`!!0`, a METHOD generic — `Channel.CreateBounded`/`CreateUnbounded` are static
/// methods on the non-generic `Channel` class, so their own type parameter is a *method* generic,
/// not a class generic) `Channel<!!0>` the two factories return.
type ChannelMethodGen = RustcCLRInteropManagedGeneric<CHANNELS_ASM, CHANNEL, (RustcCLRInteropMethodGeneric<0>,)>;

const CORELIB: &str = "System.Private.CoreLib";
const CANCELLATION_TOKEN: &str = "System.Threading.CancellationToken";
/// `System.Threading.CancellationToken` is a one-field (`CancellationTokenSource? _source`) managed
/// value type; like `Nullable<T>`/`Span<T>` elsewhere in this crate, `SIZE` is a Rust-side placeholder
/// the backend never reads — the CLR alone knows and uses the real layout.
const CANCELLATION_TOKEN_SIZE: usize = core::mem::size_of::<usize>();
type RawCancellationToken = RustcCLRInteropManagedStruct<CORELIB, CANCELLATION_TOKEN, CANCELLATION_TOKEN_SIZE>;

/// `CancellationToken.None` (the static property getter) — every `*Async` call below passes this
/// rather than hand-constructing a zeroed value type, so the CLR itself builds the value.
#[inline]
fn no_cancellation() -> RawCancellationToken {
    RawCancellationToken::vt_static0::<"get_None", RawCancellationToken>()
}

/// `Channel.CreateUnbounded<T>()` — a static generic method (`!!0 = T`), zero real arguments, on the
/// non-generic `Channel` class, returning `Channel<!!0>` (bound to `Channel<T>` at this call site).
fn create_unbounded<T>() -> RawChannel<T> {
    rustc_clr_interop_generic_method_call0::<
        CHANNELS_ASM, CHANNEL, false, "CreateUnbounded", 0u8, (), (T,), (ChannelMethodGen,), RawChannel<T>,
    >()
}

/// `Channel.CreateBounded<T>(capacity)` — same static-generic-method shape as [`create_unbounded`],
/// with one real `int capacity` argument.
fn create_bounded<T>(capacity: i32) -> RawChannel<T> {
    rustc_clr_interop_generic_method_call1::<
        CHANNELS_ASM, CHANNEL, false, "CreateBounded", 0u8, (), (T,), (ChannelMethodGen, i32), RawChannel<T>, i32,
    >(capacity)
}

/// `Channel<T>.Reader` — instance getter, `callvirt`, nested-generic def-shape return `ChannelReader<!0>`.
fn channel_reader<T>(ch: RawChannel<T>) -> RawReader<T> {
    rustc_clr_interop_generic_call1::<
        CHANNELS_ASM, CHANNEL, false, "get_Reader", 2, (T,), (ReaderMG,), RawReader<T>, RawChannel<T>,
    >(ch)
}
/// `Channel<T>.Writer` — instance getter, same shape as [`channel_reader`].
fn channel_writer<T>(ch: RawChannel<T>) -> RawWriter<T> {
    rustc_clr_interop_generic_call1::<
        CHANNELS_ASM, CHANNEL, false, "get_Writer", 2, (T,), (WriterMG,), RawWriter<T>, RawChannel<T>,
    >(ch)
}

/// `ChannelWriter<T>.TryWrite(item)` — non-blocking; `false` if the channel is full (bounded, no room)
/// or already completed.
fn writer_try_write<T>(w: RawWriter<T>, item: T) -> bool {
    rustc_clr_interop_generic_call2::<
        CHANNELS_ASM, CHANNEL_WRITER, false, "TryWrite", 2, (T,),
        (bool, RustcCLRInteropTypeGeneric<0>), bool, RawWriter<T>, T,
    >(w, item)
}

/// `System.Threading.Tasks.ValueTask` — the non-generic value type `ChannelWriter<T>.WriteAsync`
/// returns. As with `CancellationToken` above, `SIZE` is a Rust-side placeholder; the CLR knows the
/// real layout (an `object`, a `short`, and a `bool`).
const VALUE_TASK: &str = "System.Threading.Tasks.ValueTask";
type RawValueTask = RustcCLRInteropManagedStruct<CORELIB, VALUE_TASK, { core::mem::size_of::<usize>() * 2 }>;
/// `System.Threading.Tasks.ValueTask<T>` — the generic-value-type counterpart `ChannelReader<T>.ReadAsync`
/// returns, reached through [`RustcCLRInteropManagedGenericStruct`] (the value-type-generic marker, same
/// role as [`crate::nullable::Nullable`]).
type RawValueTaskT<T> = RustcCLRInteropManagedGenericStruct<CORELIB, VALUE_TASK, { core::mem::size_of::<usize>() * 2 }, (T,)>;
/// The def-shape `ValueTask<!0>` a `ReadAsync` methodref return spells before `T` is bound.
type ValueTaskMG = RustcCLRInteropManagedGenericStruct<CORELIB, VALUE_TASK, { core::mem::size_of::<usize>() * 2 }, (RustcCLRInteropTypeGeneric<0>,)>;
const TASK_CLASS: &str = "System.Threading.Tasks.Task";
type RawTaskHandle = RustcCLRInteropManagedClass<CORELIB, TASK_CLASS>;

/// `ChannelWriter<T>.WriteAsync(item, CancellationToken)` — returns `ValueTask`; here converted to a
/// plain `Task` via `.AsTask()` immediately, so the rest of this module reuses [`crate::task`]'s
/// existing Task↔Future bridge rather than adding a second one for `ValueTask`.
fn writer_write_async<T>(w: RawWriter<T>, item: T) -> Task {
    let vt: RawValueTask = rustc_clr_interop_generic_call3::<
        CHANNELS_ASM, CHANNEL_WRITER, false, "WriteAsync", 2, (T,),
        (RawValueTask, RustcCLRInteropTypeGeneric<0>, RawCancellationToken),
        RawValueTask, RawWriter<T>, T, RawCancellationToken,
    >(w, item, no_cancellation());
    Task::from_raw(vt.vt_instance0::<"AsTask", RawTaskHandle>())
}
/// `ChannelWriter<T>.TryComplete(Exception?)` — signal no more items will be written; `null` means a
/// normal (non-faulted) completion. Returns `false` if already completed.
fn writer_try_complete<T>(w: RawWriter<T>) -> bool {
    type CException = RustcCLRInteropManagedClass<CORELIB, "System.Exception">;
    rustc_clr_interop_generic_call2::<
        CHANNELS_ASM, CHANNEL_WRITER, false, "TryComplete", 2, (T,),
        (bool, CException), bool, RawWriter<T>, CException,
    >(w, crate::intrinsics::rustc_clr_interop_managed_ld_null::<CException>())
}

/// `ChannelReader<T>.TryRead(out item)` — non-blocking. `TryRead`'s `out` parameter is a managed byref,
/// which this bridge represents with [`crate::intrinsics::RustcCLRInteropByRef`] in the `Sig` slot and
/// a plain raw pointer to a Rust local as the actual runtime argument (the same shape `span.rs`'s
/// `get_Item` byref-return uses, just as an argument here rather than a return).
fn reader_try_read<T>(r: RawReader<T>) -> Option<T> {
    use crate::intrinsics::RustcCLRInteropByRef;
    let mut slot = core::mem::MaybeUninit::<T>::uninit();
    let ok = rustc_clr_interop_generic_call2::<
        CHANNELS_ASM, CHANNEL_READER, false, "TryRead", 2, (T,),
        (bool, RustcCLRInteropByRef<RustcCLRInteropTypeGeneric<0>>),
        bool, RawReader<T>, *mut T,
    >(r, slot.as_mut_ptr());
    if ok {
        // SAFETY: `TryRead` returning `true` means the CLR wrote a live `T` through the byref before
        // returning, so `slot` is now initialized.
        Some(unsafe { slot.assume_init() })
    } else {
        None
    }
}
/// `ChannelReader<T>.ReadAsync(CancellationToken)` — returns `ValueTask<T>`, converted to `Task<T>`
/// via `.AsTask()` so it composes with [`crate::task::await_task`]. `AsTask` on a value-type-generic
/// receiver needs `IS_VALUETYPE = true` and `KIND = 1` (`call instance`), exactly like
/// [`crate::nullable`]'s `get_Value`/`get_HasValue`.
fn reader_read_async<T>(r: RawReader<T>) -> TaskT<T> {
    let vt: RawValueTaskT<T> = rustc_clr_interop_generic_call2::<
        CHANNELS_ASM, CHANNEL_READER, false, "ReadAsync", 2, (T,),
        (ValueTaskMG, RawCancellationToken),
        RawValueTaskT<T>, RawReader<T>, RawCancellationToken,
    >(r, no_cancellation());
    // `AsTask()` is a value-type instance call (`IS_VALUETYPE = true`) — the receiver must be passed
    // by managed reference (`&vt`), exactly as `vt_instance0`/`nullable.rs`'s `has_value`/`get_Value`
    // take their value-type receiver.
    rustc_clr_interop_generic_call1::<
        CORELIB, VALUE_TASK, true, "AsTask", 1, (T,),
        (RustcCLRInteropManagedGeneric<CORELIB, TASK_CLASS, (RustcCLRInteropTypeGeneric<0>,)>,),
        TaskT<T>, &RawValueTaskT<T>,
    >(&vt)
}
/// `ChannelReader<T>.Completion` — a `Task` that completes once the channel is both marked complete
/// (`TryComplete`) and fully drained.
fn reader_completion<T>(r: RawReader<T>) -> Task {
    Task::from_raw(rustc_clr_interop_generic_call1::<
        CHANNELS_ASM, CHANNEL_READER, false, "get_Completion", 2, (T,),
        (RawTaskHandle,), RawTaskHandle, RawReader<T>,
    >(r))
}

/// The sending half of a [channel]/[bounded_channel] — a thin, `Copy`, `Send`+`Sync` wrapper over a
/// real managed `ChannelWriter<T>`.
///
/// **Multi-producer, by design.** Unlike `std::sync::mpsc::Sender`, this `Sender<T>` requires no
/// `.clone()` gymnastics to hand out to multiple producer threads — `ChannelWriter<T>` itself supports
/// concurrent callers, so this wrapper is `Copy` outright (it is exactly one managed reference).
#[derive(Clone, Copy)]
pub struct Sender<T> {
    h: RawWriter<T>,
}

// SAFETY: `ChannelWriter<T>`/`ChannelReader<T>` are documented by the BCL to be safe for concurrent
// use by multiple threads (that is the entire reason `System.Threading.Channels` exists over a bare
// queue) — this wrapper adds no additional state, so it inherits that guarantee whenever `T: Send`
// (a value crossing threads must itself be `Send`; the wrapper needs no `T: Sync` since no `&T` is
// ever shared, only `T` moved through the channel).
unsafe impl<T: Send> Send for Sender<T> {}
unsafe impl<T: Send> Sync for Sender<T> {}

impl<T> Sender<T> {
    /// Wrap a raw managed `ChannelWriter<T>` handle — e.g. one received from C#, so Rust can join a
    /// channel C# created (see the [`channel`] docs' cross-language nuance).
    #[inline]
    pub fn from_raw(h: RawWriter<T>) -> Self {
        Self { h }
    }

    /// Non-blocking send — `ChannelWriter<T>.TryWrite(item)`. Returns `Err(item)` (giving the value
    /// back, like `std::sync::mpsc::TrySendError`'s payload) if the channel is full (a bounded channel
    /// at capacity) or already closed.
    #[inline]
    pub fn try_send(self, item: T) -> Result<(), T>
    where
        T: Copy,
    {
        if writer_try_write(self.h, item) {
            Ok(())
        } else {
            Err(item)
        }
    }

    /// Blocking send — waits (spinning the calling thread via [`crate::task::block_on`] over the real
    /// `WriteAsync`) until the item is accepted or the channel is observed closed. Mirrors
    /// `std::sync::mpsc::Sender::send`'s blocking contract, built on the .NET-native async primitive
    /// rather than a second hand-rolled wait loop.
    #[inline]
    pub fn send_blocking(self, item: T) {
        // Precompute the `Task` BEFORE constructing the `async move` block, so the coroutine itself
        // only ever captures a non-generic [`crate::task::Task`] handle across its `.await` — never
        // the generic `ChannelWriter<T>` handle directly. See [`send_async`]'s docs for why that
        // distinction matters (a generic managed handle living in coroutine state, as opposed to
        // `Task`, hits the backend's overlapping-storage layout check).
        let task = writer_write_async(self.h, item);
        crate::task::block_on(async move { await_unit(task).await });
    }

    /// `.await`-adapted send — `ChannelWriter<T>.WriteAsync(item).AsTask()`, driven through
    /// [`crate::task::await_unit`]. Same caveat as the rest of [`crate::task`]: do not hold the
    /// returned future's `Task` across an `.await` *inside* a suspending `async fn` — drive it with
    /// [`crate::task::block_on`] instead.
    #[inline]
    pub fn send_async(self, item: T) -> impl Future<Output = ()> {
        await_unit(writer_write_async(self.h, item))
    }

    /// `ChannelWriter<T>.TryComplete(null)` — mark the channel closed for writing (no more items will
    /// be sent). Idempotent from Rust's point of view: a second call returns `false` and is otherwise a
    /// no-op. Mirrors what dropping `std::sync::mpsc::Sender` does implicitly; here it must be called
    /// explicitly since the managed `ChannelWriter<T>` has no Rust-visible destructor to hook.
    #[inline]
    pub fn close(self) -> bool {
        writer_try_complete(self.h)
    }

    /// The raw managed `ChannelWriter<T>` handle — hand this directly to C#, or to any .NET API
    /// expecting one. See the [module docs](self) for what sharing it means.
    #[inline]
    pub fn raw(self) -> RawWriter<T> {
        self.h
    }
}

/// The receiving half of a [channel]/[bounded_channel] — a thin, `Copy`, `Send`+`Sync` wrapper over a
/// real managed `ChannelReader<T>`.
///
/// **Multi-consumer, by design** — unlike `std::sync::mpsc::Receiver` (single-consumer, not `Clone`),
/// `ChannelReader<T>` supports concurrent readers competing for items (each item still goes to exactly
/// one reader), so this wrapper is `Copy`.
#[derive(Clone, Copy)]
pub struct Receiver<T> {
    h: RawReader<T>,
}

// SAFETY: see `Sender<T>`'s identical justification — `ChannelReader<T>` is BCL-documented safe for
// concurrent multi-threaded use.
unsafe impl<T: Send> Send for Receiver<T> {}
unsafe impl<T: Send> Sync for Receiver<T> {}

impl<T> Receiver<T> {
    /// Wrap a raw managed `ChannelReader<T>` handle — e.g. one received from C#, so Rust can join a
    /// channel C# created (see the [`channel`] docs' cross-language nuance).
    #[inline]
    pub fn from_raw(h: RawReader<T>) -> Self {
        Self { h }
    }

    /// Non-blocking receive — `ChannelReader<T>.TryRead(out item)`. `None` if the channel is currently
    /// empty (whether or not it is closed); use [`recv_blocking`](Receiver::recv_blocking) /
    /// [`recv_async`](Receiver::recv_async) to block/`.await` until either an item arrives or the
    /// channel is definitely, permanently drained.
    #[inline]
    pub fn try_recv(self) -> Option<T> {
        reader_try_read(self.h)
    }

    /// Blocking receive — a spin-poll (via [`crate::task::block_on`]) that alternates the
    /// non-blocking `TryRead` with the real `ReadAsync` wait, so it never touches
    /// `ChannelClosedException` — `ReadAsync` on an already-closed-and-drained channel throws, but
    /// [`recv_async`](Receiver::recv_async) checks [`Receiver::is_definitely_drained`] first and
    /// short-circuits to `None` instead. Returns `None` once the channel is closed and fully drained
    /// (mirroring `std::sync::mpsc::Receiver::recv`'s `Err` on a disconnected sender).
    #[inline]
    pub fn recv_blocking(self) -> Option<T>
    where
        T: Copy,
    {
        if let Some(v) = self.try_recv() {
            return Some(v);
        }
        if self.is_definitely_drained() {
            return None;
        }
        crate::task::block_on(self.recv_async())
    }

    /// `.await`-adapted receive — `ChannelReader<T>.ReadAsync().AsTask()`, driven through
    /// [`crate::task::await_task`]. Resolves to `None` once the channel is closed and drained,
    /// checked via [`Receiver::is_definitely_drained`] *before* issuing `ReadAsync` (which would
    /// otherwise throw `ChannelClosedException` at that point rather than signalling emptiness the
    /// way `TryRead` does).
    ///
    /// This returns a hand-written [`Future`] combinator ([`RecvFuture`]), NOT an `async fn`/
    /// `async move { … }` block. That is deliberate, for a reason [`crate::task`] documents: any
    /// **generic** managed handle (`TaskT<T>`/`ChannelReader<T>`/`ChannelWriter<T>` — anything shaped
    /// like [`crate::intrinsics::RustcCLRInteropManagedGeneric`]) that an `async fn` coroutine's
    /// desugaring captures *across* a suspension point lands in the coroutine's overlapping variant
    /// storage, which the backend's layout checker rejects outright for any GC reference (this is
    /// unconditional — it is not weakened here or anywhere in this crate). A hand-written struct
    /// implementing [`Future`] directly has no such restriction: the generic handle is an ordinary
    /// struct field, exactly how [`crate::task::TaskFuture`] itself holds `TaskT<T>`. So
    /// [`RecvFuture`] simply wraps a [`crate::task::TaskFuture`] and maps its output through `Some`.
    ///
    /// Same caveat as the rest of [`crate::task`]: do not hold this future across an `.await` *inside*
    /// a suspending `async fn` — drive it with [`crate::task::block_on`] instead.
    #[inline]
    pub fn recv_async(self) -> RecvFuture<T> {
        RecvFuture { inner: await_task(reader_read_async(self.h)) }
    }

    /// `true` once the channel is closed (writer-side [`Sender::close`]d) AND fully drained — the
    /// definitive "no more items will ever arrive" signal. Checked before [`recv_blocking`]/
    /// [`recv_async`] would otherwise wait on `ReadAsync`, which throws `ChannelClosedException` at
    /// that point instead of signalling emptiness the way `TryRead` does.
    #[inline]
    pub fn is_definitely_drained(self) -> bool {
        reader_completion(self.h).is_completed()
    }

    /// The raw managed `ChannelReader<T>` handle — hand this directly to C#, or to any .NET API
    /// expecting one.
    #[inline]
    pub fn raw(self) -> RawReader<T> {
        self.h
    }
}

/// The [`Receiver::recv_async`] future — a hand-written [`Future`] impl (see that method's docs for
/// why this cannot be an `async fn`/`async move` block) that wraps a [`crate::task::TaskFuture`] and
/// maps its resolved value through `Some`.
pub struct RecvFuture<T> {
    inner: crate::task::TaskFuture<T>,
}

impl<T> Future for RecvFuture<T> {
    type Output = Option<T>;
    fn poll(self: core::pin::Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<Option<T>> {
        // SAFETY: projecting `Pin<&mut RecvFuture<T>>` to `Pin<&mut TaskFuture<T>>` is sound — this
        // struct has exactly one field, is never `Unpin`-relevant (like `TaskFuture` itself, it holds
        // only a plain `Copy` handle, no address-sensitive data), and `RecvFuture` is never moved out
        // of after being pinned.
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
        // Two racing `recv_blocking`/`recv_async` callers can both pass the pre-close `TryRead`/
        // `is_definitely_drained` checks and then both issue a real `ReadAsync` — that race is
        // inherent to `System.Threading.Channels` itself (`Completion` finishing and the last item
        // being drained are two separate, not-jointly-atomic observations), and the LOSER's
        // `ReadAsync` throws `ChannelClosedException`, faulting its `Task<T>`. `TaskFuture::poll`
        // would otherwise re-throw that as a Rust panic (its documented behavior for any faulted
        // Task) — which would abort here, since a panic cannot unwind across a thread this backend's
        // spawned-thread trampoline can't propagate through. So this checks `is_faulted` FIRST and
        // treats it as the same "channel is closed and drained" signal `try_recv`/
        // `is_definitely_drained` already use, resolving to `None` instead of propagating the
        // exception — the graceful close semantics [`Receiver::recv_blocking`]/
        // [`Receiver::recv_async`] promise.
        if inner.is_faulted() {
            return core::task::Poll::Ready(None);
        }
        inner.poll(cx).map(Some)
    }
}

/// Create an **unbounded** channel — `Channel.CreateUnbounded<T>()`. Never blocks a sender (backed by
/// an unbounded internal queue); use [`bounded_channel`] when producers must be rate-limited by slow
/// consumers.
///
/// # The cross-language nuance (read this before treating a raw handle as Rust-only)
///
/// Exactly like [`SharedLock`], the point of [`Sender::raw`]/[`Receiver::raw`] is that **the same
/// managed `ChannelWriter<T>`/`ChannelReader<T>` object can be handed to C#**, which can genuinely
/// produce into or consume from it via the ordinary `System.Threading.Channels` API — this is not an
/// illusion or a one-way FFI shim: a `#[dotnet_export]` that returns `Sender::raw()`/`Receiver::raw()`
/// gives a C# caller a first-class `ChannelWriter<T>`/`ChannelReader<T>` it can call `TryWrite`/
/// `WriteAsync`/`TryRead`/`ReadAsync` on directly, fully interoperating with whatever Rust does on its
/// side of the same channel.
///
/// **Unlike [`SharedLock`]**, there is no analogous "discipline, not proof" caveat to spell out here
/// beyond the usual "`T` must be a boundary-crossing type" rule [`crate::collections`] already
/// documents: a channel carries values through an already-thread-safe managed queue, not a shared
/// mutable cell — passing an item through it *is* the synchronization (the .NET runtime guarantees
/// each item is observed by exactly one reader, exactly once), so there is no `UnsafeCell`-style
/// aliasing hazard for a C# producer/consumer to violate the way there is with [`SharedMutex<T>`]'s
/// protected data. `Channel<T>` is strictly simpler to share cross-language than `SharedMutex<T>`:
/// both sides get a genuinely equal, genuinely safe producer/consumer relationship to the same queue.
#[inline]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let ch = create_unbounded::<T>();
    (Sender { h: channel_writer(ch) }, Receiver { h: channel_reader(ch) })
}

/// Create a **bounded** channel with room for `capacity` buffered items — `Channel.CreateBounded<T>(capacity)`.
/// Once full, [`Sender::try_send`] returns `Err` and [`Sender::send_blocking`]/[`Sender::send_async`]
/// wait for a consumer to make room — the backpressure `std::sync::mpsc::sync_channel` provides. See
/// [`channel`]'s docs for the cross-language sharing nuance, which applies identically here.
#[inline]
pub fn bounded_channel<T>(capacity: i32) -> (Sender<T>, Receiver<T>) {
    let ch = create_bounded::<T>(capacity);
    (Sender { h: channel_writer(ch) }, Receiver { h: channel_reader(ch) })
}
