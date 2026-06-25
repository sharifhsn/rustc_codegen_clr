//! H2 real-crate SOAK: compact_str on the dotnet PAL.
//! Exercises CompactString inline (<=24 bytes, on-stack) + heap (>24 bytes) paths,
//! push_str, len, as_str, capacity, is_heap_allocated, From<&str>, fmt. Panic-safe.
//! SUCCESS = "== soak_compact_str done ==" with sane values.
use compact_str::CompactString;

fn main() {
    println!("== soak_compact_str start ==");

    // 1. inline: short string stays on the stack (<= 24 bytes)
    let inline = CompactString::new("hi");
    println!("1  inline: {} len={} heap={}", inline.as_str(), inline.len(), inline.is_heap_allocated());

    // 2. grow an inline string via push_str, still inline
    let mut s = CompactString::new("abc");
    s.push_str("def");
    println!("2  pushed: {} len={} heap={}", s.as_str(), s.len(), s.is_heap_allocated());

    // 3. heap: a string longer than 24 chars must spill to the heap
    let long = CompactString::new("this string is definitely longer than twenty-four bytes");
    println!("3  heap: len={} heap={} first10={}", long.len(), long.is_heap_allocated(), &long.as_str()[..10]);

    // 4. push_str across the inline->heap boundary
    let mut grow = CompactString::new("start-");
    for i in 0..10 {
        grow.push_str("chunk");
        let _ = i;
    }
    println!("4  grown: len={} heap={} cap>=len={}", grow.len(), grow.is_heap_allocated(), grow.capacity() >= grow.len());

    // 5. From<&str> and equality / as_str round-trip
    let from: CompactString = "from-impl".into();
    println!("5  from: {} eq={}", from.as_str(), from.as_str() == "from-impl");

    // 6. push individual chars
    let mut chars = CompactString::default();
    for c in "hello".chars() {
        chars.push(c);
    }
    println!("6  chars: {} len={}", chars.as_str(), chars.len());

    // 7. Display formatting + repeat-style build
    let repeated: CompactString = CompactString::from("ab".repeat(20));
    println!("7  repeated: len={} heap={}", repeated.len(), repeated.is_heap_allocated());

    println!("== soak_compact_str done ==");
}
