// survey_jiff: exercise jiff's civil::DateTime / Timestamp built from FIXED parts.
// No wall-clock read (no Zoned::now / Timestamp::now). Everything is deterministic.

use jiff::civil::{date, DateTime};
use jiff::{Timestamp, ToSpan, Unit};
use std::str::FromStr;

fn main() {
    // --- Build a FIXED civil DateTime from parts (year-month-day hour:min:sec). ---
    // 2024-02-29 (leap day) 13:45:30. Construction is fallible -> match, no unwrap.
    match DateTime::new(2024, 2, 29, 13, 45, 30, 0) {
        Ok(dt) => exercise(dt),
        Err(_) => println!("datetime_new = error"),
    }

    // --- Build a FIXED Timestamp from a known number of seconds since the epoch. ---
    // 1_700_000_000 s = 2023-11-14T22:13:20Z. Fallible -> match.
    match Timestamp::from_second(1_700_000_000) {
        Ok(ts) => {
            println!("ts_seconds = {}", ts.as_second());
            println!("ts_string = {}", ts);
            // Arithmetic on a Timestamp: add 36 hours, print the new instant.
            match ts.checked_add(36.hours()) {
                Ok(ts2) => println!("ts_plus_36h = {}", ts2),
                Err(_) => println!("ts_plus_36h = error"),
            }
            // Round a timestamp down to the nearest hour (deterministic).
            match ts.round(Unit::Hour) {
                Ok(tr) => println!("ts_round_hour = {}", tr),
                Err(_) => println!("ts_round_hour = error"),
            }
        }
        Err(_) => println!("ts_from_second = error"),
    }

    // --- Parse a Timestamp back from a fixed RFC 3339 string (round-trip). ---
    match Timestamp::from_str("2023-11-14T22:13:20Z") {
        Ok(ts) => println!("ts_parsed_seconds = {}", ts.as_second()),
        Err(_) => println!("ts_parsed = error"),
    }

    println!("== survey_jiff done ==");
}

fn exercise(dt: DateTime) {
    // Format to string (Display is RFC-9557-ish civil form).
    println!("dt_string = {}", dt);
    println!("dt_year = {}", dt.year());
    println!("dt_month = {}", dt.month());
    println!("dt_day = {}", dt.day());
    println!("dt_hour = {}", dt.hour());

    // --- Arithmetic: add days/hours. checked_add is fallible -> match. ---
    match dt.checked_add(3.days().hours(5)) {
        Ok(dt2) => {
            println!("dt_plus_3d5h = {}", dt2);
            // Difference back to the original, in hours (deterministic integer).
            match dt2.until((Unit::Hour, dt)) {
                Ok(span) => println!("dt_diff_hours = {}", span.get_hours()),
                Err(_) => println!("dt_diff_hours = error"),
            }
        }
        Err(_) => println!("dt_plus_3d5h = error"),
    }

    // Subtract one day (crosses the leap day back to a normal date).
    match dt.checked_sub(1.day()) {
        Ok(dt3) => println!("dt_minus_1d = {}", dt3),
        Err(_) => println!("dt_minus_1d = error"),
    }

    // --- Parse a civil DateTime back from a fixed string (round-trip). ---
    match DateTime::from_str("2024-02-29T13:45:30") {
        Ok(dt4) => println!("dt_parsed_eq = {}", dt4 == dt),
        Err(_) => println!("dt_parsed = error"),
    }

    // Derive a value from a known calendar date and compare (weekday as int).
    let d = date(2024, 2, 29);
    println!("weekday = {}", d.weekday() as i8);
    // Days since a fixed earlier date (deterministic count).
    let earlier = date(2024, 1, 1);
    match d.until((Unit::Day, earlier)) {
        Ok(span) => println!("days_since_jan1 = {}", span.get_days()),
        Err(_) => println!("days_since_jan1 = error"),
    }
}
