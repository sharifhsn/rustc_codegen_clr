//! Proof of INTERFACE INHERITANCE through `#[dotnet_interface]` — a Rust trait's supertrait list
//! becomes the .NET base-interface list: `trait IPet: IAnimal + ILoud` emits one `InterfaceImpl`
//! row (§II.22.23) per supertrait on IPet's own interface `TypeDef` (its `Extends` stays NIL —
//! §II.10.1.3 models interface inheritance as `InterfaceImpl`, never `Extends`), so C# sees
//! `interface IPet : IAnimal, ILoud` and the CLR computes the transitive closure (`impl is
//! IAnimal` holds through `IPet`). Emitted on the DEFAULT `DIRECT_PE=1` path by the hand-rolled
//! PE writer.
//!
//! `Dog` is a RUST implementor whose TypeDef lists ONLY `InterfaceImpl(IPet)` — a same-assembly
//! `implements = "IPet"` reference (the shape `export_pe`'s Pass 1.5 makes order-independent) —
//! proving the load-time transitive closure from our own metadata: `new Dog(4) is IAnimal`.
//!
//! Fail-loudly counterparts (verified manually, not compiled here):
//!   * `trait X: Clone` — passes the macro (it cannot know which idents are .NET interfaces) but
//!     fails `cargo dotnet build` with the export-time panic naming `Clone`
//!     ("not a type defined in this assembly").
//!   * `trait X: IBase<i32>` / `trait X: 'static` / `trait X: ?Sized` — loud macro-expansion
//!     errors ("generic supertraits …" / "lifetime bounds …" / "`?Trait` supertrait bounds …").
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::{dotnet_class, dotnet_interface, dotnet_methods};

#[dotnet_interface]
pub trait IAnimal {
    /// C#: `int Legs();`
    fn Legs(&self) -> i32;
}

#[dotnet_interface]
pub trait ILoud {
    /// C#: `int Volume();`
    fn Volume(&self) -> i32;
}

/// MULTIPLE supertraits => multiple `InterfaceImpl` rows: C# sees
/// `interface IPet : IAnimal, ILoud`.
#[dotnet_interface]
pub trait IPet: IAnimal + ILoud {
    /// C#: `int Cuteness();`
    fn Cuteness(&self) -> i32;
}

/// A Rust-defined managed class implementing ONLY `IPet` (same-assembly interface reference —
/// note the bare name, no `[Assembly]` prefix). Its three virtual methods satisfy all of
/// IPet+IAnimal+ILoud by name+signature (implicit interface implementation); `IAnimal`/`ILoud`
/// bind through the CLR's transitive interface closure, never appearing in Dog's own metadata.
#[dotnet_class(implements = "IPet")]
pub struct Dog {
    legs: i32,
}

#[dotnet_methods]
impl Dog {
    /// Implements `IAnimal.Legs()` — reads the field, so the consumer can tell the Rust body ran.
    pub fn Legs(this: DogHandle) -> i32 {
        this.instance0::<"read_legs", i32>()
    }
    /// Implements `ILoud.Volume()`.
    pub fn Volume(this: DogHandle) -> i32 {
        11
    }
    /// Implements `IPet.Cuteness()`.
    pub fn Cuteness(this: DogHandle) -> i32 {
        5
    }
}
