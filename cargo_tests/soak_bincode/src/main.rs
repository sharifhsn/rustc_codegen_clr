//! H2 real-crate SOAK: bincode 1.x serialize/deserialize round-trip on the dotnet PAL.
//! Exercises serde derive (Serialize/Deserialize), bincode's binary encoder/decoder,
//! Vec/String/Option/enum, integer endianness, and a fixed-size byte buffer round-trip.
//! Panic-safe: no unwrap/expect on fallible ops; all Results are matched.
//! SUCCESS = "== soak_bincode done ==" with sane values + a true match.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Inner {
    flag: bool,
    ratio: f64,
    tag: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Kind {
    Empty,
    Num(i64),
    Pair(u32, u32),
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Record {
    id: u64,
    name: String,
    values: Vec<i32>,
    inner: Inner,
    kind: Kind,
}

fn main() {
    println!("== soak_bincode start ==");

    let original = Record {
        id: 0xDEAD_BEEF,
        name: "rustc_codegen_clr".to_string(),
        values: vec![1, -2, 3, -4, 5],
        inner: Inner {
            flag: true,
            ratio: 3.5,
            tag: Some("soak".to_string()),
        },
        kind: Kind::Pair(7, 42),
    };

    // serialize
    match bincode::serialize(&original) {
        Ok(bytes) => {
            println!("1  serialize: {} bytes", bytes.len());

            // deserialize back
            match bincode::deserialize::<Record>(&bytes) {
                Ok(decoded) => {
                    println!("2  deserialize ok: id={:#x}", decoded.id);
                    println!("3  name={}", decoded.name);
                    println!("4  values={:?}", decoded.values);
                    println!(
                        "5  inner.flag={} ratio={} tag={:?}",
                        decoded.inner.flag, decoded.inner.ratio, decoded.inner.tag
                    );
                    println!("6  kind={:?}", decoded.kind);
                    println!("7  matches original = {}", decoded == original);
                }
                Err(e) => println!("2  deserialize err: {e}"),
            }
        }
        Err(e) => println!("1  serialize err: {e}"),
    }

    // also exercise a couple of enum variants explicitly
    for k in [Kind::Empty, Kind::Num(-99), Kind::Pair(1, 2)] {
        match bincode::serialize(&k) {
            Ok(b) => match bincode::deserialize::<Kind>(&b) {
                Ok(d) => println!("8  variant round-trip ok: {:?} ({} bytes) match={}", d, b.len(), d == k),
                Err(e) => println!("8  variant deser err: {e}"),
            },
            Err(e) => println!("8  variant ser err: {e}"),
        }
    }

    println!("== soak_bincode done ==");
}
