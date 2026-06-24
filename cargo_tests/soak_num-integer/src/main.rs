use num_integer::Integer;
use num_integer::Roots;

fn main() {
    // gcd / lcm on i64.
    let a_i64: i64 = 462;
    let b_i64: i64 = 1071;
    println!("i64_gcd = {}", a_i64.gcd(&b_i64));
    println!("i64_lcm = {}", a_i64.lcm(&b_i64));

    // gcd / lcm on u64.
    let a_u64: u64 = 12_345_678;
    let b_u64: u64 = 87_654_321;
    println!("u64_gcd = {}", a_u64.gcd(&b_u64));
    println!("u64_lcm = {}", a_u64.lcm(&b_u64));

    // gcd / lcm on i128.
    let a_i128: i128 = 9_999_999_967;
    let b_i128: i128 = 1_000_000_007;
    println!("i128_gcd = {}", a_i128.gcd(&b_i128));
    println!("i128_lcm = {}", a_i128.lcm(&b_i128));

    // gcd / lcm on u128.
    let a_u128: u128 = 340_282_366_920_938_463;
    let b_u128: u128 = 18_446_744_073_709_551;
    println!("u128_gcd = {}", a_u128.gcd(&b_u128));
    println!("u128_lcm = {}", a_u128.lcm(&b_u128));

    // div_floor / mod_floor / div_rem with negative operands (i64).
    let n: i64 = -17;
    let d: i64 = 5;
    println!("i64_div_floor = {}", n.div_floor(&d));
    println!("i64_mod_floor = {}", n.mod_floor(&d));
    let (q, r) = n.div_rem(&d);
    println!("i64_div_rem = {} , {}", q, r);

    // div_floor / mod_floor on u64 (no negatives).
    let un: u64 = 17;
    let ud: u64 = 5;
    println!("u64_div_floor = {}", un.div_floor(&ud));
    println!("u64_mod_floor = {}", un.mod_floor(&ud));
    let (uq, ur) = un.div_rem(&ud);
    println!("u64_div_rem = {} , {}", uq, ur);

    // div_floor / mod_floor on i128 with negatives.
    let n128: i128 = -1_000_000_000_001;
    let d128: i128 = 7;
    println!("i128_div_floor = {}", n128.div_floor(&d128));
    println!("i128_mod_floor = {}", n128.mod_floor(&d128));

    // div_floor / mod_floor on u128.
    let un128: u128 = 1_000_000_000_001;
    let ud128: u128 = 7;
    println!("u128_div_floor = {}", un128.div_floor(&ud128));
    println!("u128_mod_floor = {}", un128.mod_floor(&ud128));

    // is_even / is_odd / is_multiple_of.
    println!("i64_is_even = {}", 462_i64.is_even());
    println!("i64_is_odd = {}", 1071_i64.is_odd());
    println!("u64_is_multiple_of = {}", Integer::is_multiple_of(&100_u64, &25));

    // Roots: sqrt / cbrt on several integer widths (exact, deterministic).
    println!("i64_sqrt = {}", 1_000_000_007_i64.sqrt());
    println!("u64_sqrt = {}", 18_446_744_073_709_551_615_u64.sqrt());
    println!("i128_sqrt = {}", 123_456_789_012_345_i128.sqrt());
    println!("u128_sqrt = {}", 340_282_366_920_938_463_463_u128.sqrt());
    println!("i64_cbrt = {}", 1_000_000_000_i64.cbrt());
    println!("u64_cbrt = {}", 1_000_000_000_000_000_000_u64.cbrt());
    println!("i64_nth_root_4 = {}", 1_000_000_000_000_i64.nth_root(4));

    // extended gcd (Bezout coefficients) — deterministic struct fields.
    let egcd = 240_i64.extended_gcd(&46);
    println!("i64_extended_gcd = {} ; x={} ; y={}", egcd.gcd, egcd.x, egcd.y);

    println!("== soak_num-integer done ==");
}
