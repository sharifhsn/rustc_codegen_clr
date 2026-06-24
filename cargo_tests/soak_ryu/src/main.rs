fn main() {
    // ryu::Buffer::format produces the SHORTEST round-tripping decimal string
    // for an f64/f32. The chosen values are exact in binary floating point
    // (or have a stable, well-defined shortest representation), so the output
    // is deterministic run-to-run and across platforms.

    // --- f64 values: all exactly representable, so shortest repr is stable. ---
    let f64_vals: [f64; 8] = [
        0.0,
        1.5,
        -2.25,
        100.0,
        0.5,        // exact (2^-1)
        0.25,       // exact (2^-2)
        1e10,       // 10_000_000_000.0, exact
        4503599627370496.0, // 2^52, exact round value near the f64 mantissa limit
    ];

    for (i, &v) in f64_vals.iter().enumerate() {
        let mut buf = ryu::Buffer::new();
        let s = buf.format(v);
        println!("f64[{}] = {}", i, s);
    }

    // --- f32 values: exactly representable, shortest repr stable. ---
    let f32_vals: [f32; 6] = [
        0.0f32,
        1.5f32,
        -2.25f32,
        100.0f32,
        0.5f32,     // exact
        16777216.0f32, // 2^24, exact round value near the f32 mantissa limit
    ];

    for (i, &v) in f32_vals.iter().enumerate() {
        let mut buf = ryu::Buffer::new();
        let s = buf.format(v);
        println!("f32[{}] = {}", i, s);
    }

    // --- 0.1 is NOT exactly representable; print both its shortest ryu repr
    //     (which ryu guarantees round-trips) and a derived integer check so we
    //     have a value that is robust even if the textual shortest form drifts. ---
    {
        let v: f64 = 0.1;
        let mut buf = ryu::Buffer::new();
        let s = buf.format(v);
        println!("f64_inexact_0.1 = {}", s);
        // Derived deterministic check: scale and round to an integer.
        let scaled = (v * 100.0).round() as i64;
        println!("f64_inexact_0.1_scaled = {}", scaled);
    }

    // --- Round-trip sanity via the standard-library parser (no unwrap). ---
    {
        let original: f64 = 1234.5;
        let mut buf = ryu::Buffer::new();
        let s = buf.format(original);
        println!("roundtrip_src = {}", s);
        match s.parse::<f64>() {
            Ok(parsed) => {
                let bits_match = parsed.to_bits() == original.to_bits();
                println!("roundtrip_bits_match = {}", bits_match);
            }
            Err(_) => {
                println!("roundtrip_bits_match = parse_error");
            }
        }
    }

    println!("== soak_ryu done ==");
}
