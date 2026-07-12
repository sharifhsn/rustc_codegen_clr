#![feature(
    adt_const_params,
    unsized_const_params,
    core_intrinsics,
    lang_items
)]
#![allow(
    unused_variables,
    incomplete_features,
    unused_imports,
    dead_code,
    internal_features
)]
include!("../common.rs");
unsafe extern "C" {
    fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32;
}
use core::mem::MaybeUninit;
fn main() {
    let mut dst = [MaybeUninit::new(255); 64];
    let expect = [0; 64];

    dst.fill(MaybeUninit::new(0));
    let initialized = unsafe { &*(&dst as *const _ as *const [i32; 64]) };
    assert_eq!(initialized, &expect);
}
