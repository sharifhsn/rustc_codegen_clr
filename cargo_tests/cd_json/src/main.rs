// The END-USER experience of the `System.Text.Json` bridge: `mycorrhiza::bcl::json::Json` used like a
// small serde-ish DOM — `Json::parse(text)`, `.get("prop")`, `.index(i)`, `.as_str()`/`.as_i64()`/
// `.as_bool()`, and `.to_json_string()`. No `JsonNode`, no `get_Item`, no assembly strings, no
// enumerators. Backed by real managed `System.Text.Json` objects on the CLR heap.
//
// Every result is checked in-Rust; `main` prints `pass` then `total` (a `9000000xx` marker flags any
// failing check) and returns non-zero on any mismatch.
#![allow(dead_code)]

use mycorrhiza::bcl::json::{Json, Kind};
use mycorrhiza::system::console::Console;

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    // ---------- parse + top-level kind ----------
    let src = r#"{
        "name": "ada",
        "age": 36,
        "active": true,
        "retired": false,
        "score": 3.5,
        "nickname": null,
        "tags": ["alpha", "beta", "gamma"],
        "address": { "city": "London", "zip": 90210 }
    }"#;
    let doc = Json::parse(src).expect("parse ok");
    chk!(doc.kind(), Kind::Object);
    chk!(doc.is_object(), true);
    chk!(doc.is_array(), false);

    // ---------- object navigation + scalar reads ----------
    chk!(doc.get("name").and_then(|n| n.as_str()).as_deref(), Some("ada"));
    chk!(doc.get("age").and_then(|n| n.as_i64()), Some(36));
    chk!(doc.get("score").and_then(|n| n.as_f64()), Some(3.5));
    chk!(doc.get("active").and_then(|n| n.as_bool()), Some(true));
    chk!(doc.get("retired").and_then(|n| n.as_bool()), Some(false));

    // kind discrimination per field
    chk!(doc.get("name").map(|n| n.kind()), Some(Kind::String));
    chk!(doc.get("age").map(|n| n.kind()), Some(Kind::Number));
    chk!(doc.get("active").map(|n| n.kind()), Some(Kind::True));
    chk!(doc.get("retired").map(|n| n.kind()), Some(Kind::False));

    // a JSON `null` field comes back as `None` from `get` (Parse yields a managed-null child)
    chk!(doc.get("nickname").is_none(), true);
    // a missing property is also `None`
    chk!(doc.get("does_not_exist").is_none(), true);

    // wrong-type reads yield `None`, not a panic
    chk!(doc.get("name").and_then(|n| n.as_i64()), None);
    chk!(doc.get("age").and_then(|n| n.as_str()), None);
    chk!(doc.get("age").and_then(|n| n.as_bool()), None);

    // ---------- array navigation ----------
    let tags = doc.get("tags").expect("tags present");
    chk!(tags.kind(), Kind::Array);
    chk!(tags.is_array(), true);
    chk!(tags.len(), 3);
    chk!(tags.is_empty(), false);
    chk!(tags.index(0).and_then(|n| n.as_str()).as_deref(), Some("alpha"));
    chk!(tags.index(1).and_then(|n| n.as_str()).as_deref(), Some("beta"));
    chk!(tags.index(2).and_then(|n| n.as_str()).as_deref(), Some("gamma"));
    chk!(tags.index(3).is_none(), true); // out of range → None
    chk!(tags.index(-1).is_none(), true); // negative → None

    // len()/index() on a non-array are inert
    chk!(doc.get("name").map(|n| n.len()), Some(0));
    chk!(doc.get("name").and_then(|n| n.index(0)).is_none(), true);

    // ---------- nested object ----------
    let addr = doc.get("address").expect("address present");
    chk!(addr.kind(), Kind::Object);
    chk!(addr.get("city").and_then(|n| n.as_str()).as_deref(), Some("London"));
    chk!(addr.get("zip").and_then(|n| n.as_i64()), Some(90210));

    // ---------- top-level array document ----------
    let arr = Json::parse("[10, 20, 30]").expect("array parse");
    chk!(arr.kind(), Kind::Array);
    chk!(arr.len(), 3);
    chk!(arr.index(0).and_then(|n| n.as_i64()), Some(10));
    chk!(arr.index(2).and_then(|n| n.as_i64()), Some(30));

    // ---------- top-level scalar documents ----------
    let s = Json::parse(r#""just a string""#).expect("string parse");
    chk!(s.kind(), Kind::String);
    chk!(s.as_str().as_deref(), Some("just a string"));

    let n = Json::parse("42").expect("number parse");
    chk!(n.kind(), Kind::Number);
    chk!(n.as_i64(), Some(42));

    let b = Json::parse("true").expect("bool parse");
    chk!(b.as_bool(), Some(true));

    // literal `null` document → None from parse
    chk!(Json::parse("null").is_none(), true);
    // malformed JSON → None from parse (JsonException caught)
    chk!(Json::parse("{ not valid").is_none(), true);

    // ---------- serialize / round-trip ----------
    let compact = Json::parse(r#"{ "a" : 1 , "b" : [ 2 , 3 ] }"#).expect("parse");
    // ToJsonString emits compact JSON (no insignificant whitespace).
    chk!(compact.to_json_string().as_str(), r#"{"a":1,"b":[2,3]}"#);
    // Display uses the same serializer.
    chk!(std::format!("{compact}").as_str(), r#"{"a":1,"b":[2,3]}"#);
    // a nested node serializes just its sub-tree.
    chk!(compact.get("b").map(|b| b.to_json_string()).as_deref(), Some("[2,3]"));
    // re-parsing the serialized form navigates identically.
    let rt = Json::parse(&compact.to_json_string()).expect("round-trip parse");
    chk!(rt.get("a").and_then(|n| n.as_i64()), Some(1));
    chk!(rt.get("b").and_then(|t| t.index(1)).and_then(|n| n.as_i64()), Some(3));

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
