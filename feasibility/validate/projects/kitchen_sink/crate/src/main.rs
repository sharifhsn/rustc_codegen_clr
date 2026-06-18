//! A deterministic "kitchen sink" that exercises a broad swath of std and the
//! codegen: integer/float arithmetic, bit ops, iterators, closures, generics,
//! collections (BTree* for deterministic order — NOT HashMap), strings/formatting,
//! enums/match, Option/Result, slices, recursion. Output must be identical when
//! run natively and through rustc_codegen_clr; the differential runner diffs them.
//! Keep it deterministic: no RNG, threads, time, or HashMap iteration order.

use std::collections::{BTreeMap, BTreeSet};

fn fib(n: u64) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let t = a.wrapping_add(b);
        a = b;
        b = t;
    }
    a
}

#[derive(Debug, Clone, PartialEq)]
enum Shape {
    Circle(f64),
    Rect { w: f64, h: f64 },
}
impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle(r) => std::f64::consts::PI * r * r,
            Shape::Rect { w, h } => w * h,
        }
    }
}

fn main() {
    // --- integers: arithmetic, bit ops, wrapping/checked/overflowing ---
    let mut acc: u64 = 0;
    for i in 1u64..=20 {
        acc = acc.wrapping_mul(31).wrapping_add(i);
    }
    println!("hash-ish acc = {acc}");
    println!("checked: {:?} {:?}", 200u8.checked_add(100), 100u8.checked_add(50));
    println!("overflowing: {:?}", 250u8.overflowing_add(10));
    println!("bits: lz={} to={} ones={} rev={:#x}",
        0x0F0Fu32.leading_zeros(), 0x0F0Fu32.trailing_zeros(),
        0xF0F0u32.count_ones(), 0x12345678u32.swap_bytes());
    for s in [(-7i32, 3i32), (7, -3), (-7, -3)] {
        println!("  {} / {} = {}, % = {}", s.0, s.1, s.0 / s.1, s.0 % s.1);
    }

    // --- floats: exercises the min/max/abs intrinsics + transcendentals ---
    let xs = [2.0f64, -3.5, 0.0, -0.0, 1.5];
    let sum: f64 = xs.iter().copied().sum();
    let prod: f64 = xs.iter().copied().filter(|x| *x != 0.0).product();
    println!("float sum={:.6} prod={:.6}", sum, prod);
    println!("max={:.6} min={:.6} abs={:.6} sqrt={:.6}",
        2.0f64.max(f64::NAN), 2.0f64.min(7.0), (-4.25f64).abs(), 2.0f64.sqrt());
    println!("clamp={:.6} floor={} ceil={} round={} signum={}",
        5.5f64.clamp(0.0, 3.0), 3.7f64.floor(), 3.2f64.ceil(), 2.5f64.round(), (-2.0f64).signum());

    // --- iterators + closures + generics ---
    let v: Vec<i64> = (1..=10).map(|x| x * x).collect();
    let evens: Vec<i64> = v.iter().copied().filter(|x| x % 2 == 0).collect();
    let total: i64 = v.iter().copied().fold(0, |a, b| a + b);
    let pairs: Vec<(usize, i64)> = v.iter().copied().enumerate().rev().take(3).collect();
    println!("squares={:?}", v);
    println!("evens={:?} total={total}", evens);
    println!("last3(rev)={:?}", pairs);
    println!("max_by_key={:?}", v.iter().max_by_key(|x| *x % 7));

    // --- collections (deterministic order) ---
    let words = ["pear", "apple", "fig", "apple", "cherry", "fig", "fig"];
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for w in words {
        *counts.entry(w).or_insert(0) += 1;
    }
    println!("counts={:?}", counts);
    let uniq: BTreeSet<&str> = words.iter().copied().collect();
    println!("uniq={:?}", uniq);

    // --- strings / formatting ---
    let s = "Hello, .NET from Rust!";
    println!("len={} upper={} rev={}", s.len(), s.to_uppercase(),
        s.chars().rev().collect::<String>());
    println!("fmt: {:>8}|{:<8}|{:^8}|{:08.3}|{:+}", "ab", "ab", "ab", 3.14159, 42);
    let csv = v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
    println!("csv={csv}");

    // --- enums / Option / Result / recursion / slices ---
    let shapes = [Shape::Circle(1.0), Shape::Rect { w: 2.0, h: 3.0 }, Shape::Circle(2.5)];
    let area_sum: f64 = shapes.iter().map(Shape::area).sum();
    println!("shapes={:?} area_sum={:.6}", shapes, area_sum);
    let parsed: Result<i32, _> = "123".parse::<i32>();
    let bad: Result<i32, _> = "x".parse::<i32>();
    println!("parse ok={:?} err={}", parsed, bad.is_err());
    println!("fib(0..15)={:?}", (0..15).map(fib).collect::<Vec<_>>());
    let data = [5i32, 3, 9, 1, 7, 2, 8];
    let mut sorted = data;
    sorted.sort();
    println!("sorted={:?} binary_search(7)={:?}", sorted, sorted.binary_search(&7));

    println!("DONE");
}
