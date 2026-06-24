use bitvec::prelude::*;

// Render a BitVec as a deterministic "0/1" string.
fn render(bits: &BitVec<u8, Msb0>) -> String {
    let mut s = String::with_capacity(bits.len());
    for b in bits.iter() {
        s.push(if *b { '1' } else { '0' });
    }
    s
}

fn main() {
    // --- Build a BitVec<u8, Msb0> by pushing individual bits. ---
    // Pattern: 1 0 1 1 0 0 1 0  1 1  (10 bits) -> deterministic.
    let mut a: BitVec<u8, Msb0> = BitVec::new();
    let pattern = [true, false, true, true, false, false, true, false, true, true];
    for &bit in pattern.iter() {
        a.push(bit);
    }
    println!("a_len = {}", a.len());
    println!("a_bits = {}", render(&a));
    println!("a_count_ones = {}", a.count_ones());
    println!("a_count_zeros = {}", a.count_zeros());

    // --- set/clear ranges via fill within an index range (no panic path). ---
    // Make a fresh 16-bit vector, all zeros, then set a middle range to 1,
    // then clear a sub-range back to 0.
    let mut b: BitVec<u8, Msb0> = BitVec::repeat(false, 16);
    println!("b_init = {}", render(&b));

    // Set bits [4, 12) to true using a sub-slice fill.
    b[4..12].fill(true);
    println!("b_after_set = {}", render(&b));
    println!("b_set_ones = {}", b.count_ones());

    // Clear bits [6, 9) back to false.
    b[6..9].fill(false);
    println!("b_after_clear = {}", render(&b));
    println!("b_clear_ones = {}", b.count_ones());

    // Individual set() of a couple of indices (bounds-checked manually).
    if b.len() > 0 {
        b.set(0, true);
    }
    if b.len() > 15 {
        b.set(15, true);
    }
    println!("b_after_endpoints = {}", render(&b));

    // --- bitand / bitor with another BitVec of equal length. ---
    // Two 8-bit operands with known content.
    let x: BitVec<u8, Msb0> = bitvec![u8, Msb0; 1, 1, 0, 0, 1, 0, 1, 0];
    let y: BitVec<u8, Msb0> = bitvec![u8, Msb0; 1, 0, 1, 0, 1, 1, 0, 0];
    println!("x_bits = {}", render(&x));
    println!("y_bits = {}", render(&y));

    let and = x.clone() & y.clone();
    let or = x.clone() | y.clone();
    let xor = x.clone() ^ y.clone();
    println!("x_and_y = {}", render(&and));
    println!("x_or_y = {}", render(&or));
    println!("x_xor_y = {}", render(&xor));
    println!("and_ones = {}", and.count_ones());
    println!("or_ones = {}", or.count_ones());
    println!("xor_ones = {}", xor.count_ones());

    // --- iterate and compute a derived integer (sum of set-bit indices). ---
    let mut idx_sum: usize = 0;
    for (i, bit) in a.iter().enumerate() {
        if *bit {
            idx_sum += i;
        }
    }
    println!("a_set_index_sum = {}", idx_sum);

    // --- leading/trailing helpers exercise more bit codegen. ---
    println!("a_leading_ones = {}", a.leading_ones());
    println!("a_trailing_zeros = {}", a.trailing_zeros());
    println!("b_leading_zeros = {}", b.leading_zeros());

    // --- first_one / last_one (Option, handled without unwrap). ---
    match and.first_one() {
        Some(i) => println!("and_first_one = {}", i),
        None => println!("and_first_one = none"),
    }
    match or.last_one() {
        Some(i) => println!("or_last_one = {}", i),
        None => println!("or_last_one = none"),
    }

    // --- not (complement) of a small vector. ---
    let notx = !x.clone();
    println!("not_x = {}", render(&notx));
    println!("not_x_ones = {}", notx.count_ones());

    println!("== soak_bitvec done ==");
}
