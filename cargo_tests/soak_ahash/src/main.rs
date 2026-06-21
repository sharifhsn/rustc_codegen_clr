//! H2 real-crate SOAK: ahash on the dotnet PAL.
//! Exercises AHasher (the Hasher trait, AES/CPUID feature-detection path or the fallback),
//! AHashMap insert/get, and a few raw hashes. ahash uses RandomState seeded at runtime
//! (may touch std time/random), const-generics, and SIMD-ish word mixing -- a good codegen probe.
//! Panic-safe: no unwraps, all map lookups go through Option, hashing valid keys only.
//! SUCCESS = "== soak_ahash done ==" with deterministic structural values.
use ahash::{AHashMap, AHasher};
use std::hash::{Hash, Hasher};

// getrandom 0.3 custom backend -> dotnet PAL CSPRNG. ahash pulls getrandom 0.3,
// which rejects os="dotnet" unless a custom backend is provided. Selected by
// `--cfg getrandom_backend="custom"` (set in feasibility/dev.sh pal-build).
#[no_mangle]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    getrandom_dotnet::fill(unsafe { core::slice::from_raw_parts_mut(dest, len) });
    Ok(())
}

// Hash a value with a FIXED-seed AHasher so the numeric output is deterministic across runs.
fn fixed_hash<T: Hash>(v: &T) -> u64 {
    // AHasher::new_with_keys is available under the default feature set; if not, this is the
    // place a build error would surface. Use constant keys for reproducibility.
    let mut h = AHasher::default();
    v.hash(&mut h);
    h.finish()
}

fn main() {
    println!("== soak_ahash start ==");

    // 1. Raw AHasher over a few values. We don't assert exact numbers (seed/impl dependent),
    //    only that hashing runs and that equal inputs hash equally / distinct inputs differ.
    let h_a = fixed_hash(&"hello");
    let h_a2 = fixed_hash(&"hello");
    let h_b = fixed_hash(&"world");
    println!("1  hash(\"hello\")==hash(\"hello\"): {}", h_a == h_a2);
    println!("2  hash(\"hello\")!=hash(\"world\"): {}", h_a != h_b);

    // 2. Hash some integers and feed bytes directly via the Hasher API.
    let mut hi = AHasher::default();
    hi.write_u64(0xDEAD_BEEF_CAFE_F00D);
    hi.write(&[1u8, 2, 3, 4, 5]);
    let _ = hi.finish();
    println!("3  raw write_u64+write bytes: ok");

    // 3. AHashMap insert/get -- the headline use case. Build a small word-count map.
    let text = "the quick brown fox the lazy dog the end";
    let mut counts: AHashMap<&str, u32> = AHashMap::new();
    for w in text.split_whitespace() {
        *counts.entry(w).or_insert(0) += 1;
    }
    println!("4  map.len = {}", counts.len());
    println!("5  count[\"the\"] = {}", counts.get("the").copied().unwrap_or(0));
    println!("6  count[\"fox\"] = {}", counts.get("fox").copied().unwrap_or(0));
    println!("7  count[\"missing\"] = {}", counts.get("missing").copied().unwrap_or(0));

    // 4. With_capacity + many integer keys; iterate to compute a checksum (order-independent).
    let mut m: AHashMap<u64, u64> = AHashMap::with_capacity(64);
    for i in 0..1000u64 {
        m.insert(i, i.wrapping_mul(2654435761));
    }
    let mut present = 0u64;
    let mut sum = 0u64;
    for i in 0..1000u64 {
        if let Some(val) = m.get(&i) {
            present += 1;
            sum = sum.wrapping_add(*val);
        }
    }
    println!("8  int-map present = {} / 1000", present);
    println!("9  int-map value-checksum = {}", sum);

    // 5. Remove half and re-check size.
    for i in (0..1000u64).step_by(2) {
        m.remove(&i);
    }
    println!("10 after removing evens, len = {}", m.len());

    println!("== soak_ahash done ==");
}
