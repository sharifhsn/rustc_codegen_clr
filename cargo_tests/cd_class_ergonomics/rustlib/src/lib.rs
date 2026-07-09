//! Proof for three new `#[dotnet_class]` capabilities:
//!   1. Real `static` fields (`static_field(NAME: Type)`) — a genuine public `.NET` static field,
//!      directly visible from C# as `ClassName.Name`, plus synthesized `get_Name`/`set_Name`
//!      static methods so Rust code can read/write it too.
//!   2. Real operator overloads (`op_Addition`, `op_Equality`, …) — a `#[dotnet_methods]` static
//!      method with one of the reserved CLR operator names now gets `SpecialName` stamped, so C#
//!      binds `+`/`==` syntax to it (not just the literal `X.op_Addition(a, b)` call).
//!   3. Base constructors that take arguments (`base_ctor_args(Type, ...)`) — subclassing a base
//!      whose `.ctor` isn't parameterless, chaining the caller-supplied values through instead of
//!      always calling a parameterless `base()`.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::{dotnet_class, dotnet_methods};

// ---- 1. Static fields ----

#[dotnet_class(static_field(Count: i32))]
pub struct Counter {}

#[dotnet_methods]
impl Counter {
    /// Increments the static field by calling the synthesized `get_Count`/`set_Count` accessors
    /// on this same class — proving Rust can read/write a `#[dotnet_class]` static field through
    /// the ordinary generic static-call bridge, no new cross-boundary intrinsic needed.
    pub fn bump() -> i32 {
        let v = CounterHandle::static0::<"get_Count", i32>();
        let next = v + 1;
        CounterHandle::static1::<"set_Count", i32, ()>(next);
        next
    }
}

// ---- 2. Real operator overloads ----

#[dotnet_class]
pub struct Vector2 {
    x: i32,
    y: i32,
}

#[dotnet_methods]
impl Vector2 {
    pub fn make(x: i32, y: i32) -> Vector2Handle {
        Vector2Handle::ctor2::<i32, i32>(x, y)
    }

    pub fn get_x(this: Vector2Handle) -> i32 {
        this.instance0::<"read_x", i32>()
    }
    pub fn get_y(this: Vector2Handle) -> i32 {
        this.instance0::<"read_y", i32>()
    }

    /// `a + b` from C# — proves `SpecialName` binds the `op_Addition` name to real `+` syntax.
    pub fn op_Addition(a: Vector2Handle, b: Vector2Handle) -> Vector2Handle {
        let ax: i32 = a.instance0::<"read_x", i32>();
        let ay: i32 = a.instance0::<"read_y", i32>();
        let bx: i32 = b.instance0::<"read_x", i32>();
        let by: i32 = b.instance0::<"read_y", i32>();
        Vector2Handle::ctor2::<i32, i32>(ax + bx, ay + by)
    }

    /// `a == b` from C# — proves `SpecialName` binds `op_Equality` to real `==` syntax.
    pub fn op_Equality(a: Vector2Handle, b: Vector2Handle) -> bool {
        let ax: i32 = a.instance0::<"read_x", i32>();
        let ay: i32 = a.instance0::<"read_y", i32>();
        let bx: i32 = b.instance0::<"read_x", i32>();
        let by: i32 = b.instance0::<"read_y", i32>();
        ax == bx && ay == by
    }

    /// `a != b` — C# requires both `op_Equality`/`op_Inequality` together when either is defined.
    pub fn op_Inequality(a: Vector2Handle, b: Vector2Handle) -> bool {
        !Self::op_Equality(a, b)
    }
}

// ---- 3. Base constructors that take arguments ----

/// Extends `Widget(int seed)` — a base class with NO parameterless `.ctor` — via
/// `base_ctor_args(i32)`. The primary ctor becomes `.ctor(seed, tag)`, forwarding `seed` into
/// `base(seed)` and storing `tag` in its own field, proving both halves compose.
#[dotnet_class(
    extends = "[cd_class_ergonomics_basecs]CdClassErgonomicsBase.Widget",
    base_ctor_args(i32)
)]
pub struct Gadget {
    tag: i32,
}

#[dotnet_methods]
impl Gadget {
    pub fn make_gadget(seed: i32, tag: i32) -> GadgetHandle {
        GadgetHandle::ctor2::<i32, i32>(seed, tag)
    }
    pub fn get_tag(this: GadgetHandle) -> i32 {
        this.instance0::<"read_tag", i32>()
    }
}
