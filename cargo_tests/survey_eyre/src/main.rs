use eyre::{eyre, Report, WrapErr};
use std::error::Error as StdError;
use std::fmt;

// A concrete, custom error type so we can demonstrate downcasting back to it.
#[derive(Debug)]
struct ParseError {
    input: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "could not parse {:?}", self.input)
    }
}

impl StdError for ParseError {}

// Build a layered error: a root cause wrapped twice with extra context.
// `wrap_err` adds a new "context" message on top of the existing chain.
fn build_layered_error() -> Report {
    let root = ParseError {
        input: "not-a-number".to_string(),
    };
    Report::new(root)
        .wrap_err("failed to read config field `port`")
        .wrap_err("failed to start service")
}

fn main() {
    // eyre's Display does NOT capture a backtrace unless RUST_BACKTRACE /
    // RUST_LIB_BACKTRACE is set; we leave them unset so output is deterministic.

    // 1) An error constructed from a formatted message via the `eyre!` macro.
    let simple: Report = eyre!("simple failure code {}", 42);
    println!("simple_display = {}", simple);

    // 2) A layered report built with wrap_err (context stacking).
    let layered = build_layered_error();

    // Top-level Display shows only the outermost context message.
    println!("layered_top = {}", layered);

    // Alternate Display ({:#}) renders the whole chain inline, joined by ": ".
    // This is deterministic (no backtrace, no addresses).
    println!("layered_full = {:#}", layered);

    // 3) Walk the cause chain explicitly via std::error::Error sources.
    //    `Report::chain()` yields each link from outermost to root.
    let mut count = 0usize;
    for (i, link) in layered.chain().enumerate() {
        println!("chain[{}] = {}", i, link);
        count += 1;
    }
    println!("chain_len = {}", count);

    // The root cause is the last link in the chain.
    match layered.chain().last() {
        Some(root) => println!("root_cause = {}", root),
        None => println!("root_cause = <none>"),
    }

    // 4) Downcast the report back to our concrete ParseError.
    //    `downcast_ref` borrows; it returns Some only if the ROOT (the
    //    originally-constructed error) is of that type. After wrap_err the
    //    underlying concrete error is still ParseError.
    match layered.downcast_ref::<ParseError>() {
        Some(pe) => {
            println!("downcast_ok = true");
            println!("downcast_input = {}", pe.input);
        }
        None => println!("downcast_ok = false"),
    }

    // A downcast to a type that is NOT present must fail cleanly (no panic).
    match layered.downcast_ref::<std::io::Error>() {
        Some(_) => println!("downcast_io = true"),
        None => println!("downcast_io = false"),
    }

    // 5) Use wrap_err on a Result via the WrapErr extension trait, then
    //    inspect the produced context. Drive a deterministic failing Result.
    let parsed: Result<i64, std::num::ParseIntError> = "12x".parse::<i64>();
    let with_ctx: Result<i64, Report> =
        parsed.wrap_err("parsing the retry count failed");
    match with_ctx {
        Ok(v) => println!("result_value = {}", v),
        Err(report) => {
            println!("result_err_top = {}", report);
            println!("result_err_full = {:#}", report);
            // The root of THIS report is a ParseIntError from std.
            match report.downcast_ref::<std::num::ParseIntError>() {
                Some(_) => println!("result_root_is_parseint = true"),
                None => println!("result_root_is_parseint = false"),
            }
        }
    }

    // 6) wrap_err_with: context computed lazily (only on the error path).
    let lazy: Result<i64, Report> = "".parse::<i64>().wrap_err_with(|| {
        format!("empty string is not a valid integer (len {})", 0)
    });
    match lazy {
        Ok(v) => println!("lazy_value = {}", v),
        Err(report) => println!("lazy_err = {}", report),
    }

    println!("== survey_eyre done ==");
}
