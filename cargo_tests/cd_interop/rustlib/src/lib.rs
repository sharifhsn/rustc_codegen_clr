//! J3 — a Rust **library** compiled to a C#-referenceable .NET assembly via the real `cargo dotnet`
//! flow (the dotnet PAL target, build-std with `panic_unwind`), then CALLED from a real C# project.
//!
//! This is `rust_export`'s Tier-1 surface (the categories that need only `core`/`alloc` + arithmetic +
//! the backend-synthesized struct accessors — NO `mycorrhiza`, NO panic, NO I/O), brought onto the
//! REAL dotnet PAL instead of the surrogate target. Tier-2 (managed `System.String` return, a
//! Rust-raises-a-.NET-exception `Result`) pulls `mycorrhiza` and is deliberately omitted here.
//!
//! This crate has NO `main`/entrypoint: it is a `cdylib`. `#[no_mangle]` gives each export a stable,
//! un-mangled name AND (via the backend) marks it `Access::Extern`, which makes it a dead-code-
//! elimination ROOT — essential, since a library has no entrypoint to keep its API alive. No `main`
//! means the `std` runtime tail (`lang_start`) is unreachable and DCE'd, so an I/O-free, panic-free
//! library emits cleanly even where a bin would drag the runtime.
//!
//! Strings/slices cross the boundary as UTF-8 / element `(ptr, len)` pairs (thin pointers, directly
//! C#-usable); no Rust allocation crosses, so there is nothing to free across the boundary.

// ---- primitives (callable directly from C#) ----

/// Integer add. Proves primitive signatures: C# sees `int MainModule.rust_add(int, int)`.
#[no_mangle]
pub extern "C" fn rust_add(a: i32, b: i32) -> i32 {
    a + b
}

// ---- string marshalling (UTF-8 (ptr, len) convention) ----

/// Take a name as a UTF-8 buffer, build an **owned Rust `String`** (a heap allocation via `format!`
/// that never escapes the boundary), and write its UTF-8 bytes into the caller-provided output
/// buffer. Returns the number of bytes that *would* be written (the full length, so the caller can
/// detect truncation). Proves BOTH string directions: C# `string` → Rust `&str` (inbound) and Rust
/// `String` → C# `string` (outbound via the out-buffer).
///
/// # Safety
/// `name_ptr`/`out_ptr` must point to `name_len`/`out_cap` valid bytes for the duration of the call
/// (C# pins them with `fixed`).
#[no_mangle]
pub unsafe extern "C" fn greet(
    name_ptr: *const u8,
    name_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
) -> usize {
    let name = core::str::from_utf8(core::slice::from_raw_parts(name_ptr, name_len))
        .unwrap_or("<invalid utf8>");
    let greeting = format!("Hello, {name}, from Rust!");
    let bytes = greeting.as_bytes();
    let n = core::cmp::min(bytes.len(), out_cap);
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, n);
    bytes.len()
}

// ---- struct marshalling across the boundary ----
//
// A `#[repr(C)]` struct lowers to a CIL value-type. Thanks to de-mangling (`stable_adt_name`, which
// fires for a cdylib's local non-generic types), it is emitted under the clean, build-stable name
// `cd_interop.Point` (not `cd_interop[<hash>].Point`), so C# references it directly. The backend
// synthesizes a public `.ctor` + per-field `get_<field>` getters.

/// A simple value-type exported to .NET as `cd_interop.Point`.
#[repr(C)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Take a `Point` by value and return the sum of its fields (inbound struct marshalling).
#[no_mangle]
pub extern "C" fn point_sum(p: Point) -> i32 {
    p.x + p.y
}

/// Build and return a `Point` (outbound struct marshalling).
#[no_mangle]
pub extern "C" fn make_point(x: i32, y: i32) -> Point {
    Point { x, y }
}

// ---- collection marshalling — slices ----

/// Inbound slice (a "Vec sum" from the caller's perspective): C# `int[]` → Rust `&[i32]`, returns
/// the sum.
///
/// # Safety
/// `ptr` must point to `len` valid, initialized `i32`s for the duration of the call (C# pins them).
#[no_mangle]
pub unsafe extern "C" fn sum_slice(ptr: *const i32, len: usize) -> i32 {
    core::slice::from_raw_parts(ptr, len).iter().sum()
}
