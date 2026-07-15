//! Lifetime-safe dispatch of Rust closures onto managed UI threads.
//!
//! [`UiDispatcher`](crate::dispatch::UiDispatcher) is deliberately host-neutral. C# passes an
//! `Mycorrhiza.Interop.Helpers.IRustUiDispatcher`, normally a
//! `SynchronizationContextUiDispatcher` or `DelegateUiDispatcher`. The latter directly adapts
//! WinUI's `DispatcherQueue` and MAUI's `IDispatcher` without making this crate depend on either UI
//! framework. Unity should capture its installed synchronization context on the main thread.
//!
//! Every dispatch owns its Rust closure through a managed `RustDispatchWork` lease. Execution,
//! rejection, adapter failure, and finalization converge on one completion callback, so an accepted
//! callback that is abandoned during host shutdown cannot leak its Rust environment indefinitely.

use crate::class::Class;
use crate::delegate::Action2;
use crate::intrinsics::RustcCLRInteropManagedClass;
use core::fmt;

const HELPERS_ASSEMBLY: &str = "Mycorrhiza.Interop.Helpers";
const DISPATCHER: &str = "Mycorrhiza.Interop.Helpers.IRustUiDispatcher";
const DISPATCH_WORK: &str = "Mycorrhiza.Interop.Helpers.RustDispatchWork";
const DISPATCH_HELPER: &str = "Mycorrhiza.Interop.Helpers.RustUiDispatch";

/// Raw managed dispatcher interface used at an exported method boundary.
pub type IUiDispatcher = RustcCLRInteropManagedClass<{ HELPERS_ASSEMBLY }, { DISPATCHER }>;
type RootUiDispatcher = Class<{ HELPERS_ASSEMBLY }, { DISPATCHER }>;
type RustDispatchWork = RustcCLRInteropManagedClass<{ HELPERS_ASSEMBLY }, { DISPATCH_WORK }>;
type RustUiDispatch = RustcCLRInteropManagedClass<{ HELPERS_ASSEMBLY }, { DISPATCH_HELPER }>;
type Callback = dyn FnOnce() + Send;

/// The host rejected new UI work, usually because its dispatcher is shutting down.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchRejected;

impl fmt::Display for DispatchRejected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the managed UI dispatcher rejected new work")
    }
}

impl std::error::Error for DispatchRejected {}

/// GC-rooted handle to a managed UI dispatcher.
pub struct UiDispatcher {
    root: RootUiDispatcher,
}

impl Clone for UiDispatcher {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
        }
    }
}

impl UiDispatcher {
    /// Root a dispatcher received from C#.
    #[inline]
    pub fn from_raw(dispatcher: IUiDispatcher) -> Self {
        Self {
            root: RootUiDispatcher::from_naked_ref(dispatcher),
        }
    }

    #[inline(never)]
    fn raw(&self) -> IUiDispatcher {
        unsafe { self.root.get_naked_ref() }
    }

    /// Whether the calling thread already has access to the dispatcher's UI thread.
    #[inline]
    pub fn check_access(&self) -> bool {
        self.raw().virt0::<"get_CheckAccess", bool>()
    }

    /// Execute a closure inline on the UI thread or enqueue it from a worker thread.
    ///
    /// Returning `Ok(())` means the closure either ran inline or the host accepted ownership of it.
    /// `Err(DispatchRejected)` means the host rejected it and the closure has already been dropped.
    pub fn try_dispatch(
        &self,
        callback: impl FnOnce() + Send + 'static,
    ) -> Result<(), DispatchRejected> {
        let callback: Box<Callback> = Box::new(callback);
        let id = Box::into_raw(Box::new(callback)) as usize as i64;
        let complete = Action2::<i64, bool>::from_fn(complete_dispatch);
        let work = RustDispatchWork::ctor2(id, complete.handle());
        let accepted =
            RustUiDispatch::static2::<"TryDispatch", IUiDispatcher, RustDispatchWork, bool>(
                self.raw(),
                work,
            );
        accepted.then_some(()).ok_or(DispatchRejected)
    }
}

extern "C" fn complete_dispatch(id: i64, execute: bool) {
    if id == 0 {
        return;
    }

    // SAFETY: `UiDispatcher::try_dispatch` creates exactly one outer Box for this ID.
    // RustDispatchWork exchanges its completion delegate before invoking it, so execution,
    // rejection, and finalization can reconstruct the allocation at most once.
    let callback = unsafe { Box::from_raw(id as usize as *mut Box<Callback>) };
    if execute {
        let callback = *callback;
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback));
    }
}
