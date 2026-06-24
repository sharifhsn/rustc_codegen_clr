use encoding_rs::{SHIFT_JIS, WINDOWS_1252};

/// Print a string as a deterministic sequence of unicode escapes (U+XXXX),
/// so the output never depends on terminal encoding / font rendering.
fn escape_codepoints(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i != 0 {
            out.push(' ');
        }
        out.push_str(&format!("U+{:04X}", c as u32));
    }
    out
}

/// Lowercase hex of a byte slice, space-separated, deterministic.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if i != 0 {
            out.push(' ');
        }
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn main() {
    // ---- windows-1252 decode ----
    // 0x80 = EURO SIGN (U+20AC), 0x48='H' 0x65='e' 0x6C='l' 0x6C='l' 0x6F='o',
    // 0xE9 = 'é' (U+00E9), 0x99 = TRADE MARK SIGN (U+2122).
    let w1252_bytes: &[u8] = &[0x80, 0x48, 0x65, 0x6C, 0x6C, 0x6F, 0xE9, 0x99];
    let (w_decoded, _enc, w_had_errors) = WINDOWS_1252.decode(w1252_bytes);
    println!("w1252_decode_chars = {}", escape_codepoints(&w_decoded));
    println!("w1252_decode_len = {}", w_decoded.chars().count());
    println!("w1252_decode_had_errors = {}", w_had_errors);

    // ---- shift_jis decode ----
    // 0x82A0 = 'あ' (U+3042 HIRAGANA A), 0x82A2 = 'い' (U+3044),
    // 0x4B='K' 0x61='a' (ASCII pass-through), 0x93FA = '日' (U+65E5).
    let sjis_bytes: &[u8] = &[0x82, 0xA0, 0x82, 0xA2, 0x4B, 0x61, 0x93, 0xFA];
    let (s_decoded, _enc, s_had_errors) = SHIFT_JIS.decode(sjis_bytes);
    println!("sjis_decode_chars = {}", escape_codepoints(&s_decoded));
    println!("sjis_decode_len = {}", s_decoded.chars().count());
    println!("sjis_decode_had_errors = {}", s_had_errors);

    // ---- encode String back to bytes (round-trip) ----
    // windows-1252 encode of the decoded string -> should reproduce the input bytes.
    let (w_encoded, _enc, w_enc_unmappable) = WINDOWS_1252.encode(&w_decoded);
    println!("w1252_encode_hex = {}", hex(&w_encoded));
    println!("w1252_encode_unmappable = {}", w_enc_unmappable);
    println!(
        "w1252_roundtrip_matches = {}",
        w_encoded.as_ref() == w1252_bytes
    );

    // shift_jis encode of the decoded string -> should reproduce the input bytes.
    let (s_encoded, _enc, s_enc_unmappable) = SHIFT_JIS.encode(&s_decoded);
    println!("sjis_encode_hex = {}", hex(&s_encoded));
    println!("sjis_encode_unmappable = {}", s_enc_unmappable);
    println!(
        "sjis_roundtrip_matches = {}",
        s_encoded.as_ref() == sjis_bytes
    );

    // ---- encode a fresh UTF-8 String into each encoding ----
    // Exercises the big static encode tables in both directions on new input.
    let text = "Été 2024 \u{20AC}"; // 'É','t','é',' ','2','0','2','4',' ', EURO
    let (text_w1252, _enc, _u) = WINDOWS_1252.encode(text);
    println!("text_w1252_hex = {}", hex(&text_w1252));

    let jp = "東京タワー"; // Tokyo Tower, all in shift_jis range
    let (jp_sjis, _enc, jp_unmappable) = SHIFT_JIS.encode(jp);
    println!("jp_sjis_hex = {}", hex(&jp_sjis));
    println!("jp_sjis_unmappable = {}", jp_unmappable);

    // Re-decode that shift_jis blob to confirm the table round-trips.
    let (jp_back, _enc, jp_back_errors) = SHIFT_JIS.decode(&jp_sjis);
    println!("jp_roundtrip_matches = {}", jp_back.as_ref() == jp);
    println!("jp_back_had_errors = {}", jp_back_errors);

    // ---- unmappable case: a char with no windows-1252 mapping ----
    // U+3042 ('あ') is not representable in windows-1252; encode replaces it.
    let (bad, _enc, bad_unmappable) = WINDOWS_1252.encode("あ");
    println!("w1252_unmappable_hex = {}", hex(&bad));
    println!("w1252_unmappable_flag = {}", bad_unmappable);

    println!("== soak_encoding_rs done ==");
}
