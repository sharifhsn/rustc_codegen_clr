//! H2 real-crate SOAK: libm (software floating-point math, no_std-friendly, pure Rust).
//! Exercises f64 transcendental/elementary functions implemented in Rust software (no FPU intrinsics):
//! sin, cos, sqrt, pow, log. This stresses f64 bit-twiddling, integer<->float reinterpretation,
//! large constant tables, and tight arithmetic loops on the dotnet PAL.
//! Panic-safe: all inputs are valid finite f64 constants; no unwraps, no indexing, no Result handling.
//! SUCCESS = "== soak_libm done ==" with sane values.
use libm::{cos, log, pow, sin, sqrt};

fn approx(label: &str, got: f64, want: f64) {
    let diff = (got - want).abs();
    let ok = diff < 1e-9;
    println!("{label}: got={got:.12} want={want:.12} ok={ok}");
}

fn main() {
    println!("== soak_libm start ==");

    // sin / cos at a few well-known angles
    approx("1  sin(0)", sin(0.0), 0.0);
    approx("2  sin(pi/2)", sin(core::f64::consts::FRAC_PI_2), 1.0);
    approx("3  cos(0)", cos(0.0), 1.0);
    approx("4  cos(pi)", cos(core::f64::consts::PI), -1.0);

    // pythagorean identity sin^2 + cos^2 == 1 over a sweep
    let mut max_err = 0.0_f64;
    let mut i = 0;
    while i <= 16 {
        let x = (i as f64) * (core::f64::consts::PI / 8.0);
        let s = sin(x);
        let c = cos(x);
        let id = s * s + c * c;
        let e = (id - 1.0).abs();
        if e > max_err {
            max_err = e;
        }
        i += 1;
    }
    println!("5  identity max_err over sweep = {max_err:.3e} ok={}", max_err < 1e-9);

    // sqrt
    approx("6  sqrt(2)", sqrt(2.0), core::f64::consts::SQRT_2);
    approx("7  sqrt(144)", sqrt(144.0), 12.0);

    // pow
    approx("8  pow(2,10)", pow(2.0, 10.0), 1024.0);
    approx("9  pow(9,0.5)", pow(9.0, 0.5), 3.0);

    // log (natural log)
    approx("10 log(e)", log(core::f64::consts::E), 1.0);
    approx("11 log(1)", log(1.0), 0.0);

    // a small accumulation loop combining several functions
    let mut acc = 0.0_f64;
    let mut k = 1;
    while k <= 50 {
        let x = k as f64;
        acc += sqrt(x) + sin(x) * cos(x) - log(x) * 0.01;
        k += 1;
    }
    println!("12 combined accumulator = {acc:.6}");

    println!("== soak_libm done ==");
}
