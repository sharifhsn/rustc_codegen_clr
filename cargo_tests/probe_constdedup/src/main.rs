// Read-only static content-merging: two references to the same promoted const must be ptr::eq
// (matches native LLVM constant-merging), and Waker::will_wake (which compares vtable addresses)
// must hold across a clone.
use std::task::{RawWaker, RawWakerVTable, Waker};

static V: RawWakerVTable = RawWakerVTable::new(|_| RAW, |_| {}, |_| {}, |_| {});
const RAW: RawWaker = RawWaker::new(std::ptr::null(), &V);

fn vt_ref_a() -> &'static RawWakerVTable { &V }
fn vt_ref_b() -> &'static RawWakerVTable { &V }

// two independent promotions of an identical anonymous const
fn arr_a() -> &'static [u8; 4] { &[1, 2, 3, 4] }
fn arr_b() -> &'static [u8; 4] { &[1, 2, 3, 4] }
// a DIFFERENT const must stay distinct
fn arr_c() -> &'static [u8; 4] { &[9, 9, 9, 9] }

fn main() {
    // 1. identical read-only byte arrays promoted at two sites -> same address
    let (a, b, c) = (arr_a(), arr_b(), arr_c());
    println!("arr identical eq: {} (want true)", std::ptr::eq(a, b));
    println!("arr different eq: {} (want false)", std::ptr::eq(a, c));
    assert!(std::ptr::eq(a, b), "identical read-only arrays did not merge");
    assert!(!std::ptr::eq(a, c), "DIFFERENT read-only arrays wrongly merged");

    // 2. vtable with relocations (fn pointers) -> same address from two sites
    println!("vtable eq: {} (want true)", std::ptr::eq(vt_ref_a(), vt_ref_b()));
    assert!(std::ptr::eq(vt_ref_a(), vt_ref_b()), "identical vtable did not merge");

    // 3. the real Waker::will_wake across a clone
    let w = unsafe { Waker::from_raw(RAW) };
    let w2 = w.clone();
    println!("will_wake: {} (want true)", w.will_wake(&w2));
    assert!(w.will_wake(&w2), "will_wake false after clone");
    println!("constdedup ok");
}
