//! H2 real-crate SOAK: rand + rand_chacha on the dotnet PAL, no surrogate.
//! A SEEDED ChaCha20Rng (deterministic) drives gen_range + slice shuffle, so output is reproducible
//! and avoids PAL-entropy nondeterminism. Exercises rand's RngCore/Rng trait machinery, the ChaCha20
//! block generator (lots of u32 arithmetic / arrays), SliceRandom::shuffle, and gen_range's
//! uniform-sampling path. Panic-safe: all ranges are non-empty constants, no unwraps on fallible data.
//! SUCCESS = "== soak_rand done ==" with deterministic values.
use rand::{Rng, SeedableRng};
use rand::seq::SliceRandom;
use rand_chacha::ChaCha20Rng;

// getrandom 0.2 custom backend -> dotnet PAL CSPRNG. rand -> rand_core ->
// getrandom 0.2 rejects os="dotnet" unless a custom backend is registered.
// 0.2 uses a Cargo feature (`custom`, enabled in Cargo.toml) + this macro,
// which must be invoked in the root binary crate. Distinct symbol from 0.3/0.4.
fn dotnet_getrandom(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    getrandom_dotnet::fill(buf);
    Ok(())
}
getrandom::register_custom_getrandom!(dotnet_getrandom);

fn main() {
    println!("== soak_rand start ==");

    // Seeded -> fully deterministic across runs and platforms.
    let mut rng = ChaCha20Rng::seed_from_u64(42);

    // gen_range over non-empty ranges (panic-safe: low < high constants).
    let a = rng.gen_range(0u32..100);
    let b = rng.gen_range(0u32..100);
    let c = rng.gen_range(0u32..100);
    println!("1  gen_range 0..100: {a} {b} {c}");

    let d = rng.gen_range(-50i64..50);
    println!("2  gen_range -50..50: {d}");

    // A raw next_u32 / next_u64 from the ChaCha block fn.
    let r32 = rng.gen::<u32>();
    let r64 = rng.gen::<u64>();
    println!("3  gen u32/u64: {r32} {r64}");

    // A bounded float in [0,1).
    let f: f64 = rng.gen();
    println!("4  gen f64 in[0,1): {}", (f * 1_000_000.0) as u64);

    // Shuffle a small Vec in place (SliceRandom).
    let mut deck: Vec<u32> = (0..10).collect();
    deck.shuffle(&mut rng);
    println!("5  shuffled 0..10: {deck:?}");

    // choose() returns Option -> handled, no unwrap-panic.
    let picks: Vec<u32> = (0..5).collect();
    match picks.choose(&mut rng) {
        Some(p) => println!("6  choose: {p}"),
        None => println!("6  choose: none"),
    }

    // Sum a batch of bytes to exercise fill_bytes / the block path.
    let mut buf = [0u8; 32];
    rng.fill(&mut buf[..]);
    let sum: u32 = buf.iter().map(|&x| x as u32).sum();
    println!("7  fill 32 bytes, sum={sum}");

    println!("== soak_rand done ==");
}
