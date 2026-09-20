//! Global allocator for the .NET ("dotnet") platform.
//!
//! Backed by `System.Runtime.InteropServices.NativeMemory` through two `extern`
//! hooks that the cilly linker maps to BCL calls:
//!
//! * `rcl_dotnet_alloc(size, align)` -> `NativeMemory.AlignedAlloc((nuint)size, (nuint)align)`
//! * `rcl_dotnet_free(ptr, size, align)` -> `NativeMemory.AlignedFree((void*)ptr)`
//!
//! `size` on the free hook is unused in the direct (`NativeMemory`) mapping —
//! `AlignedFree` only needs the pointer — but is REQUIRED by the optional
//! `POOL_ALLOC=1` pooled-allocator fast path (`cilly/src/ir/builtins/pool_alloc.rs`),
//! which needs to know which per-thread size-class free list to push a freed
//! block onto (a `GlobalAlloc::dealloc` call always carries the same `Layout`
//! — hence the same size+align — the block was allocated with, so this is a
//! sound, ABI-stable addition; both hooks are always emitted by the SAME
//! linker build as the PAL that declares them, so there is no versioning
//! hazard). Do not rename these symbols.
//!
//! `realloc` is implemented with the shared `realloc_fallback` (alloc + copy +
//! free) from [`super`], and `alloc_zeroed` allocates and then zeroes the buffer,
//! mirroring the canonical minimal non-unix PALs (see `sys/alloc/zkvm.rs`).
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::alloc::Layout;
use crate::ptr;

// Allocation hooks -> System.Runtime.InteropServices.NativeMemory.
//
// The names must match EXACTLY the symbols the cilly linker patches in. Do not
// rename these.
unsafe extern "C" {
    /// `NativeMemory.AlignedAlloc((nuint)size, (nuint)align)`.
    fn rcl_dotnet_alloc(size: usize, align: usize) -> *mut u8;
    /// `NativeMemory.AlignedFree((void*)ptr)`. `size` is unused in direct mode,
    /// consumed by the pooled-allocator fast path when `POOL_ALLOC=1` (see the
    /// module doc above).
    fn rcl_dotnet_free(ptr: *mut u8, size: usize, align: usize);
}

#[inline]
pub unsafe fn alloc(layout: Layout) -> *mut u8 {
    // SAFETY: callers uphold the allocator preconditions; the hook forwards directly to
    // `NativeMemory.AlignedAlloc` and returns a pointer with the requested alignment.
    unsafe { rcl_dotnet_alloc(layout.size(), layout.align()) }
}

#[inline]
pub unsafe fn alloc_zeroed(layout: Layout) -> *mut u8 {
    let size = layout.size();
    // SAFETY: same preconditions as `alloc`.
    let ptr = unsafe { alloc(layout) };
    if !ptr.is_null() {
        // SAFETY: `ptr` points to `size` freshly allocated, writable bytes.
        unsafe { ptr::write_bytes(ptr, 0, size) };
    }
    ptr
}

#[inline]
pub unsafe fn dealloc(ptr: *mut u8, layout: Layout) {
    // SAFETY: caller upholds the deallocation preconditions; direct `NativeMemory` only needs
    // the pointer, while the optional pool uses the original size+alignment for its free list.
    unsafe { rcl_dotnet_free(ptr, layout.size(), layout.align()) }
}

#[inline]
pub unsafe fn realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    // `NativeMemory` has no aligned-realloc, so use the shared alloc+copy+free fallback that
    // preserves the original alignment.
    // SAFETY: caller upholds the allocator reallocation preconditions.
    unsafe { super::realloc_fallback(ptr, layout, new_size) }
}
