//! `System.Memory<T>` / `ReadOnlyMemory<T>` backed by a GC-owned managed array.
//!
//! [`crate::span::Span`] is the zero-copy choice for a synchronous call: it borrows Rust memory and
//! therefore cannot safely be retained by managed code or carried across an async boundary. These
//! wrappers make the opposite tradeoff. [`Memory::from_slice`](crate::memory::Memory::from_slice)
//! and [`ReadOnlyMemory::from_slice`](crate::memory::ReadOnlyMemory::from_slice) copy the input into
//! a managed `T[]`; the resulting value has no
//! Rust lifetime and may be stored by .NET code. Slices are cheap views over the same managed array.
//!
//! `T` must be a boundary-safe value type accepted by the managed-array intrinsics (the practical
//! surface today is primitives and plain value types). Managed-reference elements need the separate
//! reference-array path and are intentionally not claimed here.

use crate::ManagedSafe;
use crate::intrinsics::{
    RustcCLRInteropManagedArray, RustcCLRInteropManagedGenericStruct, RustcCLRInteropTypeGeneric,
    rustc_clr_interop_generic_call1, rustc_clr_interop_generic_call2,
    rustc_clr_interop_generic_call3, rustc_clr_interop_generic_ctor1,
    rustc_clr_interop_managed_new_arr, rustc_clr_interop_managed_set_elem,
};
use crate::span::{RawRoSpan, RawSpan, span_fill, span_get_ref};
use core::cell::Cell;
use core::marker::PhantomData;

const CORELIB: &str = "System.Private.CoreLib";

/// Raw managed `System.Memory<T>` value. Copying it shares the same backing array, matching .NET
/// value semantics.
pub type MemoryHandle<T> = RustcCLRInteropManagedGenericStruct<CORELIB, "System.Memory", 16, (T,)>;
/// Raw managed `System.ReadOnlyMemory<T>` value.
pub type ReadOnlyMemoryHandle<T> =
    RustcCLRInteropManagedGenericStruct<CORELIB, "System.ReadOnlyMemory", 16, (T,)>;

type ManagedArray<T> = RustcCLRInteropManagedArray<T, 1>;
type GenericArray = RustcCLRInteropManagedArray<RustcCLRInteropTypeGeneric<0>, 1>;

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

fn copy_to_managed_array<T: Copy + ManagedSafe>(slice: &[T]) -> ManagedArray<T> {
    let len = i32::try_from(slice.len()).expect("memory length exceeds i32");
    let array = rustc_clr_interop_managed_new_arr::<T>(len);
    for (idx, value) in slice.iter().copied().enumerate() {
        rustc_clr_interop_managed_set_elem(array, idx as i32, value);
    }
    array
}

fn memory_from_array<T>(array: ManagedArray<T>) -> MemoryHandle<T> {
    rustc_clr_interop_generic_ctor1::<
        CORELIB,
        "System.Memory",
        true,
        (T,),
        ((), GenericArray),
        MemoryHandle<T>,
        ManagedArray<T>,
    >(array)
}

fn readonly_memory_from_array<T>(array: ManagedArray<T>) -> ReadOnlyMemoryHandle<T> {
    rustc_clr_interop_generic_ctor1::<
        CORELIB,
        "System.ReadOnlyMemory",
        true,
        (T,),
        ((), GenericArray),
        ReadOnlyMemoryHandle<T>,
        ManagedArray<T>,
    >(array)
}

fn memory_len<T>(memory: &MemoryHandle<T>) -> i32 {
    rustc_clr_interop_generic_call1::<
        CORELIB,
        "System.Memory",
        true,
        "get_Length",
        1,
        (T,),
        (i32,),
        i32,
        &MemoryHandle<T>,
    >(memory)
}

fn memory_span<T>(memory: &MemoryHandle<T>) -> RawSpan<T> {
    rustc_clr_interop_generic_call1::<
        CORELIB,
        "System.Memory",
        true,
        "get_Span",
        1,
        (T,),
        (RawSpan<RustcCLRInteropTypeGeneric<0>>,),
        RawSpan<T>,
        &MemoryHandle<T>,
    >(memory)
}

fn memory_slice<T>(memory: &MemoryHandle<T>, start: i32, len: i32) -> MemoryHandle<T> {
    rustc_clr_interop_generic_call3::<
        CORELIB,
        "System.Memory",
        true,
        "Slice",
        1,
        (T,),
        (MemoryHandle<RustcCLRInteropTypeGeneric<0>>, i32, i32),
        MemoryHandle<T>,
        &MemoryHandle<T>,
        i32,
        i32,
    >(memory, start, len)
}

fn readonly_memory_len<T>(memory: &ReadOnlyMemoryHandle<T>) -> i32 {
    rustc_clr_interop_generic_call1::<
        CORELIB,
        "System.ReadOnlyMemory",
        true,
        "get_Length",
        1,
        (T,),
        (i32,),
        i32,
        &ReadOnlyMemoryHandle<T>,
    >(memory)
}

fn readonly_memory_span<T>(memory: &ReadOnlyMemoryHandle<T>) -> RawRoSpan<T> {
    rustc_clr_interop_generic_call1::<
        CORELIB,
        "System.ReadOnlyMemory",
        true,
        "get_Span",
        1,
        (T,),
        (RawRoSpan<RustcCLRInteropTypeGeneric<0>>,),
        RawRoSpan<T>,
        &ReadOnlyMemoryHandle<T>,
    >(memory)
}

fn readonly_memory_slice<T>(
    memory: &ReadOnlyMemoryHandle<T>,
    start: i32,
    len: i32,
) -> ReadOnlyMemoryHandle<T> {
    rustc_clr_interop_generic_call3::<
        CORELIB,
        "System.ReadOnlyMemory",
        true,
        "Slice",
        1,
        (T,),
        (
            ReadOnlyMemoryHandle<RustcCLRInteropTypeGeneric<0>>,
            i32,
            i32,
        ),
        ReadOnlyMemoryHandle<T>,
        &ReadOnlyMemoryHandle<T>,
        i32,
        i32,
    >(memory, start, len)
}

fn readonly_copy_to<T>(src: &ReadOnlyMemoryHandle<T>, dst: MemoryHandle<T>) {
    rustc_clr_interop_generic_call2::<
        CORELIB,
        "System.ReadOnlyMemory",
        true,
        "CopyTo",
        1,
        (T,),
        ((), MemoryHandle<RustcCLRInteropTypeGeneric<0>>),
        (),
        &ReadOnlyMemoryHandle<T>,
        MemoryHandle<T>,
    >(src, dst)
}

/// A mutable, GC-owned managed buffer. Construction copies the source into a new `T[]`.
///
/// The CLR value is boxed and rooted behind an opaque native token. Rust async state therefore
/// contains only a pointer, never the managed reference embedded inside `System.Memory<T>`.
pub struct Memory<T> {
    rooted: Cell<*mut u8>,
    _element: PhantomData<fn() -> T>,
}

impl<T> Memory<T> {
    /// Root a `System.Memory<T>` value received from managed code.
    #[inline]
    pub fn from_handle(raw: MemoryHandle<T>) -> Self {
        Self {
            rooted: Cell::new(rustc_clr_interop_managed_box_new(raw)),
            _element: PhantomData,
        }
    }

    /// Consume this owner and return the real CLR value for a managed call boundary.
    #[inline]
    pub fn into_handle(self) -> MemoryHandle<T> {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        let raw = unsafe { rustc_clr_interop_managed_box_take(rooted) };
        core::mem::forget(self);
        raw
    }

    #[inline(never)]
    fn take_handle(&self) -> MemoryHandle<T> {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        unsafe { rustc_clr_interop_managed_box_take(rooted) }
    }

    #[inline(never)]
    fn restore_handle(&self, raw: MemoryHandle<T>) {
        self.rooted.set(rustc_clr_interop_managed_box_new(raw));
    }
}

impl<T: Copy + ManagedSafe> Memory<T> {
    /// Copy a Rust slice into a managed array and wrap it as `System.Memory<T>`.
    pub fn from_slice(slice: &[T]) -> Self {
        Self::from_handle(memory_from_array(copy_to_managed_array(slice)))
    }

    /// Element count, read from the real managed `Memory<T>` value.
    pub fn len(&self) -> i32 {
        let raw = self.take_handle();
        let result = memory_len(&raw);
        self.restore_handle(raw);
        result
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cheap view over the same managed array. Bounds are checked by `Memory<T>.Slice`.
    pub fn slice(&self, start: i32, len: i32) -> Self {
        let raw = self.take_handle();
        let slice = memory_slice(&raw, start, len);
        self.restore_handle(raw);
        Self::from_handle(slice)
    }

    /// Read one element through the managed memory's `Span<T>` view.
    pub fn get(&self, index: i32) -> Option<T> {
        if index < 0 || index >= self.len() {
            return None;
        }
        let raw = self.take_handle();
        let span = memory_span(&raw);
        let result = Some(unsafe { *span_get_ref(&span, index) });
        self.restore_handle(raw);
        result
    }

    /// Write one element through the managed memory's `Span<T>` view.
    pub fn set(&mut self, index: i32, value: T) -> bool {
        if index < 0 || index >= self.len() {
            return false;
        }
        let raw = self.take_handle();
        let span = memory_span(&raw);
        unsafe { *span_get_ref(&span, index) = value };
        self.restore_handle(raw);
        true
    }

    /// Fill the view through the real `Span<T>.Fill` implementation.
    pub fn fill(&mut self, value: T) {
        let raw = self.take_handle();
        span_fill(&memory_span(&raw), value);
        self.restore_handle(raw);
    }

    /// Copy the current managed contents back into an ordinary Rust vector.
    pub fn to_vec(&self) -> Vec<T> {
        (0..self.len()).map(|idx| self.get(idx).unwrap()).collect()
    }
}

impl<T> Drop for Memory<T> {
    #[inline(never)]
    fn drop(&mut self) {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        if !rooted.is_null() {
            let _ = unsafe { rustc_clr_interop_managed_box_take::<MemoryHandle<T>>(rooted) };
        }
    }
}

/// An immutable, GC-owned managed buffer. Construction copies the source into a new `T[]`.
///
/// Like [`Memory`], the CLR value is rooted behind an opaque token and is safe to retain in a Rust
/// future across suspension points.
pub struct ReadOnlyMemory<T> {
    rooted: Cell<*mut u8>,
    _element: PhantomData<fn() -> T>,
}

impl<T> ReadOnlyMemory<T> {
    /// Root a `System.ReadOnlyMemory<T>` value received from managed code.
    #[inline]
    pub fn from_handle(raw: ReadOnlyMemoryHandle<T>) -> Self {
        Self {
            rooted: Cell::new(rustc_clr_interop_managed_box_new(raw)),
            _element: PhantomData,
        }
    }

    /// Consume this owner and return the real CLR value for a managed call boundary.
    #[inline]
    pub fn into_handle(self) -> ReadOnlyMemoryHandle<T> {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        let raw = unsafe { rustc_clr_interop_managed_box_take(rooted) };
        core::mem::forget(self);
        raw
    }

    #[inline(never)]
    fn take_handle(&self) -> ReadOnlyMemoryHandle<T> {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        unsafe { rustc_clr_interop_managed_box_take(rooted) }
    }

    #[inline(never)]
    fn restore_handle(&self, raw: ReadOnlyMemoryHandle<T>) {
        self.rooted.set(rustc_clr_interop_managed_box_new(raw));
    }
}

impl<T: Copy + ManagedSafe> ReadOnlyMemory<T> {
    /// Copy a Rust slice into a managed array and wrap it as `System.ReadOnlyMemory<T>`.
    pub fn from_slice(slice: &[T]) -> Self {
        Self::from_handle(readonly_memory_from_array(copy_to_managed_array(slice)))
    }

    pub fn len(&self) -> i32 {
        let raw = self.take_handle();
        let result = readonly_memory_len(&raw);
        self.restore_handle(raw);
        result
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cheap read-only view over the same managed array, bounds-checked by .NET.
    pub fn slice(&self, start: i32, len: i32) -> Self {
        let raw = self.take_handle();
        let slice = readonly_memory_slice(&raw, start, len);
        self.restore_handle(raw);
        Self::from_handle(slice)
    }

    /// Copy into an existing mutable managed buffer via `ReadOnlyMemory<T>.CopyTo`.
    pub fn copy_to(&self, destination: &mut Memory<T>) {
        let source = self.take_handle();
        let target = destination.take_handle();
        readonly_copy_to(&source, target);
        destination.restore_handle(target);
        self.restore_handle(source);
    }

    /// Materialise the real `ReadOnlySpan<T>` view for passing to a synchronous .NET API.
    pub fn span_handle(&self) -> RawRoSpan<T> {
        let raw = self.take_handle();
        let span = readonly_memory_span(&raw);
        self.restore_handle(raw);
        span
    }
}

impl<T> Drop for ReadOnlyMemory<T> {
    #[inline(never)]
    fn drop(&mut self) {
        let rooted = self.rooted.replace(core::ptr::null_mut());
        if !rooted.is_null() {
            let _ =
                unsafe { rustc_clr_interop_managed_box_take::<ReadOnlyMemoryHandle<T>>(rooted) };
        }
    }
}
