//! Stable regression fixture for the complete 16-bit integer atomic RMW matrix.

use std::sync::atomic::{AtomicI16, AtomicU16, Ordering::SeqCst};

unsafe extern "C" {
    fn printf(format: *const core::ffi::c_char, ...) -> core::ffi::c_int;
}

fn check_eq<T: PartialEq>(actual: T, expected: T) {
    if actual != expected {
        unsafe {
            printf(c"atomic_u16_i16_rmw: mismatch\n".as_ptr());
        }
        panic!("atomic_u16_i16_rmw mismatch");
    }
}

fn check_u16() {
    let value = AtomicU16::new(10);

    check_eq(value.fetch_add(5, SeqCst), 10);
    check_eq(value.load(SeqCst), 15);
    check_eq(value.fetch_sub(3, SeqCst), 15);
    check_eq(value.load(SeqCst), 12);

    value.store(0x00f0, SeqCst);
    check_eq(value.fetch_or(0x0f00, SeqCst), 0x00f0);
    check_eq(value.load(SeqCst), 0x0ff0);
    check_eq(value.fetch_xor(0x00ff, SeqCst), 0x0ff0);
    check_eq(value.load(SeqCst), 0x0f0f);
    check_eq(value.fetch_and(0x0ff0, SeqCst), 0x0f0f);
    check_eq(value.load(SeqCst), 0x0f00);

    value.store(10, SeqCst);
    check_eq(value.fetch_nand(12, SeqCst), 10);
    check_eq(value.load(SeqCst), !(10 & 12));
    value.store(10, SeqCst);
    check_eq(value.fetch_min(7, SeqCst), 10);
    check_eq(value.load(SeqCst), 7);
    check_eq(value.fetch_max(13, SeqCst), 7);
    check_eq(value.load(SeqCst), 13);
}

fn check_i16() {
    let value = AtomicI16::new(10);

    check_eq(value.fetch_add(5, SeqCst), 10);
    check_eq(value.load(SeqCst), 15);
    check_eq(value.fetch_sub(3, SeqCst), 15);
    check_eq(value.load(SeqCst), 12);

    value.store(0x00f0, SeqCst);
    check_eq(value.fetch_or(0x0f00, SeqCst), 0x00f0);
    check_eq(value.load(SeqCst), 0x0ff0);
    check_eq(value.fetch_xor(0x00ff, SeqCst), 0x0ff0);
    check_eq(value.load(SeqCst), 0x0f0f);
    check_eq(value.fetch_and(0x0ff0, SeqCst), 0x0f0f);
    check_eq(value.load(SeqCst), 0x0f00);

    value.store(10, SeqCst);
    check_eq(value.fetch_nand(12, SeqCst), 10);
    check_eq(value.load(SeqCst), !(10 & 12));
    value.store(10, SeqCst);
    check_eq(value.fetch_min(-7, SeqCst), 10);
    check_eq(value.load(SeqCst), -7);
    check_eq(value.fetch_max(13, SeqCst), -7);
    check_eq(value.load(SeqCst), 13);
}

fn main() {
    check_u16();
    check_i16();
    unsafe {
        printf(c"atomic_u16_i16_rmw: all checks passed\n".as_ptr());
    }
}
