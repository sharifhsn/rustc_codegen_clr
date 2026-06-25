use std::io::Write;

// Compress `input` into a Vec<u8> using brotli's std Writer interface.
// `brotli::CompressorWriter` wraps an in-memory Vec (no files / no C).
fn compress(input: &[u8], quality: u32, lgwin: u32) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    {
        let mut writer = brotli::CompressorWriter::new(&mut out, 4096, quality, lgwin);
        // Writing to a Vec is infallible in practice, but handle the Result.
        if writer.write_all(input).is_err() {
            return Vec::new();
        }
        if writer.flush().is_err() {
            return Vec::new();
        }
        // Drop the writer here to finalize the brotli stream into `out`.
    }
    out
}

// Decompress `input` into a Vec<u8> using brotli's std Writer interface.
fn decompress(input: &[u8]) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    {
        let mut writer = brotli::DecompressorWriter::new(&mut out, 4096);
        if writer.write_all(input).is_err() {
            return None;
        }
        if writer.flush().is_err() {
            return None;
        }
        // Drop finalizes; if the stream was truncated/invalid this still
        // yields whatever was decoded. We validate by round-trip equality below.
    }
    Some(out)
}

fn main() {
    // A fixed, deterministic byte buffer with real redundancy so brotli has
    // something to compress. 512 bytes built from a repeating pattern plus a
    // sentence, fully reproducible across runs and targets.
    let mut data: Vec<u8> = Vec::with_capacity(512);
    let phrase = b"the quick brown fox jumps over the lazy dog. ";
    while data.len() < 512 {
        for &b in phrase.iter() {
            if data.len() >= 512 {
                break;
            }
            data.push(b);
        }
    }
    // Truncate to exactly 512 for a fixed input length.
    data.truncate(512);

    println!("input_len = {}", data.len());

    // Use a fixed quality + window so compressed output is deterministic.
    let quality: u32 = 9;
    let lgwin: u32 = 22;
    println!("quality = {}", quality);
    println!("lgwin = {}", lgwin);

    let compressed = compress(&data, quality, lgwin);
    println!("compressed_len = {}", compressed.len());
    // The compressed stream should be smaller than the highly-redundant input.
    println!("compressed_smaller = {}", compressed.len() < data.len());

    // First byte of the brotli stream is a stable function of the input/params;
    // printing it as an int gives a deterministic structural marker.
    let first_byte = compressed.first().copied().unwrap_or(0);
    println!("compressed_first_byte = {}", first_byte);

    match decompress(&compressed) {
        Some(roundtrip) => {
            println!("roundtrip_len = {}", roundtrip.len());
            let ok = roundtrip.as_slice() == data.as_slice();
            println!("roundtrip_ok = {}", ok);
        }
        None => {
            println!("roundtrip_ok = false");
            println!("decompress_error = true");
        }
    }

    // Empty-input edge case: compress + decompress an empty buffer.
    let empty_compressed = compress(&[], quality, lgwin);
    println!("empty_compressed_len = {}", empty_compressed.len());
    match decompress(&empty_compressed) {
        Some(empty_roundtrip) => {
            println!("empty_roundtrip_len = {}", empty_roundtrip.len());
            println!("empty_roundtrip_ok = {}", empty_roundtrip.is_empty());
        }
        None => {
            println!("empty_roundtrip_ok = false");
        }
    }

    println!("== survey_brotli done ==");
}
