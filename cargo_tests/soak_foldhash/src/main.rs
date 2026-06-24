use core::hash::{BuildHasher, Hash, Hasher};

// foldhash is the std-HashMap default-style hasher. With a FIXED seed
// (FixedState) it is fully deterministic run-to-run, so its u64 outputs
// can be byte-compared between native rustc and the .NET backend.
//
// We exercise BOTH the `fast` and `quality` hashers via FixedState's
// BuildHasher impl. We never use RandomState (it pulls process entropy),
// so there is no RNG/syscall in the hot path.

// Hash a byte slice with a freshly-seeded hasher from the given BuildHasher.
fn hash_bytes<B: BuildHasher>(state: &B, data: &[u8]) -> u64 {
    let mut h = state.build_hasher();
    h.write(data);
    h.finish()
}

// Hash any Hashable value (uses derived Hash routing through the hasher).
fn hash_value<B: BuildHasher, T: Hash>(state: &B, value: &T) -> u64 {
    let mut h = state.build_hasher();
    value.hash(&mut h);
    h.finish()
}

fn main() {
    // FixedState::with_seed gives a deterministic, reproducible hasher.
    let fast = foldhash::fast::FixedState::with_seed(0x5EED_5EED_5EED_5EED);
    let quality = foldhash::quality::FixedState::with_seed(0x5EED_5EED_5EED_5EED);

    // A fixed corpus of known inputs (empty, short, exact-8, longer, binary).
    let inputs: [&[u8]; 5] = [
        b"",
        b"a",
        b"abcdefgh",
        b"hello, world!",
        b"\x00\x01\x02\x03\xff\xfe\xfd\xfc\x10\x20",
    ];

    // Fast hasher over each input.
    let mut i = 0usize;
    while i < inputs.len() {
        let data = inputs[i];
        let hf = hash_bytes(&fast, data);
        println!("fast[{}] len={} = {}", i, data.len(), hf);
        i += 1;
    }

    // Quality hasher over each input.
    let mut j = 0usize;
    while j < inputs.len() {
        let data = inputs[j];
        let hq = hash_bytes(&quality, data);
        println!("quality[{}] len={} = {}", j, data.len(), hq);
        j += 1;
    }

    // Determinism check: re-hashing the same input yields the same value.
    let again = hash_bytes(&fast, b"hello, world!");
    let first = hash_bytes(&fast, b"hello, world!");
    println!("fast_deterministic = {}", again == first);

    // Different seeds must (essentially always) produce a different hash.
    let other = foldhash::fast::FixedState::with_seed(0x1234_5678_9ABC_DEF0);
    let h_seed_a = hash_bytes(&fast, b"seed-sensitivity");
    let h_seed_b = hash_bytes(&other, b"seed-sensitivity");
    println!("fast_seed_differs = {}", h_seed_a != h_seed_b);

    // Hash structured values through the derived Hash machinery.
    let u_val: u64 = 0xDEAD_BEEF_CAFE_F00D;
    println!("fast_u64 = {}", hash_value(&fast, &u_val));

    let tuple: (u32, i16, bool) = (42, -7, true);
    println!("fast_tuple = {}", hash_value(&fast, &tuple));

    let strs: [&str; 3] = ["alpha", "beta", "gamma"];
    println!("fast_str_slice = {}", hash_value(&fast, &strs));

    // BuildHasher::hash_one convenience over a value.
    println!("quality_hash_one_u32 = {}", quality.hash_one(123_456_789u32));

    // Cross-input distinctness: collect fast hashes of a few strings, ensure
    // they are all distinct (deterministic boolean, no ordering dependence).
    let probe: [&[u8]; 4] = [b"one", b"two", b"three", b"four"];
    let h0 = hash_bytes(&fast, probe[0]);
    let h1 = hash_bytes(&fast, probe[1]);
    let h2 = hash_bytes(&fast, probe[2]);
    let h3 = hash_bytes(&fast, probe[3]);
    let all_distinct =
        h0 != h1 && h0 != h2 && h0 != h3 && h1 != h2 && h1 != h3 && h2 != h3;
    println!("fast_probe_all_distinct = {}", all_distinct);

    println!("== soak_foldhash done ==");
}
