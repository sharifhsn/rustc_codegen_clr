//! H2 real-crate SOAK: humantime (2.x) duration parse + format round-trip on the dotnet PAL.
//! Exercises humantime::parse_duration (returns Result) and format_duration (Display wrapper),
//! core::time::Duration arithmetic, String/fmt. Panic-safe: no unwrap/expect on fallible paths,
//! valid inputs only; the one error path is handled. SUCCESS = "== soak_humantime done ==".
use std::time::Duration;

fn main() {
    println!("== soak_humantime start ==");

    // parse_duration returns Result<Duration, _>; handle without unwrap/expect.
    match humantime::parse_duration("2h 30m") {
        Ok(d) => {
            println!("1  parsed secs = {}", d.as_secs()); // expect 9000
            // format_duration returns a Display wrapper; render via to_string (fmt path).
            let formatted = humantime::format_duration(d).to_string();
            println!("2  formatted = {}", formatted);
        }
        Err(_) => println!("1  parse_error = true"),
    }

    // A second valid input exercising a different unit mix.
    match humantime::parse_duration("1day 1h 1m 1s") {
        Ok(d) => println!("3  parsed2 secs = {}", d.as_secs()), // 86400+3600+60+1 = 90061
        Err(_) => println!("3  parse2_error = true"),
    }

    // format_duration of a known Duration built directly (no parse dependency).
    let direct = Duration::new(3661, 0); // 1h 1m 1s
    println!("4  direct_formatted = {}", humantime::format_duration(direct).to_string());

    // Error path must return Err (not panic) — invalid unit string.
    match humantime::parse_duration("not a duration") {
        Ok(_) => println!("5  invalid = unexpectedly_ok"),
        Err(_) => println!("5  invalid = err_as_expected"),
    }

    println!("== soak_humantime done ==");
}
