// Reproduction probe for the ThinBox / WithHeader::drop type-verifier reject (I2-at-scale).
//
// `ThinBox::<T>::drop` calls `WithHeader::drop::<T>(value: *mut T)`; the CIL type-verifier rejects
// the call with `CallArgTypeWrong { got: **T, expected: *T }` — the value pushed is the correct
// single pointer, but its IR *type* is one indirection too deep. This mirrors the failing
// rust-lang/rust alloctests `thin_box.rs` `align1big` case (Align1Big([u8; 256])).
//
// Built via build-std so the `alloc` monomorphizations are produced in-crate (the full-crate
// inlining condition under which the bug surfaces). Run under DUMP_FN to dump the offending method.
#![feature(thin_box)]
#![allow(dead_code)]
use std::boxed::ThinBox;
use std::fmt::Debug;

#[track_caller]
fn verify_aligned<T>(ptr: *const T) {
    let ptr = std::hint::black_box(ptr);
    assert!(ptr.is_aligned() && !ptr.is_null(), "misaligned ThinBox data");
}

#[track_caller]
fn check_thin_sized<T: Debug + PartialEq + Clone>(make: impl FnOnce() -> T) {
    let value = make();
    let boxed = ThinBox::new(value.clone());
    let val = &*boxed;
    verify_aligned(val as *const T);
    assert_eq!(val, &value);
}

#[track_caller]
fn check_thin_dyn<T: Debug + PartialEq + Clone>(make: impl FnOnce() -> T) {
    let value = make();
    let wanted_debug = format!("{value:?}");
    let boxed: ThinBox<dyn Debug> = ThinBox::new_unsize(value.clone());
    let val = &*boxed;
    verify_aligned(val as *const dyn Debug as *const T);
    let got_debug = format!("{val:?}");
    assert_eq!(wanted_debug, got_debug);
}

#[derive(Debug, PartialEq, Clone)]
struct Align1Big([u8; 256]);

#[derive(Debug, PartialEq, Clone)]
struct Align1Small(u8);

#[derive(Debug, PartialEq, Clone)]
#[repr(align(64))]
struct Align64NotPow2Size([u8; 79]);

fn main() {
    check_thin_sized(|| Align1Big([5u8; 256]));
    check_thin_dyn(|| Align1Big([5u8; 256]));
    check_thin_sized(|| Align1Small(50));
    check_thin_dyn(|| Align1Small(50));
    check_thin_sized(|| Align64NotPow2Size([100; 79]));
    check_thin_dyn(|| Align64NotPow2Size([100; 79]));
    println!("thinbox ok");
}
