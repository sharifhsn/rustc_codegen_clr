//! `System.Span<T>` / `ReadOnlySpan<T>` — zero-copy views over Rust memory.
//!
//! A [`Span`] / [`ReadOnlySpan`] wraps a Rust slice as a managed span *in place* (via the
//! `Span<T>(void* pointer, int length)` ctor), so a .NET API can read/write the very same bytes with
//! no copy. The wrapper borrows the slice for `'a`, so the span can't outlive the memory it views.
//!
//! Backed by the value-type-generic instance-method path (`get_Length`/`Fill`/`Clear` are `call
//! instance` on the unboxed `ref struct`) and the byref-returning indexer (`get_Item(int) -> ref T`,
//! read/written through a raw pointer). `T` must be an *unmanaged* boundary-crossing type (a primitive
//! or `#[repr(C)]` value type) — a `Span` of managed references is not representable this way.

use core::marker::PhantomData;

use crate::gen;
use crate::intrinsics::{
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2, rustc_clr_interop_generic_ctor2,
    RustcCLRInteropByRef, RustcCLRInteropManagedGenericStruct, RustcCLRInteropTypeGeneric,
};

const CORELIB: &str = "System.Private.CoreLib";

// A `Span<T>` / `ReadOnlySpan<T>` value is two words (a byref + an int length). `SIZE` is a Rust-side
// placeholder — the backend lowers this to a `ClassRef` and the CLR knows the real size.
type RawSpan<T> = RustcCLRInteropManagedGenericStruct<CORELIB, "System.Span", 16, (T,)>;
type RawRoSpan<T> = RustcCLRInteropManagedGenericStruct<CORELIB, "System.ReadOnlySpan", 16, (T,)>;

// ---- raw Span<T> members (generic over the element type) --------------------------------------
fn span_from_ptr<T>(ptr: *mut (), len: i32) -> RawSpan<T> {
    rustc_clr_interop_generic_ctor2::<
        CORELIB, "System.Span", true, (T,), ((), *mut (), i32), RawSpan<T>, *mut (), i32,
    >(ptr, len)
}
fn span_len<T>(s: &RawSpan<T>) -> i32 {
    rustc_clr_interop_generic_call1::<CORELIB, "System.Span", true, "get_Length", 1, (T,), (i32,), i32, &RawSpan<T>>(s)
}
fn span_fill<T>(s: &RawSpan<T>, value: T) {
    rustc_clr_interop_generic_call2::<CORELIB, "System.Span", true, "Fill", 1, (T,), ((), gen!(0)), (), &RawSpan<T>, T>(s, value)
}
fn span_clear<T>(s: &RawSpan<T>) {
    rustc_clr_interop_generic_call1::<CORELIB, "System.Span", true, "Clear", 1, (T,), ((),), (), &RawSpan<T>>(s)
}
// get_Item(int) -> ref T : the byref indexer. Returns a managed byref (`!0&`) taken as a raw pointer.
fn span_get_ref<T>(s: &RawSpan<T>, idx: i32) -> *mut T {
    rustc_clr_interop_generic_call2::<
        CORELIB, "System.Span", true, "get_Item", 1, (T,),
        (RustcCLRInteropByRef<RustcCLRInteropTypeGeneric<0>>, i32),
        *mut T, &RawSpan<T>, i32,
    >(s, idx)
}

fn rospan_from_ptr<T>(ptr: *const (), len: i32) -> RawRoSpan<T> {
    rustc_clr_interop_generic_ctor2::<
        CORELIB, "System.ReadOnlySpan", true, (T,), ((), *const (), i32), RawRoSpan<T>, *const (), i32,
    >(ptr, len)
}
fn rospan_len<T>(s: &RawRoSpan<T>) -> i32 {
    rustc_clr_interop_generic_call1::<CORELIB, "System.ReadOnlySpan", true, "get_Length", 1, (T,), (i32,), i32, &RawRoSpan<T>>(s)
}
// NOTE: `ReadOnlySpan<T>.get_Item` returns `ref readonly T` — a `modreq(In)`-decorated byref that a
// plain `!0&` methodref cannot match — so element read via the indexer is not exposed. It isn't needed:
// a `ReadOnlySpan` views a Rust `&[T]`, so read the elements from the Rust slice you already hold. The
// span's job here is to *hand* that immutable memory to a .NET API (via `handle()`), zero-copy.

/// A managed `System.Span<T>` viewing a Rust `&mut [T]` in place — mutations by either side are seen
/// by the other (zero-copy). Valid only for the borrow `'a` of the underlying slice.
///
/// `Span<T>` is a .NET `ref struct`, which cannot be *stored* as a field (only held on the stack), so
/// this wrapper keeps the plain `(ptr, len)` and materialises the managed `Span<T>` transiently for
/// each operation (and via [`handle`](Self::handle) when passing it to a .NET API).
pub struct Span<'a, T> {
    ptr: *mut T,
    len: i32,
    _pd: PhantomData<&'a mut [T]>,
}

impl<'a, T> Span<'a, T> {
    /// View `slice` as a managed `Span<T>` in place. The span borrows the slice, so it cannot outlive it.
    pub fn from_slice(slice: &'a mut [T]) -> Self {
        let len = i32::try_from(slice.len()).expect("span length exceeds i32");
        Self {
            ptr: slice.as_mut_ptr(),
            len,
            _pd: PhantomData,
        }
    }
    /// Materialise the managed `Span<T>` (a stack-only `ref struct`) for a single operation.
    #[inline]
    pub fn handle(&self) -> RawSpan<T> {
        span_from_ptr::<T>(self.ptr.cast(), self.len)
    }
    /// Element count.
    pub fn len(&self) -> i32 {
        self.len
    }
    /// `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Set every element to `value` (`Span<T>.Fill`).
    pub fn fill(&mut self, value: T) {
        span_fill::<T>(&self.handle(), value)
    }
    /// Zero every element (`Span<T>.Clear`).
    pub fn clear(&mut self) {
        span_clear::<T>(&self.handle())
    }
    /// The element at `i`, or `None` if out of range (read through the byref indexer).
    pub fn get(&self, i: i32) -> Option<T>
    where
        T: Copy,
    {
        if i >= 0 && i < self.len {
            Some(unsafe { *span_get_ref::<T>(&self.handle(), i) })
        } else {
            None
        }
    }
    /// Overwrite the element at `i`; `false` (no write) if out of range.
    pub fn set(&mut self, i: i32, value: T) -> bool {
        if i >= 0 && i < self.len {
            unsafe { *span_get_ref::<T>(&self.handle(), i) = value };
            true
        } else {
            false
        }
    }
}

/// A managed `System.ReadOnlySpan<T>` viewing a Rust `&[T]` in place — for handing immutable Rust data
/// to a .NET API with no copy. Valid only for the borrow `'a`. Like [`Span`], the managed `ref struct`
/// is materialised per operation rather than stored.
pub struct ReadOnlySpan<'a, T> {
    ptr: *const T,
    len: i32,
    _pd: PhantomData<&'a [T]>,
}

impl<'a, T> ReadOnlySpan<'a, T> {
    /// View `slice` as a managed `ReadOnlySpan<T>` in place.
    pub fn from_slice(slice: &'a [T]) -> Self {
        let len = i32::try_from(slice.len()).expect("span length exceeds i32");
        Self {
            ptr: slice.as_ptr(),
            len,
            _pd: PhantomData,
        }
    }
    /// Materialise the managed `ReadOnlySpan<T>` for a single operation / to pass to a .NET API.
    #[inline]
    pub fn handle(&self) -> RawRoSpan<T> {
        rospan_from_ptr::<T>(self.ptr.cast(), self.len)
    }
    /// Element count.
    pub fn len(&self) -> i32 {
        self.len
    }
    /// `true` if empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
