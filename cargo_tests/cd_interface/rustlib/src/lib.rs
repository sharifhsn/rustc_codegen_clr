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
use mycorrhiza::system::MString;

#[dotnet_interface]
pub trait ISpeaker {
    /// C#: `void Speak();` — no-arg void member.
    fn Speak(&self);
    /// C#: `int Volume();` — value return, no args.
    fn Volume(&self) -> i32;
    /// C#: `int SetVolume(int level);` — a PARAMETERIZED member with a `&mut self` receiver,
    /// exercising the non-`self` params + return path (not just no-arg methods).
    fn SetVolume(&mut self, level: i32) -> i32;
    /// C#: `int Mix(int a, int b);` — MULTIPLE parameters.
    fn Mix(&self, a: i32, b: i32) -> i32;
    /// C#: `string Describe();` — a MANAGED (`System.String`) return type, not just primitives.
    fn Describe(&self) -> MString;
}
