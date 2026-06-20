//! H2 real-crate SOAK: a real, dependency-using crate (serde_json) doing real work on the dotnet PAL.
//! JSON parse -> inspect -> mutate -> serialize round-trip. Exercises serde traits/generics, the
//! serde_json::Map (BTreeMap), String/Vec, recursion, fmt. Panic-safe (no unwraps on user data).
//! SUCCESS = "== pal_soak done ==" with sane values.
use serde_json::{json, Value};

fn main() {
    println!("== pal_soak start ==");
    let input = r#"{"name":"rustc_codegen_clr","stars":1234,"langs":["rust","csharp","cil"],"nested":{"a":1,"b":2}}"#;

    match serde_json::from_str::<Value>(input) {
        Ok(mut v) => {
            println!("1  parse: name={}", v["name"].as_str().unwrap_or("?"));
            println!("2  stars={}", v["stars"].as_u64().unwrap_or(0));
            println!("3  langs.len={}", v["langs"].as_array().map(|a| a.len()).unwrap_or(0));
            println!("4  nested.b={}", v["nested"]["b"].as_u64().unwrap_or(0));

            // mutate
            let stars = v["stars"].as_u64().unwrap_or(0);
            v["stars"] = json!(stars + 1);
            v["added"] = json!({"by": "pal_soak", "ok": true, "list": [1, 2, 3]});

            // serialize back
            match serde_json::to_string(&v) {
                Ok(s) => println!("5  reserialize: {} bytes, stars now {}", s.len(), v["stars"]),
                Err(e) => println!("5  serialize err: {e}"),
            }
            match serde_json::to_string_pretty(&v) {
                Ok(p) => println!("6  pretty: {} lines", p.lines().count()),
                Err(e) => println!("6  pretty err: {e}"),
            }

            // iterate object keys (sorted via serde_json::Map)
            if let Some(obj) = v.as_object() {
                let keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
                println!("7  keys: {keys:?}");
            }
        }
        Err(e) => println!("parse err: {e}"),
    }
    println!("== pal_soak done ==");
}
