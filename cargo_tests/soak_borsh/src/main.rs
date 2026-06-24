use borsh::{BorshDeserialize, BorshSerialize};

#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
struct Record {
    id: u32,
    flags: u8,
    score: i64,
    name: String,
    tags: Vec<u16>,
}

// Render bytes as a stable lowercase hex string (deterministic, no alloc-order issues).
fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn main() {
    // Deterministic, fixed input — no RNG, no time, no hashing-order dependence.
    let rec = Record {
        id: 0x0102_0304,
        flags: 0xAB,
        score: -1234567890,
        name: String::from("borsh-soak"),
        tags: vec![1, 2, 256, 65535],
    };

    // Serialize (round-trip step 1). borsh::to_vec returns Result; handle without unwrap.
    let bytes = match borsh::to_vec(&rec) {
        Ok(b) => b,
        Err(_) => {
            println!("serialize_error = true");
            println!("== soak_borsh done ==");
            return;
        }
    };

    println!("serialized_len = {}", bytes.len());
    println!("serialized_hex = {}", to_hex(&bytes));

    // Deserialize back (round-trip step 2).
    match Record::try_from_slice(&bytes) {
        Ok(decoded) => {
            println!("roundtrip_matches = {}", decoded == rec);
            println!("decoded_id = {}", decoded.id);
            println!("decoded_flags = {}", decoded.flags);
            println!("decoded_score = {}", decoded.score);
            println!("decoded_name = {}", decoded.name);
            println!("decoded_name_len = {}", decoded.name.len());
            println!("decoded_tags_len = {}", decoded.tags.len());
            // Sum the tags as a deterministic derived integer (u32 to avoid overflow).
            let tag_sum: u32 = decoded.tags.iter().map(|&t| t as u32).sum();
            println!("decoded_tags_sum = {}", tag_sum);
        }
        Err(_) => {
            println!("deserialize_error = true");
        }
    }

    // Also exercise a primitive round-trip to cover the scalar path.
    let n: i32 = -42;
    match borsh::to_vec(&n) {
        Ok(nb) => {
            println!("i32_hex = {}", to_hex(&nb));
            match i32::try_from_slice(&nb) {
                Ok(back) => println!("i32_roundtrip = {}", back == n),
                Err(_) => println!("i32_decode_error = true"),
            }
        }
        Err(_) => println!("i32_encode_error = true"),
    }

    println!("== soak_borsh done ==");
}
