//! H2 real-crate SOAK: lexical-core — fast number parse/write without std::fmt/parse.
//! Exercises: parse::<f64>/<i64> from bytes, write::<f64>/<i64> into a byte buffer, round-trip.
//! Panic-safe: only valid ASCII numeric inputs, all Results handled, no unwrap/expect/indexing.
//! SUCCESS = "== soak_lexical-core done ==" with sane round-tripped values.

fn main() {
    println!("== soak_lexical-core start ==");

    // --- parse i64 from bytes ---
    let int_src = b"-1234567";
    match lexical_core::parse::<i64>(int_src) {
        Ok(n) => println!("1  parse i64: {n}"),
        Err(e) => println!("1  parse i64 err: {e:?}"),
    }

    // --- parse f64 from bytes ---
    let flt_src = b"3.14159265358979";
    let parsed_f = match lexical_core::parse::<f64>(flt_src) {
        Ok(f) => {
            println!("2  parse f64: {f}");
            f
        }
        Err(e) => {
            println!("2  parse f64 err: {e:?}");
            0.0
        }
    };

    // --- write i64 into a buffer ---
    let mut ibuf = [0u8; lexical_core::BUFFER_SIZE];
    let iwritten = lexical_core::write::<i64>(987654321i64, &mut ibuf);
    match core::str::from_utf8(iwritten) {
        Ok(s) => println!("3  write i64: {s} ({} bytes)", iwritten.len()),
        Err(e) => println!("3  write i64 utf8 err: {e}"),
    }

    // --- write f64 into a buffer ---
    let mut fbuf = [0u8; lexical_core::BUFFER_SIZE];
    let fwritten = lexical_core::write::<f64>(parsed_f, &mut fbuf);
    let f_str = match core::str::from_utf8(fwritten) {
        Ok(s) => {
            println!("4  write f64: {s} ({} bytes)", fwritten.len());
            s.to_string()
        }
        Err(e) => {
            println!("4  write f64 utf8 err: {e}");
            String::new()
        }
    };

    // --- round-trip: write then re-parse the f64 ---
    match lexical_core::parse::<f64>(f_str.as_bytes()) {
        Ok(rt) => {
            let diff = (rt - parsed_f).abs();
            println!("5  round-trip f64: {rt} (diff {})", if diff < 1e-9 { "ok" } else { "BIG" });
        }
        Err(e) => println!("5  round-trip parse err: {e:?}"),
    }

    // --- round-trip i64 via buffer ---
    match lexical_core::parse::<i64>(iwritten) {
        Ok(rt) => println!("6  round-trip i64: {rt}"),
        Err(e) => println!("6  round-trip i64 err: {e:?}"),
    }

    // --- a few more integer widths ---
    let mut u32buf = [0u8; lexical_core::BUFFER_SIZE];
    let u32w = lexical_core::write::<u32>(4_000_000_000u32, &mut u32buf);
    match core::str::from_utf8(u32w) {
        Ok(s) => println!("7  write u32: {s}"),
        Err(_) => println!("7  write u32 utf8 err"),
    }

    println!("== soak_lexical-core done ==");
}
