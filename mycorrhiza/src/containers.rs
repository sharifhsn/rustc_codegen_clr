//! Reusable C#→Rust generic container — the Rust half of the WF-9 Stage-2 bridge, shipped so you
//! don't hand-write it per project.
//!
//! [`export_rust_containers!`] emits a size-erased byte vector as `#[no_mangle] extern "C"` functions
//! (`rcl_vec_*`) into *your* `cdylib`. The C# half — `RustDotnet.RustVec<T>` (near-zero-cost, for
//! `T : unmanaged`) and `RustDotnet.RustBoxVec<T>` (a `GCHandle`-boxed list for **any** managed `T`) —
//! lives in `RustDotnet.Containers.cs` and is auto-included in a C# project that sets
//! `<UseRustDotnetContainers>true</UseRustDotnetContainers>` and imports `RustDotnet.targets`.
//!
//! Usage — in your Rust `cdylib` (`crate-type = ["cdylib"]`):
//!
//! ```ignore
//! mycorrhiza::export_rust_containers!();
//! ```
//!
//! and in the C# consumer:
//!
//! ```csharp
//! using RustDotnet;
//! var xs = RustVec<int>.New();
//! xs.Push(42);
//! int v = xs.Get(0);
//! xs.Dispose();
//! ```
//!
//! The functions are size-erased (they operate on `elem_size` raw bytes, like C's `void* + size_t`),
//! so one Rust monomorphization backs `RustVec<T>` for *every* `T` the C# side instantiates.

/// Emit the size-erased container core (`rcl_vec_new`/`push`/`get`/`set`/`len`/`free`) as
/// `#[no_mangle] pub extern "C"` functions in the invoking crate. Call it **once**, at the crate
/// root of your `cdylib` — the functions must be defined in *your* crate (not a dependency) so they
/// land in your assembly's `MainModule`, where the C# wrappers look for them.
#[macro_export]
macro_rules! export_rust_containers {
    () => {
        /// A growable, element-size-erased byte buffer: `len` counts *elements*; `buf` holds
        /// `len * elem_size` contiguous bytes. Backs `RustDotnet.RustVec<T>`/`RustBoxVec<T>` on the
        /// C# side (which stores raw `T` bytes, or `GCHandle` handles, respectively).
        struct RclRustVec {
            elem_size: usize,
            len: usize,
            buf: ::std::vec::Vec<u8>,
        }

        /// `new RustVec<T>()` — an empty vector whose element is `elem_size` bytes. Returns an opaque
        /// handle (a boxed pointer as `usize`); `0` is never a valid handle.
        #[no_mangle]
        pub extern "C" fn rcl_vec_new(elem_size: usize) -> usize {
            ::std::boxed::Box::into_raw(::std::boxed::Box::new(RclRustVec {
                elem_size,
                len: 0,
                buf: ::std::vec::Vec::new(),
            })) as usize
        }

        /// `Push(value)` — append `elem_size` bytes read from `elem`.
        ///
        /// # Safety
        /// `handle` must be a live handle from `rcl_vec_new`; `elem` must point to `elem_size` bytes.
        #[no_mangle]
        pub unsafe extern "C" fn rcl_vec_push(handle: usize, elem: *const u8) {
            let v = &mut *(handle as *mut RclRustVec);
            let es = v.elem_size;
            let start = v.buf.len();
            v.buf.resize(start + es, 0);
            ::core::ptr::copy_nonoverlapping(elem, v.buf.as_mut_ptr().add(start), es);
            v.len += 1;
        }

        /// `vec[idx]` (read) — copy element `idx`'s `elem_size` bytes into `out`; returns `false`
        /// (writing nothing) if `idx` is out of range.
        ///
        /// # Safety
        /// `handle` must be live; `out` must point to `elem_size` writable bytes.
        #[no_mangle]
        pub unsafe extern "C" fn rcl_vec_get(handle: usize, idx: usize, out: *mut u8) -> bool {
            let v = &*(handle as *const RclRustVec);
            if idx >= v.len {
                return false;
            }
            let es = v.elem_size;
            ::core::ptr::copy_nonoverlapping(v.buf.as_ptr().add(idx * es), out, es);
            true
        }

        /// `vec[idx] = value` (write) — overwrite element `idx`'s bytes from `elem`; `false` if out
        /// of range.
        ///
        /// # Safety
        /// `handle` must be live; `elem` must point to `elem_size` bytes.
        #[no_mangle]
        pub unsafe extern "C" fn rcl_vec_set(handle: usize, idx: usize, elem: *const u8) -> bool {
            let v = &mut *(handle as *mut RclRustVec);
            if idx >= v.len {
                return false;
            }
            let es = v.elem_size;
            ::core::ptr::copy_nonoverlapping(elem, v.buf.as_mut_ptr().add(idx * es), es);
            true
        }

        /// `Count`.
        ///
        /// # Safety
        /// `handle` must be a live handle from `rcl_vec_new`.
        #[no_mangle]
        pub unsafe extern "C" fn rcl_vec_len(handle: usize) -> usize {
            (*(handle as *const RclRustVec)).len
        }

        /// `Dispose()` — free the backing allocation; the handle is invalid afterwards.
        ///
        /// # Safety
        /// `handle` must be a live handle from `rcl_vec_new`, freed at most once.
        #[no_mangle]
        pub unsafe extern "C" fn rcl_vec_free(handle: usize) {
            drop(::std::boxed::Box::from_raw(handle as *mut RclRustVec));
        }
    };
}
