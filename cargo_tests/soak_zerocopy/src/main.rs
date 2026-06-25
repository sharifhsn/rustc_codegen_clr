//! H2 real-crate SOAK: zerocopy 0.8 zero-cost byte<->struct reinterpretation on the dotnet PAL.
//! A #[derive(FromBytes, IntoBytes, Immutable, KnownLayout)] struct: read a struct from a byte
//! buffer (ref_from_bytes), inspect fields, mutate via as_bytes round-trip, print.
//! Exercises the derive proc-macros, byteorder little-endian wrappers, layout/alignment, slices.
//! Panic-safe: all fallible APIs go through Result/Option with explicit fallbacks (no unwrap/expect).
//! SUCCESS = "== soak_zerocopy done ==" with sane values.
use zerocopy::byteorder::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Debug)]
#[repr(C)]
struct Packet {
    magic: U32,
    version: U16,
    flags: u8,
    kind: u8,
    payload: [u8; 4],
}

fn main() {
    println!("== soak_zerocopy start ==");

    // 12-byte buffer: magic=0xDEADBEEF, version=0x0102, flags=0x05, kind=0x07, payload=[1,2,3,4]
    let buf: [u8; 12] = [
        0xEF, 0xBE, 0xAD, 0xDE, // magic (LE)
        0x02, 0x01, // version (LE)
        0x05, // flags
        0x07, // kind
        0x01, 0x02, 0x03, 0x04, // payload
    ];

    // read struct FROM bytes (by reference, zero-copy)
    match Packet::ref_from_bytes(&buf) {
        Ok(pkt) => {
            println!("1  magic=0x{:08X}", pkt.magic.get());
            println!("2  version=0x{:04X}", pkt.version.get());
            println!("3  flags={} kind={}", pkt.flags, pkt.kind);
            println!("4  payload={:?}", pkt.payload);
        }
        Err(_) => println!("1  ref_from_bytes failed"),
    }

    // owned read (FromBytes::read_from_bytes), then mutate + serialize back to bytes
    match Packet::read_from_bytes(&buf) {
        Ok(mut pkt) => {
            pkt.version = U16::new(pkt.version.get() + 1);
            pkt.flags |= 0x80;
            let out = pkt.as_bytes();
            println!("5  as_bytes len={} version_byte0={} flags_byte={}", out.len(), out[4], out[6]);
            // round-trip the mutated bytes back into a struct
            match Packet::read_from_bytes(out) {
                Ok(p2) => println!("6  roundtrip version=0x{:04X} flags=0x{:02X}", p2.version.get(), p2.flags),
                Err(_) => println!("6  roundtrip failed"),
            }
        }
        Err(_) => println!("5  read_from_bytes failed"),
    }

    // slice-of-structs: interpret a flat byte buffer as [U32]
    let nums: [u8; 8] = [10, 0, 0, 0, 20, 0, 0, 0];
    match <[U32]>::ref_from_bytes(&nums) {
        Ok(slice) => {
            let sum: u32 = slice.iter().map(|n| n.get()).sum();
            println!("7  slice len={} sum={}", slice.len(), sum);
        }
        Err(_) => println!("7  slice ref_from_bytes failed"),
    }

    // wrong-size buffer must yield Err, not panic
    let short: [u8; 3] = [1, 2, 3];
    println!("8  short buffer is_err={}", Packet::ref_from_bytes(&short).is_err());

    println!("== soak_zerocopy done ==");
}
