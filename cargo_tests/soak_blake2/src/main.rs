//! H2 real-crate SOAK: blake2 (a real crypto-hash crate) computing Blake2b512 on the dotnet PAL.
//! Exercises GenericArray/typenum generics, block-buffer state machine, 64-bit wrapping arithmetic
//! and bit rotations, the Digest trait. Panic-safe: fixed valid byte inputs, no unwraps on fallible
//! data; hex formatting done by hand (no external hex crate).
//! SUCCESS = "== soak_blake2 done ==" with the known Blake2b-512 of "abc".
use blake2::{Blake2b512, Digest};

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('?'));
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('?'));
    }
    s
}

fn main() {
    println!("== soak_blake2 start ==");

    // Known Blake2b-512 of "abc" (RFC 7693 / reference vector).
    let known_abc = "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923";

    // 1: chain_update API on "abc"
    let d1 = Blake2b512::new().chain_update(b"abc").finalize();
    let h1 = to_hex(&d1);
    println!("1  blake2b512(abc)     = {h1}");
    println!("1  matches known abc?  = {}", h1 == known_abc);

    // 2: one-shot digest of the empty string
    let d2 = Blake2b512::digest(b"");
    let h2 = to_hex(&d2);
    let known_empty = "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce";
    println!("2  blake2b512(\"\")      = {h2}");
    println!("2  matches known empty? = {}", h2 == known_empty);

    // 3: incremental update across multiple chunks (exercises block buffering)
    let mut hasher = Blake2b512::new();
    hasher.update(b"abcdbcdecdefdefgefghfghighij");
    hasher.update(b"hijkijkljklmklmnlmnomnopnopq");
    let d3 = hasher.finalize();
    let h3 = to_hex(&d3);
    println!("3  blake2b512(56-byte) = {h3}");

    // 4: digest length sanity (Blake2b512 -> 64 bytes)
    println!("4  digest len bytes    = {}", d1.len());

    println!("== soak_blake2 done ==");
}
