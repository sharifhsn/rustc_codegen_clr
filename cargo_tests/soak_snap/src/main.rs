use snap::raw::{Decoder, Encoder};

// Render the first `n` bytes of a slice as lowercase hex, no allocation surprises.
fn hex_prefix(bytes: &[u8], n: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let take = if n < bytes.len() { n } else { bytes.len() };
    let mut out = String::with_capacity(take * 2);
    let mut i = 0;
    while i < take {
        let b = bytes[i];
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
        i += 1;
    }
    out
}

fn main() {
    // A deterministic, highly compressible byte buffer: 256 repeats of a short
    // ASCII pattern. Snappy will collapse the repeats via back-references, so the
    // compressed length is a stable, meaningful number (not run-to-run noise).
    let pattern: &[u8] = b"snappy-soak-0123456789-";
    let mut data: Vec<u8> = Vec::with_capacity(pattern.len() * 256);
    let mut r = 0;
    while r < 256 {
        data.extend_from_slice(pattern);
        r += 1;
    }
    println!("input_len = {}", data.len());

    // --- Compress (round-trip step 1) ---
    let mut encoder = Encoder::new();
    let compressed: Vec<u8> = match encoder.compress_vec(&data) {
        Ok(c) => c,
        Err(_) => {
            println!("compress_error = true");
            println!("== soak_snap done ==");
            return;
        }
    };
    println!("compressed_len = {}", compressed.len());
    println!("compressed_hex_prefix = {}", hex_prefix(&compressed, 16));

    // The Snappy stream begins with a varint of the *uncompressed* length.
    // For our 5888-byte input that is a fixed, checkable header.
    println!(
        "compressed_smaller = {}",
        compressed.len() < data.len()
    );

    // --- Decompress (round-trip step 2) ---
    let mut decoder = Decoder::new();
    match decoder.decompress_vec(&compressed) {
        Ok(decompressed) => {
            println!("decompressed_len = {}", decompressed.len());
            let matches = decompressed.as_slice() == data.as_slice();
            println!("roundtrip_matches = {}", matches);
            // A small content fingerprint that does not depend on iteration order:
            // sum of bytes mod 2^32, and the first/last byte.
            let mut sum: u32 = 0;
            for &b in decompressed.iter() {
                sum = sum.wrapping_add(b as u32);
            }
            println!("decompressed_bytesum = {}", sum);
            let first = if decompressed.is_empty() { 0u8 } else { decompressed[0] };
            let last = if decompressed.is_empty() {
                0u8
            } else {
                decompressed[decompressed.len() - 1]
            };
            println!("decompressed_first = {}", first);
            println!("decompressed_last = {}", last);
        }
        Err(_) => {
            println!("decompress_error = true");
        }
    }

    // --- A second, independent vector: an empty input edge case ---
    let mut enc2 = Encoder::new();
    let empty: &[u8] = b"";
    match enc2.compress_vec(empty) {
        Ok(c) => {
            println!("empty_compressed_len = {}", c.len());
            let mut dec2 = Decoder::new();
            match dec2.decompress_vec(&c) {
                Ok(d) => println!("empty_roundtrip_len = {}", d.len()),
                Err(_) => println!("empty_decompress_error = true"),
            }
        }
        Err(_) => println!("empty_compress_error = true"),
    }

    // --- max_compress_len: a pure arithmetic surface of the crate ---
    println!(
        "max_compress_len_1000 = {}",
        snap::raw::max_compress_len(1000)
    );

    println!("== soak_snap done ==");
}
