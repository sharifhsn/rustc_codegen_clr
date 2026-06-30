//! WF-9 Stage 2 — the type-erased Rust core behind a C# generic `RustVec<T>`.
//!
//! C# cannot instantiate a *Rust* generic with a brand-new C# type (CLI §9.5 forbids explicit
//! layout on generics, and Rust structs use explicit layout). The bridge (docs/TRANSLATION_STATUS
//! §7) is a normal C# generic wrapper — legal, a thin handle-holder, no explicit layout — over ONE
//! Rust monomorphization keyed by the element *size*, operating on raw bytes via `memcpy` (exactly
//! like C's `void* + size_t`). That one monomorphization serves `RustVec<int>`, `RustVec<MyStruct>`,
//! … for any `T: unmanaged`, near-zero-cost and layout-preserving.
//!
//! This crate is that core: a size-erased growable vector exposed as `#[no_mangle] pub extern "C"`
//! functions (`public static` methods on `MainModule` after `cargo dotnet build`). The handle is an
//! opaque `usize` (a boxed pointer) so nothing but primitives + thin `*const u8`/`*mut u8` pointers
//! cross the boundary — all already-proven marshalling (cf. `cd_interop`'s `(ptr, len)` strings).
//!
//! No `main`, no I/O, panic-free (out-of-range is a `bool`, never a panic) → DCE-clean cdylib.
#![allow(clippy::missing_safety_doc)]

/// A growable, element-size-erased byte buffer. `len` counts *elements*; `buf` holds
/// `len * elem_size` bytes laid out contiguously.
struct RustVec {
    elem_size: usize,
    len: usize,
    buf: Vec<u8>,
}

/// `new RustVec<T>()` — create an empty vector whose element is `elem_size` bytes wide.
/// Returns an opaque handle (a boxed pointer as `usize`); `0` is never a valid handle.
#[no_mangle]
pub extern "C" fn rcl_vec_new(elem_size: usize) -> usize {
    let v = Box::new(RustVec {
        elem_size,
        len: 0,
        buf: Vec::new(),
    });
    Box::into_raw(v) as usize
}

/// `vec.Push(value)` — append `elem_size` bytes read from `elem` (a pointer to the caller's value).
#[no_mangle]
pub unsafe extern "C" fn rcl_vec_push(handle: usize, elem: *const u8) {
    let v = &mut *(handle as *mut RustVec);
    let es = v.elem_size;
    let start = v.buf.len();
    v.buf.resize(start + es, 0);
    core::ptr::copy_nonoverlapping(elem, v.buf.as_mut_ptr().add(start), es);
    v.len += 1;
}

/// `vec[idx]` (read) — copy element `idx`'s `elem_size` bytes into `out`. Returns `false` (writing
/// nothing) if `idx` is out of range, so the C# side can raise its own `IndexOutOfRangeException`.
#[no_mangle]
pub unsafe extern "C" fn rcl_vec_get(handle: usize, idx: usize, out: *mut u8) -> bool {
    let v = &*(handle as *const RustVec);
    if idx >= v.len {
        return false;
    }
    let es = v.elem_size;
    core::ptr::copy_nonoverlapping(v.buf.as_ptr().add(idx * es), out, es);
    true
}

/// `vec[idx] = value` (write) — overwrite element `idx`'s bytes from `elem`. Returns `false` if out
/// of range (no write).
#[no_mangle]
pub unsafe extern "C" fn rcl_vec_set(handle: usize, idx: usize, elem: *const u8) -> bool {
    let v = &mut *(handle as *mut RustVec);
    if idx >= v.len {
        return false;
    }
    let es = v.elem_size;
    core::ptr::copy_nonoverlapping(elem, v.buf.as_mut_ptr().add(idx * es), es);
    true
}

/// `vec.Count`.
#[no_mangle]
pub unsafe extern "C" fn rcl_vec_len(handle: usize) -> usize {
    (*(handle as *const RustVec)).len
}

/// Sum of every element interpreted as a little-endian `i32` (a tiny "Rust does real work over the
/// elements" check, so the demo isn't only marshalling). Only meaningful for `RustVec<int>`.
#[no_mangle]
pub unsafe extern "C" fn rcl_vec_sum_i32(handle: usize) -> i64 {
    let v = &*(handle as *const RustVec);
    let mut sum: i64 = 0;
    for i in 0..v.len {
        let mut bytes = [0u8; 4];
        core::ptr::copy_nonoverlapping(v.buf.as_ptr().add(i * v.elem_size), bytes.as_mut_ptr(), 4);
        sum += i32::from_le_bytes(bytes) as i64;
    }
    sum
}

/// `vec.Dispose()` — free the backing allocation. The handle is invalid afterwards.
#[no_mangle]
pub unsafe extern "C" fn rcl_vec_free(handle: usize) {
    drop(Box::from_raw(handle as *mut RustVec));
}
