//! H2 real-crate SOAK: the `toml` crate parsing + re-serializing on the dotnet PAL.
//! Parse a small TOML doc into toml::Value, inspect a couple of fields, then
//! toml::to_string round-trip. Exercises serde, the toml lexer/parser/deserializer,
//! String/Vec/BTreeMap, fmt. Panic-safe (no unwraps on data; handle Result/Option).
//! SUCCESS = "== soak_toml done ==" with sane values.
use toml::Value;

fn main() {
    println!("== soak_toml start ==");
    let input = r#"
title = "rustc_codegen_clr"
stars = 1234
langs = ["rust", "csharp", "cil"]

[owner]
name = "FractalFir"
active = true
"#;

    match input.parse::<Value>() {
        Ok(v) => {
            let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("?");
            println!("1  parse: title={title}");

            let stars = v.get("stars").and_then(|s| s.as_integer()).unwrap_or(-1);
            println!("2  stars={stars}");

            let langs_len = v
                .get("langs")
                .and_then(|l| l.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            println!("3  langs.len={langs_len}");

            let owner_name = v
                .get("owner")
                .and_then(|o| o.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("?");
            println!("4  owner.name={owner_name}");

            let owner_active = v
                .get("owner")
                .and_then(|o| o.get("active"))
                .and_then(|a| a.as_bool())
                .unwrap_or(false);
            println!("5  owner.active={owner_active}");

            // serialize back
            match toml::to_string(&v) {
                Ok(s) => println!("6  reserialize: {} bytes, {} lines", s.len(), s.lines().count()),
                Err(e) => println!("6  serialize err: {e}"),
            }

            // iterate top-level table keys (BTreeMap -> sorted)
            if let Some(tbl) = v.as_table() {
                let keys: Vec<&str> = tbl.keys().map(|k| k.as_str()).collect();
                println!("7  keys: {keys:?}");
            }
        }
        Err(e) => println!("parse err: {e}"),
    }
    println!("== soak_toml done ==");
}
