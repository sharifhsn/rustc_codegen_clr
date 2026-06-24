// Exercise itoa::Buffer over edge-value i64/u64/i32 inputs.
// itoa formats integers into a stack Buffer and returns a &str; it is
// infallible (no Result), no allocation, no syscalls — fully deterministic.

fn fmt_i64(v: i64) -> String {
    let mut buf = itoa::Buffer::new();
    buf.format(v).to_string()
}

fn fmt_u64(v: u64) -> String {
    let mut buf = itoa::Buffer::new();
    buf.format(v).to_string()
}

fn fmt_i32(v: i32) -> String {
    let mut buf = itoa::Buffer::new();
    buf.format(v).to_string()
}

fn main() {
    // i64 edge values: MIN, -1, 0, 1, MAX, plus a couple of mid negatives.
    let i64_vals: [i64; 8] = [
        i64::MIN,
        -9_223_372_036_854_775_807,
        -1234567890,
        -1,
        0,
        1,
        9_223_372_036_854_775_806,
        i64::MAX,
    ];
    for v in i64_vals.iter() {
        let s = fmt_i64(*v);
        // Cross-check against the std Display formatting; itoa must agree.
        let agrees = s == format!("{}", v);
        println!("i64 {:>20} = {} (agrees={})", v, s, agrees);
    }

    // u64 edge values: 0, 1, MAX, and a large round-ish value.
    let u64_vals: [u64; 5] = [
        0,
        1,
        1_000_000_000_000,
        18_446_744_073_709_551_614,
        u64::MAX,
    ];
    for v in u64_vals.iter() {
        let s = fmt_u64(*v);
        let agrees = s == format!("{}", v);
        println!("u64 {:>20} = {} (agrees={})", v, s, agrees);
    }

    // i32 edge values: MIN, -1, 0, 1, MAX.
    let i32_vals: [i32; 5] = [i32::MIN, -1, 0, 1, i32::MAX];
    for v in i32_vals.iter() {
        let s = fmt_i32(*v);
        let agrees = s == format!("{}", v);
        println!("i32 {:>11} = {} (agrees={})", v, s, agrees);
    }

    // Length checks on the extreme reprs (digit counts are stable facts).
    println!("len_i64_min = {}", fmt_i64(i64::MIN).len());
    println!("len_u64_max = {}", fmt_u64(u64::MAX).len());
    println!("len_i32_min = {}", fmt_i32(i32::MIN).len());

    // Aggregate: do every formatted value agree with std Display?
    let all_agree = i64_vals.iter().all(|v| fmt_i64(*v) == format!("{}", v))
        && u64_vals.iter().all(|v| fmt_u64(*v) == format!("{}", v))
        && i32_vals.iter().all(|v| fmt_i32(*v) == format!("{}", v));
    println!("all_agree = {}", all_agree);

    println!("== soak_itoa done ==");
}
