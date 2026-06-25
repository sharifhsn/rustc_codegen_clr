//! H2 real-crate SOAK: ciborium (CBOR codec) on the dotnet PAL.
//! A #[derive(Serialize, Deserialize)] struct -> ciborium::into_writer(Vec<u8>) ->
//! ciborium::from_reader round-trip. Exercises serde derive traits/generics, ciborium's
//! Writer/Reader over a byte slice, nested Vec/String/Option, integer + float encoding.
//! Panic-safe: handles every Result, no unwrap/expect/indexing on fallible data.
//! SUCCESS = "== soak_ciborium done ==" with a matching round-trip.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct Inner {
    tag: String,
    weight: f64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct Record {
    name: String,
    id: u64,
    active: bool,
    scores: Vec<i32>,
    note: Option<String>,
    inner: Inner,
}

fn main() {
    println!("== soak_ciborium start ==");

    let original = Record {
        name: "rustc_codegen_clr".to_string(),
        id: 0xDEAD_BEEF,
        active: true,
        scores: vec![1, -2, 3, -4, 5],
        note: Some("cbor round-trip".to_string()),
        inner: Inner {
            tag: "nested".to_string(),
            weight: 3.5,
        },
    };

    // Serialize to CBOR bytes.
    let mut buf: Vec<u8> = Vec::new();
    match ciborium::into_writer(&original, &mut buf) {
        Ok(()) => println!("1  encoded: {} bytes", buf.len()),
        Err(e) => {
            println!("1  encode err: {e:?}");
            println!("== soak_ciborium done ==");
            return;
        }
    }

    // Show first few bytes (CBOR is binary; map major type 0xA6 expected for 6-field map-as-struct).
    let head: Vec<String> = buf.iter().take(4).map(|b| format!("{b:02x}")).collect();
    println!("2  head bytes: {head:?}");

    // Deserialize back.
    match ciborium::from_reader::<Record, _>(buf.as_slice()) {
        Ok(decoded) => {
            println!("3  decoded.name={}", decoded.name);
            println!("4  decoded.id={:#x}", decoded.id);
            println!("5  decoded.scores.len={}", decoded.scores.len());
            println!(
                "6  decoded.note={}",
                decoded.note.as_deref().unwrap_or("<none>")
            );
            println!("7  decoded.inner.weight={}", decoded.inner.weight);
            println!("8  round-trip equal: {}", decoded == original);
        }
        Err(e) => println!("3  decode err: {e:?}"),
    }

    println!("== soak_ciborium done ==");
}
