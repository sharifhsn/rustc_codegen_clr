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

const CORELIB: &str = "System.Private.CoreLib";

/// Raw managed `System.Memory<T>` value. Copying it shares the same backing array, matching .NET
/// value semantics.
pub type MemoryHandle<T> = RustcCLRInteropManagedGenericStruct<CORELIB, "System.Memory", 16, (T,)>;
/// Raw managed `System.ReadOnlyMemory<T>` value.
pub type ReadOnlyMemoryHandle<T> =
    RustcCLRInteropManagedGenericStruct<CORELIB, "System.ReadOnlyMemory", 16, (T,)>;

type ManagedArray<T> = RustcCLRInteropManagedArray<T, 1>;
type GenericArray = RustcCLRInteropManagedArray<RustcCLRInteropTypeGeneric<0>, 1>;

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
#[derive(Clone, Copy)]
pub struct Memory<T> {
    raw: MemoryHandle<T>,
}

impl<T: Copy + ManagedSafe> Memory<T> {
    /// Copy a Rust slice into a managed array and wrap it as `System.Memory<T>`.
    pub fn from_slice(slice: &[T]) -> Self {
        Self {
            raw: memory_from_array(copy_to_managed_array(slice)),
        }
    }

    /// The raw managed value for passing to a .NET API that accepts `Memory<T>`.
    pub fn handle(&self) -> MemoryHandle<T> {
        self.raw
    }

    /// Element count, read from the real managed `Memory<T>` value.
    pub fn len(&self) -> i32 {
        memory_len(&self.raw)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cheap view over the same managed array. Bounds are checked by `Memory<T>.Slice`.
    pub fn slice(&self, start: i32, len: i32) -> Self {
        Self {
            raw: memory_slice(&self.raw, start, len),
        }
    }

    /// Read one element through the managed memory's `Span<T>` view.
    pub fn get(&self, index: i32) -> Option<T> {
        if index < 0 || index >= self.len() {
            return None;
        }
        let span = memory_span(&self.raw);
        Some(unsafe { *span_get_ref(&span, index) })
    }

    /// Write one element through the managed memory's `Span<T>` view.
    pub fn set(&mut self, index: i32, value: T) -> bool {
        if index < 0 || index >= self.len() {
            return false;
        }
        let span = memory_span(&self.raw);
        unsafe { *span_get_ref(&span, index) = value };
        true
    }

    /// Fill the view through the real `Span<T>.Fill` implementation.
    pub fn fill(&mut self, value: T) {
        span_fill(&memory_span(&self.raw), value)
    }

    /// Copy the current managed contents back into an ordinary Rust vector.
    pub fn to_vec(&self) -> Vec<T> {
        (0..self.len()).map(|idx| self.get(idx).unwrap()).collect()
    }
}

/// An immutable, GC-owned managed buffer. Construction copies the source into a new `T[]`.
#[derive(Clone, Copy)]
pub struct ReadOnlyMemory<T> {
    raw: ReadOnlyMemoryHandle<T>,
}

impl<T: Copy + ManagedSafe> ReadOnlyMemory<T> {
    /// Copy a Rust slice into a managed array and wrap it as `System.ReadOnlyMemory<T>`.
    pub fn from_slice(slice: &[T]) -> Self {
        Self {
            raw: readonly_memory_from_array(copy_to_managed_array(slice)),
        }
    }

    pub fn handle(&self) -> ReadOnlyMemoryHandle<T> {
        self.raw
    }

    pub fn len(&self) -> i32 {
        readonly_memory_len(&self.raw)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cheap read-only view over the same managed array, bounds-checked by .NET.
    pub fn slice(&self, start: i32, len: i32) -> Self {
        Self {
            raw: readonly_memory_slice(&self.raw, start, len),
        }
    }

    /// Copy into an existing mutable managed buffer via `ReadOnlyMemory<T>.CopyTo`.
    pub fn copy_to(&self, destination: &mut Memory<T>) {
        readonly_copy_to(&self.raw, destination.handle())
    }

    /// Materialise the real `ReadOnlySpan<T>` view for passing to a synchronous .NET API.
    pub fn span_handle(&self) -> RawRoSpan<T> {
        readonly_memory_span(&self.raw)
    }
}
