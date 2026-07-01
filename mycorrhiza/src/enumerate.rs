//! The **enumerator bridge** — wrap any .NET `IEnumerator<T>` as a Rust `impl Iterator<Item = T>`.
//!
//! Every .NET collection (and every LINQ result) is iterated through the interface pair
//! `IEnumerable<T>` / `IEnumerator<T>`. Going through the **interfaces** (all reference types) is what
//! makes this work with the current generic bridge: `List<T>.GetEnumerator()` returns the
//! *value-type* `List<T>.Enumerator`, and instance calls on a generic value type are not yet
//! supported by the backend (`src/terminator/call.rs`). The interface path yields a boxed enumerator
//! object, so the whole `MoveNext`/`get_Current` loop is plain `callvirt` on reference types, exactly
//! as C#'s `foreach` lowers on the interface path.
//!
//! ## Why the enumerator is obtained non-generically, then cast
//!
//! The obvious call — `IEnumerable<T>::GetEnumerator()` returning `IEnumerator<T>` — needs a methodref
//! whose *return* is the **definition-shape** `IEnumerator`1<!0>` (the interface's own generic), but
//! whose produced Rust value must be the *concrete* `IEnumerator<T>` local. The CIL typechecker
//! accepts a bare `!N` against a concrete type (soundly — see the WF-9 marker guard), but not a
//! *nested* generic like `IEnumerator<!0>` against `IEnumerator<T>`, and weakening the checker is
//! forbidden. So instead we take the non-generic route, which involves no `!N` at all:
//!
//! * `System.Collections.IEnumerable::GetEnumerator() -> System.Collections.IEnumerator` (non-generic
//!   reference types on both sides — no generics, no marker).
//! * `castclass` that enumerator to the generic `IEnumerator<T>` (the concrete collection's enumerator
//!   implements both interfaces, so the cast always succeeds; it is the same object reference).
//! * `IEnumerator<T>::get_Current() -> !0` — a bare `!0` return, accepted against the concrete `T`.
//! * `System.Collections.IEnumerator::MoveNext() -> bool` — on the non-generic base interface.
//!
//! A collection handle (`List<T>`, `HashSet<T>`, …) is upcast to its `IEnumerable` view with a real
//! `castclass` (a managed-reference `transmute` is ill-typed CIL); every BCL collection implements
//! the interface, so the cast is infallible.

use crate::intrinsics::{
    RustcCLRInteropManagedClass, RustcCLRInteropManagedGeneric, RustcCLRInteropTypeGeneric,
};

/// The impl assembly for the `System.Collections[.Generic]` interfaces — all live in
/// `System.Private.CoreLib`.
const CORELIB: &str = "System.Private.CoreLib";

/// Generic `IEnumerable<T>` handle — the interface view of any BCL collection. Reference type, so
/// `GetEnumerator` dispatches with `callvirt`.
pub type IEnumerable<T> = RustcCLRInteropManagedGeneric<
    { CORELIB },
    { "System.Collections.Generic.IEnumerable" },
    (T,),
>;
/// Generic `IEnumerator<T>` handle — `get_Current()` on this returns the element `!0`.
type IEnumeratorGeneric<T> = RustcCLRInteropManagedGeneric<
    { CORELIB },
    { "System.Collections.Generic.IEnumerator" },
    (T,),
>;
/// The non-generic base interface `System.Collections.IEnumerable` — its `GetEnumerator()` returns
/// the non-generic `IEnumerator` (no generic markers involved, which sidesteps the nested-generic
/// def-shape return that the typechecker cannot accept against a concrete local).
type IEnumerableNonGeneric =
    RustcCLRInteropManagedClass<{ CORELIB }, { "System.Collections.IEnumerable" }>;
/// The non-generic base interface `System.Collections.IEnumerator` — declares `MoveNext()`.
type IEnumeratorNonGeneric =
    RustcCLRInteropManagedClass<{ CORELIB }, { "System.Collections.IEnumerator" }>;

/// `System.Collections.IEnumerable::GetEnumerator()` — a `callvirt` on the non-generic interface,
/// returning the non-generic `System.Collections.IEnumerator` (a reference type). The receiver is the
/// collection cast to `IEnumerable`.
fn get_enumerator_nongeneric(enumerable: IEnumerableNonGeneric) -> IEnumeratorNonGeneric {
    enumerable.virt0::<"GetEnumerator", IEnumeratorNonGeneric>()
}

/// `IEnumerator<T>::get_Current()` — a `callvirt` returning `!0` (the interface's own generic),
/// accepted against the concrete `T` by the WF-9 marker rule.
fn get_current<T>(en: IEnumeratorGeneric<T>) -> T {
    crate::intrinsics::rustc_clr_interop_generic_call1::<
        { CORELIB },
        { "System.Collections.Generic.IEnumerator" },
        false,
        "get_Current",
        2,
        (T,),
        (RustcCLRInteropTypeGeneric<0>,),
        T,
        IEnumeratorGeneric<T>,
    >(en)
}

/// A Rust [`Iterator`] over a .NET enumerator — the general enumerator bridge. It holds both the
/// non-generic enumerator (for `MoveNext`) and its `IEnumerator<T>` view (for `get_Current`); both are
/// the *same* underlying managed object, obtained by a `castclass` at construction.
///
/// `T` is the element type (a boundary-crossing .NET type — a primitive, a `#[repr(C)]` value-type
/// struct, or a managed handle). The enumerator holds a managed reference, so `T` need not be `Copy`.
pub struct Enumerator<T> {
    /// The non-generic enumerator, used for `MoveNext()`.
    base: IEnumeratorNonGeneric,
    /// The same object, cast to `IEnumerator<T>`, used for `get_Current()`.
    typed: IEnumeratorGeneric<T>,
}

impl<T> Iterator for Enumerator<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.base.virt0::<"MoveNext", bool>() {
            Some(get_current::<T>(self.typed))
        } else {
            None
        }
    }
}

/// Upcast a collection's raw managed handle (its `RustcCLRInteropManagedGeneric<…, (T,)>`) to the
/// generic `IEnumerable<T>` interface via a real `castclass` (NOT a bare reinterpretation — a
/// managed-reference `transmute` is ill-typed CIL and the verifier rejects it). Every BCL collection
/// implements `IEnumerable<T>`, so the cast is infallible. This is the helper each
/// [`crate::collections`] wrapper uses to satisfy [`Enumerable::enumerable_handle`].
///
/// # Safety
/// `handle` must be a live managed object reference that implements `IEnumerable<T>` for this `T`
/// (i.e. any BCL collection handle whose element type is `T`).
#[inline(always)]
pub unsafe fn as_enumerable_handle<H, T>(handle: H) -> IEnumerable<T> {
    crate::intrinsics::rustc_clr_interop_managed_checked_cast::<IEnumerable<T>, H>(handle)
}

/// Upcast an `IEnumerable<T>` to the non-generic `System.Collections.IEnumerable` (a further
/// `castclass`; the generic interface extends the non-generic one).
#[inline(always)]
unsafe fn to_nongeneric_enumerable<T>(e: IEnumerable<T>) -> IEnumerableNonGeneric {
    crate::intrinsics::rustc_clr_interop_managed_checked_cast::<IEnumerableNonGeneric, IEnumerable<T>>(e)
}

/// A managed object that can produce a Rust [`Enumerator`] — i.e. anything implementing
/// `IEnumerable<T>` on the .NET side. Implemented for the [`crate::collections`] wrappers; the entry
/// point [`Self::iter_enumerator`] turns the collection into an [`Iterator`].
pub trait Enumerable<T> {
    /// The raw managed handle for `self` (the collection's own `RustcCLRInteropManagedGeneric`).
    /// Implementors return their handle; the bridge upcasts it to `IEnumerable` internally.
    fn enumerable_handle(&self) -> IEnumerable<T>;

    /// Iterate the elements by driving the .NET enumerator (`GetEnumerator` → `MoveNext`/`Current`).
    /// The collection must not be mutated during iteration (the .NET enumerator throws
    /// `InvalidOperationException` on concurrent modification, exactly as in C#).
    fn iter_enumerator(&self) -> Enumerator<T> {
        // Upcast the collection handle to the non-generic IEnumerable, get the non-generic
        // enumerator, then castclass it to the generic IEnumerator<T> for `get_Current`.
        let base = unsafe {
            get_enumerator_nongeneric(to_nongeneric_enumerable(self.enumerable_handle()))
        };
        let typed: IEnumeratorGeneric<T> =
            crate::intrinsics::rustc_clr_interop_managed_checked_cast::<
                IEnumeratorGeneric<T>,
                IEnumeratorNonGeneric,
            >(base);
        Enumerator { base, typed }
    }
}
