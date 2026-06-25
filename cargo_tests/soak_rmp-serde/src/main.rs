//! H2 real-crate SOAK: rmp-serde (MessagePack) on the dotnet PAL.
//! A #[derive(Serialize, Deserialize)] struct -> rmp_serde::to_vec -> from_slice round-trip.
//! Exercises serde derive codegen, the MessagePack encoder/decoder, Vec<u8> growth, nested
//! structs + collections. Panic-safe (no unwraps on fallible data; Result handled).
//! SUCCESS = "== soak_rmp-serde done ==" with sane values.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Inner {
    a: i32,
    b: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Record {
    name: String,
    stars: u64,
    langs: Vec<String>,
    nested: Inner,
    maybe: Option<i64>,
}

fn main() {
    println!("== soak_rmp-serde start ==");

    let rec = Record {
        name: "rustc_codegen_clr".to_string(),
        stars: 1234,
        langs: vec!["rust".to_string(), "csharp".to_string(), "cil".to_string()],
        nested: Inner { a: 7, b: true },
        maybe: Some(-42),
    };

    // serialize to MessagePack bytes
    match rmp_serde::to_vec(&rec) {
        Ok(bytes) => {
            println!("1  to_vec: {} bytes", bytes.len());
            // first few bytes as a sanity fingerprint
            let head: Vec<u8> = bytes.iter().take(4).copied().collect();
            println!("2  head bytes: {head:?}");

            // round-trip back
            match rmp_serde::from_slice::<Record>(&bytes) {
                Ok(back) => {
                    println!("3  from_slice ok");
                    println!("4  matches: {}", back == rec);
                    println!("5  name={} stars={} langs={} nested.a={} maybe={:?}",
                        back.name, back.stars, back.langs.len(), back.nested.a, back.maybe);
                }
                Err(e) => println!("3  from_slice err: {e}"),
            }
        }
        Err(e) => println!("1  to_vec err: {e}"),
    }

    // also exercise a Vec of structs round-trip
    let many: Vec<Inner> = (0..5).map(|i| Inner { a: i, b: i % 2 == 0 }).collect();
    match rmp_serde::to_vec(&many) {
        Ok(bytes) => {
            println!("6  vec to_vec: {} bytes", bytes.len());
            match rmp_serde::from_slice::<Vec<Inner>>(&bytes) {
                Ok(back) => println!("7  vec round-trip matches: {}", back == many),
                Err(e) => println!("7  vec from_slice err: {e}"),
            }
        }
        Err(e) => println!("6  vec to_vec err: {e}"),
    }

    println!("== soak_rmp-serde done ==");
}
