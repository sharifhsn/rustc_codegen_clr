use serde::{Deserialize, Serialize};

// A struct exercising several wire-format primitives postcard must encode:
// fixed-width ints, signed (zigzag) ints, bool, varint-length string and Vec.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Record {
    id: u32,
    delta: i32,
    flag: bool,
    name: String,
    tags: Vec<u16>,
}

fn to_hex(bytes: &[u8]) -> String {
    // Deterministic lowercase hex; no panics, no allocation surprises.
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let hi = b >> 4;
        let lo = b & 0x0f;
        for nyb in [hi, lo] {
            let c = if nyb < 10 {
                (b'0' + nyb) as char
            } else {
                (b'a' + (nyb - 10)) as char
            };
            s.push(c);
        }
    }
    s
}

fn main() {
    let original = Record {
        id: 4_242,
        delta: -17,
        flag: true,
        name: String::from("postcard"),
        tags: vec![1, 256, 65_535],
    };

    // Serialize to a heap Vec<u8> (postcard::to_allocvec needs the alloc feature).
    match postcard::to_allocvec(&original) {
        Ok(bytes) => {
            println!("serialized_len = {}", bytes.len());
            println!("serialized_hex = {}", to_hex(&bytes));

            // Round-trip back into a Record.
            match postcard::from_bytes::<Record>(&bytes) {
                Ok(decoded) => {
                    println!("roundtrip_matches = {}", decoded == original);
                    println!("decoded_id = {}", decoded.id);
                    println!("decoded_delta = {}", decoded.delta);
                    println!("decoded_flag = {}", decoded.flag);
                    println!("decoded_name = {}", decoded.name);
                    println!("decoded_tags_len = {}", decoded.tags.len());
                    // Deterministic derived sum (u32, no overflow for these values).
                    let sum: u32 = decoded.tags.iter().map(|&t| t as u32).sum();
                    println!("decoded_tags_sum = {}", sum);
                }
                Err(_) => {
                    println!("decode_error = true");
                }
            }
        }
        Err(_) => {
            println!("encode_error = true");
        }
    }

    println!("== soak_postcard done ==");
}
