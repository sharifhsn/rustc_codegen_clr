//! Proof of **default interface methods** (DIM, CoreCLR 3.0+) from `#[dotnet_interface]` — a Rust
//! trait fn WITH a default body becomes a virtual, non-abstract .NET interface member with a real
//! IL body (RVA != 0), emitted on the DEFAULT `DIRECT_PE=1` path by the hand-rolled PE writer.
//!
//! A C# class that implements only the abstract member(s) inherits the DIMs through the interface;
//! a class that defines the member wins over the DIM (ordinary virtual dispatch). Crucially, a
//! `self.<method>()` call INSIDE a default body is lowered to a `callvirt` back through `this`, so
//! it dispatches to the implementing class's own definition — the C# side proves all of this.

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::dotnet_interface;

#[dotnet_interface]
pub trait ICalc {
    /// Abstract member — a C# implementor MUST provide it. The DIMs below dispatch to it
    /// virtually, so each implementor's own `Base` feeds its inherited defaults.
    fn Base(&self) -> i32;

    /// Default interface method: C# classes that don't define `Doubled` get this body;
    /// `self.Base()` dispatches virtually, so the implementing class's `Base` is called.
    fn Doubled(&self) -> i32 {
        self.Base() * 2
    }

    /// Defaults may take arguments and combine self-calls with plain Rust.
    fn PlusN(&self, n: i32) -> i32 {
        self.Base() + n
    }

    /// Self-free defaults work too (a class can implement NOTHING but `Base`).
    fn Fixed(&self) -> i32 {
        7
    }

    /// A default calling ANOTHER default (`Doubled`), which in turn self-calls `Base` — two
    /// levels of interface dispatch from inside a DIM body, both overridable by the class.
    fn DoubledPlus(&self, n: i32) -> i32 {
        self.Doubled() + n
    }
}
