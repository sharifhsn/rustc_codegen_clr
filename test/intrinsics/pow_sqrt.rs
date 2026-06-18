#![feature(
    lang_items,
    adt_const_params,
    associated_type_defaults,
    core_intrinsics,
    unsized_const_params
)]
#![allow(internal_features, incomplete_features, unused_variables, dead_code)]
include!("../common.rs");
extern crate core;

// use core::intrinsics::sqrtf32;
// This intrinsic is already imported in common.rs.
use core::intrinsics::exp2f32;
use core::intrinsics::exp2f64;
use core::intrinsics::powf32;
use core::intrinsics::powf64;
use core::intrinsics::powif32;
use core::intrinsics::powif64;
use core::intrinsics::sqrtf64;

use core::intrinsics::fabs;

fn main() {
    let positive = 4.0_f32;
    let negative = -4.0_f32;
    let negative_zero = -0.0_f32;

    test_eq!(unsafe { sqrtf32(positive) }, black_box(2.0));
    test!(unsafe { sqrtf32(negative) }.is_nan());
    test_eq!(unsafe { sqrtf32(negative_zero) }, black_box(negative_zero));

    let positive = 4.0_f64;
    let negative = -4.0_f64;
    let negative_zero = -0.0_f64;

    test_eq!(unsafe { sqrtf64(positive) }, black_box(2.0));
    test!(unsafe { sqrtf64(negative) }.is_nan());
    test_eq!(unsafe { sqrtf64(negative_zero) }, black_box(negative_zero));

    let x = 2.0_f32;
    let abs_difference = unsafe { fabs(powf32(x, 2.0) - (x * x)) };
    test!(abs_difference <= black_box(f32::EPSILON));
    let x = 2.0_f64;
    let abs_difference = unsafe { fabs(powf64(x, 2.0) - (x * x)) };
    test!(abs_difference <= black_box(f64::EPSILON));
    let x = 2.0_f32;
    let abs_difference = unsafe { fabs(powif32(x, 2) - (x * x)) };
    test!(abs_difference <= black_box(f32::EPSILON));
    let x = 2.0_f64;
    let abs_difference = unsafe { fabs(powif64(x, 2) - (x * x)) };
    test!(abs_difference <= black_box(f64::EPSILON));

    let f = 2.0f32;
    // 2^2 - 4 == 0
    let abs_difference = unsafe { fabs(exp2f32(f) - 4.0) };
    test!(abs_difference <= black_box(f32::EPSILON));
    let f = 2.0f64;
    // 2^2 - 4 == 0
    let abs_difference = unsafe { fabs(exp2f64(f) - 4.0) };
    test!(abs_difference <= black_box(f64::EPSILON));
}
