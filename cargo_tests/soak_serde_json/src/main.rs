use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Record {
    id: u64,
    name: String,
    vals: Vec<f64>,
    flag: bool,
}

fn main() {
    // Use exact/round f64 values so the shortest-repr is stable across runtimes.
    let rec = Record {
        id: 42,
        name: String::from("widget"),
        vals: vec![1.0, 2.5, 4.25, 8.125],
        flag: true,
    };

    // --- Serialize: compact + pretty (round-trip step 1). ---
    let compact = match serde_json::to_string(&rec) {
        Ok(s) => s,
        Err(_) => {
            println!("to_string_error = true");
            println!("== soak_serde_json done ==");
            return;
        }
    };
    println!("compact = {}", compact);

    let pretty = match serde_json::to_string_pretty(&rec) {
        Ok(s) => s,
        Err(_) => {
            println!("to_string_pretty_error = true");
            println!("== soak_serde_json done ==");
            return;
        }
    };
    // Pretty output spans multiple lines; print its length + line count (deterministic).
    println!("pretty_len = {}", pretty.len());
    println!("pretty_lines = {}", pretty.lines().count());

    // --- Parse it back (round-trip step 2). ---
    match serde_json::from_str::<Record>(&compact) {
        Ok(back) => {
            println!("rt_id = {}", back.id);
            println!("rt_name = {}", back.name);
            println!("rt_vals_len = {}", back.vals.len());
            println!("rt_flag = {}", back.flag);
            // Sum of exact-repr floats -> exact integer-valued sum; format fixed precision.
            let sum: f64 = back.vals.iter().sum();
            println!("rt_vals_sum = {:.4}", sum);
            // Field-by-field equality with the original (no float == surprises: exact reprs).
            let same = back.id == rec.id
                && back.name == rec.name
                && back.vals == rec.vals
                && back.flag == rec.flag;
            println!("roundtrip_matches = {}", same);
        }
        Err(_) => {
            println!("from_str_error = true");
        }
    }

    // --- Parse a literal JSON with ints/floats/nested arrays via the dynamic Value API. ---
    let literal = r#"{
        "title": "demo",
        "count": 7,
        "ratio": 0.5,
        "matrix": [[1, 2, 3], [4, 5, 6]],
        "nested": { "ok": true, "tags": ["a", "b", "c"] }
    }"#;

    match serde_json::from_str::<serde_json::Value>(literal) {
        Ok(v) => {
            // Pull typed fields out of the Value tree without indexing-panics.
            let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("<none>");
            println!("lit_title = {}", title);

            let count = v.get("count").and_then(|x| x.as_u64()).unwrap_or(0);
            println!("lit_count = {}", count);

            let ratio = v.get("ratio").and_then(|x| x.as_f64()).unwrap_or(0.0);
            println!("lit_ratio = {:.4}", ratio);

            // Sum every integer in the nested matrix (exercises array iteration).
            let mut matrix_sum: i64 = 0;
            if let Some(rows) = v.get("matrix").and_then(|x| x.as_array()) {
                for row in rows {
                    if let Some(cells) = row.as_array() {
                        for cell in cells {
                            matrix_sum += cell.as_i64().unwrap_or(0);
                        }
                    }
                }
            }
            println!("lit_matrix_sum = {}", matrix_sum);

            let nested_ok = v
                .get("nested")
                .and_then(|n| n.get("ok"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            println!("lit_nested_ok = {}", nested_ok);

            let tags_len = v
                .get("nested")
                .and_then(|n| n.get("tags"))
                .and_then(|t| t.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            println!("lit_tags_len = {}", tags_len);

            // Re-serialize the dynamic Value (compact) — deterministic because object
            // key order is preserved by serde_json in insertion order for this build.
            match serde_json::to_string(&v) {
                Ok(s) => println!("lit_reserialized_len = {}", s.len()),
                Err(_) => println!("lit_reserialize_error = true"),
            }
        }
        Err(_) => {
            println!("literal_parse_error = true");
        }
    }

    println!("== soak_serde_json done ==");
}
