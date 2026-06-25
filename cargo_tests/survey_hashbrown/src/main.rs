use core::hash::{BuildHasherDefault, Hasher};

// A FIXED, seedless FNV-1a hasher so the HashMap's internal behavior is
// fully deterministic across native rustc and the .NET backend (no RNG seed,
// no foldhash per-process random state). Output is additionally made
// order-independent by iterating over a SORTED key vector.
#[derive(Default)]
struct Fnv1a(u64);

impl Hasher for Fnv1a {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut h: u64 = if self.0 == 0 {
            0xcbf2_9ce4_8422_2325
        } else {
            self.0
        };
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.0 = h;
    }
}

type FixedState = BuildHasherDefault<Fnv1a>;

fn main() {
    use hashbrown::HashMap;

    // Construct a HashMap with the FIXED hasher (no random seed).
    let mut map: HashMap<&'static str, i64, FixedState> =
        HashMap::with_hasher(FixedState::default());

    // --- insert ---
    let pairs: [(&'static str, i64); 6] = [
        ("alpha", 10),
        ("bravo", 20),
        ("charlie", 30),
        ("delta", 40),
        ("echo", 50),
        ("foxtrot", 60),
    ];
    for (k, v) in pairs.iter() {
        // insert returns the previous value (None here); ignore deterministically.
        let _ = map.insert(*k, *v);
    }
    println!("len_after_insert = {}", map.len());

    // Overwrite an existing key: insert should report the old value.
    match map.insert("bravo", 21) {
        Some(old) => println!("overwrite_old = {}", old),
        None => println!("overwrite_old = <none>"),
    }

    // --- get ---
    match map.get("charlie") {
        Some(v) => println!("get_charlie = {}", *v),
        None => println!("get_charlie = <none>"),
    }
    match map.get("missing") {
        Some(v) => println!("get_missing = {}", *v),
        None => println!("get_missing = <none>"),
    }
    println!("contains_delta = {}", map.contains_key("delta"));

    // --- remove ---
    match map.remove("echo") {
        Some(v) => println!("remove_echo = {}", v),
        None => println!("remove_echo = <none>"),
    }
    match map.remove("echo") {
        Some(v) => println!("remove_echo_again = {}", v),
        None => println!("remove_echo_again = <none>"),
    }
    println!("len_after_remove = {}", map.len());

    // --- entry API (deterministic mutation) ---
    *map.entry("alpha").or_insert(0) += 5;
    *map.entry("golf").or_insert(70) += 1;
    println!("len_after_entry = {}", map.len());

    // --- deterministic iteration via a SORTED key vector ---
    let mut keys: Vec<&'static str> = map.keys().copied().collect();
    keys.sort_unstable();
    let mut sum: i64 = 0;
    for k in keys.iter() {
        match map.get(k) {
            Some(v) => {
                println!("entry {} = {}", k, *v);
                sum += *v;
            }
            None => println!("entry {} = <none>", k),
        }
    }
    println!("value_sum = {}", sum);
    println!("key_count = {}", keys.len());

    println!("== survey_hashbrown done ==");
}
