#![feature(
    lang_items,
    adt_const_params,
    associated_type_defaults,
    core_intrinsics,
    unsized_const_params
)]
#![allow(internal_features, incomplete_features, unused_variables, dead_code)]
include!("../common.rs");

// Regression for typecheck-family-D (LocalAssigementWrong ppX/pX on a core::ops::IndexRange
// cursor). By-value array IntoIterator lowers to PolymorphicIter + an IndexRange cursor; the
// backend builds a PtrCast whose declared target is one indirection deeper than the local
// (ppIndexRange vs pIndexRange). PtrCast is an emit-noop, so the stored bits are the raw
// cursor pointer regardless — this asserts the runtime values match native rustc.
fn main() {
    // Sum via by-value array into_iter (drives the IndexRange cursor).
    let nums = black_box([10u32, 20, 30, 40, 50]);
    let mut total = 0u32;
    for n in nums {
        total += n;
    }
    test_eq!(total, 150);

    // enumerate() forces the cursor index to be read back each step.
    let xs = black_box([1i64, 2, 3, 4]);
    let mut acc = 0i64;
    for (i, x) in xs.into_iter().enumerate() {
        acc += (i as i64) * x;
    }
    test_eq!(acc, 20);

    // char array by-value iteration (the MaybeUninit<char> PolymorphicIter shape from std).
    let arr = black_box(['a', 'b', 'c', 'd', 'e']);
    let mut code_sum: u32 = 0;
    for c in arr {
        code_sum += c as u32;
    }
    test_eq!(code_sum, ('a' as u32) + ('b' as u32) + ('c' as u32) + ('d' as u32) + ('e' as u32));

    // map+fold over a by-value array (exercises the cursor across an adapter chain).
    let letters = black_box([1u8, 2, 3, 4, 5, 6]);
    let mapped: u32 = letters.into_iter().map(|b| (b + 1) as u32).sum();
    test_eq!(mapped, 2 + 3 + 4 + 5 + 6 + 7);
}
