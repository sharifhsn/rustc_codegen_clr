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

/// `ref`/`out` parameters: a `&mut T` (thin, sized `T`) non-receiver parameter maps to a managed
/// byref (`ELEMENT_TYPE_BYREF` — C# `ref T`), and `#[dotnet_out]` additionally stamps
/// `ParamAttributes.Out` (0x0002) on its `Param` row so C# sees `out T`. The C# `Cell : IRefCell`
/// implementor compiling with `ref`/`out` keywords (instead of demanding unsafe `int*`) is the
/// first proof; writes through the byref observed by the caller are the runtime proof.
#[dotnet_interface]
pub trait IRefCell {
    /// C#: `void Fill(ref int slot);` — plain `&mut T` => `ref T` (byref, Param Flags == 0).
    fn Fill(&self, slot: &mut i32);
    /// C#: `void FillOut(out int slot);` — same byref, plus `ParamAttributes.Out` => `out T`.
    fn FillOut(&self, #[dotnet_out] slot: &mut i32);
    /// C#: `int AddInto(int a, ref int acc);` — by-value and byref params mix freely.
    fn AddInto(&self, a: i32, acc: &mut i32) -> i32;
    /// C#: `static abstract void Reset(ref int v);` — byref on a `static abstract` member too
    /// (the C# implementor writes `public static void Reset(ref int v)`; dispatched generically
    /// via `T.Reset(ref v)` under `where T : IRefCell`).
    fn Reset(v: &mut i32);
}
