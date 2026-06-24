use unicode_xid::UnicodeXID;

fn main() {
    // A fixed, ordered set of known chars spanning ascii letters, digits,
    // CJK, symbols, whitespace, and combining marks. Order is hard-coded
    // (an array literal), so iteration is fully deterministic.
    let chars: [(char, &str); 14] = [
        ('A', "ascii_upper"),
        ('z', "ascii_lower"),
        ('_', "underscore"),
        ('0', "digit_zero"),
        ('9', "digit_nine"),
        (' ', "space"),
        ('+', "plus_symbol"),
        ('-', "hyphen"),
        ('$', "dollar"),
        ('\u{4E2D}', "cjk_zhong"),  // 中
        ('\u{6587}', "cjk_wen"),    // 文
        ('\u{00E9}', "latin_e_acute"), // é
        ('\u{0301}', "combining_acute"), // combining mark
        ('\u{03B1}', "greek_alpha"), // α
    ];

    // Header for the bool table.
    println!("char_label = is_xid_start , is_xid_continue");

    // Tally for a derived deterministic summary at the end.
    let mut start_count: u32 = 0;
    let mut continue_count: u32 = 0;

    for (c, label) in chars.iter() {
        let is_start = UnicodeXID::is_xid_start(*c);
        let is_continue = UnicodeXID::is_xid_continue(*c);
        if is_start {
            start_count += 1;
        }
        if is_continue {
            continue_count += 1;
        }
        // Print the codepoint as a stable integer (no glyph-rendering
        // ambiguity), plus the two bools.
        println!(
            "{} (U+{:04X}) = {} , {}",
            label, *c as u32, is_start, is_continue
        );
    }

    println!("xid_start_count = {}", start_count);
    println!("xid_continue_count = {}", continue_count);

    println!("== soak_unicode-xid done ==");
}
