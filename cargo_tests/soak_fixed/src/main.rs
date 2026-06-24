use fixed::types::{I32F32, U16F16};

fn main() {
    // ---- I32F32: signed 32.32 fixed-point ----
    // Pick values that are EXACTLY representable in binary fixed-point so
    // to_string() / Display is fully deterministic (no shortest-repr drift).
    // 12.5 = 12 + 1/2, 4.25 = 4 + 1/4 are exact in base-2.
    let a = I32F32::from_num(12.5);
    let b = I32F32::from_num(4.25);

    let add = a + b; // 16.75
    let sub = a - b; // 8.25
    let mul = a * b; // 53.125
    // Division: result may be non-terminating in binary, so derive a stable
    // integer rather than printing the raw Display. (a / b) ≈ 2.941176...
    let div = a / b;
    // Scale by 1_000_000 and round to nearest integer for a deterministic value.
    let div_scaled = (div * I32F32::from_num(1_000_000)).round().to_num::<i64>();

    println!("i32f32_a = {}", a);
    println!("i32f32_b = {}", b);
    println!("i32f32_add = {}", add);
    println!("i32f32_sub = {}", sub);
    println!("i32f32_mul = {}", mul);
    println!("i32f32_div_scaled = {}", div_scaled);
    println!("i32f32_cmp_a_gt_b = {}", a > b);
    println!("i32f32_cmp_eq = {}", add == sub + (b + b + b + sub - sub)); // false, exercises ==

    // from_num round-trip via integer (exact).
    let from_int = I32F32::from_num(1000i64);
    println!("i32f32_from_int = {}", from_int);
    println!("i32f32_to_int = {}", from_int.to_num::<i64>());

    // Negative arithmetic (signed type).
    let neg = b - a; // -8.25
    println!("i32f32_neg = {}", neg);
    println!("i32f32_neg_is_negative = {}", neg < I32F32::from_num(0));

    // ---- U16F16: unsigned 16.16 fixed-point ----
    // 3.75 = 3 + 3/4, 2.5 = 2 + 1/2 are exact.
    let c = U16F16::from_num(3.75);
    let d = U16F16::from_num(2.5);

    let uadd = c + d; // 6.25
    let usub = c - d; // 1.25
    let umul = c * d; // 9.375
    let udiv = c / d; // 1.5 (exact: 3.75 / 2.5)

    println!("u16f16_c = {}", c);
    println!("u16f16_d = {}", d);
    println!("u16f16_add = {}", uadd);
    println!("u16f16_sub = {}", usub);
    println!("u16f16_mul = {}", umul);
    println!("u16f16_div = {}", udiv);
    println!("u16f16_cmp_c_gt_d = {}", c > d);

    // Parse from string (returns Result; handle without unwrap).
    match "100.5".parse::<U16F16>() {
        Ok(parsed) => {
            println!("u16f16_parsed = {}", parsed);
            println!("u16f16_parsed_x2 = {}", parsed + parsed); // 201
        }
        Err(_) => {
            println!("u16f16_parse_error = true");
        }
    }

    // Parse an invalid string -> deterministic marker, no panic.
    match "not_a_number".parse::<U16F16>() {
        Ok(_) => println!("u16f16_bad_parse = unexpected_ok"),
        Err(_) => println!("u16f16_bad_parse = err_as_expected"),
    }

    // Bit-level determinism: raw fixed-point bit representation is exact.
    println!("i32f32_add_bits = {}", add.to_bits());
    println!("u16f16_mul_bits = {}", umul.to_bits());

    println!("== soak_fixed done ==");
}
