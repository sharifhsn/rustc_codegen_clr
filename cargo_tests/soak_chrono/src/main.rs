use chrono::{Duration, NaiveDate, NaiveDateTime, Utc};

fn main() {
    // Fixed, deterministic NaiveDate. Handle the Option without panicking.
    let date = match NaiveDate::from_ymd_opt(2026, 6, 20) {
        Some(d) => d,
        None => {
            println!("from_ymd_opt returned None (unexpected)");
            println!("== soak_chrono done ==");
            return;
        }
    };
    println!("date: {}", date.format("%Y-%m-%d"));

    // Add a Duration of days to the date.
    let later = date + Duration::days(40);
    println!("date + 40d: {}", later.format("%Y-%m-%d"));

    // Build a fixed NaiveDateTime from the date + a time, panic-safe.
    let dt: NaiveDateTime = match date.and_hms_opt(13, 30, 45) {
        Some(dt) => dt,
        None => {
            println!("and_hms_opt returned None (unexpected)");
            println!("== soak_chrono done ==");
            return;
        }
    };
    println!("datetime: {}", dt.format("%Y-%m-%d %H:%M:%S"));

    // Add a Duration (mixed days + minutes) to the datetime.
    let dt_later = dt + Duration::minutes(90);
    println!("datetime + 90m: {}", dt_later.format("%Y-%m-%d %H:%M:%S"));

    // Difference between two datetimes -> Duration.
    let diff = dt_later - dt;
    println!("diff minutes: {}", diff.num_minutes());

    // Weekday / ordinal accessors (no panics).
    use chrono::Datelike;
    println!("weekday: {}", date.weekday());
    println!("ordinal: {}", date.ordinal());

    // Optionally exercise PAL time via Utc::now (nondeterministic; only print success/shape).
    let now = Utc::now();
    let now_naive = now.naive_utc();
    // Only assert it's a plausible year so output stays deterministic-ish.
    if now_naive.date().year() >= 2020 {
        println!("Utc::now succeeded (year >= 2020)");
    } else {
        println!("Utc::now returned implausible year: {}", now_naive.date().year());
    }

    println!("== soak_chrono done ==");
}
