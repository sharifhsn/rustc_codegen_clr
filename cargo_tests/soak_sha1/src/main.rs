//! H2 real-crate SOAK: sha1 (RustCrypto). Compute SHA-1 of b"abc" and print as hex.
//! Exercises block-based hashing / compression rounds (may surface asm-like codegen issues,
//! same category check as sha2). Panic-safe: known input, known length, no unwraps/indexing
//! that can fail. SUCCESS = correct digest 0xa9993e36...9cd0d89d + "== soak_sha1 done ==".
use sha1::{Digest, Sha1};

fn main() {
    println!("== soak_sha1 start ==");

    // SHA-1("abc") = a9993e364706816aba3e25717850c26c9cd0d89d
    let mut hasher = Sha1::new();
    hasher.update(b"abc");
    let digest = hasher.finalize();

    // Render hex without any fallible ops.
    let mut hex = String::with_capacity(40);
    for byte in digest.iter() {
        let hi = byte >> 4;
        let lo = byte & 0xf;
        for nib in [hi, lo] {
            let c = if nib < 10 {
                (b'0' + nib) as char
            } else {
                (b'a' + (nib - 10)) as char
            };
            hex.push(c);
        }
    }
    println!("1  sha1(abc) = {hex}");

    let expected = "a9993e364706816aba3e25717850c26c9cd0d89d";
    println!("2  expected  = {expected}");
    println!("3  match = {}", hex == expected);

    // One-shot convenience API too.
    let oneshot = Sha1::digest(b"abc");
    println!("4  oneshot len = {}", oneshot.len());

    println!("== soak_sha1 done ==");
}
