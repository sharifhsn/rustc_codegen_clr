use serde::Deserialize;
use serde_json::Value;

// A struct deserialized directly from a JSON5 document (with comments + trailing commas).
#[derive(Deserialize)]
struct Config {
    name: String,
    version: u32,
    enabled: bool,
    ratio: f64,
    tags: Vec<String>,
}

fn main() {
    // JSON5 source: line/block comments, unquoted keys, trailing commas, single quotes,
    // hex literal, leading-plus number. All of this is INVALID JSON but valid JSON5.
    let src = r#"
        {
            // line comment
            name: 'survey',          /* block comment */
            version: 0x2A,           // hex -> 42
            enabled: true,
            ratio: +0.250000,
            tags: [ 'a', 'b', 'c', ],
        }
    "#;

    // 1) Parse into a typed struct.
    match json5::from_str::<Config>(src) {
        Ok(cfg) => {
            println!("struct_name = {}", cfg.name);
            println!("struct_version = {}", cfg.version);
            println!("struct_enabled = {}", cfg.enabled);
            // Fixed precision so float shortest-repr cannot drift across runtimes.
            println!("struct_ratio = {:.6}", cfg.ratio);
            println!("struct_tags_len = {}", cfg.tags.len());
            // Deterministic: join in declared (vec) order.
            println!("struct_tags = {}", cfg.tags.join(","));
        }
        Err(_) => println!("struct_parse = error"),
    }

    // 2) Parse the same document into an untyped serde_json::Value and probe it.
    match json5::from_str::<Value>(src) {
        Ok(v) => {
            println!("value_is_object = {}", v.is_object());
            // Field access via get() returns Option -> no panic path.
            match v.get("version").and_then(Value::as_u64) {
                Some(n) => println!("value_version = {}", n),
                None => println!("value_version = <none>"),
            }
            match v.get("name").and_then(Value::as_str) {
                Some(s) => println!("value_name = {}", s),
                None => println!("value_name = <none>"),
            }
            match v.get("tags").and_then(Value::as_array) {
                Some(arr) => println!("value_tags_len = {}", arr.len()),
                None => println!("value_tags_len = <none>"),
            }
            // Deterministic re-serialization to compact JSON (object keys keep insertion order
            // because serde_json preserves declaration order without the preserve_order feature
            // for arrays; for the object we sort keys explicitly to be safe).
            if let Some(obj) = v.as_object() {
                let mut keys: Vec<&String> = obj.keys().collect();
                keys.sort();
                println!("value_keys_sorted = {}", keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(","));
            }
        }
        Err(_) => println!("value_parse = error"),
    }

    // 3) Deserialize a scalar / array directly (json5 handles top-level non-objects).
    match json5::from_str::<Vec<i64>>("[ 1, 2, 3, /* trailing */ 4, ]") {
        Ok(nums) => {
            let sum: i64 = nums.iter().sum();
            println!("array_len = {}", nums.len());
            println!("array_sum = {}", sum);
        }
        Err(_) => println!("array_parse = error"),
    }

    // 4) Error path: malformed JSON5 must yield Err, not a panic.
    match json5::from_str::<Value>("{ unterminated: ") {
        Ok(_) => println!("malformed_parse = unexpected_ok"),
        Err(_) => println!("malformed_parse = error_as_expected"),
    }

    // 5) Serialize a struct back out to a JSON5 string (json5::to_string).
    match json5::to_string(&serde_json::json!({ "k": 7, "list": [1, 2] })) {
        Ok(s) => println!("to_string_len = {}", s.len()),
        Err(_) => println!("to_string = error"),
    }

    println!("== survey_json5 done ==");
}
