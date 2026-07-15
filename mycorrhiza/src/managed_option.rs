//! Nullable managed references without placing a GC reference in Rust enum storage.
//!
//! CLR nullable-reference types have the same runtime representation as their non-nullable
//! reference type. Rust's ordinary `Option<T>` is not used here because a managed reference inside
//! an enum can land in overlapping coroutine state.
//! [`ManagedOption`](crate::managed_option::ManagedOption) stores either no value or a
//! [`ManagedRef`](crate::managed_option::ManagedRef) containing only an opaque GCHandle token.

use core::cell::Cell;
use core::marker::PhantomData;

use crate::intrinsics::{rustc_clr_interop_managed_is_null, rustc_clr_interop_managed_ld_null};

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

/// A coroutine-safe owner for one non-null managed reference.
pub struct ManagedRef<T> {
    rooted: Cell<*mut u8>,
    _type: PhantomData<fn() -> T>,
}

impl<T> ManagedRef<T> {
    fn from_raw(raw: T) -> Self {
        Self {
            rooted: Cell::new(rustc_clr_interop_managed_box_new(raw)),
            _type: PhantomData,
        }
    }

    /// Consume the root and return the real CLR reference for a managed call boundary.
    pub fn into_raw(self) -> T {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        let raw = unsafe { rustc_clr_interop_managed_box_take(rooted) };
        core::mem::forget(self);
        raw
    }

    /// Invoke a non-capturing synchronous operation with the rooted reference.
    ///
    /// The naked reference exists only for this call and must not be retained by `operation`.
    pub fn with_raw<R>(&self, operation: fn(T) -> R) -> R
    where
        T: Copy,
    {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        let raw = unsafe { rustc_clr_interop_managed_box_take(rooted) };
        let result = operation(raw);
        self.rooted.set(rustc_clr_interop_managed_box_new(raw));
        result
    }
}

impl<T> Drop for ManagedRef<T> {
    fn drop(&mut self) {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        if !rooted.is_null() {
            let _ = unsafe { rustc_clr_interop_managed_box_take::<T>(rooted) };
        }
    }
}

/// A nullable CLR reference whose Rust representation remains safe across suspension points.
pub struct ManagedOption<T> {
    inner: Option<ManagedRef<T>>,
}

impl<T: Copy> ManagedOption<T> {
    /// Convert the CLR reference (possibly null) into a rooted Rust option.
    pub fn from_raw(raw: T) -> Self {
        if rustc_clr_interop_managed_is_null(raw) {
            Self { inner: None }
        } else {
            Self {
                inner: Some(ManagedRef::from_raw(raw)),
            }
        }
    }

    pub fn is_some(&self) -> bool {
        self.inner.is_some()
    }

    pub fn is_none(&self) -> bool {
        self.inner.is_none()
    }

    pub fn as_ref(&self) -> Option<&ManagedRef<T>> {
        self.inner.as_ref()
    }

    /// Convert back to the CLR representation. `None` becomes `null`.
    pub fn into_raw(self) -> T {
        match self.inner {
            Some(value) => value.into_raw(),
            None => rustc_clr_interop_managed_ld_null(),
        }
    }
}
