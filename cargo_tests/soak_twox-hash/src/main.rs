//! H2 real-crate SOAK: twox-hash (XxHash64). Hash a byte buffer via the Hasher trait and via the
//! one-shot oneshot path. Exercises Hasher/Default generics, wrapping arithmetic, byte-slice
//! processing, and core::hash. Panic-safe: fixed inputs, no unwraps/indexing that could fail.
//! SUCCESS = "== soak_twox-hash done ==" with deterministic hash values.
use std::hash::Hasher;
use twox_hash::XxHash64;

fn hash_bytes(seed: u64, data: &[u8]) -> u64 {
    let mut h = XxHash64::with_seed(seed);
    h.write(data);
    h.finish()
}

fn main() {
    println!("== soak_twox-hash start ==");

    let buf: &[u8] = b"The quick brown fox jumps over the lazy dog";
    println!("1  len={}", buf.len());

    // Hash with seed 0
    let h0 = hash_bytes(0, buf);
    println!("2  xxh64(seed=0)=0x{h0:016x}");

    // Hash with a non-zero seed -> should differ
    let h1 = hash_bytes(0xCAFE_BABE, buf);
    println!("3  xxh64(seed=0xcafebabe)=0x{h1:016x}");
    println!("4  seeds differ: {}", h0 != h1);

    // Incremental writes should match a single write
    let mut hi = XxHash64::with_seed(0);
    hi.write(b"The quick brown fox ");
    hi.write(b"jumps over the lazy dog");
    let h_incr = hi.finish();
    println!("5  incremental matches oneshot: {}", h_incr == h0);

    // Empty input is well-defined
    let h_empty = hash_bytes(0, b"");
    println!("6  xxh64(empty)=0x{h_empty:016x}");

    // Hash a range of small buffers (loop, wrapping math inside the crate)
    let mut acc: u64 = 0;
    for n in 0..16usize {
        let bytes: Vec<u8> = (0..n as u8).collect();
        acc ^= hash_bytes(n as u64, &bytes);
    }
    println!("7  fold over 16 buffers acc=0x{acc:016x}");

    println!("== soak_twox-hash done ==");
}
