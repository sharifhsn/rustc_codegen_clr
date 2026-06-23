// Targeted proof for the newly-wired integer/bit intrinsic arms:
//   - bitreverse usize / isize
//   - saturating_add / saturating_sub for i128
//   - float_to_int_unchecked for u128 / i128
//
// Also exercises the already-working 128-bit bswap (swap_bytes) to lock it in.
//
// Each check has a KNOWN answer computed by hand; on a miscompile the assert
// fires (non-zero exit) instead of printing the OK line.
#![feature(core_intrinsics)]
#![allow(internal_features)]

use std::hint::black_box;

unsafe fn f64_to_u128(x: f64) -> u128 {
    core::intrinsics::float_to_int_unchecked(x)
}
unsafe fn f64_to_i128(x: f64) -> i128 {
    core::intrinsics::float_to_int_unchecked(x)
}

fn main() {
    // --- bitreverse usize / isize (64-bit target) ---
    // reverse_bits of 1 puts the single set bit at the top: 1 << 63.
    assert_eq!(black_box(1usize).reverse_bits(), 1usize << 63);
    assert_eq!(black_box(0usize).reverse_bits(), 0usize);
    // usize::MAX is all ones -> reversed is still all ones.
    assert_eq!(black_box(usize::MAX).reverse_bits(), usize::MAX);
    // A known asymmetric pattern: 0x00000000_0000_00FF reversed -> 0xFF00...00.
    assert_eq!(
        black_box(0x0000_0000_0000_00FFusize).reverse_bits(),
        0xFF00_0000_0000_0000usize
    );
    assert_eq!(black_box(0isize).reverse_bits(), 0isize);
    assert_eq!(black_box(-1isize).reverse_bits(), -1isize);
    // isize 1 reversed -> i64::MIN (the sign bit set).
    assert_eq!(black_box(1isize).reverse_bits(), isize::MIN);
    println!("bitreverse usize/isize OK");

    // --- saturating_add / saturating_sub i128 ---
    assert_eq!(black_box(i128::MAX).saturating_add(1), i128::MAX);
    assert_eq!(black_box(i128::MAX).saturating_add(i128::MAX), i128::MAX);
    assert_eq!(black_box(i128::MIN).saturating_add(-1), i128::MIN);
    assert_eq!(black_box(i128::MIN).saturating_add(i128::MIN), i128::MIN);
    assert_eq!(black_box(5i128).saturating_add(7), 12i128);
    assert_eq!(black_box(-5i128).saturating_add(-7), -12i128);
    assert_eq!(black_box(i128::MAX).saturating_add(-1), i128::MAX - 1);
    // mixed-sign never overflows
    assert_eq!(black_box(i128::MAX).saturating_add(i128::MIN), -1i128);

    assert_eq!(black_box(i128::MIN).saturating_sub(1), i128::MIN);
    assert_eq!(black_box(i128::MIN).saturating_sub(i128::MAX), i128::MIN);
    assert_eq!(black_box(i128::MAX).saturating_sub(-1), i128::MAX);
    assert_eq!(black_box(i128::MAX).saturating_sub(i128::MIN), i128::MAX);
    assert_eq!(black_box(10i128).saturating_sub(3), 7i128);
    assert_eq!(black_box(3i128).saturating_sub(10), -7i128);
    // same-sign never overflows
    assert_eq!(black_box(i128::MAX).saturating_sub(i128::MAX), 0i128);
    println!("saturating_add/sub i128 OK");

    // --- float_to_int_unchecked u128 / i128 ---
    unsafe {
        assert_eq!(f64_to_u128(black_box(123.0)), 123u128);
        assert_eq!(f64_to_u128(black_box(0.0)), 0u128);
        // 2^64 is exactly representable in f64; result exceeds u64 range.
        assert_eq!(
            f64_to_u128(black_box(18_446_744_073_709_551_616.0)),
            1u128 << 64
        );
        assert_eq!(f64_to_i128(black_box(-123.0)), -123i128);
        assert_eq!(f64_to_i128(black_box(123.0)), 123i128);
        assert_eq!(
            f64_to_i128(black_box(-18_446_744_073_709_551_616.0)),
            -(1i128 << 64)
        );
    }
    println!("float_to_int_unchecked u128/i128 OK");

    // --- already-working 128-bit bswap (lock-in) ---
    assert_eq!(
        black_box(0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10u128).swap_bytes(),
        0x100F_0E0D_0C0B_0A09_0807_0605_0403_0201u128
    );
    assert_eq!(black_box(1u128).swap_bytes(), 1u128 << 120);
    println!("bswap u128 OK");

    println!("intr_bits: all checks passed");
}
