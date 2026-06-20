use base64::engine::general_purpose::STANDARD;
use base64::Engine;

fn main() {
    // Known input bytes -> known base64 output ("aGVsbG8sIHdvcmxkIQ==").
    let data: &[u8] = b"hello, world!";

    // Encode (round-trip step 1). `encode` is infallible.
    let encoded: String = STANDARD.encode(data);
    println!("encoded = {}", encoded);

    // Decode back. `decode` returns Result; handle it without unwrap/expect.
    match STANDARD.decode(encoded.as_bytes()) {
        Ok(decoded) => {
            let matches = decoded.as_slice() == data;
            println!("decoded_len = {}", decoded.len());
            println!("roundtrip_matches = {}", matches);
            // Reconstruct as text only if valid UTF-8 (no panic path).
            match core::str::from_utf8(decoded.as_slice()) {
                Ok(s) => println!("decoded_text = {}", s),
                Err(_) => println!("decoded_text = <non-utf8>"),
            }
        }
        Err(_) => {
            println!("decode_error = true");
        }
    }

    println!("== soak_base64 done ==");
}
