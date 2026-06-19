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

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct SystemTime(Duration);

pub const UNIX_EPOCH: SystemTime = SystemTime(Duration::from_secs(0));

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
    pub const MAX: SystemTime = SystemTime(Duration::MAX);

    pub const MIN: SystemTime = SystemTime(Duration::ZERO);

    pub fn now() -> SystemTime {
        // SAFETY: as above — a single argumentless BCL property read.
        let dotnet_ticks = unsafe { rcl_dotnet_unix_ticks() };
        // Rebase onto the Unix epoch. Wall-clock time on any sane system is well
        // after 1970, so the difference is non-negative; clamp the impossible
        // pre-epoch case to UNIX_EPOCH rather than wrapping.
        let since_epoch = dotnet_ticks.saturating_sub(DOTNET_TICKS_AT_UNIX_EPOCH).max(0) as u64;
        let secs = since_epoch / 10_000_000;
        let sub_ticks = since_epoch % 10_000_000;
        let sub_nanos = (sub_ticks * NANOS_PER_DOTNET_TICK) as u32;
        SystemTime(Duration::new(secs, sub_nanos))
    }

    pub fn sub_time(&self, other: &SystemTime) -> Result<Duration, Duration> {
        self.0.checked_sub(other.0).ok_or_else(|| other.0 - self.0)
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_add(*other)?))
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_sub(*other)?))
    }
}
