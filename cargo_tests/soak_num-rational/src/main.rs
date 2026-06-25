//! H2 real-crate SOAK: num-rational — Ratio<i64> arithmetic on the dotnet PAL.
//! Exercises gcd-based fraction reduction (num-integer), Add/Mul/Sub/Div operator traits over a
//! generic numeric type, Display formatting as "n/d", and round-trip recip/pow. Panic-safe: all
//! denominators are non-zero literals, no .unwrap()/.expect(), no indexing that can fail.
//! SUCCESS = "== soak_num-rational done ==" with reduced fractions matching the comments.
use num_rational::Ratio;

fn main() {
    println!("== soak_num-rational start ==");

    // 1: construction auto-reduces via gcd (2/4 -> 1/2)
    let a = Ratio::new(2i64, 4);
    println!("1  2/4 reduced     = {a}");           // 1/2
    println!("1  numer/denom     = {} / {}", a.numer(), a.denom());

    // 2: addition with unlike denominators (1/2 + 1/3 = 5/6)
    let b = Ratio::new(1i64, 3);
    let sum = a + b;
    println!("2  1/2 + 1/3       = {sum}");         // 5/6

    // 3: multiplication then reduction (2/3 * 3/4 = 1/2)
    let c = Ratio::new(2i64, 3) * Ratio::new(3i64, 4);
    println!("3  2/3 * 3/4       = {c}");           // 1/2

    // 4: subtraction yielding a negative (1/4 - 1/2 = -1/4)
    let d = Ratio::new(1i64, 4) - Ratio::new(1i64, 2);
    println!("4  1/4 - 1/2       = {d}");           // -1/4

    // 5: division (3/4 / 3/8 = 2/1)
    let e = Ratio::new(3i64, 4) / Ratio::new(3i64, 8);
    println!("5  3/4 / 3/8       = {e}");           // 2

    // 6: reciprocal (5/6 recip = 6/5)
    let r = sum.recip();
    println!("6  (5/6).recip()   = {r}");           // 6/5

    // 7: integer / fractional decomposition of 7/2 (trunc=3, fract=1/2)
    let f = Ratio::new(7i64, 2);
    println!("7  trunc(7/2)      = {}", f.trunc());  // 3
    println!("7  fract(7/2)      = {}", f.fract());  // 1/2

    // 8: comparison + is_integer
    println!("8  (6/3).is_integer = {}", Ratio::new(6i64, 3).is_integer()); // true
    println!("8  1/2 < 2/3        = {}", a < Ratio::new(2i64, 3));          // true

    // 9: pow via repeated mul (2/3)^3 = 8/27
    let base = Ratio::new(2i64, 3);
    let cubed = base * base * base;
    println!("9  (2/3)^3         = {cubed}");        // 8/27

    // 10: from integer + to f64 approximation
    let whole = Ratio::from_integer(5i64);
    println!("10 from_integer(5) = {whole}");        // 5
    let approx = (Ratio::new(1i64, 4)).to_string();
    println!("10 1/4 as string   = {approx}");       // 1/4

    println!("== soak_num-rational done ==");
}
