//! H2 real-crate SOAK: miniz_oxide (pure-Rust DEFLATE compress/decompress).
//! Round-trips a byte buffer: compress_to_vec -> decompress_to_vec, checks the
//! decompressed bytes match the original. Exercises Vec<u8> growth, bit/byte
//! manipulation, large lookup tables, slices, and arithmetic-heavy inner loops.
//! Panic-safe: builds its own input, handles the decompress Result (no unwrap on
//! fallible data), no indexing that could go out of bounds.
//! SUCCESS = "== soak_miniz_oxide done ==" with original==decompressed true.
use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::decompress_to_vec;

fn main() {
    println!("== soak_miniz_oxide start ==");

    // Build a deterministic, compressible buffer (repetitive => good ratio).
    let mut original: Vec<u8> = Vec::with_capacity(8192);
    for i in 0..8192u32 {
        // mix of a repeating pattern and some variation so DEFLATE has work to do
        let b = ((i / 16) as u8).wrapping_add((i % 7) as u8 ^ 0x5a);
        original.push(b);
    }
    println!("1  original: {} bytes", original.len());

    // Compress (level 6 is a reasonable middle ground).
    let compressed = compress_to_vec(&original, 6);
    println!("2  compressed: {} bytes", compressed.len());
    println!("3  ratio: {}%", (compressed.len() * 100) / original.len().max(1));

    // Decompress and verify.
    match decompress_to_vec(&compressed) {
        Ok(decompressed) => {
            println!("4  decompressed: {} bytes", decompressed.len());
            let matches = decompressed == original;
            println!("5  round-trip matches: {matches}");
            if !matches {
                // Surface where it diverged without panicking.
                let len_ok = decompressed.len() == original.len();
                println!("5a len_ok={len_ok}");
                let first_diff = original
                    .iter()
                    .zip(decompressed.iter())
                    .position(|(a, b)| a != b);
                println!("5b first_diff_at={:?}", first_diff);
            }
        }
        Err(e) => {
            // miniz_oxide returns a DecompressError; print it instead of unwrapping.
            println!("4  decompress error: {:?}", e);
        }
    }

    // A second, tiny round-trip to also exercise the small-input path.
    let small = b"hello hello hello world world deflate deflate deflate";
    let c2 = compress_to_vec(small, 9);
    match decompress_to_vec(&c2) {
        Ok(d2) => println!("6  small round-trip matches: {}", d2 == small),
        Err(e) => println!("6  small decompress error: {:?}", e),
    }

    println!("== soak_miniz_oxide done ==");
}
