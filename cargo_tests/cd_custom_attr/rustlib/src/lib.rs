//! Proves the general `#[dotnet_class(attr(...))]` custom-attribute surface: a Rust-defined
//! managed class carrying real ECMA-335 `CustomAttribute` rows a C# consumer can read back via
//! ordinary reflection (`Type.GetCustomAttributes()`).
//!
//! All three attribute arg shapes the backend supports are exercised, each on `System.Obsolete`
//! (a real BCL attribute in `System.Runtime` — no extra NuGet dependency needed to prove the
//! general mechanism works against a genuine external attribute type):
//!   * `NoArgClass`: a bare `[Obsolete]` — the no-arg ctor shape.
//!   * `MessageClass`: `[Obsolete("...")]` — a single positional `string` ctor arg.
//!   * `FullClass`: `[Obsolete("...", true)]` plus TWO named PROPERTY args
//!     (`DiagnosticId`/`UrlFormat`, both real settable `ObsoleteAttribute` properties on modern
//!     .NET) — positional `string`+`bool` args together with named args in one attribute.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use dotnet_macros::dotnet_class;

/// A bare `[Obsolete]` — no constructor arguments.
#[dotnet_class(attr("[System.Runtime]System.ObsoleteAttribute"))]
pub struct NoArgClass {
    value: i32,
}

/// `[Obsolete("This type is deprecated; use FullClass instead")]` — one positional `string` ctor
/// argument.
#[dotnet_class(attr(
    "[System.Runtime]System.ObsoleteAttribute",
    args("This type is deprecated; use FullClass instead")
))]
pub struct MessageClass {
    value: i32,
}

/// `[Obsolete("Use the new API", true)]` with `DiagnosticId`/`UrlFormat` set as named property
/// arguments — positional `string` + `bool` ctor args, plus two named `string` property args, all
/// on one attribute.
#[dotnet_class(attr(
    "[System.Runtime]System.ObsoleteAttribute",
    args("Use the new API", true),
    props(DiagnosticId = "RCC0001", UrlFormat = "https://example.invalid/diagnostics/{0}")
))]
pub struct FullClass {
    value: i32,
}

/// Two DIFFERENT attributes on the same type — proves multiple `attr(...)` entries accumulate
/// rather than overwrite each other.
#[dotnet_class(
    attr("[System.Runtime]System.ObsoleteAttribute"),
    attr("[System.Runtime]System.ObsoleteAttribute", args("second attribute on the same type"))
)]
pub struct MultiAttrClass {
    value: i32,
}
