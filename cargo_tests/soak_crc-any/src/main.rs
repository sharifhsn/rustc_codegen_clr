//! H2 real-crate SOAK: crc-any doing real CRC work on the dotnet PAL.
//! Exercises CRCu32 (crc32) update over byte slices, plus a couple of other widths.
//! crc-any builds tables at runtime, does u32/u64 bit twiddling, reflection, and Vec/String.
//! Panic-safe (no unwraps on fallible data, valid inputs only).
//! SUCCESS = "== soak_crc-any done ==" with sane checksum values.
use crc_any::CRCu32;

fn main() {
    println!("== soak_crc-any start ==");

    let data = b"The quick brown fox jumps over the lazy dog";

    // CRC-32 (IEEE / zip / png) over the whole slice.
    let mut crc32 = CRCu32::crc32();
    crc32.digest(data);
    let c32 = crc32.get_crc();
    println!("1  crc32 = 0x{c32:08x}");

    // Incremental update: feed the same bytes one at a time, must match.
    let mut crc32b = CRCu32::crc32();
    for &b in data.iter() {
        crc32b.digest(&[b]);
    }
    let c32b = crc32b.get_crc();
    println!("2  crc32 incremental = 0x{c32b:08x} match={}", c32b == c32);

    // CRC-32C (Castagnoli) for a different polynomial path.
    let mut crc32c = CRCu32::crc32c();
    crc32c.digest(data);
    println!("3  crc32c = 0x{:08x}", crc32c.get_crc());

    // Empty input edge case.
    let mut crc32e = CRCu32::crc32();
    crc32e.digest(b"");
    println!("4  crc32(empty) = 0x{:08x}", crc32e.get_crc());

    // Reset and reuse.
    let mut crc32r = CRCu32::crc32();
    crc32r.digest(b"123456789");
    println!("5  crc32(\"123456789\") = 0x{:08x}", crc32r.get_crc());

    println!("== soak_crc-any done ==");
}
