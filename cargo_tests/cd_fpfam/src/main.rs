// Differential repro for the named typecheck false-positive families:
//   Family B: WriteWrongAddr void-erased-address StInd  (Atomic<usize>/Cell<usize>)
//   Family C: CallArgTypeWrong catch_unwind got:*Data<dyn Fn> expected:*u8
//   Family A: WriteWrongAddr ppX/X extra-indirection StInd (MaybeUninit, byte stores)
//
// If any of these is a REAL miscompile, the backend output will diverge from
// native Rust. If they are checker false-positives, output is identical.

use std::cell::Cell;
use std::hint::black_box;
use std::mem::MaybeUninit;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};

fn family_b_atomics() -> usize {
    let a = AtomicUsize::new(0);
    a.store(black_box(40), Ordering::SeqCst);
    let prev = a.fetch_add(black_box(2), Ordering::SeqCst);
    let swapped = a.swap(black_box(99), Ordering::SeqCst);
    let loaded = a.load(Ordering::SeqCst);
    // prev=40, swapped=42, loaded=99
    let c: Cell<usize> = Cell::new(0);
    c.set(black_box(7));
    let cell = c.get();
    prev + swapped + loaded + cell // 40+42+99+7 = 188
}

fn family_a_maybeuninit() -> u64 {
    // Exercises byte/scalar stores through extra pointer-indirection layouts.
    let mut buf: [MaybeUninit<u8>; 8] = unsafe { MaybeUninit::uninit().assume_init() };
    for (i, slot) in buf.iter_mut().enumerate() {
        slot.write(black_box((i as u8).wrapping_mul(3).wrapping_add(1)));
    }
    let bytes: [u8; 8] = unsafe { std::mem::transmute(buf) };
    u64::from_le_bytes(bytes)
}

fn family_c_catch_unwind() -> i32 {
    // Plain closure returning normally.
    let r1 = panic::catch_unwind(|| black_box(315));
    // Closure capturing a heap value (Vec) -> data ptr is *Data<dyn Fn> in MIR.
    let v = vec![1usize, 2, 3, 4];
    let r2 = panic::catch_unwind(AssertUnwindSafe(|| {
        black_box(v.iter().sum::<usize>())
    }));
    let a = r1.unwrap_or(-1);
    let b = r2.map(|x| x as i32).unwrap_or(-1);
    a + b // 315 + 10 = 325
}

fn main() {
    let b = family_b_atomics();
    let a = family_a_maybeuninit();
    let c = family_c_catch_unwind();
    println!("family_b atomics/cell: {b}");
    println!("family_a maybeuninit u64: {a}");
    println!("family_c catch_unwind: {c}");
    assert_eq!(b, 188);
    assert_eq!(a, u64::from_le_bytes([1, 4, 7, 10, 13, 16, 19, 22]));
    assert_eq!(c, 325);
    println!("cd_fpfam: done");
    println!("== cd_fpfam done ==");
}
