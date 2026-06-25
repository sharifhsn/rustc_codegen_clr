use lru::LruCache;
use std::num::NonZeroUsize;

fn main() {
    // Fixed capacity 3. NonZeroUsize::new returns Option; handle without panic.
    let cap = match NonZeroUsize::new(3) {
        Some(c) => c,
        None => {
            // Unreachable for a literal 3, but keep the happy path panic-free.
            println!("cap_error = true");
            println!("== survey_lru done ==");
            return;
        }
    };

    let mut cache: LruCache<i32, &'static str> = LruCache::new(cap);
    println!("capacity = {}", cache.cap().get());

    // Insert 1,2,3 (fills the cache). put() returns the evicted value (None here).
    let _ = cache.put(1, "one");
    let _ = cache.put(2, "two");
    let _ = cache.put(3, "three");
    println!("after_fill_len = {}", cache.len());

    // Track hits/misses deterministically across a fixed access sequence.
    let mut hits = 0u32;
    let mut misses = 0u32;

    // get(&1) is a hit AND touches key 1, making it most-recently-used.
    // LRU order is now (1 newest, then 3, then 2 oldest).
    match cache.get(&1) {
        Some(_) => hits += 1,
        None => misses += 1,
    }
    println!("get_1_hit = {}", cache.contains(&1));

    // get(&9) is a miss (never inserted).
    match cache.get(&9) {
        Some(_) => hits += 1,
        None => misses += 1,
    }

    // Insert 4. Capacity is full; the LRU entry (key 2) is evicted.
    // put returns the evicted (key removed) value for the slot? No — it returns
    // the old value only if the SAME key existed. For a new key over capacity,
    // the eviction is internal, so observe it via contains() instead.
    let evicted_old_for_same_key = cache.put(4, "four");
    println!(
        "put4_returned_old_for_same_key = {}",
        evicted_old_for_same_key.is_some()
    );

    // Key 2 was least-recently-used, so it must have been evicted.
    println!("key2_evicted = {}", !cache.contains(&2));
    // Keys 1, 3, 4 must survive.
    println!("key1_present = {}", cache.contains(&1));
    println!("key3_present = {}", cache.contains(&3));
    println!("key4_present = {}", cache.contains(&4));

    // A few more probes for the hit/miss tally.
    for k in [3, 7, 4, 2] {
        match cache.peek(&k) {
            // peek() does NOT change LRU order — keeps eviction deterministic.
            Some(_) => hits += 1,
            None => misses += 1,
        }
    }

    println!("hits = {}", hits);
    println!("misses = {}", misses);

    // Collect surviving keys, then SORT for deterministic output
    // (iter() yields MRU->LRU order which is stable here, but sort to be safe).
    let mut surviving: Vec<i32> = cache.iter().map(|(k, _)| *k).collect();
    surviving.sort_unstable();
    println!("surviving_keys = {:?}", surviving);
    println!("surviving_count = {}", surviving.len());

    // Pop the LRU entry explicitly and report it.
    match cache.pop_lru() {
        Some((k, v)) => println!("pop_lru = ({}, {})", k, v),
        None => println!("pop_lru = none"),
    }
    println!("len_after_pop = {}", cache.len());

    // Exercise resize: shrink to 1, which forces eviction down to a single entry.
    if let Some(one) = NonZeroUsize::new(1) {
        cache.resize(one);
        println!("after_resize_len = {}", cache.len());
        println!("after_resize_cap = {}", cache.cap().get());
    }

    println!("== survey_lru done ==");
}
