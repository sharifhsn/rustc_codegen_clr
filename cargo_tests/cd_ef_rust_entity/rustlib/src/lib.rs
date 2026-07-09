//! Investigation (now a regression proof): can a `#[dotnet_class]`-defined Rust type serve as a
//! real EF Core entity?
//!
//! EF Core's default model-builder convention discovers an entity's mapped columns via
//! `Type.GetProperties()` — it needs genuine CLR `PropertyInfo` members (an IL `.property` row
//! with `SpecialName` `get_X`/`set_X` accessor methods), not merely a same-shaped pair of ordinary
//! public methods. This crate defines `Widget` — an int `Id` and a `Name` string, the minimal
//! EF-entity shape — via `#[dotnet_class(default_ctor = true, properties = true)]`. `default_ctor`
//! gives EF a parameterless constructor to materialize via (`Activator`-style, no primary-ctor
//! binding needed); the primary ctor gives it a `Widget(int, string)` overload too. `properties`
//! gives it real `Id`/`Name` `.NET` properties (backed by `get_Id`/`set_Id`/`get_Name`/`set_Name`
//! accessors, each `SpecialName` and linked via `MethodSemantics` into a genuine §II.22.34
//! `Property` row) — not just same-shaped ordinary methods.
//!
//! This used to fail: `#[dotnet_class]`'s only field-accessor option was `field_setters = true`,
//! which emits plain `MethodDef`s named `read_<field>`/`set_<field>` — no `PropertyDef` row, so
//! EF's `Type.GetProperties()` scan came back empty and the model builder refused to treat
//! `Widget` as a valid entity (no properties => no columns, no discoverable primary key). The fix
//! adds a genuinely NEW opt-in, `properties = true` (a separate flag from `field_setters`, backed
//! by its own `rustc_codegen_clr_add_field_properties` comptime intrinsic — see
//! `mycorrhiza::comptime` and `src/comptime.rs`'s `finish_type`), rather than making
//! `field_setters` itself emit `PropertyDef`s: once a method is linked as a property accessor via
//! `MethodSemantics`, C#/Roslyn REJECTS calling it explicitly by name (`CS0571`), so retrofitting
//! `field_setters`' `read_<field>`/`set_<field>` accessors would have broken every existing
//! explicit-call consumer (e.g. `cargo_tests/cd_typedef`, which calls `c.read_value()`,
//! `d.set_value(42)`, … directly). `properties = true` instead emits its own `get_<Field>`/
//! `set_<Field>` pair, reachable only through the property (`widget.Id`, not `widget.get_Id()`) —
//! exactly the shape EF (and any other `Type.GetProperties()` consumer) expects.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use dotnet_macros::dotnet_class;

/// The minimal EF-entity shape: an integer id + a string name, exposed as real `.NET` properties
/// (`Id`/`Name`) via `#[dotnet_class]`'s `properties = true`, plus a parameterless ctor alongside
/// the field-initializing primary ctor.
#[dotnet_class(default_ctor = true, properties = true)]
pub struct Widget {
    id: i32,
    name: mycorrhiza::system::MString,
}
