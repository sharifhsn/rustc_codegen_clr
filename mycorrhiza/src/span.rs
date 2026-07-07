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
//!
//! `Slice`/`CopyTo`/`TryCopyTo` are wired the same way (def-shape nested-generic self-return/-argument,
//! the same proven pattern as `TaskCompletionSource<T>.get_Task`). `Span<T>.IndexOf`/`Contains` are
//! **not** wired to the real `MemoryExtensions` static generic methods: that shape is a static generic
//! method (on a non-generic class) taking a generic-struct-typed argument, constrained on
//! `T: IEquatable<T>` — a combination the current WF-9 generic-method bridge doesn't reach. `contains`/
//! `index_of` below are Rust-side scans instead (still correct, just not exercising that specific BCL
//! entry point).

use core::marker::PhantomData;

use crate::gen;
use crate::intrinsics::{
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2, rustc_clr_interop_generic_call3,
    rustc_clr_interop_generic_ctor2, RustcCLRInteropByRef, RustcCLRInteropManagedGenericStruct,
    RustcCLRInteropTypeGeneric,
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
// Span<T>.Slice(int start, int length) -> Span<T> : def-shape nested-generic self-return (`Span<!0>`)
// binds against the concrete `RawSpan<T>` local — same proven pattern as `TaskCompletionSource<T>.get_Task`.
fn span_slice<T>(s: &RawSpan<T>, start: i32, len: i32) -> RawSpan<T> {
    rustc_clr_interop_generic_call3::<
        CORELIB, "System.Span", true, "Slice", 1, (T,),
        (RawSpan<RustcCLRInteropTypeGeneric<0>>, i32, i32),
        RawSpan<T>, &RawSpan<T>, i32, i32,
    >(s, start, len)
}
// Span<T>.CopyTo(Span<T> destination) -> void : the destination is passed by value (a `ref struct` is
// always a stack value), Sig slot is the def-shape `Span<!0>` (same nested-generic-arg shape as the
// `Slice` return, just in parameter position).
fn span_copy_to<T>(src: &RawSpan<T>, dst: RawSpan<T>) {
    rustc_clr_interop_generic_call2::<
        CORELIB, "System.Span", true, "CopyTo", 1, (T,),
        ((), RawSpan<RustcCLRInteropTypeGeneric<0>>),
        (), &RawSpan<T>, RawSpan<T>,
    >(src, dst)
}
// Span<T>.TryCopyTo(Span<T> destination) -> bool : non-throwing CopyTo, `false` if destination too short.
fn span_try_copy_to<T>(src: &RawSpan<T>, dst: RawSpan<T>) -> bool {
    rustc_clr_interop_generic_call2::<
        CORELIB, "System.Span", true, "TryCopyTo", 1, (T,),
        (bool, RawSpan<RustcCLRInteropTypeGeneric<0>>),
        bool, &RawSpan<T>, RawSpan<T>,
    >(src, dst)
}

fn rospan_from_ptr<T>(ptr: *const (), len: i32) -> RawRoSpan<T> {
    rustc_clr_interop_generic_ctor2::<
        CORELIB, "System.ReadOnlySpan", true, (T,), ((), *const (), i32), RawRoSpan<T>, *const (), i32,
    >(ptr, len)
}
fn rospan_len<T>(s: &RawRoSpan<T>) -> i32 {
    rustc_clr_interop_generic_call1::<CORELIB, "System.ReadOnlySpan", true, "get_Length", 1, (T,), (i32,), i32, &RawRoSpan<T>>(s)
}
// ReadOnlySpan<T>.Slice(int start, int length) -> ReadOnlySpan<T> : same def-shape nested-generic
// self-return pattern as `Span<T>.Slice`.
fn rospan_slice<T>(s: &RawRoSpan<T>, start: i32, len: i32) -> RawRoSpan<T> {
    rustc_clr_interop_generic_call3::<
        CORELIB, "System.ReadOnlySpan", true, "Slice", 1, (T,),
        (RawRoSpan<RustcCLRInteropTypeGeneric<0>>, i32, i32),
        RawRoSpan<T>, &RawRoSpan<T>, i32, i32,
    >(s, start, len)
}
// ReadOnlySpan<T>.CopyTo(Span<T> destination) -> void : the destination is a *writable* `Span<T>`,
// distinct from the receiver's own `ReadOnlySpan<T>` — mirrors `Span<T>.CopyTo`.
fn rospan_copy_to<T>(src: &RawRoSpan<T>, dst: RawSpan<T>) {
    rustc_clr_interop_generic_call2::<
        CORELIB, "System.ReadOnlySpan", true, "CopyTo", 1, (T,),
        ((), RawSpan<RustcCLRInteropTypeGeneric<0>>),
        (), &RawRoSpan<T>, RawSpan<T>,
    >(src, dst)
}
// ReadOnlySpan<T>.TryCopyTo(Span<T> destination) -> bool.
fn rospan_try_copy_to<T>(src: &RawRoSpan<T>, dst: RawSpan<T>) -> bool {
    rustc_clr_interop_generic_call2::<
        CORELIB, "System.ReadOnlySpan", true, "TryCopyTo", 1, (T,),
        (bool, RawSpan<RustcCLRInteropTypeGeneric<0>>),
        bool, &RawRoSpan<T>, RawSpan<T>,
    >(src, dst)
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
    /// A sub-span `[start, start + len)` of this span, still viewing the same underlying Rust memory
    /// (`Span<T>.Slice(int, int)`, zero-copy — writes through the sub-span are visible in the original
    /// buffer and vice versa). Panics (via the .NET `Slice` bounds check surfacing as a managed
    /// exception) if the range is out of bounds.
    pub fn slice(&self, start: i32, len: i32) -> Span<'a, T> {
        let sliced = span_slice::<T>(&self.handle(), start, len);
        // Recover the Rust pointer from the *real* `Span<T>` the .NET call handed back (via the byref
        // indexer at element 0), rather than assuming `Slice`'s implementation details — this proves
        // the managed `Slice` call actually produced a span over the expected memory. Only valid when
        // `len > 0`; an empty slice has no element to index, so fall back to plain pointer arithmetic
        // (still correct — `Slice` cannot relocate a zero-copy span either way).
        let ptr = if len > 0 {
            span_get_ref::<T>(&sliced, 0)
        } else {
            unsafe { self.ptr.add(start as usize) }
        };
        Span {
            ptr,
            len,
            _pd: PhantomData,
        }
    }
    /// Copy every element of `self` into `dest` (`Span<T>.CopyTo`) — `dest` must be at least as long,
    /// or this panics (a managed `ArgumentException`, same as real `.NET` `CopyTo`).
    pub fn copy_to(&self, dest: &mut Span<'_, T>) {
        span_copy_to::<T>(&self.handle(), dest.handle())
    }
    /// Like [`copy_to`](Self::copy_to), but returns `false` instead of panicking if `dest` is too short
    /// (`Span<T>.TryCopyTo`).
    pub fn try_copy_to(&self, dest: &mut Span<'_, T>) -> bool {
        span_try_copy_to::<T>(&self.handle(), dest.handle())
    }
    /// Copy every element of `src` into `self` (`ReadOnlySpan<T>.CopyTo`, called with `src` wrapped as
    /// a transient `ReadOnlySpan<T>`). Panics if `self` is shorter than `src`.
    pub fn copy_from_slice(&mut self, src: &[T]) {
        let ro = ReadOnlySpan::from_slice(src);
        rospan_copy_to::<T>(&ro.handle(), self.handle())
    }
    /// View the span's current memory as a plain Rust slice — a direct read of the same bytes the
    /// managed `Span<T>` views, no per-element .NET indexer call needed.
    pub fn as_slice(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len as usize) }
    }
    /// Mutable view of the span's memory as a plain Rust slice.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len as usize) }
    }
    /// Returns `true` if any element of the span equals `value` — a plain Rust-side linear scan (see
    /// the module docs for why this isn't `MemoryExtensions.Contains`).
    pub fn contains(&self, value: T) -> bool
    where
        T: PartialEq,
    {
        self.as_slice().contains(&value)
    }
    /// The index of the first element equal to `value`, or `None`. See [`contains`](Self::contains)
    /// for why this is a Rust-side scan rather than `MemoryExtensions.IndexOf`.
    pub fn index_of(&self, value: T) -> Option<i32>
    where
        T: PartialEq,
    {
        self.as_slice()
            .iter()
            .position(|e| *e == value)
            .map(|i| i as i32)
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
    /// A sub-span `[start, start + len)` of this span, viewing the same underlying Rust memory
    /// (`ReadOnlySpan<T>.Slice(int, int)`, zero-copy). Panics via the managed bounds check if the
    /// range is out of bounds.
    pub fn slice(&self, start: i32, len: i32) -> ReadOnlySpan<'a, T> {
        // `ReadOnlySpan<T>.get_Item` isn't exposed (see the module-level note on its `ref readonly T`
        // return), so — unlike `Span::slice` — we can't round-trip through the .NET call's own
        // returned handle to recover a pointer; still exercise the real `Slice` call (for parity with
        // `Span::slice` and to prove it doesn't throw on a valid range), then compute the equivalent
        // Rust pointer directly, which is always correct since `Slice` is zero-copy.
        let sliced = rospan_slice::<T>(&self.handle(), start, len);
        let _ = rospan_len::<T>(&sliced); // exercises the returned handle, proving the call succeeded
        ReadOnlySpan {
            ptr: unsafe { self.ptr.add(start as usize) },
            len,
            _pd: PhantomData,
        }
    }
    /// Copy every element of this span into `dest` (`ReadOnlySpan<T>.CopyTo`) — `dest` must be at
    /// least as long, or this panics (a managed `ArgumentException`).
    pub fn copy_to(&self, dest: &mut Span<'_, T>) {
        rospan_copy_to::<T>(&self.handle(), dest.handle())
    }
    /// Like [`copy_to`](Self::copy_to), but returns `false` instead of panicking if `dest` is too short.
    pub fn try_copy_to(&self, dest: &mut Span<'_, T>) -> bool {
        rospan_try_copy_to::<T>(&self.handle(), dest.handle())
    }
    /// View the span's memory as a plain Rust slice — always available (a Rust `&[T]` already backs
    /// this wrapper, so reads don't need a per-element .NET call the way `Span<T>`'s indexer does).
    pub fn as_slice(&self) -> &'a [T] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len as usize) }
    }
    /// `true` if any element equals `value` (Rust-side scan over [`as_slice`](Self::as_slice); see
    /// [`Span::contains`] for why this isn't `MemoryExtensions.Contains`).
    pub fn contains(&self, value: T) -> bool
    where
        T: PartialEq,
    {
        self.as_slice().contains(&value)
    }
    /// The index of the first element equal to `value`, or `None`.
    pub fn index_of(&self, value: T) -> Option<i32>
    where
        T: PartialEq,
    {
        self.as_slice()
            .iter()
            .position(|e| *e == value)
            .map(|i| i as i32)
    }
}
