use core::hash::Hasher;
use siphasher::sip::{SipHasher13, SipHasher24};

// Hash a byte slice with a fresh SipHasher13 keyed by (k0, k1).
fn hash13(k0: u64, k1: u64, data: &[u8]) -> u64 {
    let mut h = SipHasher13::new_with_keys(k0, k1);
    h.write(data);
    h.finish()
}

// Hash a byte slice with a fresh SipHasher24 keyed by (k0, k1).
fn hash24(k0: u64, k1: u64, data: &[u8]) -> u64 {
    let mut h = SipHasher24::new_with_keys(k0, k1);
    h.write(data);
    h.finish()
}

fn main() {
    // Fixed keys (deterministic). These are the SipHash reference test keys.
    let k0: u64 = 0x0706050403020100;
    let k1: u64 = 0x0f0e0d0c0b0a0908;

    // Known, fixed byte slices to hash.
    let empty: &[u8] = b"";
    let abc: &[u8] = b"abc";
    let hello: &[u8] = b"hello, world!";

    // SipHasher13 over fixed inputs.
    println!("sip13_empty = {}", hash13(k0, k1, empty));
    println!("sip13_abc = {}", hash13(k0, k1, abc));
    println!("sip13_hello = {}", hash13(k0, k1, hello));

    // SipHasher24 over the same fixed inputs.
    println!("sip24_empty = {}", hash24(k0, k1, empty));
    println!("sip24_abc = {}", hash24(k0, k1, abc));
    println!("sip24_hello = {}", hash24(k0, k1, hello));

    // Determinism check: same key + same data => same hash.
    let a = hash24(k0, k1, hello);
    let b = hash24(k0, k1, hello);
    println!("sip24_deterministic = {}", a == b);

    // Key sensitivity: a different key should (essentially always) change the hash.
    let c = hash24(k1, k0, hello);
    println!("sip24_key_sensitive = {}", a != c);

    // Incremental vs one-shot must agree (split the input into two writes).
    let mut split = SipHasher24::new_with_keys(k0, k1);
    split.write(b"hello, ");
    split.write(b"world!");
    println!("sip24_incremental_matches = {}", split.finish() == a);

    // write_u64 path over a fixed integer (exercises a non-slice write).
    let mut hi = SipHasher24::new_with_keys(k0, k1);
    hi.write_u64(0x0102030405060708);
    println!("sip24_u64 = {}", hi.finish());

    // A 64-byte buffer with fixed contents (exercises the multi-block path).
    let mut buf = [0u8; 64];
    let mut i = 0usize;
    while i < buf.len() {
        buf[i] = (i & 0xff) as u8;
        i += 1;
    }
    println!("sip13_64b = {}", hash13(k0, k1, &buf));
    println!("sip24_64b = {}", hash24(k0, k1, &buf));

    println!("== soak_siphasher done ==");
}
