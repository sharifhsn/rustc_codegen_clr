use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

fn main() {
    // ---- String width across script/category classes ----
    // Each label exercises a different width regime:
    //   ASCII (1 col/char), CJK (2 col/char "wide"), combining marks (0 col),
    //   and emoji (2 col / wide). Output is the deterministic terminal width.
    let cases: [(&str, &str); 6] = [
        ("ascii", "hello, world!"),
        ("cjk", "\u{4F60}\u{597D}"),               // 你好 — two wide CJK chars
        ("combining", "e\u{0301}"),                  // e + combining acute accent
        ("emoji", "\u{1F600}"),                      // grinning face
        ("mixed", "a\u{4F60}b"),                      // ASCII + CJK + ASCII
        ("empty", ""),
    ];

    for (label, s) in cases.iter() {
        // width() = "narrow/East-Asian-ambiguous-as-narrow" measure.
        let w = UnicodeWidthStr::width(*s);
        // width_cjk() = treats ambiguous as wide (CJK context).
        let w_cjk = UnicodeWidthStr::width_cjk(*s);
        println!(
            "str.{} = width={} width_cjk={} chars={}",
            label,
            w,
            w_cjk,
            s.chars().count()
        );
    }

    // ---- Per-character width (UnicodeWidthChar) ----
    // Option<usize>: None for control chars; handle without unwrap.
    let chars: [(&str, char); 6] = [
        ("A", 'A'),                  // narrow ASCII -> Some(1)
        ("space", ' '),              // -> Some(1)
        ("cjk_ni", '\u{4F60}'),      // 你 -> Some(2)
        ("combining", '\u{0301}'),   // combining acute -> Some(0)
        ("emoji", '\u{1F600}'),      // grinning -> Some(2)
        ("nul", '\u{0000}'),         // control -> None
    ];

    for (label, c) in chars.iter() {
        match UnicodeWidthChar::width(*c) {
            Some(w) => println!("char.{} = Some({})", label, w),
            None => println!("char.{} = None", label),
        }
        match UnicodeWidthChar::width_cjk(*c) {
            Some(w) => println!("char_cjk.{} = Some({})", label, w),
            None => println!("char_cjk.{} = None", label),
        }
    }

    // ---- A derived aggregate (deterministic integer) ----
    // Sum of widths of the printable ASCII range ' '..='~' (95 chars, all width 1).
    let mut ascii_total: usize = 0;
    for cp in 0x20u32..=0x7Eu32 {
        if let Some(c) = char::from_u32(cp) {
            if let Some(w) = UnicodeWidthChar::width(c) {
                ascii_total += w;
            }
        }
    }
    println!("ascii_printable_total_width = {}", ascii_total);

    println!("== soak_unicode-width done ==");
}
