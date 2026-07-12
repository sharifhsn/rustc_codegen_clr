// Repro for the fat-pointer-nesting typecheck flags:
//   (1) LocalAssigementWrong  got: FatPtr<u8>          expected: FatPtr<FatPtr<u8>>
//   (2) CallArgTypeWrong on String::push_str           same shape
//
// FatPtr<u8>          = &str / &[u8]   (data ptr + len)
// FatPtr<FatPtr<u8>>  = &[&str]        (slice whose element is itself a fat ptr)
//
// The hypothesis: push_str / join over slices-of-&str exercise the nested fat ptr.
// We run many inputs incl. empty, multibyte, large, to catch a wrong-memory read.

use std::hint::black_box;

fn build_via_push(parts: &[&str]) -> String {
    let mut s = String::new();
    for p in parts {
        s.push_str(p);   // String::push_str(&self, &str) — &str arg = FatPtr<u8>
        s.push_str("|"); // separator
    }
    s
}

fn join_slice(parts: &[&str]) -> String {
    // join takes &[&str] (FatPtr<FatPtr<u8>>) and pushes each &str.
    parts.join(",")
}

fn concat_slice(parts: &[&str]) -> String {
    parts.concat()
}

fn sum_lens(parts: &[&str]) -> usize {
    // Iterate the slice-of-&str, reading each fat ptr's len.
    let mut total = 0usize;
    for p in parts {
        total += p.len();
    }
    total
}

fn expected_bytes(parts: &[&str], separator: &[u8], trailing_separator: bool) -> Vec<u8> {
    let mut expected = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        if index != 0 {
            expected.extend_from_slice(separator);
        }
        expected.extend_from_slice(part.as_bytes());
    }
    if trailing_separator && !parts.is_empty() {
        expected.extend_from_slice(separator);
    }
    expected
}

fn main() {
    let cases: Vec<Vec<&str>> = vec![
        vec![],
        vec![""],
        vec!["a"],
        vec!["a", "b", "c"],
        vec!["hello", "world"],
        vec!["", "x", ""],
        vec!["héllo", "wörld", "Ω≈ç"],          // multibyte UTF-8
        vec!["日本語", "テスト", "文字列"],         // CJK multibyte
        vec!["the", "quick", "brown", "fox", "jumps"],
    ];

    // A large case: many longish strings.
    let big_owned: Vec<String> = (0..200).map(|i| format!("segment-number-{i:04}")).collect();
    let big: Vec<&str> = big_owned.iter().map(|s| s.as_str()).collect();

    for (i, parts) in cases.iter().enumerate() {
        let parts = black_box(parts.as_slice());
        let pushed = build_via_push(parts);
        let joined = join_slice(parts);
        let concated = concat_slice(parts);
        let lens = sum_lens(parts);
        assert_eq!(pushed.as_bytes(), expected_bytes(parts, b"|", true));
        assert_eq!(joined.as_bytes(), expected_bytes(parts, b",", false));
        assert_eq!(concated.as_bytes(), expected_bytes(parts, b"", false));
        assert_eq!(lens, expected_bytes(parts, b"", false).len());
        println!(
            "case {i}: push={pushed:?} join={joined:?} concat={concated:?} lens={lens}"
        );
    }

    let big_slice = black_box(big.as_slice());
    let big_joined = join_slice(big_slice);
    let big_lens = sum_lens(big_slice);
    assert_eq!(big_joined.as_bytes(), expected_bytes(big_slice, b",", false));
    assert_eq!(big_lens, expected_bytes(big_slice, b"", false).len());
    println!("big: join.len()={} lens={}", big_joined.len(), big_lens);
    // Spot-check a few bytes of the big join so a wrong-memory read would corrupt it.
    println!("big: first40={:?}", &big_joined[..40.min(big_joined.len())]);
    println!("big: last20={:?}", &big_joined[big_joined.len().saturating_sub(20)..]);

    // Direct push_str of nested borrows: &&str then deref.
    let s_owned = String::from("nested");
    let r: &str = &s_owned;
    let rr: &&str = &r;
    let mut out = String::new();
    out.push_str(*rr);
    out.push_str(black_box(rr));
    assert_eq!(out, "nestednested");
    println!("nested push: {out:?}");

    println!("cd_fatptr: done");
    println!("== cd_fatptr done ==");
}
