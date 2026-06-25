// Survey: rkyv 0.8 zero-copy archive round-trip.
//
// Exercises:
//   - #[derive(Archive, Serialize, Deserialize)] on a struct with nested
//     struct, Vec, String, and primitive fields (heavy generics + alignment).
//   - rkyv::to_bytes -> serialize to an aligned byte buffer.
//   - rkyv::access -> zero-copy view over the archived bytes (read fields in
//     place, no deserialization).
//   - rkyv::deserialize -> full round-trip back to the owned Rust type.
//
// All output is deterministic: fixed input data, integer/string fields, and a
// fixed byte length for this concrete type/layout.

use rkyv::rancor::Error;
use rkyv::{Archive, Deserialize, Serialize};

#[derive(Archive, Serialize, Deserialize, Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Archive, Serialize, Deserialize, Debug, PartialEq)]
struct Record {
    id: u64,
    name: String,
    scores: Vec<i32>,
    origin: Point,
    active: bool,
}

fn main() {
    let value = Record {
        id: 0x0102_0304_0506_0708,
        name: String::from("zero-copy"),
        scores: vec![10, 20, 30, 40],
        origin: Point { x: -7, y: 42 },
        active: true,
    };

    // --- Serialize to an aligned byte buffer (rkyv::to_bytes). ---
    match rkyv::to_bytes::<Error>(&value) {
        Ok(bytes) => {
            println!("byte_len = {}", bytes.len());

            // --- Zero-copy access: view archived fields in place. ---
            match rkyv::access::<ArchivedRecord, Error>(&bytes) {
                Ok(archived) => {
                    // Archived integers are little-endian wrappers; convert to
                    // native for deterministic printing.
                    let aid: u64 = archived.id.into();
                    println!("archived_id = {}", aid);
                    println!("archived_name = {}", archived.name.as_str());
                    println!("archived_scores_len = {}", archived.scores.len());

                    let mut sum: i64 = 0;
                    for s in archived.scores.iter() {
                        let v: i32 = s.to_native();
                        sum += v as i64;
                    }
                    println!("archived_scores_sum = {}", sum);

                    let ox: i32 = archived.origin.x.to_native();
                    let oy: i32 = archived.origin.y.to_native();
                    println!("archived_origin = ({}, {})", ox, oy);
                    println!("archived_active = {}", archived.active);

                    // --- Full deserialize round-trip. ---
                    match rkyv::deserialize::<Record, Error>(archived) {
                        Ok(restored) => {
                            println!("roundtrip_matches = {}", restored == value);
                            println!("restored_name = {}", restored.name);
                            println!("restored_id = {}", restored.id);
                        }
                        Err(_) => println!("deserialize_error = true"),
                    }
                }
                Err(_) => println!("access_error = true"),
            }
        }
        Err(_) => println!("serialize_error = true"),
    }

    println!("== survey_rkyv done ==");
}
