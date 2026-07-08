//! Proof of EVENTS DECLARED ON INTERFACES — `#[dotnet_event]` on a `#[dotnet_interface]` trait fn
//! emits abstract virtual `add_*`/`remove_*` accessor `MethodDef`s (RVA=0, Abstract|Virtual|
//! SpecialName) plus the `Event`/`EventMap`/`MethodSemantics` rows, on the DEFAULT `DIRECT_PE=1`
//! path (hand-rolled PE writer, no ilasm). A C# consumer then writes
//! `class Button : IButton { public event Action Clicked; }` — a field-like implementation of the
//! interface event — and subscribes/unsubscribes THROUGH the interface reference.
//!
//! The fn name IS the event name: the single declaration synthesizes BOTH accessors (deliberately
//! different from the class-side `#[dotnet_event("Name")]` pair form — class accessors have two
//! distinct Rust bodies, interface accessors have none, so one declaration makes a
//! missing/mismatched half impossible by construction).

#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::dotnet_interface;
use mycorrhiza::intrinsics::RustcCLRInteropManagedClass;

/// `System.Action` (non-generic, zero-arg) — the delegate type `Clicked` subscribers must match
/// (the same handle idiom as cd_event's class-side event).
type ActionHandle = RustcCLRInteropManagedClass<"System.Runtime", "System.Action">;

#[dotnet_interface]
pub trait IButton {
    /// Ordinary abstract member coexisting with the event. C#: `int Id();`
    fn Id(&self) -> i32;

    /// C#: `event Action Clicked;` on the interface. Expands to abstract
    /// `void add_Clicked(Action)` + `void remove_Clicked(Action)` + the Event metadata row.
    /// Exactly one non-receiver parameter (the subscriber delegate), no return type.
    #[dotnet_event]
    fn Clicked(&self, handler: ActionHandle);
}
