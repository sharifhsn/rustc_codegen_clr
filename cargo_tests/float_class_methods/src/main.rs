// Regression: methods whose *declaring type* is `System.Double` / `System.Single`
// (`f64::min`/`max`, `f32::min`/`max`, fma, powi, the IEEE `minimum`/`maximum`, …).
//
// `ClassRef::double`/`single` used to be created with `is_valuetype = false`, so the IL
// exporter emitted `class [System.Runtime]System.Double` instead of `valuetype ...`. The CLR
// then rejected the *type load* of `System.Double` with
//   System.TypeLoadException: Could not load type 'System.Double' ... value type mismatch
// the instant a method whose declaring type is `System.Double` was JITted — i.e. any program
// that called `f64::min`/`max` crashed before producing output. Plain f64 arithmetic/printing
// was unaffected (it never names `System.Double` as a declaring type), which is why this hid.
//
// Fix: `System.Double`/`System.Single` are .NET value types -> `is_valuetype = true`.
// Known-answer: this program self-asserts; any wrong byte / crash is a non-zero exit.
use std::hint::black_box as bb;

fn main() {
    // f64::min / max ignore NaN (return the non-NaN operand).
    assert_eq!(bb(1.0f64).min(bb(f64::NAN)), 1.0);
    assert_eq!(bb(1.0f64).max(bb(f64::NAN)), 1.0);
    assert_eq!(bb(f64::NAN).min(bb(2.0f64)), 2.0);
    assert_eq!(bb(f64::NAN).max(bb(2.0f64)), 2.0);
    assert!(bb(f64::NAN).min(bb(f64::NAN)).is_nan());

    // Signed-zero handling: min of (+0,-0) is -0, max is +0 (exact bit pattern).
    assert_eq!(bb(0.0f64).min(bb(-0.0f64)).to_bits(), (-0.0f64).to_bits());
    assert_eq!(bb(0.0f64).max(bb(-0.0f64)).to_bits(), (0.0f64).to_bits());

    // f32 analogues.
    assert_eq!(bb(2.0f32).min(bb(f32::NAN)), 2.0);
    assert_eq!(bb(2.0f32).max(bb(f32::NAN)), 2.0);
    assert_eq!(bb(0.0f32).min(bb(-0.0f32)).to_bits(), (-0.0f32).to_bits());

    // mul_add / powi route through System.Double::FusedMultiplyAdd / Pow.
    assert_eq!(bb(2.0f64).mul_add(bb(3.0f64), bb(4.0f64)), 10.0);
    assert_eq!(bb(2.0f32).mul_add(bb(3.0f32), bb(4.0f32)), 10.0);
    assert_eq!(bb(2.0f64).powi(bb(10)), 1024.0);

    println!("float_class_methods: all checks passed");
}
