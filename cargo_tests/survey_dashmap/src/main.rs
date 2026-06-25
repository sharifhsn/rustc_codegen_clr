use dashmap::DashMap;

fn main() {
    // Sharded concurrent map. We exercise it SINGLE-THREADED here so the
    // output is fully deterministic (no thread interleaving / atomics race
    // visible to the observer). The map still uses its real internal shards
    // and atomics, so this is a faithful threading/atomics probe of the data
    // structure itself.
    let map: DashMap<i64, i64> = DashMap::new();

    // Insert a FIXED set of keys -> values (value = key * 10).
    let keys: [i64; 8] = [7, 1, 4, 9, 2, 6, 3, 5];
    for &k in keys.iter() {
        // insert returns Option<old>; on a fresh key it is None.
        let prev = map.insert(k, k * 10);
        if prev.is_some() {
            println!("unexpected_existing_key = {}", k);
        }
    }

    println!("len_after_insert = {}", map.len());
    println!("is_empty = {}", map.is_empty());

    // Get each key back (in a fixed key order) and sum the values.
    let mut got_sum: i64 = 0;
    let mut missing: i64 = 0;
    let lookup_order: [i64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    for &k in lookup_order.iter() {
        match map.get(&k) {
            Some(entry) => {
                got_sum += *entry.value();
            }
            None => {
                missing += 1;
            }
        }
    }
    println!("got_sum = {}", got_sum);
    println!("missing_lookups = {}", missing);

    // contains_key checks (deterministic booleans).
    println!("contains_7 = {}", map.contains_key(&7));
    println!("contains_8 = {}", map.contains_key(&8));

    // Sum ALL values by collecting into a Vec and sorting by key first, so the
    // iteration order (which is shard-dependent and NOT deterministic) does not
    // leak into the output. We sort, then print.
    let mut pairs: Vec<(i64, i64)> = map.iter().map(|e| (*e.key(), *e.value())).collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut total: i64 = 0;
    for (k, v) in pairs.iter() {
        total += *v;
        println!("pair k={} v={}", k, v);
    }
    println!("total_values = {}", total);

    // Mutate one entry in place via get_mut, then re-read.
    if let Some(mut e) = map.get_mut(&4) {
        *e.value_mut() += 1000;
    }
    match map.get(&4) {
        Some(e) => println!("mutated_4 = {}", *e.value()),
        None => println!("mutated_4 = <missing>"),
    }

    // Remove a key; remove returns Option<(K, V)>.
    match map.remove(&9) {
        Some((k, v)) => println!("removed k={} v={}", k, v),
        None => println!("removed = <none>"),
    }
    println!("len_after_remove = {}", map.len());

    // entry API: or_insert on an absent key, then on a present key.
    *map.entry(100).or_insert(0) += 1;
    *map.entry(100).or_insert(0) += 1;
    match map.get(&100) {
        Some(e) => println!("entry_100 = {}", *e.value()),
        None => println!("entry_100 = <missing>"),
    }

    println!("final_len = {}", map.len());

    println!("== survey_dashmap done ==");
}
