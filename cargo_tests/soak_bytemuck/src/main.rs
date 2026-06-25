//! H2 real-crate SOAK: bytemuck on the dotnet PAL.
//! Exercises POD slice casts (&[u32]<->&[u8]), unaligned reads, byte<->primitive
//! reinterpretation. These hit transmute/pointer-metadata/align codegen paths.
//! Panic-safe: all bytemuck APIs used here are infallible or checked; no unwrap on
//! fallible casts (use try_* / handle errors). SUCCESS = "== soak_bytemuck done ==".
use bytemuck::{cast_slice, pod_read_unaligned, bytes_of, from_bytes};

fn main() {
    println!("== soak_bytemuck start ==");

    // 1. cast &[u32] -> &[u8]  (widen view; len*4 bytes, little-endian on this target)
    let words: [u32; 4] = [0x01020304, 0x05060708, 0x090A0B0C, 0x0D0E0F10];
    let bytes: &[u8] = cast_slice(&words);
    println!("1  u32->u8: words.len={} bytes.len={}", words.len(), bytes.len());
    println!("2  bytes[0..4]={:?}", &bytes[0..4]);

    // 2. cast &[u8] -> &[u32]  (narrow view; requires alignment — words buf is aligned)
    let back: &[u32] = cast_slice(bytes);
    println!("3  u8->u32: back.len={} back[0]=0x{:08X}", back.len(), back[0]);
    println!("4  roundtrip_eq={}", back == &words[..]);

    // 3. pod_read_unaligned: pull a u32 out of a deliberately unaligned offset
    let raw: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0x11, 0x22, 0x33, 0x44];
    let v: u32 = pod_read_unaligned(&raw[1..5]); // offset 1 = unaligned
    println!("5  unaligned u32 @off1 = 0x{:08X}", v);
    let v2: u32 = pod_read_unaligned(&raw[3..7]);
    println!("6  unaligned u32 @off3 = 0x{:08X}", v2);

    // 4. bytes_of / from_bytes single-value reinterpret
    let n: u32 = 0xCAFEBABE;
    let nb: &[u8] = bytes_of(&n);
    println!("7  bytes_of(0xCAFEBABE) len={} first={:#04X}", nb.len(), nb[0]);
    let r: &u32 = from_bytes(nb);
    println!("8  from_bytes roundtrip={}", *r == n);

    // 5. checked cast that should FAIL gracefully (misaligned len) -> exercises Result path
    let odd: [u8; 6] = [1, 2, 3, 4, 5, 6];
    match bytemuck::try_cast_slice::<u8, u32>(&odd) {
        Ok(s) => println!("9  unexpected ok len={}", s.len()),
        Err(e) => println!("9  try_cast_slice err (expected): {:?}", e),
    }

    println!("== soak_bytemuck done ==");
}
