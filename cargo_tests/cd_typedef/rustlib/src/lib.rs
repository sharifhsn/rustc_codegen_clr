//! Defining a managed .NET class from a real Rust `struct` via the `#[dotnet_class]` proc-macro, and
//! attaching methods with `#[dotnet_methods]`.
//!
//! `#[dotnet_class]` turns the annotated struct into a managed class — it expands to a
//! `rustc_codegen_clr_comptime_entrypoint` fn whose MIR the backend's comptime interpreter
//! (`src/comptime.rs`) reads to register a real `ClassDef`. Beyond the base capability it emits:
//!   * a **parameterized primary constructor** initializing the fields, plus a public `read_<field>()`
//!     accessor per field;
//!   * (`default_ctor = true`) an additional **parameterless constructor** — so `Counter` has TWO
//!     overloaded ctors;
//!   * (`field_setters = true`) a **`set_<field>(value)` mutator** per field.
//!
//! `#[dotnet_methods]` on an `impl Counter` block re-opens the class (the backend's `finish_type` is
//! idempotent) and attaches:
//!   * a **static** method `Counter.make(value, step)` returning a fresh `CounterHandle`;
//!   * a **virtual instance** method `c.sum()` (its first arg is the `CounterHandle` receiver).
//!
//! No `#[unsafe(no_mangle)]` exports: the class, its ctors, accessors, mutators, and methods are all
//! synthesized/aliased by the backend.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use dotnet_macros::{dotnet_class, dotnet_methods};

/// A Rust-defined .NET reference type `Counter : System.Object` with private fields `value: int32`
/// and `step: int64`, a primary ctor `Counter(int32, int64)`, a parameterless ctor `Counter()`,
/// `read_value()`/`read_step()` accessors, and `set_value(int32)`/`set_step(int64)` mutators.
#[dotnet_class(default_ctor = true, field_setters = true)]
pub struct Counter {
    value: i32,
    step: i64,
}

#[dotnet_methods]
impl Counter {
    /// Static factory: `Counter.make(value, step)`. Builds a `Counter` via the managed primary ctor
    /// (`newobj`) and hands back the handle — proving a Rust-defined static method callable as
    /// `Counter.make(…)` from C#.
    pub fn make(value: i32, step: i64) -> CounterHandle {
        CounterHandle::ctor2::<i32, i64>(value, step)
    }

    /// Virtual instance method `c.sum()` = `value + step` (widening `value` to i64). Its first
    /// parameter is the `CounterHandle` receiver, so the backend attaches it as an instance method.
    /// The field values are read back through the backend-synthesized `read_*` accessors.
    pub fn sum(this: CounterHandle) -> i64 {
        let value: i32 = this.instance0::<"read_value", i32>();
        let step: i64 = this.instance0::<"read_step", i64>();
        (value as i64) + step
    }
}

/// A Rust-defined .NET reference type whose FIELDS are themselves managed reference types — two
/// `Counter` handles. Proves a `#[dotnet_class]` field can be another Rust-defined managed class (not
/// just a primitive): the primary ctor stores each `Counter` reference, and `read_left()`/`read_right()`
/// hand them back to C#.
#[dotnet_class]
pub struct Pair {
    left: CounterHandle,
    right: CounterHandle,
}

#[dotnet_methods]
impl Pair {
    /// Instance method summing both member counters — exercises reading a managed-type field and
    /// dispatching an instance method (`sum`) on it, all Rust-side.
    pub fn total(this: PairHandle) -> i64 {
        let left: CounterHandle = this.instance0::<"read_left", CounterHandle>();
        let right: CounterHandle = this.instance0::<"read_right", CounterHandle>();
        Counter::sum(left) + Counter::sum(right)
    }
}
