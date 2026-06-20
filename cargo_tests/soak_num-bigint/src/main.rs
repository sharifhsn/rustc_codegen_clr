//! H2 real-crate SOAK: num-bigint doing real arbitrary-precision arithmetic on the dotnet PAL.
//! Computes factorial(30) and a power via iterative multiply, prints the big decimal string and
//! its bit length. Exercises BigUint allocation/Vec<u64>-limb storage, Mul/Add, ToString (fmt),
//! and bit-length queries. Panic-safe: only valid inputs, no unwraps that can fail.
//! SUCCESS = "== soak_num-bigint done ==" with sane values.
use num_bigint::BigUint;

fn main() {
    println!("== soak_num-bigint start ==");

    // factorial(30) via iterative multiply
    let mut fact = BigUint::from(1u32);
    for i in 1u32..=30 {
        fact = fact * BigUint::from(i);
    }
    let fact_str = fact.to_string();
    println!("1  30! = {fact_str}");
    println!("2  30! digits = {}", fact_str.len());
    println!("3  30! bits = {}", fact.bits());

    // 2^128 via iterative multiply
    let mut pow = BigUint::from(1u32);
    let two = BigUint::from(2u32);
    for _ in 0..128 {
        pow = &pow * &two;
    }
    println!("4  2^128 = {pow}");
    println!("5  2^128 bits = {}", pow.bits());

    // a sanity sum: 30! + 2^128
    let sum = &fact + &pow;
    println!("6  30!+2^128 bits = {}", sum.bits());

    // round-trip: parse the decimal string back into a BigUint and compare
    match fact_str.parse::<BigUint>() {
        Ok(reparsed) => println!("7  reparse-eq = {}", reparsed == fact),
        Err(_) => println!("7  reparse failed"),
    }

    println!("== soak_num-bigint done ==");
}
