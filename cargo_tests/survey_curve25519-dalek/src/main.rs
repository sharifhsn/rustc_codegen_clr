// Survey crate for curve25519-dalek (crypto category).
//
// Exercises the core curve arithmetic surface DETERMINISTICALLY:
//   - Scalar construction from fixed canonical bytes
//   - scalar * basepoint (variable-base + fixed-base scalar mul)
//   - EdwardsPoint addition / negation / identity
//   - compression to 32 canonical bytes -> hex
//   - decompression round-trip
//
// All inputs are FIXED byte arrays, so every run produces byte-identical
// output. No RNG, no clock, no hashing of nondeterministic input.

use curve25519_dalek::constants::ED25519_BASEPOINT_POINT;
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;

// Lowercase-hex encoder (no external `hex` dep; fully deterministic).
fn to_hex(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(LUT[(b >> 4) as usize] as char);
        s.push(LUT[(b & 0x0f) as usize] as char);
    }
    s
}

fn main() {
    // A fixed, canonical scalar (< group order l). 0x01..0x20 reduced.
    let s_bytes: [u8; 32] = [
        0x07, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x00,
    ];
    // `from_bytes_mod_order` is infallible (always reduces) -> no panic path.
    let s1: Scalar = Scalar::from_bytes_mod_order(s_bytes);
    println!("scalar1_hex = {}", to_hex(s1.as_bytes()));

    // A second fixed scalar from a small integer.
    let s2: Scalar = Scalar::from(123_456_789_u64);
    println!("scalar2_hex = {}", to_hex(s2.as_bytes()));

    // Fixed-base scalar mul: s1 * B  (uses precomputed tables -> field-arith heavy).
    let p1: EdwardsPoint = ED25519_BASEPOINT_POINT * s1;
    let c1 = p1.compress();
    println!("point1_compressed_hex = {}", to_hex(c1.as_bytes()));

    // s2 * B
    let p2: EdwardsPoint = ED25519_BASEPOINT_POINT * s2;
    let c2 = p2.compress();
    println!("point2_compressed_hex = {}", to_hex(c2.as_bytes()));

    // Point addition: p1 + p2.
    let psum: EdwardsPoint = p1 + p2;
    println!("psum_compressed_hex = {}", to_hex(psum.compress().as_bytes()));

    // Point subtraction / negation: p1 - p2 == p1 + (-p2).
    let pdiff: EdwardsPoint = p1 - p2;
    let pdiff_alt: EdwardsPoint = p1 + (-p2);
    println!("pdiff_compressed_hex = {}", to_hex(pdiff.compress().as_bytes()));
    println!("pdiff_eq_negadd = {}", pdiff == pdiff_alt);

    // Distributive identity: (s1 + s2) * B == s1*B + s2*B.
    let s_sum = s1 + s2;
    let p_combined: EdwardsPoint = ED25519_BASEPOINT_POINT * s_sum;
    println!("distributive_holds = {}", p_combined == psum);

    // Doubling: 2*B via scalar vs B+B.
    let two = Scalar::from(2u64);
    let dbl_scalar: EdwardsPoint = ED25519_BASEPOINT_POINT * two;
    let dbl_add: EdwardsPoint = ED25519_BASEPOINT_POINT + ED25519_BASEPOINT_POINT;
    println!("doubling_consistent = {}", dbl_scalar == dbl_add);

    // Identity element: P + 0 == P, and P + (-P) == identity.
    let id = EdwardsPoint::identity();
    println!("identity_compressed_hex = {}", to_hex(id.compress().as_bytes()));
    let p1_plus_id: EdwardsPoint = p1 + id;
    println!("p_plus_identity_eq = {}", p1_plus_id == p1);
    let p1_minus_p1: EdwardsPoint = p1 - p1;
    println!("p_minus_self_is_identity = {}", p1_minus_p1 == id);

    // Compression round-trip: compress -> decompress -> recompress matches.
    let recovered: bool = match CompressedEdwardsY(*c1.as_bytes()).decompress() {
        Some(dp) => dp == p1 && dp.compress().as_bytes() == c1.as_bytes(),
        None => false,
    };
    println!("decompress_roundtrip_ok = {}", recovered);

    // Scalar arithmetic: (s1 * s2) bytes, and s1 + s1 == 2*s1.
    let s_prod = s1 * s2;
    println!("scalar_product_hex = {}", to_hex(s_prod.as_bytes()));
    let s1_dbl = s1 + s1;
    let s1_times2 = s1 * two;
    println!("scalar_double_consistent = {}", s1_dbl == s1_times2);

    println!("== survey_curve25519-dalek done ==");
}
