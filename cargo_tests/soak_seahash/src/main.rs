//! H2 real-crate SOAK: seahash, a fast non-cryptographic hash. Exercises u64 wrapping arithmetic,
//! rotates, slice chunking, and the streaming hasher API. Panic-safe (no unwraps, fixed inputs).
//! SUCCESS = "== soak_seahash done ==" with stable, deterministic hash values.
use seahash::SeaHasher;
use std::hash::Hasher;

fn main() {
    println!("== soak_seahash start ==");

    // 1. one-shot hash of a fixed byte slice (deterministic)
    let data = b"rustc_codegen_clr -> .NET CIL";
    let h1 = seahash::hash(data);
    println!("1  hash(data)={h1:#018x}");

    // 2. empty input
    let h_empty = seahash::hash(b"");
    println!("2  hash(empty)={h_empty:#018x}");

    // 3. hash with explicit seed
    let h_seed = seahash::hash_seeded(data, 1, 2, 3, 4);
    println!("3  hash_seeded={h_seed:#018x}");

    // 4. streaming hasher matches one-shot
    let mut hasher = SeaHasher::new();
    hasher.write(data);
    let h_stream = hasher.finish();
    println!("4  stream={h_stream:#018x} matches_oneshot={}", h_stream == h1);

    // 5. incremental writes over chunks of increasing length
    let mut acc = SeaHasher::new();
    let mut total = 0usize;
    for i in 1u64..=64 {
        let buf = vec![i as u8; i as usize];
        acc.write(&buf);
        total += buf.len();
    }
    println!("5  incremental over {total} bytes -> {:#018x}", acc.finish());

    // 6. hashes of distinct inputs differ (collision sanity)
    let a = seahash::hash(b"alpha");
    let b = seahash::hash(b"bravo");
    println!("6  distinct={} a={a:#018x} b={b:#018x}", a != b);

    // 7. accumulate a checksum over a range of small slices
    let mut xor: u64 = 0;
    for n in 0u64..256 {
        let bytes = n.to_le_bytes();
        xor ^= seahash::hash(&bytes);
    }
    println!("7  xor-of-256-hashes={xor:#018x}");

    println!("== soak_seahash done ==");
}
