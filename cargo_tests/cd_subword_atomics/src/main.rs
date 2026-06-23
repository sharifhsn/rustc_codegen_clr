//! Sub-word atomic CAS/swap regression guard (.NET 8 emulation path).
//! Each op routes through `atomic_cmpxchng{8,16}_correct` / `atomic_xchng{8,16}_correct`,
//! which previously crashed with `InvalidProgramException`. Output must be byte-identical
//! to native rustc; the asserts also make it self-checking.
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{AtomicBool, AtomicI16, AtomicI8, AtomicU16, AtomicU8};

fn main() {
    // --- u8 compare_exchange (success + failure-no-write) ---
    let a = AtomicU8::new(5);
    assert_eq!(a.compare_exchange(5, 9, SeqCst, SeqCst), Ok(5));
    assert_eq!(a.load(SeqCst), 9);
    assert_eq!(a.compare_exchange(5, 7, SeqCst, SeqCst), Err(9)); // mismatch: no write
    assert_eq!(a.load(SeqCst), 9);

    // --- u8 swap ---
    assert_eq!(a.swap(42, SeqCst), 9);
    assert_eq!(a.load(SeqCst), 42);

    // --- i8 (signed sub-word) ---
    let i = AtomicI8::new(-5);
    assert_eq!(i.compare_exchange(-5, -1, SeqCst, SeqCst), Ok(-5));
    assert_eq!(i.swap(127, SeqCst), -1);
    assert_eq!(i.load(SeqCst), 127);

    // --- u16 / i16 ---
    let u = AtomicU16::new(0xBEEF);
    assert_eq!(u.compare_exchange(0xBEEF, 0x1234, SeqCst, SeqCst), Ok(0xBEEF));
    assert_eq!(u.swap(0xFFFF, SeqCst), 0x1234);
    let s = AtomicI16::new(-1000);
    assert_eq!(s.compare_exchange(-1000, 1000, SeqCst, SeqCst), Ok(-1000));
    assert_eq!(s.load(SeqCst), 1000);

    // --- bool (1-byte) CAS ---
    let b = AtomicBool::new(false);
    assert_eq!(b.compare_exchange(false, true, SeqCst, SeqCst), Ok(false));
    assert_eq!(b.compare_exchange(false, true, SeqCst, SeqCst), Err(true));
    assert!(b.load(SeqCst));

    // --- the transitive case: catch_unwind exercises a static Atomic<u8> CAS internally.
    // A *non-panicking* closure still drives the panic-count `compare_exchange`, so it
    // exercises the same sub-word-CAS helper without pulling in the orthogonal
    // panic-message-routing divergence (panic text → stdout vs native stderr).
    let caught: Result<i32, _> = std::panic::catch_unwind(|| std::hint::black_box(315));
    assert_eq!(caught.unwrap(), 315);

    println!("cd_subword_atomics: all checks passed");
}
