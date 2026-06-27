#![feature(time_systemtime_limits)]
use std::time::{Duration, SystemTime};
fn main() {
    let epoch = SystemTime::UNIX_EPOCH;
    // pre-epoch must be representable now
    println!("EPOCH - 1s is_some       = {} (want true)", epoch.checked_sub(Duration::new(1,0)).is_some());
    // the exact failing test (system_time_duration_since_max_range_on_unix):
    let min = epoch - Duration::new(i64::MAX as u64 + 1, 0);
    let max = epoch + Duration::new(i64::MAX as u64, 999_999_999);
    println!("min == SystemTime::MIN    = {} (want true)", min == SystemTime::MIN);
    println!("max == SystemTime::MAX    = {} (want true)", max == SystemTime::MAX);
    let delta_a = max.duration_since(min).expect("dsince");
    println!("MAX-MIN == Duration::MAX  = {} (want true)", delta_a == Duration::MAX);
    let delta_b = min.duration_since(max).expect_err("dsince err").duration();
    println!("MIN-MAX err dur==Dur::MAX = {} (want true)", delta_b == Duration::MAX);
    // sanity: now() still sane + round-trips
    let now = SystemTime::now();
    println!("now-EPOCH secs            = {:?} (want ~1.78e9)", now.duration_since(epoch).map(|d| d.as_secs()));
    println!("(now-1h)+1h == now        = {} (want true)", (now - Duration::new(3600,0)) + Duration::new(3600,0) == now);
    println!("done");
}
