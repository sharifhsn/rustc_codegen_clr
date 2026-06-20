//! H2 real-crate SOAK: indexmap on the dotnet PAL.
//! IndexMap<String,i32>: insert several, get, iterate in INSERTION ORDER; print ordered keys + sum.
//! Exercises hashing (SipHash/RandomState), the indexmap hash-bucket + entries Vec, generics over
//! String/i32, iteration order guarantees. Panic-safe (no unwraps on fallible lookups).
//! SUCCESS = "== soak_indexmap done ==" with sane values.
use indexmap::IndexMap;

fn main() {
    println!("== soak_indexmap start ==");

    let mut m: IndexMap<String, i32> = IndexMap::new();
    // Insert in a deliberately non-alphabetical order to prove insertion-order iteration.
    m.insert("delta".to_string(), 40);
    m.insert("alpha".to_string(), 10);
    m.insert("charlie".to_string(), 30);
    m.insert("bravo".to_string(), 20);
    // Update existing key: should NOT change its position, only its value.
    m.insert("alpha".to_string(), 11);

    println!("1  len={}", m.len());

    // get() lookups (handle Option, no unwrap that could panic)
    match m.get("charlie") {
        Some(v) => println!("2  get(charlie)={}", v),
        None => println!("2  get(charlie)=<missing>"),
    }
    match m.get("missing") {
        Some(v) => println!("3  get(missing)={}", v),
        None => println!("3  get(missing)=<none>"),
    }

    // Iterate in insertion order: delta, alpha, charlie, bravo
    let keys: Vec<&str> = m.keys().map(|k| k.as_str()).collect();
    println!("4  keys: {:?}", keys);

    // Sum of values via iteration
    let sum: i32 = m.values().copied().sum();
    println!("5  sum={}", sum);

    // Positional access by index (insertion-order indexed)
    match m.get_index(0) {
        Some((k, v)) => println!("6  index0=({}, {})", k, v),
        None => println!("6  index0=<none>"),
    }

    // Per-entry dump in order
    for (i, (k, v)) in m.iter().enumerate() {
        println!("7  entry[{}] {} => {}", i, k, v);
    }

    println!("== soak_indexmap done ==");
}
