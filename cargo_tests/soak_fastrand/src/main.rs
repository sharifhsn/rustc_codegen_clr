//! H2 real-crate SOAK: fastrand on the dotnet PAL, no surrogate.
//! fastrand is a tiny PRNG (Wyrand-style) with NO getrandom dependency when explicitly seeded,
//! so it should run fully deterministically and contrast with the rand/getrandom path.
//! Rng::with_seed(42) drives u32 / usize / bounded ranges / shuffle / choice, all panic-safe:
//! ranges are non-empty constants, choice() returns Option (handled), no unwrap/index-panic.
//! SUCCESS = "== soak_fastrand done ==" with deterministic values.
use fastrand::Rng;

fn main() {
    println!("== soak_fastrand start ==");

    // Explicitly seeded -> deterministic, no entropy/getrandom needed.
    let mut rng = Rng::with_seed(42);

    // Raw u32 / u64 from the Wyrand state.
    let a = rng.u32(..);
    let b = rng.u64(..);
    println!("1  u32/u64: {a} {b}");

    // Bounded ints over non-empty ranges (panic-safe: low < high constants).
    let r0 = rng.u32(0..100);
    let r1 = rng.u32(0..100);
    let r2 = rng.i64(-50..50);
    println!("2  bounded u32 0..100: {r0} {r1}  i64 -50..50: {r2}");

    // usize range (exercises pointer-width arithmetic).
    let idx = rng.usize(0..10);
    println!("3  usize 0..10: {idx}");

    // A bool and a float in [0,1).
    let flag = rng.bool();
    let f = rng.f64();
    println!("4  bool: {flag}  f64*1e6: {}", (f * 1_000_000.0) as u64);

    // Shuffle a small Vec in place.
    let mut deck: Vec<u32> = (0..10).collect();
    rng.shuffle(&mut deck);
    println!("5  shuffled 0..10: {deck:?}");

    // choice() returns Option -> handled, no unwrap-panic.
    let picks: Vec<u32> = (0..5).collect();
    match rng.choice(&picks) {
        Some(p) => println!("6  choice: {p}"),
        None => println!("6  choice: none"),
    }

    // Fill a buffer's worth of bytes and sum them (exercises the byte path).
    let mut sum: u32 = 0;
    for _ in 0..32 {
        sum += rng.u8(..) as u32;
    }
    println!("7  32 random bytes, sum={sum}");

    println!("== soak_fastrand done ==");
}
