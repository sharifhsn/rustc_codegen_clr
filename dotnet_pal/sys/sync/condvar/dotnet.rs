//! `sys::sync::Condvar` for the .NET ("dotnet") platform — REAL condition variable.
//!
//! There is no generic Parker-only `Condvar` arm in std (the generic ones are
//! `futex` and `pthread`, both unavailable on os=dotnet), so this is a small
//! bespoke arm built on a managed `System.Threading.SemaphoreSlim(0,
//! int.MaxValue)` used as a WAKEUP COUNTER — the textbook semaphore condvar. It
//! composes correctly across multiple waiters (a single shared
//! `ManualResetEventSlim` would race on Reset between waiters), and a notify
//! delivered before the matching wait is not lost (the semaphore counts permits).
//!
//! Algorithm (the classic counting-semaphore condvar):
//!   * `wait`: register as a waiter (`waiters += 1`), UNLOCK the mutex, block for
//!     a permit (`sem.Wait()`), then RE-LOCK the mutex. The unlock/wait gap is
//!     safe because a `notify` that races in deposits a permit that `Wait`
//!     immediately consumes (no lost wakeup).
//!   * `notify_one`: release ONE permit if any thread is waiting.
//!   * `notify_all`: release as many permits as there are current waiters.
//!
//! `waiters` over-counts harmlessly: a permit not consumed by its intended waiter
//! (because that waiter already left) only causes a permitted SPURIOUS wakeup of
//! the next waiter — never a missed one. The std `Condvar` API explicitly allows
//! spurious wakeups, so this is contract-correct.
//!
//! The semaphore handle is lazily CAS-installed on first use (word-width
//! `AtomicUsize`, dodging the sub-word atomic hazard), exactly like
//! `sys/sync/mutex/dotnet.rs`.

use crate::sys::sync::Mutex;
use crate::sync::atomic::{AtomicUsize, Ordering};
use crate::time::Duration;

unsafe extern "C" {
    fn rcl_dotnet_condvar_new() -> *mut u8;
    fn rcl_dotnet_condvar_wait(h: *mut u8);
    fn rcl_dotnet_condvar_wait_timeout(h: *mut u8, millis: usize) -> bool;
    fn rcl_dotnet_condvar_release(h: *mut u8, n: usize);
}

pub struct Condvar {
    /// `0` = not yet initialized; otherwise a `*mut u8` GCHandle to the managed
    /// `SemaphoreSlim`, stored as a word so the CAS is word-width.
    sem: AtomicUsize,
    /// Number of threads currently parked in (or about to park in) `wait`. Read
    /// by `notify_all` to size the release; an over-count is harmless (extra
    /// permits cause only permitted spurious wakeups).
    waiters: AtomicUsize,
}

unsafe impl Send for Condvar {}
unsafe impl Sync for Condvar {}

impl Condvar {
    #[inline]
    pub const fn new() -> Condvar {
        Condvar { sem: AtomicUsize::new(0), waiters: AtomicUsize::new(0) }
    }

    /// Live managed-semaphore handle, lazily allocated + CAS-installed on first
    /// use. On a lost race the fresh semaphore is leaked and the winner adopted
    /// (mirrors `sys/sync/mutex/dotnet.rs::handle`).
    #[inline]
    fn sem(&self) -> *mut u8 {
        let cur = self.sem.load(Ordering::Acquire);
        if cur != 0 {
            return cur as *mut u8;
        }
        let fresh = unsafe { rcl_dotnet_condvar_new() } as usize;
        match self.sem.compare_exchange(0, fresh, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => fresh as *mut u8,
            Err(winner) => winner as *mut u8,
        }
    }

    #[inline]
    pub fn notify_one(&self) {
        // Wake one waiter, if any. `release(..., 1)` is a no-op when there is no
        // waiter only in the sense that the permit would sit unconsumed; to avoid
        // accumulating phantom permits we gate on the waiter count.
        if self.waiters.load(Ordering::Acquire) > 0 {
            let h = self.sem();
            unsafe { rcl_dotnet_condvar_release(h, 1) };
        }
    }

    #[inline]
    pub fn notify_all(&self) {
        let n = self.waiters.load(Ordering::Acquire);
        if n > 0 {
            let h = self.sem();
            unsafe { rcl_dotnet_condvar_release(h, n) };
        }
    }

    pub unsafe fn wait(&self, mutex: &Mutex) {
        let h = self.sem();
        self.waiters.fetch_add(1, Ordering::AcqRel);
        // Release the mutex before blocking; a notify racing in deposits a permit
        // that the Wait below consumes immediately (token-not-lost).
        unsafe { mutex.unlock() };
        unsafe { rcl_dotnet_condvar_wait(h) };
        self.waiters.fetch_sub(1, Ordering::AcqRel);
        // Re-acquire the mutex before returning to the caller.
        mutex.lock();
    }

    pub unsafe fn wait_timeout(&self, mutex: &Mutex, dur: Duration) -> bool {
        let h = self.sem();
        // Clamp to the BCL `Wait(int)` overload (<= i32::MAX ms).
        let millis = dur.as_millis().min(i32::MAX as u128) as usize;
        self.waiters.fetch_add(1, Ordering::AcqRel);
        unsafe { mutex.unlock() };
        let notified = unsafe { rcl_dotnet_condvar_wait_timeout(h, millis) };
        self.waiters.fetch_sub(1, Ordering::AcqRel);
        mutex.lock();
        // Return value is `true` if woken by a notify, `false` if it timed out.
        notified
    }
}
