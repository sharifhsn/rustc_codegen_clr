//! H2 real-crate SOAK: md-5 (RustCrypto). Computes MD5 of b"abc" and prints hex.
//! Exercises the digest/block-buffer machinery, fixed-size arrays, bit ops, generic
//! GenericArray output. Panic-safe: no unwraps, fixed valid input, manual hex formatting.
//! SUCCESS = digest line == "900150983cd24fb0d6963f7d28e17f72" + "== soak_md-5 done ==".
use md5::{Digest, Md5};
use std::fmt::Write as _;

fn main() {
    println!("== soak_md-5 start ==");

    let mut hasher = Md5::new();
    hasher.update(b"abc");
    let digest = hasher.finalize();

    // Manual hex formatting (no external hex crate, avoids any panics).
    let mut hex = String::new();
    for byte in digest.iter() {
        // write! to a String is infallible; ignore the Result rather than unwrap.
        let _ = write!(hex, "{:02x}", byte);
    }

    println!("1  md5(\"abc\") = {hex}");
    let expected = "900150983cd24fb0d6963f7d28e17f72";
    println!("2  expected     = {expected}");
    println!("3  match = {}", hex == expected);

    // A second, longer input to exercise multi-block hashing.
    let mut hasher2 = Md5::new();
    hasher2.update(b"The quick brown fox jumps over the lazy dog");
    let digest2 = hasher2.finalize();
    let mut hex2 = String::new();
    for byte in digest2.iter() {
        let _ = write!(hex2, "{:02x}", byte);
    }
    println!("4  md5(fox)    = {hex2}");
    println!("5  fox match = {}", hex2 == "9e107d9d372bb6826bd81d3542a419d6");

    // Empty-input edge case.
    let digest3 = Md5::digest(b"");
    let mut hex3 = String::new();
    for byte in digest3.iter() {
        let _ = write!(hex3, "{:02x}", byte);
    }
    println!("6  md5(\"\")     = {hex3}");
    println!("7  empty match = {}", hex3 == "d41d8cd98f00b204e9800998ecf8427e");

    println!("== soak_md-5 done ==");
}
