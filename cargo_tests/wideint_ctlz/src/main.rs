// Regression: `u128`/`i128` leading_zeros / trailing_zeros (the `ctlz`/`cttz` intrinsics).
//
// The 128-bit arms called `System.{U,}Int128::{Leading,Trailing}ZeroCount`, which return a
// 128-bit value, and then narrowed the result to u32 with a raw `conv.u4` (`ctx.int_cast`).
// `conv.u4` is invalid IL applied to a `System.UInt128`/`System.Int128` *struct* operand: the
// runtime did not truncate, it read garbage (e.g. `1u128.leading_zeros()` returned 2386363928
// instead of 127). The popcount arm already did this right via `op_Explicit` (`int_to_int`);
// the fix routes ctlz/cttz the same way.
//
// Known-answer: self-asserts; black_box forces the runtime lowering (not const-eval).
use std::hint::black_box as bb;

fn main() {
    // u128 leading_zeros
    assert_eq!(bb(1u128).leading_zeros(), 127);
    assert_eq!(bb(0u128).leading_zeros(), 128);
    assert_eq!(bb(u128::MAX).leading_zeros(), 0);
    assert_eq!(bb(1u128 << 64).leading_zeros(), 63);
    assert_eq!(bb(0xFFFF_FFFF_FFFF_FFFFu128).leading_zeros(), 64);

    // u128 trailing_zeros
    assert_eq!(bb(1u128).trailing_zeros(), 0);
    assert_eq!(bb(0u128).trailing_zeros(), 128);
    assert_eq!(bb(1u128 << 100).trailing_zeros(), 100);
    assert_eq!(bb(u128::MAX).trailing_zeros(), 0);

    // i128 leading_zeros / trailing_zeros
    assert_eq!(bb(1i128).leading_zeros(), 127);
    assert_eq!(bb(0i128).leading_zeros(), 128);
    assert_eq!(bb(-1i128).leading_zeros(), 0);
    assert_eq!(bb(1i128 << 64).trailing_zeros(), 64);
    assert_eq!(bb(i128::MIN).leading_zeros(), 0);
    assert_eq!(bb(i128::MIN).trailing_zeros(), 127);

    // count_ones still correct (the popcount arm, which was already right).
    assert_eq!(bb(u128::MAX).count_ones(), 128);
    assert_eq!(bb(0xF0F0u128).count_ones(), 8);

    println!("wideint_ctlz: all checks passed");
}
