//! Exporting Rust functions to C# ergonomically, via the `#[dotnet_export]` proc-macro.
//!
//! Each `#[dotnet_export]` fn below is written as ordinary, idiomatic Rust — `&str`/`String`
//! parameters and returns, plain primitives. The macro leaves the function untouched (still callable
//! from Rust) and emits a hidden `#[unsafe(no_mangle)] extern "C"` shim that marshals the managed seam: the
//! string types cross as a real managed `System.String`, so the C# consumer calls
//! `MainModule.greet("World")` and gets back a `string` — with NO hand-written `(ptr, len)` buffer
//! dance (contrast `cargo_tests/cd_interop`, which spells that dance out by hand).
//!
//! No entrypoint: this is a `cdylib`. `#[unsafe(no_mangle)]` (emitted by the macro) roots each export against
//! dead-code elimination, so the library's API survives even without a `main`.

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::{
    dotnet_class, dotnet_dto, dotnet_export, dotnet_interface, dotnet_methods,
};
use mycorrhiza::managed_option::ManagedOption;
use mycorrhiza::system::MString;

/// Greets one caller through ordinary managed strings.
///
/// # Arguments
///
/// - `name`: Name to include in the greeting; `<` and `&` remain escaped in IntelliSense.
///
/// # Returns
///
/// A greeting produced by managed Rust.
#[dotnet_export(
    attr("[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("greet"), props(Stable = true, Order = 1)),
    return_attr("[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("greeting-result")),
    param_attr(name, "[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("greeting-name"))
)]
pub fn greet(name: &str) -> String {
    format!("Hello, {name}, from Rust!")
}

/// Round-trips an explicitly nullable exported managed string.
#[dotnet_export]
pub fn maybe_greet(name: ManagedOption<MString>) -> ManagedOption<MString> {
    name
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

/// `Result<T, E>` is resolved inside the generated shim: `Ok(T)` returns `T`, while `Err(E)`
/// becomes a managed exception carrying `E`'s `Display` text. The `Result` itself never crosses the
/// managed seam or becomes the payload of the shim's `catch_unwind`.
///
/// # Arguments
///
/// - `ok`: Whether the operation should produce an answer.
///
/// # Returns
///
/// The checked answer.
///
/// # Errors
///
/// Thrown when the checked answer is unavailable.
#[dotnet_export(error = "exception")]
pub fn checked_answer(ok: bool) -> Result<i32, String> {
    ok.then_some(42)
        .ok_or_else(|| "checked answer failed".to_string())
}

#[derive(Debug)]
pub struct NativeArgumentError {
    status: i32,
}

impl core::fmt::Display for NativeArgumentError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("native argument rejected")
    }
}

impl mycorrhiza::error::ManagedError for NativeArgumentError {
    fn exception_kind(&self) -> mycorrhiza::error::ManagedExceptionKind {
        mycorrhiza::error::ManagedExceptionKind::Argument
    }

    fn native_status(&self) -> Option<i32> {
        Some(self.status)
    }
}

/// Demonstrates a typed managed exception with structured native status diagnostics.
#[dotnet_export(error = "managed")]
pub fn typed_failure() -> Result<i32, NativeArgumentError> {
    Err(NativeArgumentError { status: 4_221 })
}

/// A documented managed quote used to prove type, constructor, property, and method XML IDs.
#[dotnet_dto]
pub struct RiskQuote {
    /// Quote value in whole basis points.
    pub value: i32,
    /// Whether the quote is active.
    pub active: bool,
}

/// A DTO proving that Rust's required/optional managed-reference wrappers become `string` and
/// `string?` across constructors and properties.
#[dotnet_dto]
pub struct NullableProfile {
    /// Required display name.
    #[dotnet_attr("[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("required-field"))]
    #[dotnet_property_attr("[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("required-property"), props(Stable = true))]
    pub required_name: String,
    /// Optional display name.
    pub optional_name: ManagedOption<MString>,
}

#[dotnet_class(
    constructor_visibility = "private",
    static_field(MarkerState: i32),
    static_field_attr(MarkerState, "[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("static-field"))
)]
pub struct DocumentationCalculator {}

#[dotnet_methods]
impl DocumentationCalculator {
    /// Projects a quote over a number of periods.
    ///
    /// # Arguments
    ///
    /// - `periods`: Number of periods to project.
    ///
    /// # Returns
    ///
    /// The projected whole-number value.
    #[dotnet(
        attr("[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("project")),
        return_attr("[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("project-result")),
        param_attr(periods, "[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("project-periods"))
    )]
    pub fn project(periods: i32) -> i32 {
        periods
    }

    /// Returns a required managed string.
    pub fn required_label(value: String) -> String {
        value
    }

    /// Round-trips an explicitly nullable managed string.
    pub fn optional_label(value: ManagedOption<MString>) -> ManagedOption<MString> {
        value
    }
}

/// A documented generic managed interface.
///
/// # Type Parameters
///
/// - `T`: Value stored by the interface.
#[dotnet_interface]
pub trait IDocumentedBox<T> {
    /// Stores one value.
    ///
    /// # Arguments
    ///
    /// - `value`: Value to store.
    fn put(&mut self, value: T);

    /// Echoes a method-generic value.
    ///
    /// # Arguments
    ///
    /// - `value`: Value to echo.
    ///
    /// # Type Parameters
    ///
    /// - `U`: Echoed value type.
    ///
    /// # Returns
    ///
    /// The supplied value.
    fn echo<U>(&self, value: U) -> U;

    /// Gets the number of stored values.
    #[dotnet_property(attr("[Mycorrhiza.Interop.Helpers]Mycorrhiza.Interop.Helpers.RustApiAttribute", args("count-property")))]
    fn get_Count(&self) -> i32;

    /// Accepts and returns a required managed string.
    fn required_label(&self, value: MString) -> MString;

    /// Accepts and returns an explicitly nullable managed string.
    fn optional_label(
        &self,
        value: ManagedOption<MString>,
    ) -> ManagedOption<MString>;

    /// Gets an explicitly nullable managed property.
    #[dotnet_property]
    fn get_OptionalName(&self) -> ManagedOption<MString>;
}

// ---- Case A: `Task`/`TaskT<T>` returns (docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md Tier C §6) ----------
//
// A plain, non-`async` fn that itself constructs and returns a `mycorrhiza::task::Task` /
// `TaskT<T>` passes straight through the seam as an ordinary managed handle — no new marshalling
// code, `async fn` sugar stays rejected (unrelated, larger follow-up).

/// `Task delayed_ping()` — a non-generic `Task` a C# caller can `await`; completes synchronously
/// (the work — none, here — is already done by the time the Task is constructed).
#[dotnet_export]
pub fn delayed_ping() -> mycorrhiza::task::Task {
    mycorrhiza::task::future_to_task_unit(async {})
}

/// `Task<int> compute_answer()` — a result-bearing `Task<T>`, produced via
/// `mycorrhiza::task::future_to_task`. C# `await`s it and gets back the `int`.
#[dotnet_export]
pub fn compute_answer() -> mycorrhiza::task::TaskT<i32> {
    mycorrhiza::task::future_to_task(async { 42 })
}

// ---- Case B: ordinary Vec<T> -> T[]; explicit RustOwnedVec<T> -> RustVec<T> ---------------------
mycorrhiza::export_rust_containers!();

/// Ordinary application collections become ordinary managed arrays.
#[dotnet_export]
pub fn range(start: i32, end: i32) -> Vec<i32> {
    (start..end).collect()
}

/// A second array element type proves the mapping is generic rather than hardcoded to `i32`.
#[dotnet_export]
pub fn squares(n: i32) -> Vec<i64> {
    (0..n as i64).map(|x| x * x).collect()
}

/// Explicit Rust ownership keeps the disposable, low-copy `RustVec<T>` contract available.
#[dotnet_export]
pub fn rust_owned_range(end: i32) -> mycorrhiza::containers::RustOwnedVec<i32> {
    (0..end).collect::<Vec<_>>().into()
}
