//! H2 real-crate SOAK: the `unicode-normalization` crate on the dotnet PAL.
//! NFC/NFD normalization of strings with combining characters (exercises the
//! Unicode decomposition/composition tables + iterator chains over chars).
//! Panic-safe: fixed valid &str inputs, no unwraps/indexing that can fail.
//! SUCCESS = "== soak_unicode-normalization done ==".
use unicode_normalization::UnicodeNormalization;

fn main() {
    println!("== soak_unicode-normalization start ==");

    // "é" written as base 'e' + combining acute accent (U+0301): 2 chars, NFC -> 1 char.
    let combining = "e\u{0301}";
    let nfc: String = combining.nfc().collect();
    let nfd: String = combining.nfd().collect();
    println!("1  src chars: {}", combining.chars().count());
    println!("2  nfc chars: {}", nfc.chars().count());
    println!("3  nfd chars: {}", nfd.chars().count());
    println!("4  nfc bytes: {}", nfc.len());
    println!("5  nfd bytes: {}", nfd.len());

    // Precomposed "é" (U+00E9): NFD should decompose it into 2 chars.
    let precomposed = "\u{00e9}";
    let pre_nfd: String = precomposed.nfd().collect();
    let pre_nfc: String = precomposed.nfc().collect();
    println!("6  precomp src chars: {}", precomposed.chars().count());
    println!("7  precomp nfd chars: {}", pre_nfd.chars().count());
    println!("8  precomp nfc chars: {}", pre_nfc.chars().count());

    // Round-trip: NFC(NFD(x)) should re-compose back to the precomposed form.
    let roundtrip: String = pre_nfd.nfc().collect();
    println!("9  roundtrip == precomposed: {}", roundtrip == precomposed);

    // A longer mixed string with several combining marks.
    let mixed = "a\u{0300}e\u{0301}i\u{0302}o\u{0303}u\u{0308}";
    let mixed_nfc: String = mixed.nfc().collect();
    let mixed_nfd: String = mixed.nfd().collect();
    println!("10 mixed src chars: {}", mixed.chars().count());
    println!("11 mixed nfc chars: {}", mixed_nfc.chars().count());
    println!("12 mixed nfd chars: {}", mixed_nfd.chars().count());

    // ASCII passthrough: normalization should be a no-op for plain ASCII.
    let ascii = "hello world";
    let ascii_nfc: String = ascii.nfc().collect();
    println!("13 ascii nfc unchanged: {}", ascii_nfc == ascii);

    println!("== soak_unicode-normalization done ==");
}
