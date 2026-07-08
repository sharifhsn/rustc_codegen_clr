//! Proof of PROPERTIES ON INTERFACES — `#[dotnet_property]` on `get_<Prop>`/`set_<Prop>` trait
//! fns inside a `#[dotnet_interface]` trait emits abstract accessor `MethodDef`s (RVA=0,
//! Abstract|Virtual|SpecialName) plus the `Property`/`PropertyMap`/`MethodSemantics` rows, on the
//! DEFAULT `DIRECT_PE=1` path (hand-rolled PE writer, no ilasm). A C# consumer then writes
//! `class Speaker : IVolume { public int Volume { get; set; } }` — an auto-property
//! implementation of the interface property — and reads/writes it THROUGH the interface
//! reference.
//!
//! The accessor's OWN signature carries the property's value type (never spelled separately): a
//! getter's return type, a setter's single by-value parameter. A getter alone declares a
//! get-only property (`Name` below); a setter with no getter (write-only) is rejected at the
//! macro level.

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::dotnet_interface;
use mycorrhiza::system::MString;

#[dotnet_interface]
pub trait IVolume {
    /// Ordinary abstract member coexisting with the properties. C#: `int Id();`
    fn Id(&self) -> i32;

    /// C#: `int Volume { get; set; }` — a read-write property.
    #[dotnet_property]
    fn get_Volume(&self) -> i32;
    #[dotnet_property]
    fn set_Volume(&mut self, value: i32);

    /// C#: `string Name { get; }` — a get-only property (no matching setter).
    #[dotnet_property]
    fn get_Name(&self) -> MString;
}
