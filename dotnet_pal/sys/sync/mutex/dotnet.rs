//! `sys::sync::Mutex` for the .NET ("dotnet") platform — REAL mutual exclusion.
//!
//! Injected as the dedicated `target_os = "dotnet"` arm of
//! `sys/sync/mutex/mod.rs`. Backed by a managed `System.Threading.SemaphoreSlim`
//! created with `initialCount = 1`, `maxCount = 1`: a single-permit counting
//! semaphore is mutually exclusive and NON-reentrant, exactly matching the std
//! `sys::sync::Mutex` contract (and a real `Monitor` would be reentrant, which
//! std forbids — hence `SemaphoreSlim`, not `lock`/`Monitor`).
//!
//! The managed semaphore is reached through an opaque `*mut u8` GCHandle returned
//! by the BCL binding `rcl_dotnet_mutex_new` (see
//! `cilly/src/ir/builtins/dotnet.rs`). Because std `Mutex::new` is a `const fn`
//! and we cannot allocate a managed object at const-eval time, the handle is
//! lazily installed on first use: `handle` starts at `0` (the null sentinel) and
//! the first thread to need it CAS-installs a freshly-allocated semaphore. If two
//! threads race, the loser frees nothing and adopts the winner's handle — the
//! extra `SemaphoreSlim` is leaked (acceptable for this slice; there is no Free
//! binding, and a `Mutex` lives for the program's lifetime in practice).
//!
//! Word-width `AtomicUsize` is used deliberately to sidestep the sub-word atomic
//! page/alignment hazard (WF-5) that affects `Atomic<u8>` statics on this backend.

use crate::sync::atomic::{AtomicUsize, Ordering};

unsafe extern "C" {
    fn rcl_dotnet_mutex_new() -> *mut u8;
    fn rcl_dotnet_mutex_lock(h: *mut u8);
    fn rcl_dotnet_mutex_unlock(h: *mut u8);
    fn rcl_dotnet_mutex_trylock(h: *mut u8) -> bool;
}

pub struct Mutex {
    /// `0` = not yet initialized; otherwise a `*mut u8` GCHandle to the managed
    /// `SemaphoreSlim`, stored as a word so the CAS is word-width.
    handle: AtomicUsize,
}

unsafe impl Send for Mutex {}
unsafe impl Sync for Mutex {}

impl Mutex {
    #[inline]
    pub const fn new() -> Mutex {
        Mutex { handle: AtomicUsize::new(0) }
    }

    /// Returns the live managed-semaphore handle, allocating + CAS-installing it
    /// on first use. On a lost race the freshly-allocated semaphore is leaked and
    /// the winner's handle is adopted.
    #[inline]
    fn handle(&self) -> *mut u8 {
        let cur = self.handle.load(Ordering::Acquire);
        if cur != 0 {
            return cur as *mut u8;
        }
        let fresh = unsafe { rcl_dotnet_mutex_new() } as usize;
        match self.handle.compare_exchange(0, fresh, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => fresh as *mut u8,
            // Lost the race: another thread installed its semaphore first. Adopt
            // it; `fresh` is leaked (no Free binding in this slice).
            Err(winner) => winner as *mut u8,
        }
    }

    #[inline]
    pub fn lock(&self) {
        let h = self.handle();
        unsafe { rcl_dotnet_mutex_lock(h) };
    }

    #[inline]
    pub unsafe fn unlock(&self) {
        // A correctly-paired `unlock` always follows a `lock`, so the handle is
        // already installed; `handle()` simply loads it.
        let h = self.handle();
        unsafe { rcl_dotnet_mutex_unlock(h) };
    }

    #[inline]
    pub fn try_lock(&self) -> bool {
        let h = self.handle();
        unsafe { rcl_dotnet_mutex_trylock(h) }
    }
}
