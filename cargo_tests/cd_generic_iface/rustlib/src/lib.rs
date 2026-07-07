//! Proof of the generic-interface-instantiation unlock (`rustc_codegen_clr_add_generic_interface_impl`,
//! `docs/MYCORRHIZA_ERGONOMICS_BACKLOG.md`'s Tier C item 1): `#[dotnet_class(implements = "…")]`
//! previously could only reference a NON-generic managed interface — the interface `ClassRef` was
//! always built with an empty generics list, with no macro-level way to inject a generic argument
//! (discovered when the `IAsyncEnumerable<T>` Tier-A spike hit this wall). This crate implements a
//! real generic BCL interface, `System.IEquatable<int>`, bound to a concrete type argument supplied
//! entirely as a string spec (`"…<[Asm]Ns.Ty>"`) — never derived from a Rust type, exactly like
//! `extends=`'s own superclass reference.
//!
//! A clean load + correct dispatch is itself the proof: if the interface metadata were wrong (e.g.
//! an unbound open generic `IEquatable\`1` instead of the closed `IEquatable<int>`), the CLR would
//! reject the type at load time with a `TypeLoadException` — this crate's `IntBox` type loading and
//! its `Equals(int)` dispatching correctly through the interface is the whole test.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code, non_snake_case)]

use dotnet_macros::{dotnet_class, dotnet_methods};

/// A Rust-defined managed reference type implementing `System.IEquatable<int>`. Its single field
/// `value` backs equality (via the backend-synthesized `read_value` accessor the primary ctor emits).
#[dotnet_class(implements = "[System.Runtime]System.IEquatable<valuetype [System.Runtime]System.Int32>")]
pub struct IntBox {
    value: i32,
}

#[dotnet_methods]
impl IntBox {
    /// Implements `IEquatable<int>.Equals(int) -> bool`. The managed method name (`Equals`) and
    /// signature (`instance bool Equals(int32)`) match the interface member exactly, so it binds
    /// implicitly (no explicit `.override` needed, same as the non-generic `implements=` case).
    pub fn Equals(this: IntBoxHandle, other: i32) -> bool {
        this.instance0::<"read_value", i32>() == other
    }

    /// Plain accessor, not part of the interface — lets the C# consumer read the wrapped value
    /// directly for an independent sanity check alongside the interface-dispatched equality.
    pub fn Value(this: IntBoxHandle) -> i32 {
        this.instance0::<"read_value", i32>()
    }
}
