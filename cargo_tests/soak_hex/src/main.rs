//! H2 real-crate SOAK: hex (0.4) encode/decode round-trip on the dotnet PAL.
//! Exercises hex::encode (infallible, returns String) and hex::decode (returns Result),
//! Vec<u8>/String, slice compares, and core::str. Panic-safe: no unwrap/expect on
//! fallible paths, valid inputs only. SUCCESS = "== soak_hex done ==".

fn main() {
    // Known input bytes -> known lowercase hex output "68656c6c6f2c20776f726c6421".
    let data: &[u8] = b"hello, world!";

    // Encode (round-trip step 1). `hex::encode` is infallible, returns String.
    let encoded: String = hex::encode(data);
    println!("encoded = {}", encoded);
    println!("encoded_len = {}", encoded.len());

    // Upper-case variant exercises a second code path.
    let encoded_upper: String = hex::encode_upper(data);
    println!("encoded_upper = {}", encoded_upper);

    // Decode back. `hex::decode` returns Result; handle it without unwrap/expect.
    match hex::decode(&encoded) {
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

    // decode_to_slice fixed-buffer path (also Result, no panic on valid input).
    let mut buf = [0u8; 13];
    match hex::decode_to_slice(&encoded, &mut buf) {
        Ok(()) => println!("decode_to_slice_matches = {}", &buf[..] == data),
        Err(_) => println!("decode_to_slice_error = true"),
    }

    // Decoding invalid hex must return Err (not panic) — exercises the error path safely.
    match hex::decode("zz") {
        Ok(_) => println!("invalid_decode = unexpectedly_ok"),
        Err(_) => println!("invalid_decode = err_as_expected"),
    }

    println!("== soak_hex done ==");
}
