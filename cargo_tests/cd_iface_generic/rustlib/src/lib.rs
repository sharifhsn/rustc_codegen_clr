//! Proof of GENERIC `#[dotnet_interface]` — a Rust `trait IBox<T>` becomes a genuine generic
//! .NET interface DEFINITION, emitted on the DEFAULT `DIRECT_PE=1` path by the hand-rolled PE
//! writer: the TypeDef is named with the CLS backtick-arity suffix (`IBox`1`), carries one
//! ECMA-335 `GenericParam` row (§II.22.20) named `T`, and its members' signatures reference the
//! parameter as `ELEMENT_TYPE_VAR 0` (C#'s `T`). A C# consumer implements ANY instantiation
//! (`class IntBox : IBox<int>`, `class StrBox : IBox<string>` — two instantiations prove genuine
//! genericity, not a monomorphized fake) and dispatches through it polymorphically.
//!
//! The macro also emits a PARAMETERIZED handle alias, `IBoxHandle<T>` — a Rust-side reference to
//! an instantiation (`IBoxHandle<i32>` = `IBox<int>`), usable in exported signatures: `take_box`
//! below exercises the PE writer's in-assembly open-generic `TypeSpec` resolution (a
//! `GENERICINST` whose open type is this assembly's own TypeDef, not a dangling external
//! `TypeRef`).

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::dotnet_interface;

#[dotnet_interface]
pub trait IBox<T> {
    /// C#: `T Get();` — the generic parameter in RETURN position.
    fn Get(&self) -> T;
    /// C#: `void Put(T value);` — the generic parameter in PARAMETER position.
    fn Put(&mut self, value: T);
    /// C#: `int Count();` — non-generic members mix freely on a generic interface.
    fn Count(&self) -> i32;
}

/// C# calls `MainModule.take_box(IBox<int>)` — the parameter type is an INSTANTIATION of this
/// assembly's own generic interface, so its declared signature forces the PE writer to resolve
/// `IBox`1<int32>` to a `TypeSpec` over the local open TypeDef (the `find_open_generic_def`
/// path). The handle is opaque to this body on purpose: the check is metadata
/// resolution + C#-side assignability, not Rust-side dispatch (a stretch goal, per the plan).
#[unsafe(no_mangle)]
pub extern "C" fn take_box(b: IBoxHandle<i32>) -> i32 {
    let _ = b;
    7
}
