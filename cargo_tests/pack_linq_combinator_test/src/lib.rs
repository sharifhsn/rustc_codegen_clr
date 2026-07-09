//! Proves `Mycorrhiza.Interop.Helpers.dll`, bundled into a `.nupkg` by `cargo dotnet pack`,
//! actually resolves and FUNCTIONS at runtime when consumed by a fresh C# app that never runs
//! `cargo dotnet` — not just that the dll file happens to sit in the output folder. Two
//! `TypedPredicate`s built against deliberately DIFFERENT `Param` instances (the realistic case:
//! two independently-authored builder functions), combined with `&`, forces
//! `mycorrhiza::linq`'s parameter-rebind path — which calls into
//! `Mycorrhiza.Linq.ParameterRebinder` in that bundled assembly.
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, non_snake_case)]

use mycorrhiza::linq::{Expr, Param, TypedPredicate};

struct Dummy;

#[no_mangle]
pub extern "C" fn linq_combinator_smoke() -> i32 {
    // Each predicate built with its OWN Param::new call -> distinct ParameterExpression
    // instances, exactly the case rebind_param (and thus ParameterRebinder.cs) must handle.
    // Real concrete type (System.Int32, not System.Object) — the body compares the parameter's
    // value directly, no property access, matching cd_linq_expr's proven pattern.
    let p1 = Param::new("System.Int32", "x");
    let pred_a: TypedPredicate<Dummy> = TypedPredicate::new(p1, p1.expr().ge(Expr::const_i32(18)));

    let p2 = Param::new("System.Int32", "y");
    let pred_b: TypedPredicate<Dummy> = TypedPredicate::new(p2, p2.expr().lt(Expr::const_i32(65)));

    let combined = pred_a & pred_b; // forces rebind_param -> ParameterRebinder.
    let s = combined.text();
    // A successful rebind renders with ONE shared parameter name throughout (whichever side won,
    // here "x"), never a mix of "x" and "y" — a failed/absent rebind would leave "y" untouched.
    if s.contains("AndAlso") && s.contains("x") && !s.contains(" y ") && !s.contains("(y") {
        42
    } else {
        -1
    }
}
