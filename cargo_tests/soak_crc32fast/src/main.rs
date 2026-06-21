//! H2 real-crate SOAK: crc32fast (CRC-32/ISO-HDLC) on the dotnet PAL.
//! Exercises runtime CPU-feature detection (std::is_x86_feature_detected! / CPUID) plus the
//! SSE4.2 + PCLMULQDQ SIMD fast path and the software (table-based) fallback. Panic-safe: fixed
//! valid byte inputs, no unwraps on fallible data; CRC compared against well-known constants.
//! SUCCESS = "== soak_crc32fast done ==" with crc32("123456789") == 0xCBF43926.
use crc32fast::Hasher;

fn main() {
    println!("== soak_crc32fast start ==");

    // 1: canonical CRC-32 check value. crc32("123456789") == 0xCBF43926.
    let mut h1 = Hasher::new();
    h1.update(b"123456789");
    let c1 = h1.finalize();
    println!("1  crc32(123456789)   = {c1:#010x}");
    println!("1  matches 0xCBF43926? = {}", c1 == 0xCBF43926);

    // 2: empty input -> 0.
    let h2 = Hasher::new();
    let c2 = h2.finalize();
    println!("2  crc32(\"\")          = {c2:#010x}");
    println!("2  matches 0?          = {}", c2 == 0);

    // 3: incremental update across chunks must equal a single-shot update.
    //    Drives the block-buffering / combine logic and a larger buffer through the SIMD path.
    let data: Vec<u8> = (0u32..4096).map(|i| (i & 0xff) as u8).collect();
    let mut single = Hasher::new();
    single.update(&data);
    let cs = single.finalize();

    let mut chunked = Hasher::new();
    for chunk in data.chunks(7) {
        chunked.update(chunk);
    }
    let cc = chunked.finalize();
    println!("3  single  4096 bytes  = {cs:#010x}");
    println!("3  chunked 4096 bytes  = {cc:#010x}");
    println!("3  single == chunked?  = {}", cs == cc);

    // 4: Hasher::combine — exercises the GF(2) matrix combine path.
    let (a, b) = data.split_at(2048);
    let mut ha = Hasher::new();
    ha.update(a);
    let mut hb = Hasher::new();
    hb.update(b);
    let combined = {
        let mut hc = ha.clone();
        hc.combine(&hb);
        hc.finalize()
    };
    println!("4  combine(a,b)        = {combined:#010x}");
    println!("4  combine == single?  = {}", combined == cs);

    println!("== soak_crc32fast done ==");
}
