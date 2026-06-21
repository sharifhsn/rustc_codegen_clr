use num_traits::{Float, PrimInt, Signed};

// Generic over an integer trait: clamp + pow + abs, all non-panicking.
fn int_workout<T: PrimInt + Signed + std::fmt::Display>(x: T, lo: T, hi: T, exp: u32) -> T {
    let clamped = if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    };
    // PrimInt::pow takes u32; small exponent keeps it in range for our inputs.
    let powed = clamped.pow(exp);
    powed.abs()
}

// Generic over a float trait: abs, powi, sqrt, max/min — all total functions here.
fn float_workout<T: Float + std::fmt::Display>(x: T, y: T) -> (T, T, T, T) {
    let a = x.abs();
    let p = x.powi(2);
    let s = y.sqrt();
    let m = x.max(y);
    (a, p, s, m)
}

fn main() {
    // PrimInt exercise with i32.
    let r1 = int_workout::<i32>(-7, -3, 10, 2);
    println!("int_workout(-7, -3, 10, exp=2) = {}", r1); // clamp(-7)->-3, (-3)^2=9, abs=9
    let r2 = int_workout::<i32>(50, -3, 10, 3);
    println!("int_workout(50, -3, 10, exp=3) = {}", r2); // clamp(50)->10, 10^3=1000

    // num_traits free functions on integers.
    let z = num_traits::pow(2i32, 10);
    println!("num_traits::pow(2, 10) = {}", z);
    println!("num_traits::abs(-42i32) = {}", num_traits::abs(-42i32));
    println!("clamp(15, 0, 10) = {}", num_traits::clamp(15i32, 0, 10));

    // PrimInt bit ops (generic-heavy).
    let bits = 0b1011_0000u32;
    println!("count_ones(0b10110000) = {}", PrimInt::count_ones(bits));
    println!("leading_zeros = {}", PrimInt::leading_zeros(bits));
    println!("trailing_zeros = {}", PrimInt::trailing_zeros(bits));

    // Float exercise with f64.
    let (a, p, s, m) = float_workout::<f64>(-2.5, 9.0);
    println!("float_workout(-2.5, 9.0) = abs={} powi2={} sqrt={} max={}", a, p, s, m);

    // num_traits::Zero / One generic constants via type params.
    let zero = <f64 as num_traits::Zero>::zero();
    let one = <i32 as num_traits::One>::one();
    println!("zero={} one={}", zero, one);

    // cast between numeric types (NumCast), non-panicking Option result.
    let casted: Option<i32> = num_traits::cast::<f64, i32>(3.99);
    match casted {
        Some(v) => println!("cast 3.99_f64 -> i32 = {}", v),
        None => println!("cast failed"),
    }

    println!("== soak_num-traits done ==");
}
