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
use crate::{ManagedReferenceType, ManagedRootableType, NativeStorageSafe};

#[doc = "__rustc_codegen_clr_intrinsic_v1"]
#[doc(hidden)]
#[inline(never)]
pub unsafe fn rustc_clr_interop_managed_box_new<T>(_value: T) -> *mut u8 {
    core::intrinsics::abort()
}

#[doc = "__rustc_codegen_clr_intrinsic_v1"]
#[doc(hidden)]
#[inline(never)]
pub unsafe fn rustc_clr_interop_managed_box_get<T>(_handle: *mut u8) -> T {
    core::intrinsics::abort()
}

#[doc = "__rustc_codegen_clr_intrinsic_v1"]
#[doc(hidden)]
#[inline(never)]
pub unsafe fn rustc_clr_interop_managed_box_take<T>(_handle: *mut u8) -> T {
    core::intrinsics::abort()
}

/// Release a GCHandle token without materializing or dropping its managed target.
#[doc = "__rustc_codegen_clr_intrinsic_v1"]
#[doc(hidden)]
#[inline(never)]
pub unsafe fn rustc_clr_interop_managed_box_free(_handle: *mut u8) {
    core::intrinsics::abort()
}

/// A coroutine-safe owner for one managed interop value.
///
/// Ownership of the opaque GCHandle token may move between threads, but shared concurrent access is
/// intentionally unavailable because root consumption is coordinated through an interior `Cell`.
///
/// ```compile_fail
/// use mycorrhiza::{managed_option::ManagedRef, system::MObject};
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<ManagedRef<MObject>>();
/// ```
pub struct ManagedRef<T: ManagedRootableType> {
    rooted: Cell<*mut u8>,
    _type: PhantomData<fn() -> T>,
}

impl<T: ManagedRootableType> ManagedRef<T> {
    /// Root one direct CLR value behind an opaque GCHandle token.
    ///
    /// This is public only so generated `#[dotnet_export]` seams in downstream crates can keep
    /// managed parameters and results outside `catch_unwind`'s Rust closure/`Result` storage.
    /// Ordinary callers should prefer the higher-level rooted wrappers.
    #[doc(hidden)]
    pub fn from_raw(raw: T) -> Self {
        Self {
            rooted: Cell::new(unsafe { rustc_clr_interop_managed_box_new(raw) }),
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

    /// Copy the rooted value into a direct CLR local while preserving this owner.
    ///
    /// The GCHandle remains live while its target is copied out. Callers must use that naked value
    /// only transiently (normally as an immediate managed-call argument); Rust-owned storage must
    /// continue to hold `Self`.
    #[doc(hidden)]
    pub fn copy_raw(&self) -> T
    where
        T: Copy,
    {
        unsafe { rustc_clr_interop_managed_box_get(self.rooted.get()) }
    }

    /// Invoke a non-capturing synchronous operation with the rooted reference.
    ///
    /// The naked reference exists only for this call and must not be retained by `operation`.
    pub fn with_raw<R>(&self, operation: fn(T) -> R) -> R
    where
        T: Copy,
    {
        // `copy_raw` leaves the GCHandle live before invoking arbitrary Rust code, so a caller that
        // catches an unwind still owns a valid root. The copied raw value remains a direct CLR local
        // and is never carried by a Rust Drop guard or aggregate.
        operation(self.copy_raw())
    }
}

// SAFETY: ownership of a GCHandle table token may move between threads. `ManagedRef` deliberately
// remains `!Sync` because its `Cell` permits only exclusive-owner access to that token.
unsafe impl<T: ManagedRootableType> Send for ManagedRef<T> {}
// SAFETY: the Rust representation contains only an opaque native GCHandle token and type-level
// phantom data. The managed target itself never occupies this storage.
unsafe impl<T: ManagedRootableType> NativeStorageSafe for ManagedRef<T> {}

impl<T: ManagedRootableType> Drop for ManagedRef<T> {
    fn drop(&mut self) {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        if !rooted.is_null() {
            unsafe { rustc_clr_interop_managed_box_free(rooted) };
        }
    }
}

/// A nullable CLR reference whose Rust representation remains safe across suspension points.
/// ```compile_fail
/// use mycorrhiza::managed_option::ManagedOption;
/// let _ = ManagedOption::<i32>::from_raw(1);
/// ```
///
/// Managed value structs are rootable with [`ManagedRef`] but are not nullable reference types:
///
/// ```compile_fail
/// #![feature(adt_const_params, unsized_const_params)]
/// use mycorrhiza::{intrinsics::RustcCLRInteropManagedStruct, managed_option::ManagedOption};
/// type Value = RustcCLRInteropManagedStruct<"Tests", "Value", 8>;
/// let value = unsafe { core::mem::zeroed::<Value>() };
/// let _ = ManagedOption::<Value>::from_raw(value);
/// ```
///
/// Managed byrefs are not rootable values:
///
/// ```compile_fail
/// #![feature(adt_const_params, unsized_const_params)]
/// use mycorrhiza::{intrinsics::RustcCLRInteropByRef, managed_option::ManagedRef};
/// type ByRef = RustcCLRInteropByRef<i32>;
/// fn reject(_: ManagedRef<ByRef>) {}
/// ```
pub struct ManagedOption<T: ManagedReferenceType> {
    inner: Option<ManagedRef<T>>,
}

// SAFETY: `Option<ManagedRef<T>>` contains only the token described above plus a native enum tag.
unsafe impl<T: ManagedReferenceType> NativeStorageSafe for ManagedOption<T> {}

impl<T: ManagedReferenceType + Copy> ManagedOption<T> {
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

#[cfg(test)]
mod tests {
    use super::{ManagedOption, ManagedRef};
    use crate::ManagedReferenceType;
    use crate::intrinsics::{
        RustcCLRInteropManagedArray, RustcCLRInteropManagedGeneric, RustcCLRInteropManagedStruct,
    };
    use crate::system::MObject;

    type RawValue = RustcCLRInteropManagedStruct<"Tests", "Value", 8>;

    fn assert_send<T: Send>() {}
    fn assert_reference<T: ManagedReferenceType>() {}

    #[test]
    fn rooted_tokens_are_movable_but_cell_owned() {
        assert_send::<ManagedRef<MObject>>();
        assert_send::<ManagedRef<RawValue>>();
        assert_send::<ManagedOption<MObject>>();
        assert_reference::<MObject>();
        assert_reference::<RustcCLRInteropManagedArray<MObject, 1>>();
        assert_reference::<RustcCLRInteropManagedGeneric<"Tests", "Generic", (MObject,)>>();
    }
}
