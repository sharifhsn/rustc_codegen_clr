//! `sys::sync::thread_parking::Parker` for the .NET ("dotnet") platform — REAL parking.
//!
//! The Class-D threading KEYSTONE (research: `docs/THREADING_PAL_RESEARCH.md`).
//! Injected as the dedicated `target_os = "dotnet"` arm of
//! `sys/sync/thread_parking/mod.rs`. Once this Parker is real, std's GENERIC
//! `queue`-based `Once`/`RwLock` (which build purely on `thread::park`/`unpark` +
//! atomics) and the dotnet `Condvar` work unmodified — the same code the
//! Linux/futex target runs — and rayon's lazy global-pool init no longer aborts
//! in the `no_threads` `Once`.
//!
//! ## State machine (mirrors `sys/sync/thread_parking/id.rs`)
//!
//! A single word-width atom (`AtomicIsize`, chosen over `AtomicI8` to sidestep the
//! sub-word atomic page/alignment hazard, WF-5) holds one of:
//!   * `EMPTY    =  0` — no token, not parked.
//!   * `PARKED   = -1` — a thread is (about to be) blocked in `park`.
//!   * `NOTIFIED =  1` — an `unpark` token is waiting to be consumed.
//!
//! The blocking itself is a managed COUNTING `System.Threading.SemaphoreSlim(0,
//! int.MaxValue)`, reached through an opaque `*mut u8` GCHandle from the BCL
//! bindings `rcl_dotnet_park_*` (see `cilly/src/ir/builtins/dotnet.rs`). `park`
//! blocks on `Wait` (consume a permit), `unpark` deposits one with `Release`.
//!
//! ## Token-not-lost (and why a counting semaphore, not a reset event)
//!
//! The atom carries the fast path: an `unpark` BEFORE a `park` stores `NOTIFIED`,
//! which `park`'s opening `fetch_sub` observes and consumes WITHOUT ever touching
//! the semaphore. The semaphore is touched only on a real block/wake, exactly once
//! each, so the permit count stays balanced. Crucially, a counting semaphore is
//! token-not-lost FOR FREE: a `Release` that races just ahead of the matching
//! `Wait` leaves a permit in the count, so the `Wait` returns immediately. (An
//! earlier `ManualResetEventSlim` variant deadlocked rayon: resetting a
//! level-triggered event after consuming a wakeup raced a concurrent `unpark` and
//! lost it. The counting semaphore needs no reset and has no such race.) Any rare
//! extra permit is simply a permitted spurious wakeup.
//!
//! ## Handle lifetime
//!
//! `Parker::new_in_place` cannot run managed code at the point std calls it (it is
//! reached through a raw `*mut Parker` write), so the semaphore is allocated
//! lazily and CAS-installed on first use into a word-width `AtomicUsize`, exactly
//! like `sys/sync/mutex/dotnet.rs`. On a lost race the loser's freshly-allocated
//! semaphore is leaked and the winner's handle adopted (acceptable: one semaphore
//! per live `Parker`, freed implicitly at process exit — no Free binding).

use crate::pin::Pin;
use crate::sync::atomic::Ordering::{Acquire, Release, SeqCst};
use crate::sync::atomic::{AtomicIsize, AtomicUsize};
use crate::time::Duration;

unsafe extern "C" {
    fn rcl_dotnet_park_new() -> *mut u8;
    fn rcl_dotnet_park_wait(h: *mut u8);
    fn rcl_dotnet_park_wait_timeout(h: *mut u8, millis: usize) -> bool;
    fn rcl_dotnet_park_release(h: *mut u8);
}

const EMPTY: isize = 0;
const PARKED: isize = -1;
const NOTIFIED: isize = 1;

pub struct Parker {
    /// EMPTY / PARKED / NOTIFIED. Word-width to dodge the sub-word atomic hazard.
    state: AtomicIsize,
    /// `0` = semaphore not yet installed; otherwise a `*mut u8` GCHandle to the
    /// managed `SemaphoreSlim`, stored as a word so the CAS is word-width.
    sem: AtomicUsize,
}

unsafe impl Send for Parker {}
unsafe impl Sync for Parker {}

impl Parker {
    /// Construct a fresh Parker (EMPTY, semaphore not yet installed).
    pub fn new() -> Parker {
        Parker { state: AtomicIsize::new(EMPTY), sem: AtomicUsize::new(0) }
    }

    /// Std requires in-place construction (the Parker is pinned and never moved).
    pub unsafe fn new_in_place(parker: *mut Parker) {
        unsafe { parker.write(Parker::new()) }
    }

    /// Returns the live managed-semaphore handle, allocating + CAS-installing it on
    /// first use. On a lost race the freshly-allocated semaphore is leaked and the
    /// winner's handle adopted (mirrors `sys/sync/mutex/dotnet.rs::handle`).
    #[inline]
    fn sem(&self) -> *mut u8 {
        let cur = self.sem.load(Acquire);
        if cur != 0 {
            return cur as *mut u8;
        }
        let fresh = unsafe { rcl_dotnet_park_new() } as usize;
        match self.sem.compare_exchange(0, fresh, Acquire, Acquire) {
            Ok(_) => fresh as *mut u8,
            Err(winner) => winner as *mut u8,
        }
    }

    // Pinned receiver to match the std `Parker` contract; the Parker never moves.
    pub unsafe fn park(self: Pin<&Self>) {
        // Consume a token if one is already waiting: NOTIFIED -> EMPTY, else
        // EMPTY -> PARKED. (Same opening move as id.rs's `fetch_sub(1)`.)
        if self.state.fetch_sub(1, Acquire) == NOTIFIED {
            // A token was already waiting (now consumed by the fetch_sub); the
            // `unpark` that stored it ran BEFORE we parked and did not block on the
            // semaphore, so there is no permit to drain. Return without blocking.
            return;
        }
        // state was EMPTY -> PARKED. Block until a permit is released. Loop to
        // tolerate spurious wakeups: only return once the atom shows NOTIFIED.
        let sem = self.sem();
        loop {
            unsafe { rcl_dotnet_park_wait(sem) };
            // A real unpark sets NOTIFIED and releases exactly one permit; observe
            // it with acquire ordering, then reset to EMPTY for the next park.
            if self.state.load(Acquire) == NOTIFIED {
                self.state.store(EMPTY, SeqCst);
                return;
            }
            // Spurious wake (a stray permit with no NOTIFIED token): wait again.
        }
    }

    pub unsafe fn park_timeout(self: Pin<&Self>, dur: Duration) {
        if self.state.fetch_sub(1, Acquire) == NOTIFIED {
            return;
        }
        let sem = self.sem();
        // Clamp to <= i32::MAX ms (the BCL `Wait(int)` overload); std already
        // chunks long sleeps, but clamp defensively.
        let millis = dur.as_millis().min(i32::MAX as u128) as usize;
        unsafe { rcl_dotnet_park_wait_timeout(sem, millis) };
        // Whether woken by `unpark` or by timeout, force the atom back to EMPTY
        // with seqcst ordering so we observe all `unpark` writes. If we timed out
        // BEFORE the unpark released its permit, that permit stays in the count and
        // is harmlessly consumed by the next `park`'s `Wait` (a permitted spurious
        // wakeup) — never lost.
        self.state.swap(EMPTY, SeqCst);
    }

    pub fn unpark(self: Pin<&Self>) {
        // Deposit a token. If a thread was PARKED, release one semaphore permit to
        // wake it.
        if self.state.swap(NOTIFIED, Release) == PARKED {
            let sem = self.sem();
            unsafe { rcl_dotnet_park_release(sem) };
        }
    }
}
