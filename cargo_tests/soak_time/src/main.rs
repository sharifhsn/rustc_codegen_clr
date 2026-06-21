use time::macros::{date, datetime};
use time::{Duration, PrimitiveDateTime};

// A fixed, well-known format description (ISO-ish). Built at runtime, panic-safe.
fn main() {
    // Deterministic Date via the compile-time `date!` macro (no fallible parsing).
    let d = date!(2026 - 06 - 20);
    println!("date: {}-{:02}-{:02}", d.year(), d.month() as u8, d.day());

    // Add a Duration of days to the Date.
    let later = d + Duration::days(40);
    println!(
        "date + 40d: {}-{:02}-{:02}",
        later.year(),
        later.month() as u8,
        later.day()
    );

    // Deterministic PrimitiveDateTime via `datetime!`.
    let dt: PrimitiveDateTime = datetime!(2026 - 06 - 20 13:30:45);
    println!(
        "datetime: {}-{:02}-{:02} {:02}:{:02}:{:02}",
        dt.year(),
        dt.month() as u8,
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    );

    // Add a mixed Duration to the PrimitiveDateTime.
    let dt_later = dt + Duration::minutes(90);
    println!(
        "datetime + 90m: {}-{:02}-{:02} {:02}:{:02}:{:02}",
        dt_later.year(),
        dt_later.month() as u8,
        dt_later.day(),
        dt_later.hour(),
        dt_later.minute(),
        dt_later.second()
    );

    // Difference between two datetimes -> Duration.
    let diff: Duration = dt_later - dt;
    println!("diff minutes: {}", diff.whole_minutes());

    // Weekday / ordinal accessors (infallible).
    println!("weekday: {}", d.weekday());
    println!("ordinal: {}", d.ordinal());

    // Exercise the format machinery: format the Date with the well-known Rfc3339-ish
    // format for a *DateTime*. Use a format_description built at runtime, panic-safe.
    match time::format_description::parse(
        "[year]-[month]-[day]T[hour]:[minute]:[second]",
    ) {
        Ok(fmt) => match dt.format(&fmt) {
            Ok(s) => println!("formatted: {}", s),
            Err(e) => println!("format error: {:?}", e),
        },
        Err(e) => println!("parse fmt error: {:?}", e),
    }

    println!("== soak_time done ==");
}
