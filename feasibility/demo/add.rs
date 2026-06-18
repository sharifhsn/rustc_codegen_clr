//! A tiny piece of Rust logic, compiled to a .NET assembly by rustc_codegen_clr
//! and executed on the CoreCLR runtime. Proves Rust code runs inside .NET 8.
//!
//! NOTE: the *reverse* direction — exposing this to be called ergonomically from
//! C#/EF Core — is the project's least-finished area (the `mycorrhiza` /
//! `dotnet_typedef!` interop layer). This demo deliberately stays on the
//! supported path: a self-contained Rust program that runs on .NET.

fn fib(n: u64) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let t = a + b;
        a = b;
        b = t;
    }
    a
}

fn main() {
    // Some real Rust: iterators, closures, Vec.
    let xs: Vec<u64> = (0..15).map(fib).collect();
    let sum: u64 = xs.iter().sum();
    println!("fib(0..15) = {xs:?}");
    println!("sum        = {sum}");
}
