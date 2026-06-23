//! Rust-via-rustc_codegen_clr side of a fair head-to-head vs hand-written C#, both on .NET 8.
//! Two workloads chosen to isolate the two axes that matter:
//!   - `numeric`     : tight integer loop, ZERO allocation -> measures raw codegen quality
//!                     (cilly optimizer + RyuJIT) vs Roslyn + RyuJIT.
//!   - `alloc_churn` : allocate+fill+sum+drop a buffer many times -> measures the memory model
//!                     (Rust Vec -> unmanaged heap + deterministic Drop  vs  C# array -> GC heap).
//! Each prints `best=<microseconds>` over 3 runs after a warmup. The summed result is printed as a
//! sink so the optimizer can't elide the work. Keep this byte-identical in logic to the C# version.

use std::time::Instant;

#[inline(never)]
fn numeric(n: u64) -> u64 {
    let mut acc: u64 = 0;
    let mut i: u64 = 0;
    while i < n {
        acc = acc.wrapping_add(i.wrapping_mul(i) ^ (i >> 3));
        i += 1;
    }
    acc
}

#[inline(never)]
fn alloc_churn(iters: usize, k: usize) -> u64 {
    let mut acc: u64 = 0;
    for _ in 0..iters {
        let mut v: Vec<u64> = vec![0; k];
        for (j, slot) in v.iter_mut().enumerate() {
            *slot = j as u64;
        }
        for &x in &v {
            acc = acc.wrapping_add(x);
        }
        // `v` dropped here -> deterministic unmanaged free (no GC pressure).
    }
    acc
}

/// Same allocation pattern as `alloc_churn`, but with fully INDEXED `while` loops (no iterator
/// adapters anywhere) — matches C#'s indexed `for`. Comparing this to `alloc_churn` isolates the
/// iterator-codegen cost; comparing it to C# isolates the raw allocation (malloc/free vs gen0) cost.
#[inline(never)]
fn alloc_churn_indexed(iters: usize, k: usize) -> u64 {
    let mut acc: u64 = 0;
    let mut it = 0usize;
    while it < iters {
        let mut v: Vec<u64> = vec![0; k];
        let mut j = 0usize;
        while j < k {
            v[j] = j as u64;
            j += 1;
        }
        let mut j = 0usize;
        while j < k {
            acc = acc.wrapping_add(v[j]);
            j += 1;
        }
        it += 1;
    }
    acc
}

fn bench<F: Fn() -> u64>(name: &str, runs: u32, f: F) {
    let mut best = u128::MAX;
    let mut sink = 0u64;
    for _ in 0..runs {
        let t = Instant::now();
        sink = f();
        let us = t.elapsed().as_micros();
        if us < best {
            best = us;
        }
    }
    println!("{name}: best={best}us (sink={sink})");
}

fn main() {
    // warmup (JIT + alloc paths)
    let _ = numeric(5_000_000);
    let _ = alloc_churn(10_000, 256);
    let _ = alloc_churn_indexed(10_000, 256);
    bench("numeric", 3, || numeric(300_000_000));
    bench("alloc_churn", 3, || alloc_churn(300_000, 512));
    bench("alloc_churn_indexed", 3, || alloc_churn_indexed(300_000, 512));
}
