#![feature(
    lang_items,
    adt_const_params,
    associated_type_defaults,
    core_intrinsics,
    let_chains,
    never_type,
    portable_simd,
    unsized_const_params,
    pointer_is_aligned_to
)]
#![allow(internal_features, incomplete_features, unused_variables, dead_code)]

include!("../common.rs");

#[inline(never)]
fn sum(values: &[u32]) -> u32 {
    values.iter().copied().sum()
}

fn main() {
    use core::simd::Simd;

    test_eq!(core::mem::size_of::<[u32; 32]>(), 128);
    test_eq!(core::mem::align_of::<[u32; 32]>(), 4);
    test_eq!(core::mem::size_of::<Simd<u32, 32>>(), 128);
    test_eq!(core::mem::align_of::<Simd<u32, 32>>(), 128);

    let plain = [3_u32; 32];
    // A 1024-bit SIMD value is wider than the managed Vector512 surface and therefore uses the
    // fixed-array fallback. It has the same U32 element/count/size as `plain`, but Rust requires
    // 128-byte alignment instead of 4-byte alignment. Those must be distinct synthetic CLR types.
    let wide = Simd::<u32, 32>::from_array([5_u32; 32]);
    let wide_array = wide.to_array();

    test_eq!(sum(&plain), 96);
    test_eq!(sum(&wide_array), 160);
    test_eq!(wide_array[0], 5);
    test_eq!(wide_array[31], 5);
}
