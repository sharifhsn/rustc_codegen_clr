use roaring::RoaringBitmap;

fn main() {
    // Build bitmap A: a few disjoint ranges + scattered singletons.
    // insert_range returns the count of NEWLY inserted values (u64) — deterministic.
    let mut a = RoaringBitmap::new();
    let added_lo = a.insert_range(0..1000); // 1000 values: 0..=999
    let added_hi = a.insert_range(5000..5100); // 100 values: 5000..=5099
    println!("a_added_lo = {}", added_lo);
    println!("a_added_hi = {}", added_hi);
    // Scattered singletons across container boundaries (>65536 forces a 2nd container).
    for v in [123u32, 999, 65535, 65536, 70000, 100000] {
        a.insert(v);
    }
    println!("a_len = {}", a.len());

    // Build bitmap B: overlaps A partially.
    let mut b = RoaringBitmap::new();
    b.insert_range(500..1500); // 500..=1499 (overlaps A's 500..=999)
    b.insert_range(70000..70010); // overlaps A's 70000
    b.insert(123);
    b.insert(200000);
    println!("b_len = {}", b.len());

    // contains: deterministic membership probes.
    println!("a_contains_0 = {}", a.contains(0));
    println!("a_contains_999 = {}", a.contains(999));
    println!("a_contains_1000 = {}", a.contains(1000));
    println!("a_contains_65536 = {}", a.contains(65536));
    println!("b_contains_200000 = {}", b.contains(200000));
    println!("b_contains_123 = {}", b.contains(123));

    // Set algebra via operators (compressed-container codegen exercised here).
    let union = &a | &b;
    let intersection = &a & &b;
    let difference = &a - &b; // values in A not in B
    println!("union_len = {}", union.len());
    println!("intersection_len = {}", intersection.len());
    println!("difference_len = {}", difference.len());

    // Symmetric difference for good measure.
    let sym = &a ^ &b;
    println!("symdiff_len = {}", sym.len());

    // min / max return Option — handle without unwrap.
    match a.min() {
        Some(m) => println!("a_min = {}", m),
        None => println!("a_min = none"),
    }
    match a.max() {
        Some(m) => println!("a_max = {}", m),
        None => println!("a_max = none"),
    }
    match union.max() {
        Some(m) => println!("union_max = {}", m),
        None => println!("union_max = none"),
    }

    // Iterate the FIRST N values of the union, sum + reproduce them.
    // iter() yields values in ascending order -> deterministic.
    let n = 8usize;
    let mut first: Vec<u32> = Vec::with_capacity(n);
    let mut sum_first: u64 = 0;
    for v in union.iter().take(n) {
        sum_first += v as u64;
        first.push(v);
    }
    println!("union_first_{} = {:?}", n, first);
    println!("union_first_sum = {}", sum_first);

    // Derived rank-style counts: how many union values are below thresholds.
    let below_1000 = union.iter().take_while(|&v| v < 1000).count();
    println!("union_below_1000 = {}", below_1000);

    // is_subset / is_disjoint relationships (bool -> deterministic).
    println!("intersection_subset_a = {}", intersection.is_subset(&a));
    println!("a_disjoint_b = {}", a.is_disjoint(&b));

    // Round-trip an intersection check: every value in `intersection`
    // must be contained in both A and B.
    let mut roundtrip_ok = true;
    for v in intersection.iter() {
        if !(a.contains(v) && b.contains(v)) {
            roundtrip_ok = false;
            break;
        }
    }
    println!("intersection_roundtrip_ok = {}", roundtrip_ok);

    println!("== soak_roaring done ==");
}
