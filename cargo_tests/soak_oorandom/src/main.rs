//! H2 real-crate SOAK: oorandom (11) deterministic PRNG on the dotnet PAL.
//! oorandom is a pure-Rust, zero-dependency PCG implementation. Seeds are fixed so
//! output is fully deterministic. Exercises Rand32::new/rand_u32/rand_range,
//! Rand64::new/rand_u64/rand_range, rand_float, and u32/u64 wrapping/shift math.
//! Panic-safe: rand_range requires a non-empty range; all ranges below are valid
//! constants, no unwrap/expect, no indexing. SUCCESS = "== soak_oorandom done ==".
use oorandom::{Rand32, Rand64};

fn main() {
    println!("== soak_oorandom start ==");

    // --- Rand32: fixed seed -> deterministic stream ---
    let mut r32 = Rand32::new(0x1234_5678_9abc_def0);
    let a = r32.rand_u32();
    let b = r32.rand_u32();
    let c = r32.rand_u32();
    println!("1  rand32_u32: {} {} {}", a, b, c);

    // rand_range (half-open [low, high)); all values must land in range.
    let mut in_range_32 = true;
    for _ in 0..8 {
        let v = r32.rand_range(10..20);
        if !(10..20).contains(&v) {
            in_range_32 = false;
        }
    }
    println!("2  rand32_range_10_20_ok: {}", in_range_32);

    // rand_float in [0, 1)
    let f = r32.rand_float();
    println!("3  rand32_float_in_unit: {}", (0.0..1.0).contains(&f));

    // --- Rand64: fixed seed -> deterministic stream ---
    let mut r64 = Rand64::new(0x0fed_cba9_8765_4321_1122_3344_5566_7788);
    let x = r64.rand_u64();
    let y = r64.rand_u64();
    let z = r64.rand_u64();
    println!("4  rand64_u64: {} {} {}", x, y, z);

    let mut in_range_64 = true;
    for _ in 0..8 {
        let v = r64.rand_range(1000..2000);
        if !(1000..2000).contains(&v) {
            in_range_64 = false;
        }
    }
    println!("5  rand64_range_1000_2000_ok: {}", in_range_64);

    let g = r64.rand_float();
    println!("6  rand64_float_in_unit: {}", (0.0..1.0).contains(&g));

    // --- Determinism check: two generators with the same seed agree ---
    let mut r_a = Rand32::new(42);
    let mut r_b = Rand32::new(42);
    let mut deterministic = true;
    for _ in 0..16 {
        if r_a.rand_u32() != r_b.rand_u32() {
            deterministic = false;
        }
    }
    println!("7  rand32_deterministic: {}", deterministic);

    // --- Simple histogram sanity: collect into a Vec, sum it ---
    let mut r_h = Rand64::new(7);
    let samples: Vec<u64> = (0..32).map(|_| r_h.rand_range(0..100)).collect();
    let sum: u64 = samples.iter().sum();
    let all_lt_100 = samples.iter().all(|&v| v < 100);
    println!("8  hist_count={} sum={} all_lt_100={}", samples.len(), sum, all_lt_100);

    println!("== soak_oorandom done ==");
}
