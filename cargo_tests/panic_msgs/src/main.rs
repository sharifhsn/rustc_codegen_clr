// Checked-failure panic-message codegen repro for the `Assert` terminator family.
// Each case must produce the EXACT native Rust panic message and be catchable (unwind),
// NOT crash with "missing method abort". This covers the BROKEN_TESTS.md slice/vec
// panic-message cluster (swap_panics, vec::test_index_out_of_bounds) and the broader
// checked-arithmetic asserts (overflow / div-by-zero / remainder-by-zero) that previously
// all routed through the surrogate `assert_*` -> unbodied `abort` crash path.
//
// overflow-checks are forced ON in both profiles (see Cargo.toml) so the Overflow /
// OverflowNeg asserts are actually emitted.
use std::hint::black_box;
use std::panic;

fn cap(label: &str, expected_substr: &str, f: impl FnOnce() + panic::UnwindSafe) {
    let prev = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let res = panic::catch_unwind(f);
    panic::set_hook(prev);
    match res {
        Ok(()) => panic!("MISMATCH {label}: expected a panic, got none"),
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            println!("{label}: [{msg}]");
            assert!(
                msg.contains(expected_substr),
                "MISMATCH {label}: [{msg}] does not contain [{expected_substr}]"
            );
        }
    }
}

fn main() {
    // BoundsCheck (index, len) -> panic_bounds_check
    cap("oob_index", "index out of bounds: the len is 3 but the index is 7", || {
        let v = [1, 2, 3];
        let i = black_box(7usize);
        let _ = black_box(v[i]);
    });
    cap("swap_oob", "index out of bounds: the len is 4 but the index is 4", || {
        let mut x = ["a", "b", "c", "d"];
        x.swap(black_box(4), black_box(2));
    });
    // DivisionByZero -> panic_div_zero
    cap("div_zero", "attempt to divide by zero", || {
        let d = black_box(0i32);
        let _ = black_box(black_box(5i32) / d);
    });
    // RemainderByZero -> panic_rem_zero
    cap("rem_zero", "attempt to calculate the remainder with a divisor of zero", || {
        let d = black_box(0i32);
        let _ = black_box(black_box(5i32) % d);
    });
    // Overflow(Add) -> panic_add_overflow (overflow-checks ON)
    cap("add_overflow", "attempt to add with overflow", || {
        let a = black_box(u8::MAX);
        let _ = black_box(a + black_box(1u8));
    });
    // Overflow(Mul) -> panic_mul_overflow
    cap("mul_overflow", "attempt to multiply with overflow", || {
        let a = black_box(i32::MAX);
        let _ = black_box(a * black_box(2i32));
    });
    // OverflowNeg -> panic_neg_overflow
    cap("neg_overflow", "attempt to negate with overflow", || {
        let a = black_box(i32::MIN);
        let _ = black_box(-a);
    });

    println!("panic_msgs OK");
}
