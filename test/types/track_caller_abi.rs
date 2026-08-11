#![feature(adt_const_params, core_intrinsics, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

include!("../common.rs");

#[derive(Clone, Copy)]
struct Zst;

#[track_caller]
#[inline(never)]
fn tracked(_: Zst, value: u32) -> (u32, u32) {
    (
        core::panic::Location::caller().line(),
        value.wrapping_add(7),
    )
}

static TRACKED_POINTER: fn(Zst, u32) -> (u32, u32) = tracked;

fn main() {
    let (direct_line, (reported_line, direct_value)) = (line!(), tracked(Zst, 11));
    test_eq!(reported_line, direct_line);
    test_eq!(direct_value, 18);

    let local_pointer: fn(Zst, u32) -> (u32, u32) = tracked;
    let (local_line, local_value) = black_box(local_pointer)(Zst, 13);
    test!(local_line != 0);
    test_eq!(local_value, 20);

    let (static_line, static_value) = black_box(TRACKED_POINTER)(Zst, 17);
    test!(static_line != 0);
    test_eq!(static_value, 24);
}
