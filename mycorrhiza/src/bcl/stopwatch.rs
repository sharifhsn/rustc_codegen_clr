//! An idiomatic Rust wrapper over [`System.Diagnostics.Stopwatch`] (assembly `System.Private.CoreLib`)
//! — a high-resolution measurer of elapsed time, backed by a real managed object on the CLR heap.
//!
//! This is a thin, honest handle to a managed `Stopwatch`: every method delegates straight to the
//! corresponding .NET member, so the behaviour matches what the equivalent C# would do. No extra
//! Rust-side timing is invented.
//!
//! ```ignore
//! use mycorrhiza::bcl::stopwatch::Stopwatch;
//!
//! let sw = Stopwatch::start_new();        // like `Stopwatch.StartNew()` — already running
//! // … do work …
//! sw.stop();
//! println!("elapsed: {} ms", sw.elapsed_millis());
//! let d: std::time::Duration = sw.elapsed();   // millisecond resolution — see the note below
//! ```
//!
//! ## Mapping to the .NET surface
//!
//! | Rust                              | .NET member                                  |
//! |-----------------------------------|----------------------------------------------|
//! | [`Stopwatch::new`]                | `new Stopwatch()` (created stopped)          |
//! | [`Stopwatch::start_new`]          | `static Stopwatch.StartNew()` (running)      |
//! | [`Stopwatch::start`]              | `void Start()`                               |
//! | [`Stopwatch::stop`]               | `void Stop()`                                |
//! | [`Stopwatch::reset`]              | `void Reset()`                               |
//! | [`Stopwatch::restart`]            | `void Restart()`                             |
//! | [`Stopwatch::is_running`]         | `bool IsRunning { get; }`                    |
//! | [`Stopwatch::elapsed_millis`]     | `long ElapsedMilliseconds { get; }`          |
//! | [`Stopwatch::elapsed_ticks`]      | `long ElapsedTicks { get; }`                 |
//! | [`Stopwatch::elapsed`]            | derived from `ElapsedMilliseconds`           |
//! | [`Stopwatch::get_timestamp`]      | `static long GetTimestamp()`                 |
//! | [`Display`](core::fmt::Display)   | `object.ToString()` (the elapsed `TimeSpan`) |
//!
//! **On [`elapsed_ticks`](Stopwatch::elapsed_ticks).** Those are *Stopwatch* ticks, whose length is
//! platform-frequency-dependent — they are **not** the 100-nanosecond `TimeSpan`/`DateTime` ticks.
//! The managed `Stopwatch.Frequency` static field is not part of the idiomatic binding surface, so
//! this wrapper cannot convert raw ticks to a wall-clock duration itself. Use
//! [`elapsed_millis`](Stopwatch::elapsed_millis) or [`elapsed`](Stopwatch::elapsed) for a portable
//! duration; reach for `elapsed_ticks` only when you also know the platform frequency.

use crate::system::MString;

/// The raw managed-handle alias for `System.Diagnostics.Stopwatch` (impl assembly
/// `System.Private.CoreLib` — a reference assembly forwards the type and throws `TypeLoadException`
/// at JIT, so method-body refs must name the impl assembly). A [`Stopwatch`] wraps one of these;
/// [`Stopwatch::handle`] hands it back for lower-level BCL calls.
pub type MStopwatch = crate::intrinsics::RustcCLRInteropManagedClass<
    "System.Private.CoreLib",
    "System.Diagnostics.Stopwatch",
>;

/// A managed `System.Diagnostics.Stopwatch`. See the [module docs](self).
///
/// A plain handle to a managed stopwatch (the .NET GC owns the object, so there is no `Drop`).
/// Construct it stopped with [`Stopwatch::new`] or already-running with [`Stopwatch::start_new`],
/// then query the elapsed time as it runs or after [`stop`](Stopwatch::stop). The mutating methods
/// take `&self` because they mutate the *managed* object, not the Rust handle.
#[derive(Clone, Copy)]
pub struct Stopwatch(MStopwatch);

impl Stopwatch {
    /// `new Stopwatch()` — a fresh, **stopped**, zeroed stopwatch. Call [`start`](Stopwatch::start)
    /// to begin measuring.
    #[inline(always)]
    pub fn new() -> Self {
        Stopwatch(MStopwatch::ctor0())
    }

    /// `Stopwatch.StartNew()` — a fresh stopwatch that is **already running** (the idiomatic way to
    /// begin a measurement).
    #[inline(always)]
    pub fn start_new() -> Self {
        Stopwatch(MStopwatch::static0::<"StartNew", MStopwatch>())
    }

    /// The current high-resolution timestamp counter value (`Stopwatch.GetTimestamp()`) — a raw tick
    /// count for manual interval measurement (subtract two readings and scale by the platform
    /// frequency).
    #[inline(always)]
    pub fn get_timestamp() -> i64 {
        MStopwatch::static0::<"GetTimestamp", i64>()
    }

    /// Wrap an existing managed `Stopwatch` handle (e.g. one returned by another BCL call).
    #[inline(always)]
    pub fn from_handle(h: MStopwatch) -> Self {
        Stopwatch(h)
    }

    /// The underlying managed handle, for lower-level BCL calls.
    #[inline(always)]
    pub fn handle(self) -> MStopwatch {
        self.0
    }

    /// Start (or resume) measuring elapsed time (`Stopwatch.Start`). A no-op if already running.
    #[inline(always)]
    pub fn start(&self) {
        self.0.instance0::<"Start", ()>()
    }

    /// Stop measuring elapsed time (`Stopwatch.Stop`). The accumulated elapsed time is retained, so a
    /// later [`start`](Stopwatch::start) resumes from where it left off.
    #[inline(always)]
    pub fn stop(&self) {
        self.0.instance0::<"Stop", ()>()
    }

    /// Stop and zero the elapsed time (`Stopwatch.Reset`).
    #[inline(always)]
    pub fn reset(&self) {
        self.0.instance0::<"Reset", ()>()
    }

    /// Zero the elapsed time and start measuring again from zero (`Stopwatch.Restart`).
    #[inline(always)]
    pub fn restart(&self) {
        self.0.instance0::<"Restart", ()>()
    }

    /// Whether the stopwatch is currently running (`Stopwatch.IsRunning`).
    #[inline(always)]
    pub fn is_running(&self) -> bool {
        self.0.instance0::<"get_IsRunning", bool>()
    }

    /// Total elapsed time in whole milliseconds (`Stopwatch.ElapsedMilliseconds`).
    #[inline(always)]
    pub fn elapsed_millis(&self) -> i64 {
        self.0.instance0::<"get_ElapsedMilliseconds", i64>()
    }

    /// Total elapsed time in raw **Stopwatch** ticks (`Stopwatch.ElapsedTicks`). These are *not*
    /// 100-nanosecond ticks — their length depends on the platform frequency (see the module docs).
    #[inline(always)]
    pub fn elapsed_ticks(&self) -> i64 {
        self.0.instance0::<"get_ElapsedTicks", i64>()
    }

    /// Total elapsed time as a [`std::time::Duration`], at **millisecond** resolution (derived from
    /// [`elapsed_millis`](Stopwatch::elapsed_millis)). For sub-millisecond precision you need the raw
    /// [`elapsed_ticks`](Stopwatch::elapsed_ticks) and the platform frequency.
    #[inline(always)]
    pub fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.elapsed_millis().max(0) as u64)
    }

    /// The managed `ToString()` of the underlying object, as an idiomatic Rust [`String`] (the
    /// elapsed `TimeSpan`'s textual form, e.g. `"00:00:01.2340000"`).
    #[inline(always)]
    pub fn to_rust_string(self) -> std::string::String {
        crate::system::DotNetString::from_handle(self.0.to_mstring()).to_rust_string()
    }
}

impl Default for Stopwatch {
    /// A fresh, stopped stopwatch (same as [`Stopwatch::new`]).
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Display for Stopwatch {
    /// Formats via the managed `Stopwatch.ToString()` (the elapsed `TimeSpan`'s textual form).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s: MString = self.0.to_mstring();
        core::fmt::Display::fmt(&crate::system::DotNetString::from_handle(s), f)
    }
}
