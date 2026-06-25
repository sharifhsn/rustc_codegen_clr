use rayon::prelude::*;

fn main() {
    // --- Parallel map+sum over a fixed range (work-stealing thread pool). ---
    // Sum of squares 0^2 + 1^2 + ... + 999^2 = 332833500 (a fixed, order-independent
    // reduction, so the parallel result is deterministic regardless of scheduling).
    let sum_sq: u64 = (0..1000u64).into_par_iter().map(|x| x * x).sum();
    println!("sum_of_squares = {}", sum_sq);

    // Cross-check against the closed-form n(n-1)(2n-1)/6 for n = 1000.
    let n: u64 = 1000;
    let closed_form = (n - 1) * n * (2 * n - 1) / 6;
    println!("closed_form_matches = {}", sum_sq == closed_form);

    // --- Parallel reduce (associative, commutative -> deterministic). ---
    // Sum of 1..=1000 = 500500.
    let sum_lin: u64 = (1..=1000u64).into_par_iter().reduce(|| 0, |a, b| a + b);
    println!("sum_linear = {}", sum_lin);

    // --- Parallel filter+count over a fixed range. ---
    // Count of even numbers in 0..1000 = 500.
    let evens: usize = (0..1000u64).into_par_iter().filter(|x| x % 2 == 0).count();
    println!("even_count = {}", evens);

    // --- Parallel sort of a fixed vector (result is deterministic). ---
    let mut data: Vec<i32> = vec![
        37, -5, 100, 0, 42, -100, 7, 7, 256, -1, 999, 12, 88, -42, 3, 64, -64, 1, 500, -500,
    ];
    let original_len = data.len();
    data.par_sort();
    // Head + tail of the sorted vec are deterministic.
    let head = data.first().copied().unwrap_or(i32::MIN);
    let tail = data.last().copied().unwrap_or(i32::MAX);
    println!("sorted_len = {}", original_len);
    println!("sorted_head = {}", head);
    println!("sorted_tail = {}", tail);

    // Verify the vector is actually sorted (non-decreasing) without panicking.
    let is_sorted = data.windows(2).all(|w| w[0] <= w[1]);
    println!("is_sorted = {}", is_sorted);

    // Print first five sorted elements as a labeled, deterministic line.
    let mut head5 = String::new();
    for (i, v) in data.iter().take(5).enumerate() {
        if i > 0 {
            head5.push(',');
        }
        head5.push_str(&v.to_string());
    }
    println!("sorted_head5 = {}", head5);

    // --- Parallel sort_by (descending) of a copy -> deterministic ordering. ---
    let mut desc = data.clone();
    desc.par_sort_by(|a, b| b.cmp(a));
    let desc_head = desc.first().copied().unwrap_or(i32::MIN);
    println!("desc_head = {}", desc_head);

    // --- map-reduce pipeline: sum of cubes of 1..=20 = 44100. ---
    let cubes: u64 = (1..=20u64).into_par_iter().map(|x| x * x * x).sum();
    println!("sum_of_cubes_1_20 = {}", cubes);

    // --- Parallel collect into a Vec, then a deterministic fold. ---
    let collected: Vec<u64> = (0..100u64).into_par_iter().map(|x| x + 1).collect();
    let collected_sum: u64 = collected.iter().copied().sum();
    println!("collected_len = {}", collected.len());
    println!("collected_sum = {}", collected_sum);

    println!("== survey_rayon done ==");
}
