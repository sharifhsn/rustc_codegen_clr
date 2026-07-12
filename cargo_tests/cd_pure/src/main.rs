// J1 — pure Rust compute + println, NO dependencies. Proves `cargo dotnet run`
// compiles arbitrary pure Rust to a .NET assembly and runs it with correct output
// and exit 0, with ZERO hand-config.

/// A tiny bit of real compute so the output is not a constant the optimizer can
/// fold to a literal: sum of the first N squares + an iterator/closure chain.
fn sum_of_squares(n: u64) -> u64 {
    (1..=n).map(|x| x * x).sum()
}

fn fib(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let next = a + b;
        a = b;
        b = next;
    }
    a
}

fn main() {
    let n = 10u64;
    let sq = sum_of_squares(n); // 385
    let f = fib(20); // 6765

    // Exercise heap (String/Vec) so std::alloc + the dotnet PAL are in play.
    let words: Vec<String> = (1..=3).map(|i| format!("item-{i}")).collect();
    let joined = words.join(", ");

    println!("hello from cargo dotnet (pure Rust on the .NET PAL)");
    println!("sum_of_squares({n}) = {sq}");
    println!("fib(20) = {f}");
    println!("words = [{joined}]");

    // A cheap assertion to make exit-code propagation meaningful: if compute were
    // miscompiled, this would panic (non-zero exit) instead of printing the line.
    assert_eq!(sq, 385);
    assert_eq!(f, 6765);
    assert_eq!(joined, "item-1, item-2, item-3");
    println!("cd_pure: all checks passed");
    println!("== cd_pure done ==");
}
