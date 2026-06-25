use slab::Slab;

fn main() {
    // slab::Slab is a slot map: insert returns a stable usize key; get/remove
    // address by key. Keys are assigned deterministically (lowest free slot),
    // so the whole exercise is reproducible without any HashMap iteration.
    let mut slab: Slab<String> = Slab::new();

    // --- insert several values, capturing keys ---------------------------
    let k_alpha = slab.insert(String::from("alpha"));
    let k_bravo = slab.insert(String::from("bravo"));
    let k_charlie = slab.insert(String::from("charlie"));
    let k_delta = slab.insert(String::from("delta"));

    println!("k_alpha = {}", k_alpha);
    println!("k_bravo = {}", k_bravo);
    println!("k_charlie = {}", k_charlie);
    println!("k_delta = {}", k_delta);
    println!("len_after_insert = {}", slab.len());

    // --- get by key (no panic path: match the Option) --------------------
    match slab.get(k_charlie) {
        Some(v) => println!("get_charlie = {}", v),
        None => println!("get_charlie = <missing>"),
    }
    println!("contains_bravo = {}", slab.contains(k_bravo));

    // --- remove an interior value ----------------------------------------
    // slab.remove panics on a vacant key, so guard with contains first.
    if slab.contains(k_bravo) {
        let removed = slab.remove(k_bravo);
        println!("removed_value = {}", removed);
    } else {
        println!("removed_value = <vacant>");
    }
    println!("len_after_remove = {}", slab.len());
    println!("contains_bravo_after_remove = {}", slab.contains(k_bravo));

    // --- reinsert: slab reuses the freed slot, so the key is deterministic-
    let k_echo = slab.insert(String::from("echo"));
    println!("k_echo = {}", k_echo);
    println!("k_echo_reused_bravo_slot = {}", k_echo == k_bravo);
    println!("len_after_reinsert = {}", slab.len());

    // --- try_remove returns Option (no panic) on a now-vacant probe ------
    match slab.try_remove(9999) {
        Some(_) => println!("try_remove_absent = unexpected_some"),
        None => println!("try_remove_absent = none"),
    }

    // --- get_mut: mutate in place, deterministically ---------------------
    if let Some(v) = slab.get_mut(k_alpha) {
        v.push_str("!!");
    }
    match slab.get(k_alpha) {
        Some(v) => println!("alpha_mutated = {}", v),
        None => println!("alpha_mutated = <missing>"),
    }

    // --- print remaining entries BY KEY in ascending key order ----------
    // Slab's iterator yields by ascending key already, but collect+sort to be
    // explicit and order-independent of internal representation.
    let mut entries: Vec<(usize, String)> =
        slab.iter().map(|(k, v)| (k, v.clone())).collect();
    entries.sort_by_key(|(k, _)| *k);
    for (k, v) in &entries {
        println!("entry[{}] = {}", k, v);
    }

    println!("final_len = {}", slab.len());
    println!("is_empty = {}", slab.is_empty());
    println!("capacity_at_least_len = {}", slab.capacity() >= slab.len());

    println!("== survey_slab done ==");
}
