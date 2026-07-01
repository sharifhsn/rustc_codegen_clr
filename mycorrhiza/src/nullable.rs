//! `System.Nullable<T>` ↔ Rust `Option<T>`.
//!
//! Many BCL APIs take or return a `Nullable<T>` (a generic **value type**). This bridges it to a Rust
//! `Option<T>` using the value-type-generic instance-method path (`get_HasValue()`/`get_Value()` are
//! `call instance` on the unboxed valuetype). The common direction — turning a `.NET`-produced
//! `Nullable<T>` into an `Option<T>` — is [`Nullable::to_option`]; construct a present value with
//! [`some`].

use crate::intrinsics::{
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_ctor1,
    RustcCLRInteropManagedGenericStruct, RustcCLRInteropTypeGeneric,
};

const CORELIB: &str = "System.Private.CoreLib";
const NULLABLE: &str = "System.Nullable";

/// A managed `System.Nullable<T>` value. `SIZE` is a Rust-side placeholder — the backend lowers this
/// to a `ClassRef` and the CLR knows the real size — so one fixed size works for every `T`.
pub type Nullable<T> = RustcCLRInteropManagedGenericStruct<CORELIB, NULLABLE, 16, (T,)>;

/// `new Nullable<T>(value)` — a *present* nullable wrapping `value`.
pub fn some<T>(value: T) -> Nullable<T> {
    rustc_clr_interop_generic_ctor1::<
        CORELIB,
        NULLABLE,
        true,
        (T,),
        ((), RustcCLRInteropTypeGeneric<0>),
        Nullable<T>,
        T,
    >(value)
}

/// `Nullable<T>::get_HasValue()` — value-type instance getter, `true` if a value is present.
fn has_value<T>(n: &Nullable<T>) -> bool {
    rustc_clr_interop_generic_call1::<
        CORELIB,
        NULLABLE,
        true,
        "get_HasValue",
        1,
        (T,),
        (bool,),
        bool,
        &Nullable<T>,
    >(n)
}
/// `Nullable<T>::get_Value()` — value-type instance getter returning the wrapped `!0` (only valid
/// when `HasValue`; the caller checks first).
fn get_value<T>(n: &Nullable<T>) -> T {
    rustc_clr_interop_generic_call1::<
        CORELIB,
        NULLABLE,
        true,
        "get_Value",
        1,
        (T,),
        (RustcCLRInteropTypeGeneric<0>,),
        T,
        &Nullable<T>,
    >(n)
}

/// Turn a `.NET` `Nullable<T>` into a Rust `Option<T>` (`Some` iff `HasValue`).
pub trait NullableExt<T> {
    /// `Some(value)` if the nullable has a value, else `None`.
    fn to_option(&self) -> Option<T>;
}
impl<T> NullableExt<T> for Nullable<T> {
    fn to_option(&self) -> Option<T> {
        if has_value::<T>(self) {
            Some(get_value::<T>(self))
        } else {
            None
        }
    }
}
