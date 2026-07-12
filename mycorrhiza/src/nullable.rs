//! `System.Nullable<T>` ↔ Rust `Option<T>`.
//!
//! Many BCL APIs take or return a `Nullable<T>` (a generic **value type**). This bridges it to a Rust
//! `Option<T>` using the value-type-generic instance-method path (`get_HasValue()`/`get_Value()` are
//! `call instance` on the unboxed valuetype). The common direction — turning a `.NET`-produced
//! `Nullable<T>` into an `Option<T>` — is [`Nullable::to_option`]; construct a present value with
//! [`some`].

use crate::intrinsics::{
    RustcCLRInteropManagedGenericStruct, RustcCLRInteropTypeGeneric,
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_ctor1,
};

const CORELIB: &str = "System.Private.CoreLib";
const NULLABLE: &str = "System.Nullable";

/// A managed `System.Nullable<T>` value. `SIZE` is a Rust-side placeholder — the backend lowers this
/// to a `ClassRef` and the CLR knows the real size — so one fixed size works for every `T`.
pub type Nullable<T> = RustcCLRInteropManagedGenericStruct<CORELIB, NULLABLE, 16, (T,)>;

/// `default(Nullable<T>)` — an *absent* nullable (`HasValue == false`). A real `System.Nullable<T>`'s
/// all-zero-bytes representation IS its default/no-value state (the CLR itself relies on this — e.g.
/// zero-initialized array elements), so a zeroed buffer is exact, not an approximation.
pub fn none<T>() -> Nullable<T> {
    // SAFETY: `Nullable<T>`'s only field is `size_hint: [u8; 16]` (see the struct's own doc) — an
    // all-zero byte buffer is a valid value for any `T`, and the CLR's own `default(Nullable<T>)` is
    // exactly this same all-zero representation.
    unsafe { core::mem::zeroed() }
}

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

/// `Option<T>` -> `Nullable<T>` — the ergonomic escape hatch for `#[dotnet_export]` return values.
/// A bare Rust `Option<T>` can't cross the export seam directly (its layout is whatever niche/tag
/// encoding rustc picked for this specific `T`, not `System.Nullable<T>`'s fixed `{bool,T}` layout —
/// see `docs/RUST_PARITY_ROADMAP.md`'s WF-8 writeup), so an exported fn computing an `Option<T>`
/// internally converts at the boundary: `.into()` (or `Nullable::from(opt)`), then returns the
/// `Nullable<T>` — which `#[dotnet_export]` DOES marshal, straight through to a real C# `T?`.
impl<T> From<Option<T>> for Nullable<T> {
    fn from(opt: Option<T>) -> Nullable<T> {
        match opt {
            Some(v) => some(v),
            None => none(),
        }
    }
}
