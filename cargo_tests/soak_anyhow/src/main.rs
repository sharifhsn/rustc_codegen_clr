use anyhow::{anyhow, Context, Error, Result};
use std::fmt;

// A concrete error type we can downcast back to (dyn Error + downcast).
#[derive(Debug)]
struct LowLevel {
    code: i32,
}

impl fmt::Display for LowLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "low-level failure (code {})", self.code)
    }
}

impl std::error::Error for LowLevel {}

// Returns an anyhow::Error wrapping our concrete error type.
fn read_thing() -> Result<()> {
    Err(Error::new(LowLevel { code: 42 }))
}

// Adds context, building an error chain (cause chain of length >= 2).
fn load_config() -> Result<()> {
    read_thing().context("failed while loading config")
}

fn main() {
    // Build an anyhow::Error via context/chain.
    let result = load_config().context("startup aborted");

    match result {
        Ok(()) => {
            println!("unexpected_ok = true");
        }
        Err(err) => {
            // Top-level display.
            println!("error = {}", err);

            // Print the full chain (no panic path).
            let mut depth = 0usize;
            for cause in err.chain() {
                println!("chain[{}] = {}", depth, cause);
                depth += 1;
            }
            println!("chain_len = {}", depth);

            // downcast_ref to the concrete leaf type.
            match err.downcast_ref::<LowLevel>() {
                Some(low) => println!("downcast_code = {}", low.code),
                None => println!("downcast = none"),
            }

            // root_cause should be the LowLevel error.
            let root = err.root_cause();
            println!("root_cause = {}", root);
        }
    }

    // A second error built via the anyhow! macro, with format args.
    let e2: Error = anyhow!("dynamic error with value {}", 7);
    println!("e2 = {}", e2);

    println!("== soak_anyhow done ==");
}
