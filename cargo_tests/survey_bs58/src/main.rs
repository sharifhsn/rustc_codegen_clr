// Survey crate for the `bs58` base58 codec.
// Deterministic: fixed input bytes, fixed-alphabet encode/decode round-trip.
// No RNG, no time, no threads, no I/O beyond stdout. Output is byte-stable.

fn main() {
    // Fixed input bytes -> a known, stable base58 string.
    // "hello, world!" in the default Bitcoin alphabet encodes to "tQEJ4hXg9k5h4r5fAyW".
    let data: &[u8] = b"hello, world!";

    // Encode (round-trip step 1). `into_string` is infallible.
    let encoded: String = bs58::encode(data).into_string();
    println!("encoded = {}", encoded);
    println!("encoded_len = {}", encoded.len());

    // Decode back. `into_vec` returns Result; handle it without unwrap/expect.
    match bs58::decode(&encoded).into_vec() {
        Ok(decoded) => {
            let matches = decoded.as_slice() == data;
            println!("decoded_len = {}", decoded.len());
            println!("roundtrip_matches = {}", matches);
            match core::str::from_utf8(decoded.as_slice()) {
                Ok(s) => println!("decoded_text = {}", s),
                Err(_) => println!("decoded_text = <non-utf8>"),
            }
        }
        Err(_) => {
            println!("decode_error = true");
        }
    }

    // A second fixed vector: all byte values 0x00..=0x0F (exercises leading-zero handling).
    // Leading zero bytes map to leading '1' chars in base58.
    let bytes2: [u8; 16] = [
        0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
        0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
    ];
    let enc2: String = bs58::encode(&bytes2).into_string();
    println!("enc2 = {}", enc2);
    // Count the leading '1' chars deterministically (should equal the 2 leading zero bytes).
    let leading_ones = enc2.chars().take_while(|&c| c == '1').count();
    println!("enc2_leading_ones = {}", leading_ones);

    match bs58::decode(&enc2).into_vec() {
        Ok(decoded2) => {
            println!("enc2_roundtrip_matches = {}", decoded2.as_slice() == &bytes2[..]);
        }
        Err(_) => {
            println!("enc2_decode_error = true");
        }
    }

    // Decode a deliberately invalid string (contains '0', not in the base58 alphabet).
    // Must return an Err, no panic.
    match bs58::decode("invalid0string").into_vec() {
        Ok(_) => println!("invalid_decoded_ok = true"),
        Err(_) => println!("invalid_rejected = true"),
    }

    // Empty input: encodes to empty string, decodes back to empty vec.
    let empty_enc: String = bs58::encode(b"").into_string();
    println!("empty_enc_is_empty = {}", empty_enc.is_empty());
    match bs58::decode("").into_vec() {
        Ok(v) => println!("empty_decoded_len = {}", v.len()),
        Err(_) => println!("empty_decode_error = true"),
    }

    println!("== survey_bs58 done ==");
}
