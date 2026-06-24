use fixedbitset::FixedBitSet;

// Render a FixedBitSet as a deterministic '0'/'1' pattern string (LSB-index first).
fn pattern(bits: &FixedBitSet) -> String {
    let mut s = String::with_capacity(bits.len());
    for i in 0..bits.len() {
        s.push(if bits.contains(i) { '1' } else { '0' });
    }
    s
}

fn main() {
    // --- Build set A deterministically: insert a fixed set of indices. ---
    let mut a = FixedBitSet::with_capacity(16);
    a.insert(1);
    a.insert(3);
    a.insert(5);
    a.insert(7);
    a.insert(9);
    println!("a_len = {}", a.len());
    println!("a_pattern = {}", pattern(&a));
    println!("a_count_ones = {}", a.count_ones(..));
    println!("a_contains_3 = {}", a.contains(3));
    println!("a_contains_4 = {}", a.contains(4));

    // --- put(): set a bit, returns previous value. ---
    let prev_put_4 = a.put(4); // was clear -> false
    let prev_put_5 = a.put(5); // was set   -> true
    println!("put_4_prev = {}", prev_put_4);
    println!("put_5_prev = {}", prev_put_5);
    println!("a_after_put_pattern = {}", pattern(&a));

    // --- toggle(): flip bits. ---
    a.toggle(0); // clear -> set
    a.toggle(1); // set   -> clear
    println!("a_after_toggle_pattern = {}", pattern(&a));
    println!("a_after_toggle_count = {}", a.count_ones(..));

    // --- Build set B. ---
    let mut b = FixedBitSet::with_capacity(16);
    b.insert(2);
    b.insert(3);
    b.insert(4);
    b.insert(5);
    b.insert(6);
    println!("b_pattern = {}", pattern(&b));
    println!("b_count_ones = {}", b.count_ones(..));

    // --- Set ops: union / intersection / difference into fresh copies. ---
    let mut u = a.clone();
    u.union_with(&b);
    println!("union_pattern = {}", pattern(&u));
    println!("union_count = {}", u.count_ones(..));

    let mut inter = a.clone();
    inter.intersect_with(&b);
    println!("intersection_pattern = {}", pattern(&inter));
    println!("intersection_count = {}", inter.count_ones(..));

    // difference(&b) yields an iterator of indices in `a` not in `b`.
    let diff_indices: Vec<usize> = a.difference(&b).collect();
    println!("difference_indices = {:?}", diff_indices);
    println!("difference_count = {}", diff_indices.len());

    // --- ones() iteration is in ascending index order (deterministic). ---
    let union_ones: Vec<usize> = u.ones().collect();
    println!("union_ones = {:?}", union_ones);

    // Derived integer: sum of set indices in the union (exercises iteration + arithmetic).
    let mut idx_sum: usize = 0;
    for i in u.ones() {
        idx_sum += i;
    }
    println!("union_index_sum = {}", idx_sum);

    // is_clear / clear round-trip on a fresh set.
    let mut c = FixedBitSet::with_capacity(8);
    println!("c_is_clear_initial = {}", c.is_clear());
    c.insert(7);
    println!("c_is_clear_after_insert = {}", c.is_clear());
    c.clear();
    println!("c_is_clear_after_clear = {}", c.is_clear());

    println!("== soak_fixedbitset done ==");
}
