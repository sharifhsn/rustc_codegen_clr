//! Reusable C#→Rust generic container — the Rust half of the WF-9 Stage-2 bridge, shipped so you
//! don't hand-write it per project.
//!
//! [`export_rust_containers!`] emits a size-erased byte vector as `#[unsafe(no_mangle)] extern "C"` functions
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
//!
//! Two sibling macros follow the same recipe:
//! - [`export_rust_hashmap!`] emits the size-erased `rcl_map_*` core behind
//!   `RustDotnet.RustHashMap<K, V>` (both `unmanaged`, hashed by their raw key bytes).
//! - [`export_rust_string!`] emits the `rcl_str_*` core behind `RustDotnet.RustString` — a mutable,
//!   Rust-owned UTF-8 buffer that marshals to/from a managed `System.String`.

/// Emit the size-erased container core (`rcl_vec_new`/`push`/`get`/`set`/`len`/`free`) as
/// `#[unsafe(no_mangle)] pub extern "C"` functions in the invoking crate. Call it **once**, at the crate
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
        #[unsafe(no_mangle)]
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
        #[unsafe(no_mangle)]
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
        #[unsafe(no_mangle)]
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
        #[unsafe(no_mangle)]
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
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_vec_len(handle: usize) -> usize {
            (*(handle as *const RclRustVec)).len
        }

        /// `Dispose()` — free the backing allocation; the handle is invalid afterwards.
        ///
        /// # Safety
        /// `handle` must be a live handle from `rcl_vec_new`, freed at most once.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_vec_free(handle: usize) {
            drop(::std::boxed::Box::from_raw(handle as *mut RclRustVec));
        }
    };
}

/// Emit the size-erased hash-map core (`rcl_map_new`/`insert`/`get`/`remove`/`contains`/`len`/`free`)
/// as `#[unsafe(no_mangle)] pub extern "C"` functions in the invoking crate. Backs the shipped
/// `RustDotnet.RustHashMap<K, V>` C# wrapper (both `K` and `V` are `unmanaged`, stored as their raw
/// bytes). Call it **once**, at the crate root of your `cdylib` — same rule as
/// [`export_rust_containers!`].
///
/// Keys are hashed and compared by their raw `key_size` bytes, so a C# `K : unmanaged` maps directly
/// (a `RustDotnet.RustHashMap<int, long>` uses the 4 key bytes / 8 value bytes as the identity).
#[macro_export]
macro_rules! export_rust_hashmap {
    () => {
        /// A size-erased hash map: raw `key_size` key bytes -> raw `val_size` value bytes. Backs
        /// `RustDotnet.RustHashMap<K, V>` (which memcpy's the raw `K`/`V` bytes across the seam).
        struct RclRustMap {
            key_size: usize,
            val_size: usize,
            map: ::std::collections::HashMap<::std::vec::Vec<u8>, ::std::vec::Vec<u8>>,
        }

        /// `new RustHashMap<K, V>()` — an empty map whose key is `key_size` bytes and value is
        /// `val_size` bytes. Returns an opaque handle (`0` is never valid).
        #[unsafe(no_mangle)]
        pub extern "C" fn rcl_map_new(key_size: usize, val_size: usize) -> usize {
            ::std::boxed::Box::into_raw(::std::boxed::Box::new(RclRustMap {
                key_size,
                val_size,
                map: ::std::collections::HashMap::new(),
            })) as usize
        }

        /// `map[key] = value` — insert or overwrite; returns `true` if a previous value was replaced.
        ///
        /// # Safety
        /// `handle` must be live; `key`/`val` must point to `key_size`/`val_size` bytes respectively.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_map_insert(
            handle: usize,
            key: *const u8,
            val: *const u8,
        ) -> bool {
            let m = &mut *(handle as *mut RclRustMap);
            let k = ::std::slice::from_raw_parts(key, m.key_size).to_vec();
            let v = ::std::slice::from_raw_parts(val, m.val_size).to_vec();
            m.map.insert(k, v).is_some()
        }

        /// `map.TryGetValue(key, out value)` — copy the value bytes into `out`; returns `false`
        /// (writing nothing) if the key is absent.
        ///
        /// # Safety
        /// `handle` must be live; `key` must point to `key_size` bytes; `out` to `val_size` writable
        /// bytes.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_map_get(handle: usize, key: *const u8, out: *mut u8) -> bool {
            let m = &*(handle as *const RclRustMap);
            let k = ::std::slice::from_raw_parts(key, m.key_size);
            match m.map.get(k) {
                Some(v) => {
                    ::core::ptr::copy_nonoverlapping(v.as_ptr(), out, m.val_size);
                    true
                }
                None => false,
            }
        }

        /// `map.ContainsKey(key)`.
        ///
        /// # Safety
        /// `handle` must be live; `key` must point to `key_size` bytes.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_map_contains(handle: usize, key: *const u8) -> bool {
            let m = &*(handle as *const RclRustMap);
            let k = ::std::slice::from_raw_parts(key, m.key_size);
            m.map.contains_key(k)
        }

        /// `map.Remove(key)` — returns `true` if a value was removed.
        ///
        /// # Safety
        /// `handle` must be live; `key` must point to `key_size` bytes.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_map_remove(handle: usize, key: *const u8) -> bool {
            let m = &mut *(handle as *mut RclRustMap);
            let k = ::std::slice::from_raw_parts(key, m.key_size);
            m.map.remove(k).is_some()
        }

        /// `Count`.
        ///
        /// # Safety
        /// `handle` must be a live handle from `rcl_map_new`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_map_len(handle: usize) -> usize {
            (*(handle as *const RclRustMap)).map.len()
        }

        /// `Dispose()` — free the backing allocation; the handle is invalid afterwards.
        ///
        /// # Safety
        /// `handle` must be a live handle from `rcl_map_new`, freed at most once.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_map_free(handle: usize) {
            drop(::std::boxed::Box::from_raw(handle as *mut RclRustMap));
        }
    };
}

/// Emit the Rust-owned UTF-8 string core (`rcl_str_new`/`push_bytes`/`len`/`copy_to`/`free`) as
/// `#[unsafe(no_mangle)] pub extern "C"` functions in the invoking crate. Backs the shipped
/// `RustDotnet.RustString` C# wrapper (a mutable, Rust-owned UTF-8 buffer that marshals to/from a
/// managed `System.String` as UTF-8). Call it **once**, at the crate root of your `cdylib`.
///
/// The buffer holds raw UTF-8 bytes; the C# side encodes/decodes with `System.Text.Encoding.UTF8`,
/// so `len` is a **byte** count (not a char/UTF-16 count).
#[macro_export]
macro_rules! export_rust_string {
    () => {
        /// A growable, Rust-owned UTF-8 byte buffer. Backs `RustDotnet.RustString`.
        struct RclRustString {
            buf: ::std::vec::Vec<u8>,
        }

        /// `new RustString()` — an empty string. Returns an opaque handle (`0` is never valid).
        #[unsafe(no_mangle)]
        pub extern "C" fn rcl_str_new() -> usize {
            ::std::boxed::Box::into_raw(::std::boxed::Box::new(RclRustString {
                buf: ::std::vec::Vec::new(),
            })) as usize
        }

        /// `Append(text)` — append `len` UTF-8 bytes read from `bytes`.
        ///
        /// # Safety
        /// `handle` must be live; `bytes` must point to `len` readable bytes of valid UTF-8.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_str_push_bytes(handle: usize, bytes: *const u8, len: usize) {
            let s = &mut *(handle as *mut RclRustString);
            let src = ::std::slice::from_raw_parts(bytes, len);
            s.buf.extend_from_slice(src);
        }

        /// The length in **UTF-8 bytes**.
        ///
        /// # Safety
        /// `handle` must be a live handle from `rcl_str_new`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_str_len(handle: usize) -> usize {
            (*(handle as *const RclRustString)).buf.len()
        }

        /// Copy the whole UTF-8 buffer into `out` (which must hold at least `rcl_str_len` bytes).
        /// The C# side calls `rcl_str_len` first to size its scratch buffer, then decodes UTF-8.
        ///
        /// # Safety
        /// `handle` must be live; `out` must point to at least `rcl_str_len(handle)` writable bytes.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_str_copy_to(handle: usize, out: *mut u8) {
            let s = &*(handle as *const RclRustString);
            ::core::ptr::copy_nonoverlapping(s.buf.as_ptr(), out, s.buf.len());
        }

        /// `Clear()` — truncate to empty (keeps the allocation).
        ///
        /// # Safety
        /// `handle` must be a live handle from `rcl_str_new`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_str_clear(handle: usize) {
            (*(handle as *mut RclRustString)).buf.clear();
        }

        /// `Dispose()` — free the backing allocation; the handle is invalid afterwards.
        ///
        /// # Safety
        /// `handle` must be a live handle from `rcl_str_new`, freed at most once.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn rcl_str_free(handle: usize) {
            drop(::std::boxed::Box::from_raw(handle as *mut RclRustString));
        }
    };
}
