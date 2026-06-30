//! Defining a managed .NET class from a real Rust `struct` via the `#[dotnet_class]` proc-macro.
//!
//! `#[dotnet_class]` (in `dotnet_macros`) turns the annotated struct into a managed class — it expands
//! to the same `rustc_codegen_clr_comptime_entrypoint` shape the older declarative `dotnet_typedef!`
//! produces, which the backend's comptime interpreter (`src/comptime.rs`) reads to register a real
//! `ClassDef`. Beyond `dotnet_typedef!`, it also emits a **parameterized primary constructor** that
//! initializes the fields, plus a public `read_<field>()` accessor per field — so C# can
//! `new Counter(5, 100)` and observe the result. No `#[no_mangle]` exports: the class, its ctor, and
//! its accessors are all synthesized by the backend.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use dotnet_macros::dotnet_class;

/// A Rust-defined .NET reference type `Counter : System.Object` with private fields `value: int32`
/// and `step: int64`, a primary ctor `Counter(int32, int64)`, and `read_value()` / `read_step()`.
#[dotnet_class]
pub struct Counter {
    value: i32,
    step: i64,
}
