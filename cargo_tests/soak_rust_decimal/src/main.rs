// Soak test for `rust_decimal` (i128-backed fixed-point Decimal).
// Exercises 128-bit integer codegen via parse / add / mul / div / round_dp / scale / to_string.
// All output is exact decimal text (no binary floats), so it is deterministic run-to-run.

use rust_decimal::Decimal;
use std::str::FromStr;

// Parse helper that never panics: prints a deterministic marker on failure.
fn parse(s: &str) -> Decimal {
    match Decimal::from_str(s) {
        Ok(d) => d,
        Err(_) => {
            println!("parse_error = {}", s);
            Decimal::ZERO
        }
    }
}

fn main() {
    // --- Parsing (from_str) ---
    let pi = parse("3.14159");
    let a = parse("1234.5678");
    let b = parse("0.0001");
    let big = parse("79228162514264337593543950335"); // Decimal::MAX mantissa, scale 0
    println!("pi = {}", pi);
    println!("a = {}", a);
    println!("b = {}", b);
    println!("big = {}", big);

    // --- scale() round-trips (the fractional-digit count) ---
    println!("pi_scale = {}", pi.scale());
    println!("a_scale = {}", a.scale());
    println!("b_scale = {}", b.scale());
    println!("big_scale = {}", big.scale());

    // --- Addition ---
    let sum = a + b;
    println!("sum = {}", sum);

    // --- Multiplication (grows the 128-bit mantissa) ---
    let prod = pi * a;
    println!("prod = {}", prod);

    // checked_mul to avoid any overflow panic path on the big value
    match big.checked_mul(parse("2")) {
        Some(v) => println!("big_times_2 = {}", v),
        None => println!("big_times_2 = overflow"),
    }

    // --- Division (non-terminating -> rust_decimal truncates to 28 sig digits, deterministic) ---
    let ten = parse("10");
    let three = parse("3");
    match ten.checked_div(three) {
        Some(q) => println!("ten_div_three = {}", q),
        None => println!("ten_div_three = div_error"),
    }
    let quot = a / pi;
    println!("a_div_pi = {}", quot);

    // --- round_dp (banker's rounding to N decimal places) ---
    println!("pi_round_2 = {}", pi.round_dp(2));
    println!("pi_round_4 = {}", pi.round_dp(4));
    println!("quot_round_6 = {}", quot.round_dp(6));
    // Half-to-even boundary cases (deterministic by definition).
    println!("round_2_5 = {}", parse("2.5").round_dp(0));
    println!("round_3_5 = {}", parse("3.5").round_dp(0));
    println!("round_125 = {}", parse("0.125").round_dp(2));

    // --- Negative values + comparisons (sign bit on the i128 representation) ---
    let neg = parse("-42.500");
    println!("neg = {}", neg);
    println!("neg_abs = {}", neg.abs());
    println!("neg_lt_zero = {}", neg < Decimal::ZERO);
    println!("neg_normalized = {}", neg.normalize()); // strips trailing zeros

    // --- to_string of a few derived values (exercise Display formatting) ---
    let total = pi + a + b + neg;
    println!("total = {}", total);
    println!("total_round_3 = {}", total.round_dp(3));

    // --- A small deterministic accumulation loop (repeated 128-bit adds/muls) ---
    let mut acc = Decimal::ZERO;
    let step = parse("0.01");
    let mut i = 0u32;
    while i < 100 {
        acc += step;
        i += 1;
    }
    println!("acc_100x_0.01 = {}", acc); // exactly 1.00 (no float drift)
    println!("acc_eq_one = {}", acc == parse("1.00"));

    // mantissa() returns the i128 backing integer — direct 128-bit value print.
    println!("a_mantissa = {}", a.mantissa());
    println!("big_mantissa = {}", big.mantissa());

    println!("== soak_rust_decimal done ==");
}
