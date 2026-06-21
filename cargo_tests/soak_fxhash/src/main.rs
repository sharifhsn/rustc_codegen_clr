//! H2 real-crate SOAK: fxhash (0.2) FxHashMap insert/get/iter on the dotnet PAL.
//! Exercises the FxHasher (a fast non-cryptographic hash) wired through std's
//! HashMap via FxBuildHasher: insert, get (-> Option), iteration, and len.
//! Pure hash + std collections, no I/O. Panic-safe: no unwrap/expect, all lookups
//! handle Option, no indexing that could fail. SUCCESS = "== soak_fxhash done ==".

use fxhash::{FxHashMap, FxHashSet};

fn main() {
    // --- FxHashMap insert/get ---
    let mut map: FxHashMap<&str, u32> = FxHashMap::default();
    map.insert("one", 1);
    map.insert("two", 2);
    map.insert("three", 3);
    map.insert("four", 4);
    // Overwrite an existing key (exercises the update path).
    map.insert("two", 22);

    println!("map_len = {}", map.len());

    // get -> Option, handled without unwrap.
    match map.get("two") {
        Some(v) => println!("get_two = {}", v),
        None => println!("get_two = <missing>"),
    }
    match map.get("missing") {
        Some(v) => println!("get_missing = {}", v),
        None => println!("get_missing = <none-as-expected>"),
    }

    // contains_key path.
    println!("contains_three = {}", map.contains_key("three"));

    // --- Iterate + fold (deterministic aggregate; iteration order may vary, sum is stable) ---
    let mut sum: u64 = 0;
    let mut count: u32 = 0;
    for (_k, v) in map.iter() {
        sum += *v as u64;
        count += 1;
    }
    println!("iter_count = {}", count);
    println!("value_sum = {}", sum);

    // --- Hash a numeric-keyed map (different key type exercises a second FxHasher path) ---
    let mut nums: FxHashMap<u64, u64> = FxHashMap::default();
    let mut i: u64 = 0;
    while i < 16 {
        nums.insert(i, i * i);
        i += 1;
    }
    println!("nums_len = {}", nums.len());
    match nums.get(&7) {
        Some(v) => println!("nums_7 = {}", v),
        None => println!("nums_7 = <missing>"),
    }

    // --- FxHashSet dedup ---
    let mut set: FxHashSet<u32> = FxHashSet::default();
    for x in [3u32, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5].iter() {
        set.insert(*x);
    }
    println!("set_len = {}", set.len());
    println!("set_has_9 = {}", set.contains(&9));
    println!("set_has_7 = {}", set.contains(&7));

    println!("== soak_fxhash done ==");
}
