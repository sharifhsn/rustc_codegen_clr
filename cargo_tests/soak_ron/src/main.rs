//! H2 real-crate SOAK: ron (Rusty Object Notation) on the dotnet PAL.
//! A #[derive(Serialize, Deserialize)] struct -> ron::to_string -> ron::from_str round-trip.
//! Exercises serde derive, ron's serializer/deserializer, nested structs/enums/Vec/Option,
//! String/fmt. Panic-safe (no unwraps on fallible data; Result/Option handled explicitly).
//! SUCCESS = "== soak_ron done ==" with sane values.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Config {
    name: String,
    stars: u64,
    enabled: bool,
    langs: Vec<String>,
    nested: Nested,
    maybe: Option<i32>,
    kind: Kind,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Nested {
    a: i32,
    b: i32,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Kind {
    Simple,
    Tagged(u32),
    Struct { x: f64, y: f64 },
}

fn main() {
    println!("== soak_ron start ==");

    let original = Config {
        name: "rustc_codegen_clr".to_string(),
        stars: 1234,
        enabled: true,
        langs: vec!["rust".to_string(), "csharp".to_string(), "cil".to_string()],
        nested: Nested { a: 1, b: 2 },
        maybe: Some(42),
        kind: Kind::Struct { x: 1.5, y: -2.5 },
    };

    // serialize
    match ron::to_string(&original) {
        Ok(s) => {
            println!("1  serialize: {} bytes", s.len());

            // round-trip back
            match ron::from_str::<Config>(&s) {
                Ok(parsed) => {
                    println!("2  parse: name={}", parsed.name);
                    println!("3  stars={}", parsed.stars);
                    println!("4  langs.len={}", parsed.langs.len());
                    println!("5  nested.b={}", parsed.nested.b);
                    println!("6  maybe={:?}", parsed.maybe);
                    println!("7  kind={:?}", parsed.kind);
                    println!("8  round-trip eq={}", parsed == original);
                }
                Err(e) => println!("2  parse err: {e}"),
            }
        }
        Err(e) => println!("1  serialize err: {e}"),
    }

    // also exercise the pretty serializer
    match ron::ser::to_string_pretty(&original, ron::ser::PrettyConfig::default()) {
        Ok(p) => println!("9  pretty: {} lines", p.lines().count()),
        Err(e) => println!("9  pretty err: {e}"),
    }

    println!("== soak_ron done ==");
}
