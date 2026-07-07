//! Proof of `#[dotnet_interface]` — a Rust `trait` becomes a genuine C#-consumable .NET interface,
//! emitted on the DEFAULT `DIRECT_PE=1` path by the hand-rolled PE writer (Interface+Abstract
//! `TypeDef` with abstract `MethodDef` members). See `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md`
//! finding #2.
//!
//! Each trait method takes `&self` (the interface's implicit receiver) and has no body — it maps to
//! an abstract interface member. A C# consumer implements the interface (`class Parrot : ISpeaker`)
//! and uses it polymorphically; `typeof(ISpeaker).IsInterface` is true.

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::dotnet_interface;

#[dotnet_interface]
pub trait ISpeaker {
    /// C#: `void Speak();`
    fn Speak(&self);
    /// C#: `int Volume();`
    fn Volume(&self) -> i32;
}
