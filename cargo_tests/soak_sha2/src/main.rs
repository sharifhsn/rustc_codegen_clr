//! H2 real-crate SOAK: sha2 (a real crypto-hash crate) computing SHA-256 on the dotnet PAL.
//! Exercises GenericArray/typenum generics, block-buffer state machine, lots of wrapping integer
//! arithmetic + bit rotations, the Digest trait. Panic-safe: fixed valid byte inputs, no unwraps
//! on fallible data; hex formatting done by hand (no external hex crate).
//! SUCCESS = "== soak_sha2 done ==" with the known SHA-256 of "abc" = ba7816bf...
use sha2::{Digest, Sha256};

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('?'));
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('?'));
    }
    s
}

fn main() {
    println!("== soak_sha2 start ==");

    // 1: chain_update API on "abc"
    let d1 = Sha256::new().chain_update(b"abc").finalize();
    let h1 = to_hex(&d1);
    println!("1  sha256(abc)        = {h1}");
    println!("1  matches known abc? = {}", h1 == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");

    // 2: one-shot digest of the empty string
    let d2 = Sha256::digest(b"");
    let h2 = to_hex(&d2);
    println!("2  sha256(\"\")         = {h2}");
    println!("2  matches known empty? = {}", h2 == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");

    // 3: incremental update across multiple chunks (exercises block buffering)
    let mut hasher = Sha256::new();
    hasher.update(b"abcdbcdecdefdefgefghfghighij");
    hasher.update(b"hijkijkljklmklmnlmnomnopnopq");
    let d3 = hasher.finalize();
    let h3 = to_hex(&d3);
    println!("3  sha256(56-byte msg) = {h3}");
    println!("3  matches known?      = {}", h3 == "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");

    // 4: digest length sanity
    println!("4  digest len bytes    = {}", d1.len());

    println!("== soak_sha2 done ==");
}
