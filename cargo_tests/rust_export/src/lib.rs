//! WF-7 — a Rust **library** compiled to a .NET class-library assembly.
//!
//! This crate has NO `main`/entrypoint: it is a `cdylib`, and the backend emits it as a referenceable
//! .NET assembly (named after the crate, `rust_export`) whose `#[no_mangle]` functions are `public
//! static` methods on `MainModule`. Because Rust compiles to managed CIL, C# calls them as ordinary
//! managed methods (see the companion `rust_export_cs`). No `main` means no `std` runtime init
//! (`lang_start`).
//!
//! `#[no_mangle]` gives each export a stable, un-mangled name AND (via the backend) marks it
//! `Access::Extern`, which makes it a dead-code-elimination ROOT — essential here, since a library has
//! no entrypoint to keep its API alive.
//!
//! P1 covered primitive signatures. P2 (the string functions below) covers **marshalling** via the
//! standard FFI convention: strings cross the boundary as UTF-8 `(ptr, len)` pairs (thin pointers,
//! directly C#-usable). `rust_strlen` proves the inbound direction (C# `string` → Rust `&str`); `greet`
//! proves the outbound direction by building an owned Rust `String` and copying its UTF-8 bytes into a
//! C#-provided buffer (no Rust allocation crosses the boundary → nothing to free across it). An
//! idiomatic `string Greet(string)` wrapper on the C# side is a thin layer over this.
//!
//! (Returning a managed `System.String` directly — the most idiomatic shape — is a follow-up: it
//! currently hits a codegen mismatch where the interop-call result is typed `void` in an exported
//! function. The out-buffer convention here is allocation-safe and codegen-clean.)

// ---- P1: primitives (callable directly from C#) ----

#[no_mangle]
pub extern "C" fn rust_add(a: i32, b: i32) -> i32 {
    a + b
}

#[no_mangle]
pub extern "C" fn rust_mul(a: i32, b: i32) -> i32 {
    a * b
}

#[no_mangle]
pub extern "C" fn rust_fib(n: i32) -> i32 {
    if n < 2 {
        n
    } else {
        rust_fib(n - 1) + rust_fib(n - 2)
    }
}

#[no_mangle]
pub extern "C" fn rust_add_f64(a: f64, b: f64) -> f64 {
    a + b
}

// ---- P2: string marshalling (UTF-8 (ptr, len) convention) ----

/// Reconstruct a `&str` from a C#-supplied UTF-8 buffer and return its `char` count. Proves the
/// **inbound** string direction (C# `string` → Rust `&str`).
///
/// # Safety
/// `ptr` must point to `len` valid, initialized bytes for the duration of the call (C# pins them).
#[no_mangle]
pub unsafe extern "C" fn rust_strlen(ptr: *const u8, len: usize) -> i32 {
    let s = core::str::from_utf8(core::slice::from_raw_parts(ptr, len)).unwrap_or("");
    s.chars().count() as i32
}

/// Take a name as a UTF-8 buffer, build an **owned Rust `String`**, and write its UTF-8 bytes into the
/// caller-provided output buffer. Returns the number of bytes written (the full length needed, so the
/// caller can detect truncation). Proves the **outbound** string direction (Rust `String` → C#
/// `string`) including a Rust-side heap allocation (`format!`) that never escapes the boundary.
///
/// # Safety
/// `name_ptr`/`out_ptr` must point to `name_len`/`out_cap` valid bytes for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn greet(
    name_ptr: *const u8,
    name_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
) -> usize {
    let name =
        core::str::from_utf8(core::slice::from_raw_parts(name_ptr, name_len)).unwrap_or("<invalid utf8>");
    let greeting = format!("Hello, {name}, from Rust!");
    let bytes = greeting.as_bytes();
    let n = core::cmp::min(bytes.len(), out_cap);
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, n);
    bytes.len()
}
