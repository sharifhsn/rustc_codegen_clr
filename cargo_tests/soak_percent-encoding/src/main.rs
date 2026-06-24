use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC, CONTROLS};

// A custom AsciiSet: like CONTROLS but also escaping a handful of URL-significant
// characters. Deterministic, fully const.
const CUSTOM: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'#')
    .add(b'?')
    .add(b'{')
    .add(b'}');

fn main() {
    // Several deterministic inputs covering ASCII, spaces, reserved chars,
    // and multi-byte UTF-8.
    let inputs: [&str; 5] = [
        "hello world",
        "a/b?c=d&e=f",
        "100% done!",
        "café — résumé",
        "παράδειγμα/路径",
    ];

    for (i, input) in inputs.iter().enumerate() {
        // Encode with NON_ALPHANUMERIC (escapes everything but [A-Za-z0-9]).
        let enc_na = utf8_percent_encode(input, NON_ALPHANUMERIC).to_string();
        // Encode with the custom AsciiSet (lighter touch).
        let enc_custom = utf8_percent_encode(input, CUSTOM).to_string();

        println!("input[{}] = {}", i, input);
        println!("input[{}].len = {}", i, input.len());
        println!("enc_non_alnum[{}] = {}", i, enc_na);
        println!("enc_custom[{}] = {}", i, enc_custom);

        // Round-trip: decode the NON_ALPHANUMERIC encoding back to bytes,
        // then to a str. percent_decode_str -> Cow<[u8]>; decode_utf8 -> Result.
        let decoded = percent_decode_str(&enc_na);
        match decoded.decode_utf8() {
            Ok(s) => {
                let matches = s.as_ref() == *input;
                println!("decoded[{}] = {}", i, s);
                println!("roundtrip_matches[{}] = {}", i, matches);
            }
            Err(_) => {
                println!("decoded[{}] = <non-utf8>", i);
                println!("roundtrip_matches[{}] = false", i);
            }
        }
    }

    // A direct byte-level decode check on a hand-written escape sequence.
    // "%41%42%43" -> "ABC".
    let manual = percent_decode_str("%41%42%43");
    match manual.decode_utf8() {
        Ok(s) => println!("manual_decode = {}", s),
        Err(_) => println!("manual_decode = <non-utf8>"),
    }

    // Decode a sequence with an invalid (incomplete) escape; percent-encoding
    // leaves stray '%' bytes as-is rather than panicking.
    let stray = percent_decode_str("ab%2");
    match stray.decode_utf8() {
        Ok(s) => println!("stray_decode = {}", s),
        Err(_) => println!("stray_decode = <non-utf8>"),
    }

    println!("== soak_percent-encoding done ==");
}
