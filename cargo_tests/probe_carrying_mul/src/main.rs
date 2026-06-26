// Regression probe for the `carrying_mul_add` signedness miscompile (gaps-campaign / I2-at-scale).
// The backend widened the four operands from the *unsigned* result type `U` instead of the operand
// type `T`, routing negative signed operands through a zero-extending conversion (wrong high half).
// Exercises the signed i64 (128-bit-promoted) and i32 (64-bit-promoted) paths with NEGATIVE
// operands, where sign- vs zero-extension diverge; output must match native rustc exactly.
// Surfaced by rust-lang/rust coretests `num::i64::test_carrying_mul_add`.
#![feature(signed_bigint_helpers)]
use std::hint::black_box;

fn main() {
    let cases_i64: [(i64, i64, i64, i64); 6] = [
        (-1, -1, 0, 0),
        (-1, 1, 0, 0),
        (i64::MIN, i64::MIN, 0, 0),
        (i64::MIN, -1, i64::MAX, 1),
        (-1234567890123, 9876543210, -42, -7),
        (i64::MAX, i64::MIN, i64::MIN, i64::MAX),
    ];
    for (a, b, c, d) in cases_i64 {
        let (lo, hi) = black_box(a).carrying_mul_add(black_box(b), black_box(c), black_box(d));
        println!("i64 {a} {b} {c} {d} -> lo={lo} hi={hi}");
    }
    let cases_i32: [(i32, i32, i32, i32); 3] =
        [(-1, -1, 0, 0), (i32::MIN, i32::MIN, 0, 0), (-7, 9, -3, -5)];
    for (a, b, c, d) in cases_i32 {
        let (lo, hi) = black_box(a).carrying_mul_add(black_box(b), black_box(c), black_box(d));
        println!("i32 {a} {b} {c} {d} -> lo={lo} hi={hi}");
    }
}
