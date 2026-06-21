//! H2 real-crate SOAK: memchr (2.x) byte + substring search on the dotnet PAL.
//! Exercises memchr::memchr / memchr2 / memrchr (single-byte SIMD search) and
//! memchr::memmem::find / Finder (substring search). These paths typically run
//! runtime CPU-feature detection (CPUID / target_feature) to pick SIMD vs scalar,
//! so this is a good probe for feature-detection codegen on the PAL.
//! Panic-safe: all searches return Option; no unwrap/expect, no indexing that
//! could go out of bounds. SUCCESS = "== soak_memchr done ==".

use memchr::memmem;

fn main() {
    let haystack: &[u8] = b"the quick brown fox jumps over the lazy dog";

    // Single-byte forward search -> Option<usize>. 'q' is at index 4.
    match memchr::memchr(b'q', haystack) {
        Some(pos) => println!("memchr_q = {}", pos),
        None => println!("memchr_q = none"),
    }

    // A byte that is not present -> None (safe, no panic).
    match memchr::memchr(b'Z', haystack) {
        Some(pos) => println!("memchr_Z = {}", pos),
        None => println!("memchr_Z = none"),
    }

    // Two-byte search: first of either 'x' or 'z'.
    match memchr::memchr2(b'x', b'z', haystack) {
        Some(pos) => println!("memchr2_xz = {}", pos),
        None => println!("memchr2_xz = none"),
    }

    // Reverse single-byte search: last space.
    match memchr::memrchr(b' ', haystack) {
        Some(pos) => println!("memrchr_space = {}", pos),
        None => println!("memrchr_space = none"),
    }

    // Substring search via the convenience function -> Option<usize>.
    // "fox" begins at index 16.
    match memmem::find(haystack, b"fox") {
        Some(pos) => println!("memmem_fox = {}", pos),
        None => println!("memmem_fox = none"),
    }

    // Substring not present -> None.
    match memmem::find(haystack, b"cat") {
        Some(pos) => println!("memmem_cat = {}", pos),
        None => println!("memmem_cat = none"),
    }

    // Reuse a prebuilt Finder (exercises the heuristic/searcher construction path).
    let finder = memmem::Finder::new(b"the");
    let mut count = 0usize;
    let mut start = 0usize;
    // Bounded loop over non-overlapping matches; all bounds checked via Option.
    while start <= haystack.len() {
        match finder.find(&haystack[start..]) {
            Some(rel) => {
                count += 1;
                // advance past this match; rel + 1 keeps us in-bounds (<= len).
                start = start + rel + 1;
            }
            None => break,
        }
    }
    println!("the_occurrences = {}", count);

    // rfind substring path.
    match memmem::rfind(haystack, b"the") {
        Some(pos) => println!("memmem_rfind_the = {}", pos),
        None => println!("memmem_rfind_the = none"),
    }

    println!("== soak_memchr done ==");
}
