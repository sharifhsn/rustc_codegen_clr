use num_bigint::BigInt;
use num_traits::{One, Zero, Pow};
fn main() {
    let mut f = BigInt::one();
    for i in 1..=200u32 { f *= i; }
    println!("200!={}", f);
    let (mut a, mut b) = (BigInt::zero(), BigInt::one());
    for _ in 0..500 { let c = &a + &b; a = b; b = c; }
    println!("fib500={}", a);
    let p = BigInt::from(7u32).pow(300u32);
    println!("7^300={}", p);
    println!("mod={}", &p % BigInt::from(1_000_000_000_000_000_000u64));
    let x = BigInt::parse_bytes(b"123456789012345678901234567890", 10).unwrap();
    let y = BigInt::parse_bytes(b"987654321098765432109876543210", 10).unwrap();
    println!("xy={}", &x * &y);
    println!("ydivx={} ymodx={}", &y / &x, &y % &x);
    let n = BigInt::from(-12345678901234567890i128);
    println!("shl={}", &n << 40);
    println!("pow3={}", n.pow(3u32));
    println!("gcd-ish={}", BigInt::from(123456789u64).pow(7u32) % BigInt::from(97u32));
    println!("== soak_num_bigint done ==");
}
