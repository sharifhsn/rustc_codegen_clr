//! H2 real-crate SOAK: blake3 cryptographic hash on the dotnet PAL.
//! SIMD-heavy crate; CPUID / runtime-feature-detection candidate. Exercises the portable
//! reference path plus any platform_detect feature gates. Panic-safe: known inputs, no unwraps
//! on fallible ops, fixed-size hash output. SUCCESS = "== soak_blake3 done ==" with the
//! well-known BLAKE3 test vector for b"abc".
fn main() {
    println!("== soak_blake3 start ==");

    // Known BLAKE3 test vector: hash("abc") =
    // 6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85
    let h = blake3::hash(b"abc");
    println!("1  hash(abc) = {}", h.to_hex());

    // Empty-input vector: af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
    let e = blake3::hash(b"");
    println!("2  hash(empty) = {}", e.to_hex());

    // Incremental hashing via Hasher (exercises a different code path / buffering).
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"a");
    hasher.update(b"b");
    hasher.update(b"c");
    let inc = hasher.finalize();
    println!("3  incremental(abc) = {}", inc.to_hex());
    println!("4  incremental matches one-shot: {}", inc == h);

    // Larger buffer to push past a single block / chunk and into the tree/compress paths.
    let big = vec![0x61u8; 10_000]; // 10k 'a' bytes
    let bh = blake3::hash(&big);
    println!("5  hash(10k 'a') = {}", bh.to_hex());

    // Raw bytes accessor (no hex formatting path).
    let bytes = h.as_bytes();
    println!("6  first byte = {:#04x}, len = {}", bytes[0], bytes.len());

    println!("== soak_blake3 done ==");
}
