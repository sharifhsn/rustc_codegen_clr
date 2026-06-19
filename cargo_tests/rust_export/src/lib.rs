//! WF-7 — a Rust **library** compiled to a .NET class-library assembly.
//!
//! This crate has NO `main`/entrypoint: it is a `cdylib`, and the backend emits it as a referenceable
//! .NET assembly (named after the crate, `rust_export`) whose `#[no_mangle]` functions are `public
//! static` methods on `MainModule`. Because Rust compiles to managed CIL, C# calls them as ordinary
//! managed methods (see the companion `rust_export_cs`). No `main` means no `std` runtime init
//! (`lang_start`), so none of the std-runtime weak statics (gettid/posix_spawn/…) are pulled in — a
//! library is the clean shape for "C# imports a Rust module".
//!
//! `#[no_mangle]` gives each export a stable, un-mangled name AND (via the backend) marks it
//! `Access::Extern`, which makes it a dead-code-elimination ROOT — essential here, since a library has
//! no entrypoint to otherwise keep its API alive.

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
