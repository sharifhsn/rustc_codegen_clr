//! H2 real-crate SOAK: data-encoding (2.x) BASE32 / HEXLOWER encode + decode
//! round-trip on the dotnet PAL. Exercises the constant `Encoding` tables, the
//! lookup/bit-packing encode path (-> String) and the fallible decode path
//! (-> Result<Vec<u8>, DecodeError>). Panic-safe: valid inputs only, every
//! fallible call is matched (no unwrap/expect/indexing-that-can-fail).
//! SUCCESS = "== soak_data-encoding done ==".

use data_encoding::{BASE32, HEXLOWER};

fn main() {
    let data: &[u8] = b"hello, world!";

    // ---- HEXLOWER round-trip ----
    // encode is infallible, returns String. Known output for this input:
    // "68656c6c6f2c20776f726c6421".
    let hex_enc: String = HEXLOWER.encode(data);
    println!("hex_enc = {}", hex_enc);
    println!("hex_enc_len = {}", hex_enc.len());

    match HEXLOWER.decode(hex_enc.as_bytes()) {
        Ok(decoded) => {
            println!("hex_decoded_len = {}", decoded.len());
            println!("hex_roundtrip_matches = {}", decoded.as_slice() == data);
        }
        Err(_) => println!("hex_decode_error = true"),
    }

    // ---- BASE32 round-trip ----
    // encode is infallible, returns String. Padding ('=') path included.
    let b32_enc: String = BASE32.encode(data);
    println!("b32_enc = {}", b32_enc);
    println!("b32_enc_len = {}", b32_enc.len());

    match BASE32.decode(b32_enc.as_bytes()) {
        Ok(decoded) => {
            println!("b32_decoded_len = {}", decoded.len());
            let matches = decoded.as_slice() == data;
            println!("b32_roundtrip_matches = {}", matches);
            match core::str::from_utf8(decoded.as_slice()) {
                Ok(s) => println!("b32_decoded_text = {}", s),
                Err(_) => println!("b32_decoded_text = <non-utf8>"),
            }
        }
        Err(_) => println!("b32_decode_error = true"),
    }

    // ---- Error path (exercised safely, must return Err not panic) ----
    // '!' is not a valid base32 symbol.
    match BASE32.decode(b"!!!!!!!!") {
        Ok(_) => println!("invalid_b32 = unexpectedly_ok"),
        Err(_) => println!("invalid_b32 = err_as_expected"),
    }

    // encode_len is a pure arithmetic helper over the encoding's bit width.
    println!("hex_encode_len_for_13 = {}", HEXLOWER.encode_len(13));
    println!("b32_encode_len_for_13 = {}", BASE32.encode_len(13));

    println!("== soak_data-encoding done ==");
}
