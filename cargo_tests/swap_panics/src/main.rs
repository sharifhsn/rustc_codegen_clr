// slice::swap out-of-bounds panic-message codegen repro.
// Mirrors upstream coretests slice::swap_panics::{index_a_equals_len, index_b_equals_len,
// index_a_greater_than_len, index_b_greater_than_len}. Each #[should_panic] asserts the
// EXACT panic message: e.g. swap(4,2) on a len-4 slice -> "the len is 4 but the index is 4".
// Verifying the message requires the bounds-check codegen to format the correct index/len.
use std::panic;

fn expect_panic(label: &str, expected_substr: &str, f: impl FnOnce() + panic::UnwindSafe) {
    let prev = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let res = panic::catch_unwind(f);
    panic::set_hook(prev);
    match res {
        Ok(()) => panic!("MISMATCH {label}: expected panic, none occurred"),
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            println!("{label}: got=[{msg}] want-substr=[{expected_substr}]");
            assert!(
                msg.contains(expected_substr),
                "MISMATCH {label}: [{msg}] !contains [{expected_substr}]"
            );
        }
    }
}

fn main() {
    expect_panic(
        "a_equals_len",
        "the len is 4 but the index is 4",
        || {
            let mut x = ["a", "b", "c", "d"];
            x.swap(4, 2);
        },
    );
    expect_panic(
        "b_equals_len",
        "the len is 4 but the index is 4",
        || {
            let mut x = ["a", "b", "c", "d"];
            x.swap(2, 4);
        },
    );
    expect_panic(
        "a_greater_than_len",
        "the len is 4 but the index is 5",
        || {
            let mut x = ["a", "b", "c", "d"];
            x.swap(5, 2);
        },
    );
    expect_panic(
        "b_greater_than_len",
        "the len is 4 but the index is 5",
        || {
            let mut x = ["a", "b", "c", "d"];
            x.swap(2, 5);
        },
    );
    println!("swap_panics OK");
}
