//! Proof of GENERIC METHODS on `#[dotnet_interface]` — a Rust `fn Echo<T>(&self, value: T) -> T`
//! trait method becomes a genuine generic .NET method DEFINITION, emitted on the DEFAULT
//! `DIRECT_PE=1` path by the hand-rolled PE writer: the member's `MethodDefSig` blob carries
//! `SIG_GENERIC` (0x10) + a compressed `GenParamCount` (§II.23.2.1), one METHOD-owned ECMA-335
//! `GenericParam` row (§II.22.20, coded `TypeOrMethodDef` owner tag 1) is emitted per declared
//! parameter, and the `T` positions are `ELEMENT_TYPE_MVAR` (`!!N` — C#'s method-level `T`).
//! A C# consumer implements it as an ordinary generic interface method (`public T Echo<T>(T
//! value) => value;`), calls it at any instantiation (value AND reference types — genuine
//! genericity, not a monomorphized fake), and the reflection stack round-trips the definition
//! (`IsGenericMethodDefinition`, `MakeGenericMethod(...).Invoke(...)` — the hardest loader-side
//! validation of the def-shape metadata).
//!
//! `IPicker<T>` additionally mixes the two generic namespaces on one member: `U Pick<U>(T a,
//! U b)` — the owning INTERFACE's parameter (`ELEMENT_TYPE_VAR 0`, `!0`, from
//! `cd_iface_generic`'s feature) and the METHOD's own (`ELEMENT_TYPE_MVAR 0`, `!!0`) in a single
//! signature.

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::dotnet_interface;

#[dotnet_interface]
pub trait IConverter {
    /// C#: `int Describe();` — ordinary members mix freely with generic ones.
    fn Describe(&self) -> i32;
    /// C#: `T Echo<T>(T value);` — one method generic parameter, in BOTH param and return
    /// position.
    fn Echo<T>(&self, value: T) -> T;
    /// C#: `K First<K, V>(K key, V value);` — TWO method generic parameters (two method-owned
    /// `GenericParam` rows, Numbers 0 and 1).
    fn First<K, V>(&self, key: K, value: V) -> K;
}

#[dotnet_interface]
pub trait IPicker<T> {
    /// C#: `U Pick<U>(T a, U b);` — the owning interface's `T` (`!0`) and the method's own `U`
    /// (`!!0`) mixed in one signature.
    fn Pick<U>(&self, a: T, b: U) -> U;
    /// C#: `T Base();` — a type-generic-only member alongside, for contrast.
    fn Base(&self) -> T;
}

/// A trivial exported probe — keeps the class library shape (an exported `MainModule` surface)
/// identical to the sibling `cd_iface_*` crates.
#[no_mangle]
pub extern "C" fn genmethod_probe() -> i32 {
    11
}
