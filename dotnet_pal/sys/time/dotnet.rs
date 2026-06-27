//! Clocks for the .NET ("dotnet") platform.
//!
//! Backs `std::time::Instant` and `std::time::SystemTime` with the .NET BCL
//! through three `extern "C"` hooks that the cilly linker maps to BCL calls
//! (see `cilly/src/ir/builtins/dotnet.rs`):
//!
//! * `rcl_dotnet_instant_ticks() -> i64` => `System.Diagnostics.Stopwatch.GetTimestamp()`
//!   — a monotonic, high-resolution tick counter (the platform's QPC-style
//!   timer). Its absolute zero is arbitrary; only differences are meaningful,
//!   which is exactly the `Instant` contract.
//! * `rcl_dotnet_instant_freq() -> i64` => `System.Diagnostics.Stopwatch.Frequency`
//!   — ticks per second for the counter above. Read once and cached.
//! * `rcl_dotnet_unix_ticks() -> i64` => `System.DateTime.UtcNow.Ticks`
//!   — wall-clock time as 100-ns intervals since `0001-01-01T00:00:00Z`. We
//!   rebase it onto the Unix epoch in Rust (subtracting a constant) so the
//!   binding stays a single property read with no static-field load.
//!
//! The tick->`Duration` conversion is done in Rust with 128-bit intermediates
//! to avoid overflow, so the BCL side only ever hands back raw `i64` counters.
//! `Instant`/`SystemTime` themselves are stored as a `Duration`, mirroring the
//! shared `unsupported` arm; only `now()` differs.
#![forbid(unsafe_op_in_unsafe_fn)]

use crate::time::Duration;

// FIXED extern contract — the names must match EXACTLY on the linker side, where
// they are mapped to the .NET BCL (`System.Diagnostics.Stopwatch`, `System.DateTime`).
unsafe extern "C" {
    /// `Stopwatch.GetTimestamp()`: monotonic high-resolution tick count.
    fn rcl_dotnet_instant_ticks() -> i64;
    /// `Stopwatch.Frequency`: ticks per second of the counter above.
    fn rcl_dotnet_instant_freq() -> i64;
    /// `DateTime.UtcNow.Ticks`: 100-ns intervals since `0001-01-01T00:00:00Z`.
    fn rcl_dotnet_unix_ticks() -> i64;
}

/// .NET `DateTime` ticks are 100-ns intervals; there are 10_000_000 per second.
const NANOS_PER_DOTNET_TICK: u64 = 100;
/// Ticks (100 ns) between `0001-01-01` (DateTime zero) and `1970-01-01` (Unix
/// epoch). `621_355_968_000_000_000` = 62_135_596_800 s * 10_000_000.
const DOTNET_TICKS_AT_UNIX_EPOCH: i64 = 621_355_968_000_000_000;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Instant(Duration);

/// Wall-clock time as a SIGNED offset from the Unix epoch, mirroring the unix
/// `Timespec` (`secs: i64`, `nanos` always normalized into `0..1_000_000_000`).
///
/// The previous representation was a plain `Duration`, which is UNSIGNED: it could
/// not represent any time before 1970, and made `SystemTime::{MIN, MAX}` wrong
/// (`MIN` was the epoch, `MAX` was `u64::MAX` seconds) — so `UNIX_EPOCH - 1s`
/// returned `None`, file timestamps before the epoch were unrepresentable, and the
/// coretests/std `system_time_duration_since_max_range_on_unix` regression test
/// (which the `cfg(unix)` target runs) aborted. Because `nanos` stays normalized,
/// the derived `Ord` compares `secs` then `nanos`, i.e. chronological order.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct SystemTime {
    secs: i64,
    nanos: u32,
}

pub const UNIX_EPOCH: SystemTime = SystemTime { secs: 0, nanos: 0 };

/// Convert a raw monotonic tick count + frequency into a `Duration` since the
/// counter's (arbitrary) zero, using 128-bit math so the multiply by 1e9 cannot
/// overflow for any realistic uptime.
fn ticks_to_duration(ticks: i64, freq: i64) -> Duration {
    // `freq` is the platform timer frequency; it is always positive. Guard the
    // pathological zero just in case so we never divide by zero.
    let freq = if freq <= 0 { 1 } else { freq as u128 };
    let ticks = ticks.max(0) as u128;
    let nanos = ticks.wrapping_mul(1_000_000_000) / freq;
    let secs = (nanos / 1_000_000_000) as u64;
    let sub_nanos = (nanos % 1_000_000_000) as u32;
    Duration::new(secs, sub_nanos)
}

impl Instant {
    pub fn now() -> Instant {
        // SAFETY: the hooks take no arguments and return a plain `i64`; the
        // linker maps them to side-effect-free BCL property reads.
        let ticks = unsafe { rcl_dotnet_instant_ticks() };
        let freq = unsafe { rcl_dotnet_instant_freq() };
        Instant(ticks_to_duration(ticks, freq))
    }

    pub fn checked_sub_instant(&self, other: &Instant) -> Option<Duration> {
        self.0.checked_sub(other.0)
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_add(*other)?))
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_sub(*other)?))
    }
}

impl SystemTime {
    pub const MAX: SystemTime = SystemTime { secs: i64::MAX, nanos: 999_999_999 };

    pub const MIN: SystemTime = SystemTime { secs: i64::MIN, nanos: 0 };

    pub fn now() -> SystemTime {
        // SAFETY: as above — a single argumentless BCL property read.
        let dotnet_ticks = unsafe { rcl_dotnet_unix_ticks() };
        // Rebase onto the Unix epoch. Wall-clock time on any sane system is well
        // after 1970, so the offset is non-negative and fits in i64 seconds; clamp
        // the impossible pre-epoch case to UNIX_EPOCH rather than wrapping.
        let since_epoch = dotnet_ticks.saturating_sub(DOTNET_TICKS_AT_UNIX_EPOCH).max(0) as u64;
        let secs = (since_epoch / 10_000_000) as i64;
        let sub_ticks = since_epoch % 10_000_000;
        let nanos = (sub_ticks * NANOS_PER_DOTNET_TICK) as u32;
        SystemTime { secs, nanos }
    }

    pub fn sub_time(&self, other: &SystemTime) -> Result<Duration, Duration> {
        if *self >= *other {
            // `self - other` as a non-negative `Duration`. The seconds difference is
            // computed with wrapping arithmetic (the unix arm relies on the same
            // modular semantics) so the extreme `MAX - MIN == Duration::MAX` case
            // does not overflow-panic.
            let (secs, nanos) = if self.nanos >= other.nanos {
                (self.secs.wrapping_sub(other.secs) as u64, self.nanos - other.nanos)
            } else {
                (
                    self.secs.wrapping_sub(other.secs).wrapping_sub(1) as u64,
                    self.nanos + 1_000_000_000 - other.nanos,
                )
            };
            Ok(Duration::new(secs, nanos))
        } else {
            match other.sub_time(self) {
                Ok(d) => Err(d),
                Err(d) => Ok(d),
            }
        }
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<SystemTime> {
        let mut secs = self.secs.checked_add_unsigned(other.as_secs())?;
        // Each operand's nanos are < 1e9, so the sum fits in a u32 and carries at
        // most one second.
        let mut nanos = self.nanos + other.subsec_nanos();
        if nanos >= 1_000_000_000 {
            nanos -= 1_000_000_000;
            secs = secs.checked_add(1)?;
        }
        Some(SystemTime { secs, nanos })
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<SystemTime> {
        let mut secs = self.secs.checked_sub_unsigned(other.as_secs())?;
        let mut nanos = self.nanos as i32 - other.subsec_nanos() as i32;
        if nanos < 0 {
            nanos += 1_000_000_000;
            secs = secs.checked_sub(1)?;
        }
        Some(SystemTime { secs, nanos: nanos as u32 })
    }
}
