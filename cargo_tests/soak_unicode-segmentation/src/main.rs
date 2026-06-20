//! H2 real-crate SOAK: unicode-segmentation on the dotnet PAL with NO surrogate.
//! Exercises grapheme/word segmentation over a string with combining marks + an emoji.
//! This pulls in the crate's Unicode tables, iterator state machines, char/str scanning,
//! and (via collect) Vec/String + fmt. Panic-safe: no unwraps on fallible data, valid input.
//! SUCCESS = "== soak_unicode-segmentation done ==" with sane counts.
use unicode_segmentation::UnicodeSegmentation;

fn main() {
    println!("== soak_unicode-segmentation start ==");

    // "a" + combining acute, "e", then a grinning-face emoji.
    // Bytes are multibyte; combining mark joins with preceding 'a' into ONE grapheme.
    let s = "a\u{0301}e\u{1F600}";

    println!("1  byte_len={}", s.len());
    println!("2  char_count={}", s.chars().count());

    // Extended grapheme clusters (true = extended).
    let graphemes: Vec<&str> = s.graphemes(true).collect();
    println!("3  grapheme_count={}", graphemes.len());
    for (i, g) in graphemes.iter().enumerate() {
        println!("   g[{}] bytes={} chars={}", i, g.len(), g.chars().count());
    }

    // Word segmentation over a small natural-language sentence (valid input only).
    let sentence = "The quick brown fox jumps";
    let words: Vec<&str> = sentence.unicode_words().collect();
    println!("4  word_count={}", words.len());
    println!("5  first_word={}", words.first().copied().unwrap_or("<none>"));

    // word_indices: pair (offset, word) — exercises a different iterator path.
    let wi_count = sentence.split_word_bound_indices().count();
    println!("6  word_bound_index_count={}", wi_count);

    println!("== soak_unicode-segmentation done ==");
}
