//! H2 real-crate SOAK: lz4_flex (a pure-Rust LZ4 compressor/decompressor) on the dotnet PAL.
//! Exercises the size-prepended round-trip path: compress_prepend_size +
//! decompress_size_prepended. Stresses byte-slice copy loops, match-finding hash tables,
//! Vec growth, little-endian length encoding, and heavy slice indexing inside the codec.
//! Panic-safe: fixed valid inputs; the only fallible call (decompress) returns a Result that
//! we MATCH (never unwrap), so a decode error prints instead of panicking.
//! SUCCESS = "== soak_lz4_flex done ==" with round-trip matches == true.
use lz4_flex::{compress_prepend_size, decompress_size_prepended};

fn roundtrip(label: &str, input: &[u8]) {
    let compressed = compress_prepend_size(input);
    print!(
        "{label}  in={} comp={} ",
        input.len(),
        compressed.len()
    );
    match decompress_size_prepended(&compressed) {
        Ok(out) => {
            let matches = out.as_slice() == input;
            println!("out={} matches={}", out.len(), matches);
        }
        Err(_) => {
            println!("out=ERR matches=false (decompress error)");
        }
    }
}

fn main() {
    println!("== soak_lz4_flex start ==");

    // 1: highly compressible repetitive buffer (exercises match-finding/back-references).
    let mut repetitive = Vec::with_capacity(4096);
    for _ in 0..512 {
        repetitive.extend_from_slice(b"abcdefgh");
    }
    roundtrip("1", &repetitive);

    // 2: short ASCII text.
    let text = b"the quick brown fox jumps over the lazy dog; the quick brown fox.";
    roundtrip("2", text);

    // 3: pseudo-random-ish (low compressibility) bytes via a simple LCG, deterministic.
    let mut prng: u32 = 0x1234_5678;
    let mut noisy = Vec::with_capacity(2048);
    for _ in 0..2048 {
        prng = prng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        noisy.push((prng >> 24) as u8);
    }
    roundtrip("3", &noisy);

    // 4: empty input edge case.
    roundtrip("4", b"");

    // 5: single byte.
    roundtrip("5", b"Z");

    println!("== soak_lz4_flex done ==");
}
