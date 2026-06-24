use core::hash::Hasher;
use wyhash::{wyhash, WyHash};

// Hash a byte slice with a fresh WyHash hasher at a fixed seed.
fn hash_bytes(seed: u64, data: &[u8]) -> u64 {
    let mut h = WyHash::with_seed(seed);
    h.write(data);
    h.finish()
}

// Hash a u64 with a fresh WyHash hasher at a fixed seed.
fn hash_u64(seed: u64, value: u64) -> u64 {
    let mut h = WyHash::with_seed(seed);
    h.write_u64(value);
    h.finish()
}

fn main() {
    // Fixed seed -> fully deterministic output.
    const SEED: u64 = 0x1234_5678_9abc_def0;

    // --- Free `wyhash` function over several known byte slices. ---
    let inputs: [&[u8]; 5] = [
        b"",
        b"a",
        b"hello, world!",
        b"The quick brown fox jumps over the lazy dog",
        b"\x00\x01\x02\x03\x04\x05\x06\x07",
    ];
    let mut idx: u32 = 0;
    for input in inputs.iter() {
        let v = wyhash(input, SEED);
        println!("wyhash[{}] len={} = {}", idx, input.len(), v);
        idx += 1;
    }

    // --- Hasher trait path over the same slices (should match the free fn). ---
    let mut all_match = true;
    let mut idx2: u32 = 0;
    for input in inputs.iter() {
        let free = wyhash(input, SEED);
        let hasher = hash_bytes(SEED, input);
        let same = free == hasher;
        if !same {
            all_match = false;
        }
        println!("hasher[{}] = {} (matches_free={})", idx2, hasher, same);
        idx2 += 1;
    }
    println!("hasher_matches_free_all = {}", all_match);

    // --- Hash several known integers via the Hasher trait. ---
    let ints: [u64; 6] = [0, 1, 42, 255, 1_000_000, u64::MAX];
    let mut idx3: u32 = 0;
    for value in ints.iter() {
        let v = hash_u64(SEED, *value);
        println!("hash_u64[{}] in={} = {}", idx3, value, v);
        idx3 += 1;
    }

    // --- Determinism check: hashing the same input twice yields the same value. ---
    let a = hash_bytes(SEED, b"determinism");
    let b = hash_bytes(SEED, b"determinism");
    println!("deterministic_repeat = {}", a == b);

    // --- Seed sensitivity: different seeds should (almost surely) differ. ---
    let s0 = wyhash(b"seed-test", 0);
    let s1 = wyhash(b"seed-test", 1);
    println!("seed0 = {}", s0);
    println!("seed1 = {}", s1);
    println!("seeds_differ = {}", s0 != s1);

    println!("== soak_wyhash done ==");
}
