//! Proof of `#[dotnet_interface]` **static abstract members** — a Rust trait fn with NO `self`
//! receiver becomes a .NET 7+ `static abstract` interface member (the `INumber<T>` generic-math
//! shape), emitted on the DEFAULT `DIRECT_PE=1` path by the hand-rolled PE writer with Roslyn's
//! exact `Public|Static|Virtual|HideBySig|Abstract` (0x4D6) MethodDef flags.
//!
//! A C# consumer implements the members as `public static …` and dispatches them generically via
//! `T.Member(…)` under a `where T : IParse` constraint. Instance members (with `&self`) mix
//! freely in the same trait, proving both member kinds coexist on one interface TypeDef.

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::dotnet_interface;

#[dotnet_interface]
pub trait IParse {
    /// C#: `static abstract int Make();` — no-arg static abstract.
    fn Make() -> i32;
    /// C#: `static abstract int Add(int a, int b);` — PARAMETERIZED static abstract.
    fn Add(a: i32, b: i32) -> i32;
    /// C#: `int Describe();` — an INSTANCE member alongside the statics, proving both kinds
    /// coexist on the same interface.
    fn Describe(&self) -> i32;
}
