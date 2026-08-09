//! Runnable acceptance binary. No application module uses `unsafe` or raw ABI values.

use pinvoke_safe_rust_facade::{describe, increment, running_totals, sum_squares};

fn main() {
    let values = [2, -3, 6];
    assert_eq!(sum_squares(&values), Ok(49));

    let mut mutable = [4, 0, -2];
    increment(&mut mutable).expect("native increment should succeed");
    assert_eq!(mutable, [5, 1, -1]);

    let mut overflow = [i32::MAX];
    assert_eq!(increment(&mut overflow).unwrap_err().code(), 1);
    assert_eq!(overflow, [i32::MAX]);

    assert_eq!(
        describe("managed Rust 🦀", &values).as_deref(),
        Ok("managed Rust 🦀: count=3, sum=5")
    );
    assert_eq!(running_totals(&values), Ok(vec![2, -1, 5]));

    println!(
        "Safe Rust P/Invoke acceptance OK: slices, UTF-8, String, Vec, Result"
    );
}
