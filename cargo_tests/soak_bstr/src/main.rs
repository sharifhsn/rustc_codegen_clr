use bstr::{BStr, ByteSlice};

fn main() {
    // BStr from a byte slice, printed via its Display/Debug-safe formatting.
    let data: &[u8] = b"hello world, Hello BSTR";
    let bs: &BStr = BStr::new(data);
    println!("bstr = {}", bs);
    println!("len = {}", bs.len());

    // split_str: split on a substring, collect into owned Vec<u8> chunks (no indexing).
    let mut nparts = 0usize;
    for part in data.split_str(" ") {
        // `part` is &[u8]; print as lossy UTF-8 (never panics).
        println!("part[{}] = {}", nparts, part.as_bstr());
        nparts += 1;
    }
    println!("nparts = {}", nparts);

    // find: returns Option<usize> — handle the None case explicitly.
    match data.find("world") {
        Some(idx) => println!("found 'world' at {}", idx),
        None => println!("'world' not found"),
    }
    match data.find("absent-needle") {
        Some(idx) => println!("found 'absent' at {}", idx),
        None => println!("'absent-needle' not found"),
    }

    // find_byte: another Option-returning API.
    match data.find_byte(b',') {
        Some(idx) => println!("comma at {}", idx),
        None => println!("no comma"),
    }

    // to_uppercase / to_lowercase on a byte string -> owned Vec<u8>.
    let upper = data.to_uppercase();
    println!("upper = {}", upper.as_bstr());
    let lower = data.to_lowercase();
    println!("lower = {}", lower.as_bstr());

    // Also exercise the &str -> bytes path.
    let s: &str = "Café au lait";
    let sb = s.as_bytes().as_bstr();
    println!("str-as-bstr = {}", sb);
    println!("str-upper = {}", s.as_bytes().to_uppercase().as_bstr());

    // contains_str: boolean predicate.
    println!("contains 'BSTR' = {}", data.contains_str("BSTR"));
    println!("contains 'xyz' = {}", data.contains_str("xyz"));

    // Count chars (Unicode-aware) without panicking.
    let nchars = sb.chars().count();
    println!("nchars = {}", nchars);

    println!("== soak_bstr done ==");
}
