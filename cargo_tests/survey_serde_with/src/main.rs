// survey_serde_with: exercise serde_with's #[serde_as] conversions through a
// serde_json round-trip. All output is deterministic (fixed inputs, integer/
// string fields, sorted map iteration) so it can be byte-compared between
// native rustc and the .NET backend.

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr, DurationSeconds};
use std::time::Duration;

#[serde_as]
#[derive(Serialize, Deserialize)]
struct Config {
    // DisplayFromStr: a numeric field is serialized as a *string* via Display
    // and parsed back via FromStr. Exercises the macros + the std num Display/FromStr.
    #[serde_as(as = "DisplayFromStr")]
    port: u16,

    // DurationSeconds<u64>: a Duration encoded as an integer number of seconds.
    #[serde_as(as = "DurationSeconds<u64>")]
    timeout: Duration,

    // Vec<(K, V)> serialized as a JSON *map* instead of an array of pairs.
    // Keys chosen so insertion order is already sorted -> deterministic.
    #[serde_as(as = "Vec<(DisplayFromStr, _)>")]
    weights: Vec<(u32, i64)>,

    // A plain field carried through untouched, for contrast.
    name: String,
}

fn main() {
    let cfg = Config {
        port: 8080,
        timeout: Duration::from_secs(30),
        weights: vec![(1, 100), (2, 200), (3, 300)],
        name: String::from("survey"),
    };

    // Serialize -> JSON string (round-trip step 1).
    match serde_json::to_string(&cfg) {
        Ok(json) => {
            println!("serialized = {}", json);

            // Deserialize back (round-trip step 2).
            match serde_json::from_str::<Config>(&json) {
                Ok(back) => {
                    println!("port = {}", back.port);
                    println!("timeout_secs = {}", back.timeout.as_secs());
                    println!("name = {}", back.name);
                    println!("weights_len = {}", back.weights.len());

                    // Deterministic: weights are already in ascending key order.
                    let mut sum: i64 = 0;
                    for (k, v) in &back.weights {
                        println!("weight[{}] = {}", k, v);
                        sum += *v;
                    }
                    println!("weights_sum = {}", sum);

                    // Round-trip equality on the scalar fields (no float drift).
                    let port_ok = back.port == cfg.port;
                    let timeout_ok = back.timeout == cfg.timeout;
                    let name_ok = back.name == cfg.name;
                    let weights_ok = back.weights == cfg.weights;
                    println!("port_ok = {}", port_ok);
                    println!("timeout_ok = {}", timeout_ok);
                    println!("name_ok = {}", name_ok);
                    println!("weights_ok = {}", weights_ok);
                    println!(
                        "roundtrip_ok = {}",
                        port_ok && timeout_ok && name_ok && weights_ok
                    );
                }
                Err(_) => println!("deserialize_error = true"),
            }
        }
        Err(_) => println!("serialize_error = true"),
    }

    println!("== survey_serde_with done ==");
}
