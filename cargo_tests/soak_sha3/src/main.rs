//! H2 real-crate SOAK: sha3 (Keccak/SHA-3) computing SHA3-256 on the dotnet PAL.
//! Exercises the Keccak-f[1600] permutation: heavy u64 bit-rotations, XORs, and the
//! sponge state machine, plus GenericArray/typenum generics and the Digest trait.
//! Panic-safe: fixed valid byte inputs, no unwraps on fallible data; hex by hand.
//! SUCCESS = "== soak_sha3 done ==" with the known SHA3-256 of "abc".
use sha3::{Digest, Sha3_256};

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('?'));
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('?'));
    }
    s
}

fn main() {
    println!("== soak_sha3 start ==");

    // 1: one-shot digest of "abc"
    let d1 = Sha3_256::digest(b"abc");
    let h1 = to_hex(&d1);
    println!("1  sha3_256(abc)       = {h1}");
    println!(
        "1  matches known abc?  = {}",
        h1 == "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
    );

    // 2: one-shot digest of the empty string
    let d2 = Sha3_256::digest(b"");
    let h2 = to_hex(&d2);
    println!("2  sha3_256(\"\")        = {h2}");
    println!(
        "2  matches known empty? = {}",
        h2 == "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
    );

    // 3: incremental update across multiple chunks (exercises sponge absorb buffering)
    let mut hasher = Sha3_256::new();
    hasher.update(b"abcdbcdecdefdefgefghfghighij");
    hasher.update(b"hijkijkljklmklmnlmnomnopnopq");
    let d3 = hasher.finalize();
    let h3 = to_hex(&d3);
    println!("3  sha3_256(56-byte)   = {h3}");

    // 4: digest length sanity
    println!("4  digest len bytes    = {}", d1.len());

    println!("== soak_sha3 done ==");
}
