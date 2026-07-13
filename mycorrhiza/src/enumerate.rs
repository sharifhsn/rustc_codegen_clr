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
//! whose *return* is the **definition-shape** `IEnumerator<!0>` (the interface's own generic), but
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
    RustcCLRInteropManagedClass, RustcCLRInteropManagedGeneric,
    RustcCLRInteropManagedGenericStruct, RustcCLRInteropTypeGeneric,
};

/// The impl assembly for the `System.Collections[.Generic]` interfaces — all live in
/// `System.Private.CoreLib`.
const CORELIB: &str = "System.Private.CoreLib";

/// Generic `IEnumerable<T>` handle — the interface view of any BCL collection. Reference type, so
/// `GetEnumerator` dispatches with `callvirt`.
pub type IEnumerable<T> =
    RustcCLRInteropManagedGeneric<{ CORELIB }, "System.Collections.Generic.IEnumerable", (T,)>;
/// Generic `IEnumerator<T>` handle — `get_Current()` on this returns the element `!0`.
type IEnumeratorGeneric<T> =
    RustcCLRInteropManagedGeneric<{ CORELIB }, "System.Collections.Generic.IEnumerator", (T,)>;
/// The non-generic base interface `System.Collections.IEnumerable` — its `GetEnumerator()` returns
/// the non-generic `IEnumerator` (no generic markers involved, which sidesteps the nested-generic
/// def-shape return that the typechecker cannot accept against a concrete local).
type IEnumerableNonGeneric =
    RustcCLRInteropManagedClass<{ CORELIB }, "System.Collections.IEnumerable">;
/// The non-generic base interface `System.Collections.IEnumerator` — declares `MoveNext()`.
type IEnumeratorNonGeneric =
    RustcCLRInteropManagedClass<{ CORELIB }, "System.Collections.IEnumerator">;

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
        "System.Collections.Generic.IEnumerator",
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
///
/// Never `Dispose()`d — unlike a C# `foreach`, which the compiler wraps in `try/finally` to dispose
/// even on early `break`. Dropping this without disposing is fine for the common in-memory BCL
/// enumerators (a no-op `Dispose`), but an enumerator backed by a real resource (a lock, a stream) will
/// leak that resource until GC if iteration stops early.
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

/// A managed `KeyValuePair<K, V>` value — the element type produced by enumerating a
/// `Dictionary<K, V>`. It is a generic **value type**, so `get_Key()`/`get_Value()` are reached with
/// `call instance` on the unboxed valuetype (the value-type-generic instance-method path). The `SIZE`
/// parameter is a Rust-side placeholder only — the backend lowers this to a `ClassRef` and the CLR
/// knows the real size regardless of `K`/`V` — so a single fixed `SIZE` works for every `K`, `V`.
pub type KeyValuePair<K, V> = RustcCLRInteropManagedGenericStruct<
    { CORELIB },
    "System.Collections.Generic.KeyValuePair",
    16,
    (K, V),
>;

/// `KeyValuePair<K, V>::get_Key()` — a value-type instance getter (`call instance`, receiver by `&`),
/// returning the class generic `!0`.
fn kvp_key<K, V>(kvp: &KeyValuePair<K, V>) -> K {
    crate::intrinsics::rustc_clr_interop_generic_call1::<
        { CORELIB },
        "System.Collections.Generic.KeyValuePair",
        true,
        "get_Key",
        1,
        (K, V),
        (RustcCLRInteropTypeGeneric<0>,),
        K,
        &KeyValuePair<K, V>,
    >(kvp)
}
/// `KeyValuePair<K, V>::get_Value()` — value-type instance getter returning `!1`.
fn kvp_value<K, V>(kvp: &KeyValuePair<K, V>) -> V {
    crate::intrinsics::rustc_clr_interop_generic_call1::<
        { CORELIB },
        "System.Collections.Generic.KeyValuePair",
        true,
        "get_Value",
        1,
        (K, V),
        (RustcCLRInteropTypeGeneric<1>,),
        V,
        &KeyValuePair<K, V>,
    >(kvp)
}

/// A Rust [`Iterator`] over a `Dictionary<K, V>`'s `(key, value)` entries. Drives the enumerator over
/// `KeyValuePair<K, V>` (a generic value type) and splits each pair into `(K, V)` with the value-type
/// instance getters. Yielded in the dictionary's own enumeration order.
pub struct EntryIter<K, V> {
    inner: Enumerator<KeyValuePair<K, V>>,
}
impl<K, V> Iterator for EntryIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<(K, V)> {
        let kvp = self.inner.next()?;
        Some((kvp_key::<K, V>(&kvp), kvp_value::<K, V>(&kvp)))
    }
}

/// A managed object that enumerates as `KeyValuePair<K, V>` (i.e. a `Dictionary<K, V>`), yielding a
/// Rust iterator of `(K, V)` pairs. The blanket [`Enumerable`] gives the raw `KeyValuePair` stream;
/// this splits each pair into a Rust tuple.
pub trait EnumerableEntries<K, V>: Enumerable<KeyValuePair<K, V>> {
    /// Iterate `(key, value)` entries by driving the .NET enumerator.
    fn iter_entries(&self) -> EntryIter<K, V> {
        EntryIter {
            inner: self.iter_enumerator(),
        }
    }
}
impl<K, V, C: Enumerable<KeyValuePair<K, V>>> EnumerableEntries<K, V> for C {}

/// Marker asserting that every live value of handle type `H` is a managed reference whose .NET class
/// genuinely implements `IEnumerable<T>` — i.e. `H` is a legal `castclass` source for
/// [`IEnumerable<T>`]. This is what makes [`as_enum_handle`]'s upcast infallible, and it turns that
/// upcast into a **safe** function: the invariant is proven once, where the handle type is defined
/// (typically right next to the `dotnet_generic!` alias for a BCL collection, which is documented to
/// implement the interface), rather than re-asserted with `unsafe` at every call site. This mirrors the
/// [`crate::ManagedSafe`] / [`crate::StackOnly`] marker-trait pattern used elsewhere in this crate.
///
/// # Safety
/// Implement this only for a handle type you know — from BCL documentation or your own binding's class
/// declaration — corresponds to a .NET type implementing `System.Collections.Generic.IEnumerable<T>`
/// for this exact `T`. Getting it wrong turns the `castclass` in `as_enum_handle` into a runtime
/// `InvalidCastException` the first time the resulting `IEnumerable<T>` is used.
pub unsafe trait ImplementsIEnumerable<T> {}

/// Upcast a collection's raw managed handle (its `RustcCLRInteropManagedGeneric<…, (T,)>`) to the
/// generic `IEnumerable<T>` interface via a real `castclass` (NOT a bare reinterpretation — a
/// managed-reference `transmute` is ill-typed CIL and the verifier rejects it). Safe: the
/// [`ImplementsIEnumerable<T>`] bound is the caller's proof (checked once, at the `unsafe impl`, not at
/// every call site) that `H`'s .NET class implements `IEnumerable<T>`, so the cast is infallible. This
/// is the helper each [`crate::collections`] wrapper uses to satisfy [`Enumerable::enumerable_handle`].
#[inline(always)]
pub fn as_enum_handle<H: ImplementsIEnumerable<T>, T>(handle: H) -> IEnumerable<T> {
    // The `ImplementsIEnumerable<T>` bound is exactly the invariant this cast needs; the callee
    // itself is a plain (safe) generic cast helper, so no `unsafe` block is needed here.
    crate::intrinsics::rustc_clr_interop_managed_checked_cast::<IEnumerable<T>, H>(handle)
}

/// The unchecked escape hatch for a handle type that doesn't (yet) implement
/// [`ImplementsIEnumerable<T>`] — e.g. a one-off cast in application code that doesn't want to wire the
/// marker impl. Prefer implementing [`ImplementsIEnumerable<T>`] for your handle type and calling the
/// safe [`as_enum_handle`] instead; that pays the safety proof once instead of at every call site.
///
/// # Safety
/// `handle` must be a live managed object reference that implements `IEnumerable<T>` for this `T`
/// (i.e. any BCL collection handle whose element type is `T`).
#[inline(always)]
pub unsafe fn as_enum_handle_unchecked<H, T>(handle: H) -> IEnumerable<T> {
    // SAFETY (of the *unsafe fn contract*, not any operation below): forwarded to the caller of this
    // function per its doc; the callee itself is a plain (safe) generic cast helper.
    crate::intrinsics::rustc_clr_interop_managed_checked_cast::<IEnumerable<T>, H>(handle)
}

/// Upcast an `IEnumerable<T>` to the non-generic `System.Collections.IEnumerable` (a further
/// `castclass`; the generic interface extends the non-generic one).
#[inline(always)]
unsafe fn to_nongeneric_enumerable<T>(e: IEnumerable<T>) -> IEnumerableNonGeneric {
    crate::intrinsics::rustc_clr_interop_managed_checked_cast::<IEnumerableNonGeneric, IEnumerable<T>>(
        e,
    )
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
