// Differential soak crate for the `az` crate (casts and checked casts).
//
// Exercises checked / saturating / wrapping / overflowing casts between:
//   i32 <-> u8, i64 <-> f64, f64 <-> i32, u128 <-> u64
// for several values including out-of-range ones. All output is deterministic:
// every printed value is an integer, bool, or fixed-string marker. The only
// floats produced are exact whole numbers (e.g. 300.0), formatted with fixed
// precision via {:.1} so their textual repr cannot vary run-to-run.

use az::{CheckedAs, OverflowingAs, SaturatingAs, WrappingAs};

// Helper to render a checked-cast Option<T> as a deterministic string.
fn opt_i32(v: Option<i32>) -> String {
    match v {
        Some(x) => format!("Some({})", x),
        None => "None".to_string(),
    }
}
fn opt_u8(v: Option<u8>) -> String {
    match v {
        Some(x) => format!("Some({})", x),
        None => "None".to_string(),
    }
}
fn opt_u64(v: Option<u64>) -> String {
    match v {
        Some(x) => format!("Some({})", x),
        None => "None".to_string(),
    }
}

fn main() {
    // ---- i32 -> u8 (narrowing, signed -> unsigned) ----
    // In-range, out-of-range high, and negative.
    let i32_vals: [i32; 3] = [200, 300, -5];
    for &v in i32_vals.iter() {
        let c: Option<u8> = v.checked_as::<u8>();
        let s: u8 = v.saturating_as::<u8>();
        let w: u8 = v.wrapping_as::<u8>();
        let (o, of): (u8, bool) = v.overflowing_as::<u8>();
        println!(
            "i32_to_u8 v={} checked={} sat={} wrap={} over=({},{})",
            v,
            opt_u8(c),
            s,
            w,
            o,
            of
        );
    }

    // ---- u8 -> i32 (widening, always exact) ----
    let u8_vals: [u8; 2] = [0, 255];
    for &v in u8_vals.iter() {
        let c: Option<i32> = v.checked_as::<i32>();
        let s: i32 = v.saturating_as::<i32>();
        let w: i32 = v.wrapping_as::<i32>();
        let (o, of): (i32, bool) = v.overflowing_as::<i32>();
        println!(
            "u8_to_i32 v={} checked={} sat={} wrap={} over=({},{})",
            v,
            opt_i32(c),
            s,
            w,
            o,
            of
        );
    }

    // ---- i64 -> f64 (int -> float; exact for these values) ----
    // Use exact-representable integers so the float text is stable.
    let i64_vals: [i64; 3] = [0, 1_000_000, -1_000_000];
    for &v in i64_vals.iter() {
        // checked_as int->float is always Some; print derived integer to avoid
        // any float-format ambiguity, plus a fixed-precision float.
        let c: Option<f64> = v.checked_as::<f64>();
        match c {
            Some(f) => {
                let back: i64 = f as i64;
                println!("i64_to_f64 v={} checked_float={:.1} back_to_i64={}", v, f, back);
            }
            None => println!("i64_to_f64 v={} checked=None", v),
        }
    }

    // ---- f64 -> i32 (float -> int; checked rejects out-of-range / non-finite) ----
    // Values: in-range whole, large out-of-range, NaN, +inf, negative whole.
    let big: f64 = 5_000_000_000.0; // > i32::MAX
    let neg_big: f64 = -5_000_000_000.0; // < i32::MIN
    let f64_vals: [f64; 6] = [42.0, big, neg_big, f64::NAN, f64::INFINITY, -7.0];
    let f64_names: [&str; 6] = ["42", "big", "neg_big", "nan", "inf", "neg7"];
    for i in 0..f64_vals.len() {
        let v = f64_vals[i];
        let name = f64_names[i];
        // checked_as is NaN/inf-safe and returns None for NaN / out-of-range.
        let c: Option<i32> = v.checked_as::<i32>();
        // az's cast kinds have different domains:
        //   - saturating_as: defined for NaN? no — panics on NaN, but is
        //     defined for +/-inf (saturates to MAX/MIN).
        //   - wrapping_as / overflowing_as: only defined for FINITE inputs;
        //     they panic on NaN and on +/-inf.
        // Gate each so the soak stays panic-free and fully deterministic.
        if v.is_nan() {
            println!(
                "f64_to_i32 name={} checked={} sat=nan wrap=nan over=(nan,nan)",
                name,
                opt_i32(c)
            );
        } else if v.is_finite() {
            let s: i32 = v.saturating_as::<i32>();
            let w: i32 = v.wrapping_as::<i32>();
            let (o, of): (i32, bool) = v.overflowing_as::<i32>();
            println!(
                "f64_to_i32 name={} checked={} sat={} wrap={} over=({},{})",
                name,
                opt_i32(c),
                s,
                w,
                o,
                of
            );
        } else {
            // +/-inf: only saturating is defined.
            let s: i32 = v.saturating_as::<i32>();
            println!(
                "f64_to_i32 name={} checked={} sat={} wrap=inf over=(inf,inf)",
                name,
                opt_i32(c),
                s
            );
        }
    }

    // ---- u128 -> u64 (narrowing, unsigned) ----
    let u128_vals: [u128; 3] = [
        0,
        u64::MAX as u128,              // exactly fits
        (u64::MAX as u128) + 1_000u128, // out of range
    ];
    let u128_names: [&str; 3] = ["zero", "u64max", "over_u64max"];
    for i in 0..u128_vals.len() {
        let v = u128_vals[i];
        let name = u128_names[i];
        let c: Option<u64> = v.checked_as::<u64>();
        let s: u64 = v.saturating_as::<u64>();
        let w: u64 = v.wrapping_as::<u64>();
        let (o, of): (u64, bool) = v.overflowing_as::<u64>();
        println!(
            "u128_to_u64 name={} checked={} sat={} wrap={} over=({},{})",
            name,
            opt_u64(c),
            s,
            w,
            o,
            of
        );
    }

    // ---- u64 -> u128 (widening, always exact) ----
    let u64_vals: [u64; 2] = [0, u64::MAX];
    for &v in u64_vals.iter() {
        let c: Option<u128> = v.checked_as::<u128>();
        let s: u128 = v.saturating_as::<u128>();
        match c {
            Some(x) => println!("u64_to_u128 v={} checked=Some({}) sat={}", v, x, s),
            None => println!("u64_to_u128 v={} checked=None sat={}", v, s),
        }
    }

    println!("== soak_az done ==");
}
