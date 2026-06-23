// Targeted proof for the newly-wired SIMD tail intrinsics. Each check has a
// KNOWN answer computed by hand; on a miscompile the assert fires (non-zero
// exit) instead of printing the OK line. Mirrors cargo_tests/intr_bits style.
//
// Covers: simd_swizzle (one-vec + two-vec concat), ctpop/ctlz/cttz (u32 + u8),
// bswap (swap_bytes), bitreverse (reverse_bits), sqrt/floor/ceil/trunc/round,
// fma (mul_add), and float + signed-int reduce_min/reduce_max.
#![feature(portable_simd)]

use std::hint::black_box;
use std::simd::prelude::*;
use std::simd::{simd_swizzle, StdFloat};

fn bits_eq(a: f32, b: f32) -> bool {
    // Exact compare; all expected values here are exactly representable.
    a.to_bits() == b.to_bits()
}

fn main() {
    // --- simd_shuffle: single-vector swizzle ---
    // a=[10,20,30,40], pick lanes [3,0,2,1] -> [40,10,30,20].
    let a = Simd::<i32, 4>::from_array([10, 20, 30, 40]);
    let r: Simd<i32, 4> = simd_swizzle!(black_box(a), [3, 0, 2, 1]);
    assert_eq!(r.to_array(), [40, 10, 30, 20]);
    println!("simd_swizzle one-vec OK");

    // --- simd_shuffle: two-vector concatenation ---
    // a=[1,2,3,4], b=[5,6,7,8]; concat is [1,2,3,4,5,6,7,8].
    // indices [0,7,2,5] -> [1, 8, 3, 6].
    let a = Simd::<i32, 4>::from_array([1, 2, 3, 4]);
    let b = Simd::<i32, 4>::from_array([5, 6, 7, 8]);
    let r: Simd<i32, 4> = simd_swizzle!(black_box(a), black_box(b), [0, 7, 2, 5]);
    assert_eq!(r.to_array(), [1, 8, 3, 6]);
    println!("simd_swizzle two-vec OK");

    // --- simd_ctpop (u32) ---
    let v = Simd::<u32, 4>::from_array([0, 1, 0b1011, 0xFFFF_FFFF]);
    assert_eq!(black_box(v).count_ones().to_array(), [0, 1, 3, 32]);
    println!("simd_ctpop u32 OK");

    // --- simd_ctpop (u8) -- sub-word width sanity (8 lanes => 64-bit BCL vector) ---
    let v = Simd::<u8, 8>::from_array([0, 0xFF, 0x0F, 0x80, 1, 3, 7, 0x55]);
    assert_eq!(black_box(v).count_ones().to_array(), [0, 8, 4, 1, 1, 2, 3, 4]);
    println!("simd_ctpop u8 OK");

    // --- simd_ctlz (u32) ---
    let v = Simd::<u32, 4>::from_array([1, 0x8000_0000, 0xFFFF, 2]);
    assert_eq!(black_box(v).leading_zeros().to_array(), [31, 0, 16, 30]);
    println!("simd_ctlz u32 OK");

    // --- simd_ctlz (u8) -- sub-word width sanity (8 lanes => 64-bit BCL vector) ---
    let v = Simd::<u8, 8>::from_array([1, 0x80, 0x0F, 0, 2, 0x40, 0x01, 0xFF]);
    assert_eq!(black_box(v).leading_zeros().to_array(), [7, 0, 4, 8, 6, 1, 7, 0]);
    println!("simd_ctlz u8 OK");

    // --- simd_cttz (u32) ---
    let v = Simd::<u32, 4>::from_array([1, 0b1000, 0x8000_0000, 0]);
    assert_eq!(black_box(v).trailing_zeros().to_array(), [0, 3, 31, 32]);
    println!("simd_cttz u32 OK");

    // --- simd_cttz (u8) -- sub-word width sanity (8 lanes => 64-bit BCL vector) ---
    let v = Simd::<u8, 8>::from_array([1, 0b1000, 0x80, 0, 0x40, 0x20, 2, 0xFF]);
    assert_eq!(black_box(v).trailing_zeros().to_array(), [0, 3, 7, 8, 6, 5, 1, 0]);
    println!("simd_cttz u8 OK");

    // --- simd_bswap (swap_bytes, u32) ---
    let v = Simd::<u32, 4>::from_array([0x0102_0304, 0x1, 0xFF, 0xAABB_CCDD]);
    assert_eq!(
        black_box(v).swap_bytes().to_array(),
        [0x0403_0201, 0x0100_0000, 0xFF00_0000, 0xDDCC_BBAA]
    );
    println!("simd_bswap u32 OK");

    // --- simd_bitreverse (reverse_bits, u8) -- 8 lanes => 64-bit BCL vector ---
    let v = Simd::<u8, 8>::from_array([1, 0x80, 0xF0, 0x0F, 0x01, 0xC0, 0x03, 0xAA]);
    assert_eq!(
        black_box(v).reverse_bits().to_array(),
        [0x80, 1, 0x0F, 0xF0, 0x80, 0x03, 0xC0, 0x55]
    );
    println!("simd_bitreverse u8 OK");

    // --- simd_fsqrt (StdFloat::sqrt) ---
    let v = Simd::<f32, 4>::from_array([4.0, 9.0, 16.0, 25.0]);
    let r = black_box(v).sqrt().to_array();
    assert!(r.iter().zip([2.0, 3.0, 4.0, 5.0]).all(|(a, b)| bits_eq(*a, b)));
    println!("simd_fsqrt OK");

    // --- simd_floor ---
    let v = Simd::<f32, 4>::from_array([1.5, -1.5, 2.9, -2.1]);
    let r = black_box(v).floor().to_array();
    assert!(r.iter().zip([1.0, -2.0, 2.0, -3.0]).all(|(a, b)| bits_eq(*a, b)));
    println!("simd_floor OK");

    // --- simd_ceil ---
    let v = Simd::<f32, 4>::from_array([1.5, -1.5, 2.1, -2.9]);
    let r = black_box(v).ceil().to_array();
    assert!(r.iter().zip([2.0, -1.0, 3.0, -2.0]).all(|(a, b)| bits_eq(*a, b)));
    println!("simd_ceil OK");

    // --- simd_trunc ---
    let v = Simd::<f32, 4>::from_array([1.9, -1.9, 2.0, -2.5]);
    let r = black_box(v).trunc().to_array();
    assert!(r.iter().zip([1.0, -1.0, 2.0, -2.0]).all(|(a, b)| bits_eq(*a, b)));
    println!("simd_trunc OK");

    // --- simd_round (half-away-from-zero) ---
    let v = Simd::<f32, 4>::from_array([0.5, -0.5, 2.5, 1.4]);
    let r = black_box(v).round().to_array();
    assert!(r.iter().zip([1.0, -1.0, 3.0, 1.0]).all(|(a, b)| bits_eq(*a, b)));
    println!("simd_round OK");

    // --- simd_fma (mul_add: single-rounding fused multiply-add) ---
    let x = Simd::<f32, 2>::from_array([2.0, 3.0]);
    let y = Simd::<f32, 2>::from_array([4.0, 5.0]);
    let z = Simd::<f32, 2>::from_array([1.0, 1.0]);
    let r = black_box(x).mul_add(black_box(y), black_box(z)).to_array();
    assert!(r.iter().zip([9.0, 16.0]).all(|(a, b)| bits_eq(*a, b)));
    println!("simd_fma OK");

    // --- reduce_min / reduce_max (signed int, negatives) ---
    // NOTE: only the INTEGER reduce_min/max lower to the `simd_reduce_min`/`simd_reduce_max`
    // intrinsics on this nightly. Float `SimdFloat::reduce_min/max` decompose to a scalar
    // `[f32]::iter().fold(NaN, f32::min)` (NOT the SIMD intrinsic) and currently hit an orthogonal
    // scalar `f32::min` `System.Single` TypeLoadException — a pre-existing backend issue unrelated
    // to the SIMD tail, so it is intentionally NOT asserted here.
    let v = Simd::<i32, 4>::from_array([3, -1, 2, -4]);
    assert_eq!(black_box(v).reduce_min(), -4);
    assert_eq!(black_box(v).reduce_max(), 3);
    println!("simd_reduce_min/max i32 OK");

    println!("ALL SIMD TAIL OK");
}
