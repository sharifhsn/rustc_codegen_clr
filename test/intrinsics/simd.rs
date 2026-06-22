#![feature(
    lang_items,
    adt_const_params,
    associated_type_defaults,
    core_intrinsics,
    ptr_metadata,
    unsized_const_params,
    portable_simd
)]
#![allow(internal_features, incomplete_features, unused_variables, dead_code)]
include!("../common.rs");
use core::simd::cmp::SimdOrd;
use core::simd::num::{SimdFloat, SimdInt};
use core::simd::Simd;
fn main() {
    test_eq!(
        black_box(Simd::from_array([4, 6, 8, 10])),
        Simd::from_array([4, 6, 8, 10])
    );
    let a = Simd::from_array([0, 1, 2, 3]);
    let b = Simd::from_array([4, 5, 6, 7]);
    test_eq!(a + b, Simd::from_array([4, 6, 8, 10]));
    let a = Simd::from_array([4, 5, 6, 7]);
    let b = Simd::from_array([0, 1, 2, 3]);
    test_eq!(a - b, Simd::from_array([4, 4, 4, 4]));
    // Tier-1 element-wise value ops: mul (BCL), div/xor/shl/shr (per-lane), and/or (BCL).
    let a = Simd::from_array([2, 3, 4, 5]);
    let b = Simd::from_array([1, 1, 2, 5]);
    test_eq!(a * b, Simd::from_array([2, 3, 8, 25]));
    test_eq!(a / b, Simd::from_array([2, 3, 2, 1]));
    let a = Simd::from_array([0b1100u32, 0b1010, 0xFF, 0]);
    let b = Simd::from_array([0b1010u32, 0b0110, 0x0F, 7]);
    test_eq!(a ^ b, Simd::from_array([0b0110u32, 0b1100, 0xF0, 7]));
    test_eq!(a & b, Simd::from_array([0b1000u32, 0b0010, 0x0F, 0]));
    test_eq!(a | b, Simd::from_array([0b1110u32, 0b1110, 0xFF, 7]));
    let v = Simd::from_array([1u32, 2, 3, 4]);
    let sh = Simd::from_array([1u32, 2, 3, 0]);
    test_eq!(v << sh, Simd::from_array([2u32, 8, 24, 4]));
    let v = Simd::from_array([16u32, 16, 16, 16]);
    let sh = Simd::from_array([1u32, 2, 3, 4]);
    test_eq!(v >> sh, Simd::from_array([8u32, 4, 2, 1]));
    // Per-lane numeric cast (`simd_as`): i32 -> f32. (`cast` internally uses `simd_select`.)
    let f = Simd::from_array([1i32, 2, 3, 4]);
    let g: Simd<f32, 4> = f.cast();
    test_eq!(g, Simd::from_array([1.0f32, 2.0, 3.0, 4.0]));
    // Tier-2 horizontal reductions (integer).
    let v = Simd::from_array([1i32, 2, 3, 4]);
    test_eq!(v.reduce_sum(), 10);
    test_eq!(v.reduce_product(), 24);
    test_eq!(v.reduce_max(), 4);
    test_eq!(v.reduce_min(), 1);
    test_eq!(v.reduce_and(), 0);
    test_eq!(v.reduce_or(), 7);
    test_eq!(v.reduce_xor(), 4);
    // Reductions (float, ordered add via `simd_reduce_add_ordered`).
    // NOTE: `SimdFloat::reduce_max/min` deliberately use a *scalar* `f32::max` fold (for NaN
    // semantics), not the `simd_reduce_max` intrinsic, so they exercise core's float-closure path
    // (a separate, pre-existing concern) rather than this SIMD work — not tested here.
    let fv = Simd::from_array([1.0f32, 2.0, 3.0, 4.0]);
    test_eq!(fv.reduce_sum(), 10.0);
    // Element-wise min/max lower to `simd_lt`/`simd_gt` + `simd_select`.
    let a = Simd::from_array([1i32, 5, 3, 8]);
    let b = Simd::from_array([4i32, 2, 6, 7]);
    test_eq!(a.simd_min(b), Simd::from_array([1i32, 2, 3, 7]));
    test_eq!(a.simd_max(b), Simd::from_array([4i32, 5, 6, 8]));
}
