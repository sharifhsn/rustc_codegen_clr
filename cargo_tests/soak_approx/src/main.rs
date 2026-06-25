//! H2 real-crate SOAK: `approx` doing float comparisons on the dotnet PAL.
//! Exercises relative_eq!/abs_diff_eq!/ulps_eq! plus the trait methods on f64.
//! Panic-safe: no unwraps/indexing; only prints bools and counts.
//! SUCCESS = "== soak_approx done ==" with sane values.
use approx::{abs_diff_eq, relative_eq, ulps_eq};

fn main() {
    println!("== soak_approx start ==");

    // Basic relative equality on values that differ by rounding noise.
    let a: f64 = 0.1 + 0.2;
    let b: f64 = 0.3;
    println!("1  relative_eq(0.1+0.2, 0.3) = {}", relative_eq!(a, b));
    println!("2  abs_diff_eq(0.1+0.2, 0.3) = {}", abs_diff_eq!(a, b));

    // Clearly unequal values.
    println!("3  relative_eq(1.0, 2.0) = {}", relative_eq!(1.0_f64, 2.0_f64));
    println!("4  abs_diff_eq(1.0, 2.0) = {}", abs_diff_eq!(1.0_f64, 2.0_f64));

    // With explicit epsilon / max_relative.
    println!(
        "5  abs_diff_eq epsilon=0.5 (1.0,1.4) = {}",
        abs_diff_eq!(1.0_f64, 1.4_f64, epsilon = 0.5)
    );
    println!(
        "6  relative_eq max_relative=0.1 (100.0,105.0) = {}",
        relative_eq!(100.0_f64, 105.0_f64, max_relative = 0.1)
    );

    // ULPs-based comparison.
    let c: f64 = 1.0;
    let d: f64 = 1.0 + f64::EPSILON;
    println!("7  ulps_eq(1.0, 1.0+EPSILON) = {}", ulps_eq!(c, d));

    // Sweep a small range and count how many are "close" to their rounded value.
    let mut close = 0u32;
    for i in 0..10u32 {
        let x = (i as f64) * 0.1;
        let rounded = (x * 10.0).round() / 10.0;
        if relative_eq!(x, rounded, max_relative = 1e-9) {
            close += 1;
        }
    }
    println!("8  close-to-rounded count (of 10) = {close}");

    // f32 path too.
    let e: f32 = 0.1 + 0.2;
    let f: f32 = 0.3;
    println!("9  relative_eq f32 (0.1+0.2, 0.3) = {}", relative_eq!(e, f));

    println!("== soak_approx done ==");
}
