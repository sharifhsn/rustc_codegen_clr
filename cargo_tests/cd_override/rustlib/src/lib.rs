//! Proof of the `#[dotnet_class]` virtual-method-override spike
//! (`docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md` Tier C finding #1): a Rust-defined managed class can
//! now explicitly `.override` a *base class's* virtual method (`System.Object.ToString()`), not
//! just satisfy an *interface* member (`implements=`, which binds implicitly by name+signature
//! with no vtable-slot concept). `#[dotnet_override("[System.Runtime]System.Object")]` emits a
//! real ECMA-335 `.override` (a `MethodImpl` metadata row), landing the override in `Object`'s
//! own vtable slot rather than creating a new one.
//!
//! **Why this specific proof matters** (per the finding's own "smallest safe first step"): a
//! *new-slot shadow method* (same name+signature, no `.override`) would ALSO satisfy calling
//! `greeter.ToString()` on the concrete type — the only way to tell the difference is calling it
//! through an `Object`-typed reference, which dispatches virtually through `Object`'s own vtable
//! slot. If this were a shadow instead of a real override, that call would print the BCL's default
//! `Object.ToString()` (the type's full name), not this override's text.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::{dotnet_class, dotnet_methods};
use mycorrhiza::system::{DotNetString, MString};

/// A Rust-defined managed reference type overriding `System.Object.ToString()`. Its single field
/// `label` backs the override (via the backend-synthesized `read_label` accessor the primary ctor
/// emits).
#[dotnet_class]
pub struct Greeter {
    label: i32,
}

#[dotnet_methods]
impl Greeter {
    /// Explicit `.override` of `System.Object.ToString()` — not implicit `implements=` binding
    /// (there is no interface here at all), and not an ordinary new-slot virtual (which would
    /// only satisfy calls through the CONCRETE type, not through an `Object`-typed reference).
    #[dotnet_override("[System.Runtime]System.Object")]
    pub fn ToString(this: GreeterHandle) -> MString {
        let label = this.instance0::<"read_label", i32>();
        DotNetString::from(format!("Greeter #{label}").as_str()).handle()
    }
}
