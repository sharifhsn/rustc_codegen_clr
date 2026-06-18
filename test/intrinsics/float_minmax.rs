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

// Verification harness for the Rust<->.NET float min/max/abs mapping documented in
// docs/semantics_mapping.md. Calls the intrinsics the codegen now dispatches directly,
// and checks against ground truth captured from native Rust. Raw bits (`to_bits`) are
// used wherever a sign of zero or a NaN distinction matters.
use core::intrinsics::{
    fabs, maximum_number_nsz_f32, maximum_number_nsz_f64, maximumf32, maximumf64,
    minimum_number_nsz_f32, minimum_number_nsz_f64, minimumf32, minimumf64,
};

const PZERO32: u32 = 0x0000_0000; // +0.0
const NZERO32: u32 = 0x8000_0000; // -0.0
const PZERO64: u64 = 0x0000_0000_0000_0000;
const NZERO64: u64 = 0x8000_0000_0000_0000;

fn main() {
    let nan = black_box(f32::NAN);
    let one = black_box(1.0f32);
    let two = black_box(2.0f32);
    let pinf = black_box(f32::INFINITY);
    let ninf = black_box(f32::NEG_INFINITY);
    let pz = black_box(0.0f32);
    let nz = black_box(-0.0f32);

    // maxNum / minNum (== f32::max / f32::min): NaN is IGNORED, the number is returned.
    test_eq!(maximum_number_nsz_f32(nan, one).to_bits(), one.to_bits());
    test_eq!(maximum_number_nsz_f32(one, nan).to_bits(), one.to_bits());
    test_eq!(minimum_number_nsz_f32(nan, one).to_bits(), one.to_bits());
    test!(maximum_number_nsz_f32(nan, nan).is_nan()); // both NaN -> NaN
    test_eq!(maximum_number_nsz_f32(one, two), 2.0);
    test_eq!(minimum_number_nsz_f32(one, two), 1.0);
    // nsz: the *sign* of a zero result is unspecified, so only assert it is zero-valued.
    test!(maximum_number_nsz_f32(pz, nz) == 0.0);
    test!(minimum_number_nsz_f32(pz, nz) == 0.0);

    // IEEE maximum / minimum (== f32::maximum / minimum): NaN is PROPAGATED.
    test!(maximumf32(nan, one).is_nan());
    test!(maximumf32(one, nan).is_nan());
    test!(minimumf32(nan, one).is_nan());
    test_eq!(maximumf32(one, two), 2.0);
    test_eq!(minimumf32(one, two), 1.0);
    // IEEE: signed zero IS ordered, -0 < +0.
    test_eq!(maximumf32(pz, nz).to_bits(), PZERO32);
    test_eq!(maximumf32(nz, pz).to_bits(), PZERO32);
    test_eq!(minimumf32(pz, nz).to_bits(), NZERO32);
    // infinities
    test_eq!(maximumf32(pinf, one), f32::INFINITY);
    test_eq!(minimumf32(ninf, one), f32::NEG_INFINITY);

    // abs (now the generic `fabs` intrinsic): |-0| = +0, |-inf| = +inf, |NaN| = NaN.
    test_eq!(fabs(nz).to_bits(), PZERO32);
    test_eq!(fabs(ninf), f32::INFINITY);
    test!(fabs(nan).is_nan());
    test_eq!(fabs(black_box(-1.0f32)), 1.0);

    // f64 spot-checks of each family.
    test_eq!(maximum_number_nsz_f64(black_box(f64::NAN), black_box(1.0f64)).to_bits(), 1.0f64.to_bits());
    test!(maximumf64(black_box(f64::NAN), black_box(1.0f64)).is_nan());
    test_eq!(maximumf64(black_box(0.0f64), black_box(-0.0f64)).to_bits(), PZERO64);
    test_eq!(minimumf64(black_box(0.0f64), black_box(-0.0f64)).to_bits(), NZERO64);
    test_eq!(fabs(black_box(-0.0f64)).to_bits(), PZERO64);
    test_eq!(fabs(black_box(-2.0f64)), 2.0);
}
