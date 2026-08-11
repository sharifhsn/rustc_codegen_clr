#![feature(adt_const_params, core_intrinsics, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

include!("../common.rs");

#[inline(never)]
fn call_generic<T>(function: fn() -> T) -> T {
    function()
}

#[inline(never)]
fn produce_u32() -> u32 {
    41
}

#[inline(never)]
fn round_trip_pointer<T>(value: &T, function: fn(&T) -> *const T) -> *const T {
    function(value)
}

fn borrow_as_ptr<T>(value: &T) -> *const T {
    value
}

fn main() {
    test_eq!(black_box(call_generic::<u32>)(produce_u32), 41);

    let value = 73_u64;
    let pointer = black_box(round_trip_pointer::<u64>)(&value, borrow_as_ptr::<u64>);
    test_eq!(unsafe { *pointer }, value);
}
