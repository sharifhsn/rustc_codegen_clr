//! H2 real-crate SOAK: serde derive (#[derive(Serialize, Deserialize)]) + serde_json round-trip.
//! Exercises the DERIVE-expanded Serialize/Deserialize impls (Visitor, SeqAccess/MapAccess generics),
//! nested struct, Vec, Option, several scalar field types. Panic-safe (no unwraps on fallible ops).
//! SUCCESS = "== soak_serde done ==" with sane values + a clean round-trip equality.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Config {
    name: String,
    count: u64,
    ratio: f64,
    enabled: bool,
    tags: Vec<String>,
    maybe: Option<i32>,
    nothing: Option<i32>,
    origin: Point,
    points: Vec<Point>,
}

fn main() {
    println!("== soak_serde start ==");

    let cfg = Config {
        name: "rustc_codegen_clr".to_string(),
        count: 1234,
        ratio: 3.5,
        enabled: true,
        tags: vec!["rust".to_string(), "csharp".to_string(), "cil".to_string()],
        maybe: Some(42),
        nothing: None,
        origin: Point { x: 0, y: 0 },
        points: vec![Point { x: 1, y: 2 }, Point { x: 3, y: 4 }],
    };

    // Serialize (derive-generated Serialize)
    let json = match serde_json::to_string(&cfg) {
        Ok(s) => {
            println!("1  serialize: {} bytes", s.len());
            s
        }
        Err(e) => {
            println!("1  serialize err: {e}");
            println!("== soak_serde done ==");
            return;
        }
    };

    // Deserialize (derive-generated Deserialize + Visitor)
    match serde_json::from_str::<Config>(&json) {
        Ok(back) => {
            println!("2  deserialize ok: name={}", back.name);
            println!("3  count={} ratio={} enabled={}", back.count, back.ratio, back.enabled);
            println!("4  tags.len={} maybe={:?} nothing={:?}", back.tags.len(), back.maybe, back.nothing);
            println!("5  origin=({},{}) points.len={}", back.origin.x, back.origin.y, back.points.len());
            // round-trip equality across the derive-expanded impls
            println!("6  round-trip eq: {}", back == cfg);
        }
        Err(e) => {
            println!("2  deserialize err: {e}");
        }
    }

    // Deserialize from an external literal (different field order, exercises MapAccess key matching)
    let literal = r#"{"name":"lit","count":7,"ratio":1.25,"enabled":false,"tags":["a"],"maybe":null,"nothing":99,"origin":{"x":-1,"y":-2},"points":[]}"#;
    match serde_json::from_str::<Config>(literal) {
        Ok(c) => println!("7  literal: name={} count={} maybe={:?} nothing={:?} origin=({},{})",
            c.name, c.count, c.maybe, c.nothing, c.origin.x, c.origin.y),
        Err(e) => println!("7  literal err: {e}"),
    }

    println!("== soak_serde done ==");
}
