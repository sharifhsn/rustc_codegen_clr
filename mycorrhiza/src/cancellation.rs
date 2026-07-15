//! Idiomatic managed cancellation with owned callback-registration lifetimes.

use crate::delegate::Action0;
use crate::intrinsics::RustcCLRInteropManagedStruct;

const CORELIB: &str = "System.Private.CoreLib";
const TOKEN: &str = "System.Threading.CancellationToken";
const REGISTRATION: &str = "System.Threading.CancellationTokenRegistration";

/// A genuine CLR `System.Threading.CancellationToken` value.
pub type CancellationToken =
    RustcCLRInteropManagedStruct<CORELIB, TOKEN, { core::mem::size_of::<usize>() }>;
type RawRegistration = RustcCLRInteropManagedStruct<CORELIB, REGISTRATION, 16>;
type Callback = dyn Fn() + Send + Sync;

/// Zero-sized Rust error used by cooperative cancellation checks. Export a `Result<_,
/// CancellationRequested>` through `#[dotnet_export(cancellation = "task")]` to turn this marker
/// into genuine CLR task cancellation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancellationRequested;

#[allow(unused_variables)]
#[inline(never)]
fn rustc_clr_interop_managed_box_new<T>(value: T) -> *mut u8 {
    core::intrinsics::abort()
}

#[allow(unused_variables)]
#[inline(never)]
unsafe fn rustc_clr_interop_managed_box_take<T>(handle: *mut u8) -> T {
    core::intrinsics::abort()
}

impl CancellationToken {
    /// `CancellationToken.None`.
    #[inline]
    pub fn none() -> Self {
        Self::vt_static0::<"get_None", Self>()
    }

    #[inline]
    pub fn is_cancellation_requested(self) -> bool {
        self.vt_instance0::<"get_IsCancellationRequested", bool>()
    }

    #[inline]
    pub fn can_be_canceled(self) -> bool {
        self.vt_instance0::<"get_CanBeCanceled", bool>()
    }

    /// Throw the CLR `OperationCanceledException` associated with this token when cancellation was
    /// requested. Managed task bridges observe that exception as cancellation.
    #[inline]
    pub fn throw_if_cancellation_requested(self) {
        self.vt_instance0::<"ThrowIfCancellationRequested", ()>();
    }

    /// Return a Rust result suitable for `?`-based cooperative cancellation without throwing
    /// through the Rust body.
    #[inline]
    pub fn ensure_not_canceled(self) -> Result<(), CancellationRequested> {
        if self.is_cancellation_requested() {
            Err(CancellationRequested)
        } else {
            Ok(())
        }
    }

    /// Register an owned Rust callback. The returned guard keeps its closure alive and blocks in
    /// `Dispose` as necessary before freeing it, so a racing callback never observes freed state.
    pub fn register(self, callback: impl Fn() + Send + Sync + 'static) -> CancellationRegistration {
        let callback: Box<Callback> = Box::new(callback);
        let mut owner = Box::new(callback);
        let env = (&mut *owner as *mut Box<Callback>).cast::<()>();
        let action = unsafe { Action0::from_owned_env(env, cancellation_trampoline) };
        let registration = self
            .vt_instance1::<"Register", crate::bindings::System::Action, RawRegistration>(
                action.handle(),
            );
        CancellationRegistration {
            registration,
            active: true,
            _action: action,
            callback: Some(owner),
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::none()
    }
}

/// Coroutine-safe view of a managed cancellation token.
///
/// The raw CLR value contains a managed reference and therefore cannot be stored directly in
/// Rust's overlapping async-state layout. `Cancellation` keeps the boxed value behind a GCHandle
/// token (a plain native pointer in Rust state), briefly unboxing and immediately re-rooting it in
/// non-async helper calls when polling.
pub struct Cancellation {
    rooted_token: *mut u8,
}

impl Cancellation {
    #[inline]
    pub fn from_token(token: CancellationToken) -> Self {
        Self {
            rooted_token: rustc_clr_interop_managed_box_new(token),
        }
    }

    #[inline(never)]
    pub fn is_cancellation_requested(&mut self) -> bool {
        let token =
            unsafe { rustc_clr_interop_managed_box_take::<CancellationToken>(self.rooted_token) };
        let requested = token.is_cancellation_requested();
        self.rooted_token = rustc_clr_interop_managed_box_new(token);
        requested
    }

    #[inline(never)]
    pub fn can_be_canceled(&mut self) -> bool {
        let token =
            unsafe { rustc_clr_interop_managed_box_take::<CancellationToken>(self.rooted_token) };
        let can_be_canceled = token.can_be_canceled();
        self.rooted_token = rustc_clr_interop_managed_box_new(token);
        can_be_canceled
    }

    /// Coroutine-safe `?`-based cooperative cancellation check.
    #[inline]
    pub fn ensure_not_canceled(&mut self) -> Result<(), CancellationRequested> {
        if self.is_cancellation_requested() {
            Err(CancellationRequested)
        } else {
            Ok(())
        }
    }
}

impl Drop for Cancellation {
    #[inline(never)]
    fn drop(&mut self) {
        let _ =
            unsafe { rustc_clr_interop_managed_box_take::<CancellationToken>(self.rooted_token) };
    }
}

extern "C" fn cancellation_trampoline(env: *mut ()) {
    let callback = unsafe { &*(env as *const Box<Callback>) };
    callback();
}

/// Owns a managed cancellation registration, its delegate, and its Rust closure as one lifetime.
/// Dropping it performs the synchronous CLR `Dispose`, which waits out a racing callback before the
/// Rust environment is reclaimed.
pub struct CancellationRegistration {
    registration: RawRegistration,
    active: bool,
    _action: Action0,
    callback: Option<Box<Box<Callback>>>,
}

impl CancellationRegistration {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Attempt non-blocking unregistration. A `false` result preserves and returns the live guard,
    /// because the callback may already be running.
    pub fn try_unregister(mut self) -> Result<(), Self> {
        assert!(self.active, "registration is already inactive");
        if self.registration.vt_instance0::<"Unregister", bool>() {
            self.active = false;
            self.callback.take();
            Ok(())
        } else {
            Err(self)
        }
    }

    /// Synchronously unregister and wait for an in-flight callback before releasing its closure.
    pub fn dispose(mut self) {
        self.dispose_inner();
    }

    fn dispose_inner(&mut self) {
        if self.active {
            self.registration.vt_instance0::<"Dispose", ()>();
            self.active = false;
            self.callback.take();
        }
    }
}

impl Drop for CancellationRegistration {
    fn drop(&mut self) {
        self.dispose_inner();
    }
}
