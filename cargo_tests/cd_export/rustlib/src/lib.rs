//! Exporting Rust functions to C# ergonomically, via the `#[dotnet_export]` proc-macro.
//!
//! Each `#[dotnet_export]` fn below is written as ordinary, idiomatic Rust — `&str`/`String`
//! parameters and returns, plain primitives. The macro leaves the function untouched (still callable
//! from Rust) and emits a hidden `#[no_mangle] extern "C"` shim that marshals the managed seam: the
//! string types cross as a real managed `System.String`, so the C# consumer calls
//! `MainModule.greet("World")` and gets back a `string` — with NO hand-written `(ptr, len)` buffer
//! dance (contrast `cargo_tests/cd_interop`, which spells that dance out by hand).
//!
//! No entrypoint: this is a `cdylib`. `#[no_mangle]` (emitted by the macro) roots each export against
//! dead-code elimination, so the library's API survives even without a `main`.

use dotnet_macros::dotnet_export;

/// `string greet(string)` — inbound `&str`, outbound `String`, both as managed `System.String`.
#[dotnet_export]
pub fn greet(name: &str) -> String {
    format!("Hello, {name}, from Rust!")
}

/// `int add(int, int)` — primitives pass through the seam unchanged.
#[dotnet_export]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// `long sum_u64(...)` mixed-width primitives, and a `bool` return.
#[dotnet_export]
pub fn is_even(n: i64) -> bool {
    n % 2 == 0
}

/// `double scale(double, int)` — mixed float/int primitives.
#[dotnet_export]
pub fn scale(x: f64, by: i32) -> f64 {
    x * (by as f64)
}

/// `string shout(string)` — `String` inbound (owned), `String` outbound; proves the owned-string
/// parameter arm (distinct from the `&str` borrow arm).
#[dotnet_export]
pub fn shout(mut s: String) -> String {
    s.make_ascii_uppercase();
    s.push('!');
    s
}

/// `int str_len(string)` — inbound `&str`, primitive return (counts UTF-8 bytes, proving the string
/// content actually crossed intact).
#[dotnet_export]
pub fn str_len(s: &str) -> i32 {
    s.len() as i32
}

/// `string version()` — no parameters, a `&'static str` return (outbound-only string marshalling).
#[dotnet_export]
pub fn version() -> &'static str {
    "cd_export 1.0"
}

/// `void note(string)` — a unit-returning export (no return marshalling). It has an observable
/// side effect only through the returned length in the paired `noted_len` below, so C# can assert it
/// ran; here we just prove a `-> ()` export links and is callable.
#[dotnet_export]
pub fn ping() {
    // Deliberately empty: proves a no-arg, unit-return export links and is callable from C#.
}

/// Deliberately panics with a runtime-computed message (not a literal — proves the payload text
/// actually survives the seam, not just a hardcoded string). Proves the panic-safety of the
/// `#[dotnet_export]`-generated shim: without a `catch_unwind` in the shim, this would unwind into
/// the `extern "C"` boundary and hard-abort the whole process (`Environment.FailFast`); with it, the
/// panic is caught inside the shim and re-raised as a genuine, catchable `System.Exception` carrying
/// the extracted message, so a C# `try`/`catch` around the call sees an ordinary managed exception
/// instead of the process dying.
#[dotnet_export]
pub fn boom(reason: &str) -> i32 {
    panic!("boom: {reason}");
    #[allow(unreachable_code)]
    0
}
