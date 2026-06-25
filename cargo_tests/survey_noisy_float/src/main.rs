// Survey: noisy_float::types::R64 — checked f64 wrappers (NaN-panicking floats).
// Exercises arithmetic + comparisons on FINITE values only, so no checked-panic
// path is hit. All output is derived as fixed-precision floats / ints / bools,
// so it is deterministic and byte-comparable between native rustc and the .NET backend.

use noisy_float::prelude::{r64, R64};

fn main() {
    // R64 wraps an f64 and asserts the value is finite (no NaN / Inf). We only
    // ever feed it finite operands, so construction is on the happy path.
    let a: R64 = r64(3.5);
    let b: R64 = r64(2.0);

    // --- Arithmetic (all operators forward to the inner f64, then re-check) ---
    println!("sum = {:.6}", (a + b).raw());
    println!("diff = {:.6}", (a - b).raw());
    println!("prod = {:.6}", (a * b).raw());
    println!("quot = {:.6}", (a / b).raw());
    println!("rem = {:.6}", (a % b).raw());
    println!("neg = {:.6}", (-a).raw());

    // --- Comparisons (R64 is Ord/Eq because NaN is excluded by construction) ---
    println!("a_gt_b = {}", a > b);
    println!("a_lt_b = {}", a < b);
    println!("a_eq_a = {}", a == a);
    println!("a_ne_b = {}", a != b);
    println!("a_ge_b = {}", a >= b);

    // Ordering yields a discriminant we can print deterministically.
    let ord = a.cmp(&b);
    println!("cmp_a_b = {}", match ord {
        core::cmp::Ordering::Less => "Less",
        core::cmp::Ordering::Equal => "Equal",
        core::cmp::Ordering::Greater => "Greater",
    });

    // --- min / max (Ord-based, total because there is no NaN) ---
    println!("min = {:.6}", a.min(b).raw());
    println!("max = {:.6}", a.max(b).raw());

    // --- Constructors: checked `try_new` (returns Option) vs raw accessor ---
    // Finite input -> Some; we print a marker rather than unwrapping.
    match R64::try_new(1.25) {
        Some(v) => println!("try_new_finite = {:.6}", v.raw()),
        None => println!("try_new_finite = none"),
    }
    // A non-finite input -> None on the checked path; still no panic.
    match R64::try_new(f64::INFINITY) {
        Some(v) => println!("try_new_inf = {:.6}", v.raw()),
        None => println!("try_new_inf = none"),
    }

    // --- A small deterministic reduction over a fixed slice of R64 values ---
    let xs = [r64(1.0), r64(2.5), r64(4.0), r64(0.5)];
    let mut acc = r64(0.0);
    for x in xs {
        acc = acc + x;
    }
    println!("sum_slice = {:.6}", acc.raw());

    // Const-like value derived purely from the wrapper, formatted as an int.
    let scaled = (acc * r64(2.0)).raw() as i64;
    println!("scaled_int = {}", scaled);

    println!("== survey_noisy_float done ==");
}
