// Regression (P2-S2, differential-oracle): `{:?}` (Debug) formatting of any float, and the
// `f64::abs`/`copysign`/`mul_add` intrinsics, crashed at runtime with
//   System.TypeLoadException: Could not load type 'System.Double' ... due to value type mismatch
//   at ...core::fmt::float::GeneralFormat::already_rounded_value_should_use_exponential(Double* self)
//
// Root cause: a SURVIVING corner of the P2-S1 `System.Double`/`System.Single`-must-be-`valuetype`
// fix. P2-S1 made `ClassRef::double`/`single` value types, but the `Abs`/`CopySign`/
// `FusedMultiplyAdd` references still rendered as `class [System.Runtime]System.Double` (the
// final IL contained BOTH `class` and `valuetype` references to `System.Double` in one assembly).
// The `class`-prefixed reference makes the CLR reject the *type load* of the (genuinely value-type)
// CoreLib `System.Double` with "value type mismatch" the instant such a method is JITted.
// `{:?}` on a float reaches `GeneralFormat::already_rounded_value_should_use_exponential`, whose
// body calls `f64::abs` -> `System.Double::Abs`, so every Debug-format of a float crashed —
// including derived `#[derive(Debug)]` on any struct/enum/tuple/Vec/Option holding a float.
// Plain `{}` (Display) of a float was unaffected, which is why this hid behind the green gate.
//
// Fix: normalize the known BCL primitive value types (`System.Double`/`Single`/`Half`/`Int128`/
// `UInt128`) to the `valuetype` prefix at the IL-rendering boundary (cilly il_exporter::class_ref),
// regardless of which path interned the ClassRef. Safe: these CoreLib names are unconditionally
// .NET value types.
//
// Known-answer: self-asserting; any wrong byte / crash is a non-zero exit / stderr output.
use std::hint::black_box as bb;

fn main() {
    // Bare Debug of f64 / f32 — the exact crash trigger.
    assert_eq!(format!("{:?}", bb(2.0f64)), "2.0");
    assert_eq!(format!("{:?}", bb(1.5f32)), "1.5");
    assert_eq!(format!("{:?}", bb(0.1f64)), "0.1");
    assert_eq!(format!("{:?}", bb(-3.25f64)), "-3.25");

    // Special values.
    assert_eq!(format!("{:?}", bb(f64::NAN)), "NaN");
    assert_eq!(format!("{:?}", bb(f64::INFINITY)), "inf");
    assert_eq!(format!("{:?}", bb(f64::NEG_INFINITY)), "-inf");
    assert_eq!(format!("{:?}", bb(-0.0f64)), "-0.0");

    // Debug of containers / derived Debug holding floats.
    assert_eq!(format!("{:?}", bb(vec![1.0f64, 2.5, 3.0])), "[1.0, 2.5, 3.0]");
    assert_eq!(format!("{:?}", bb((1.0f64, 2.0f64))), "(1.0, 2.0)");
    assert_eq!(format!("{:?}", bb(Some(3.14f64))), "Some(3.14)");

    #[derive(Debug)]
    enum Shape {
        Circle(f64),
        Rect { w: f64, h: f64 },
    }
    assert_eq!(format!("{:?}", bb(Shape::Circle(2.0))), "Circle(2.0)");
    assert_eq!(
        format!("{:?}", bb(Shape::Rect { w: 3.0, h: 4.0 })),
        "Rect { w: 3.0, h: 4.0 }"
    );

    // The float intrinsics that routed through the `class`-prefixed `System.Double`.
    assert_eq!(bb(-2.5f64).abs(), 2.5);
    assert_eq!(bb(3.0f64).copysign(bb(-1.0)), -3.0);
    assert_eq!(bb(2.0f64).mul_add(bb(3.0), bb(4.0)), 10.0);
    assert_eq!(bb(-2.5f32).abs(), 2.5);

    // Exponential / general debug path (the actual GeneralFormat method).
    assert_eq!(format!("{:?}", bb(1e300f64)), "1e300");
    assert_eq!(format!("{:?}", bb(1e-300f64)), "1e-300");

    println!("float_debug_fmt: all checks passed");
}
