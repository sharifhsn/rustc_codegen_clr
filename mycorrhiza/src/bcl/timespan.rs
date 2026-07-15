//! An idiomatic Rust wrapper over the .NET value type `System.TimeSpan`
//! (assembly `System.Private.CoreLib`) — a time interval measured in 100-nanosecond *ticks*.
//!
//! `TimeSpan` is a managed **value type** (a `struct` wrapping a single `long _ticks`), so a
//! [`DotNetTimeSpan`] is stored inline, is `Copy`, and never touches the GC heap. It maps the
//! most-used members of the BCL type onto Rust names:
//!
//! * **Constructors** → associated fns: [`from_ticks`](DotNetTimeSpan::from_ticks),
//!   [`from_days`](DotNetTimeSpan::from_days), [`from_hours`](DotNetTimeSpan::from_hours),
//!   [`from_minutes`](DotNetTimeSpan::from_minutes), [`from_seconds`](DotNetTimeSpan::from_seconds),
//!   [`from_milliseconds`](DotNetTimeSpan::from_milliseconds), and [`zero`](DotNetTimeSpan::zero).
//! * **Component properties** → getters: [`ticks`](DotNetTimeSpan::ticks),
//!   [`days`](DotNetTimeSpan::days), [`hours`](DotNetTimeSpan::hours),
//!   [`minutes`](DotNetTimeSpan::minutes), [`seconds`](DotNetTimeSpan::seconds),
//!   [`milliseconds`](DotNetTimeSpan::milliseconds), and the `total_*` fractional accessors.
//! * **Arithmetic** → [`add`](DotNetTimeSpan::add), [`subtract`](DotNetTimeSpan::subtract),
//!   [`negate`](DotNetTimeSpan::negate), [`duration`](DotNetTimeSpan::duration).
//! * **Std traits** → [`Display`](core::fmt::Display)/[`Debug`](core::fmt::Debug) (via `ToString`),
//!   [`PartialEq`]/[`Eq`] and [`PartialOrd`]/[`Ord`] (via `TimeSpan.CompareTo`).
//!
//! ```ignore
//! use mycorrhiza::bcl::timespan::DotNetTimeSpan;
//!
//! let a = DotNetTimeSpan::from_minutes(1.5);
//! assert_eq!(a.total_seconds(), 90.0);
//! let b = a.add(DotNetTimeSpan::from_seconds(30.0));
//! assert_eq!(b.total_seconds(), 120.0);
//! assert!(b > a);
//! println!("{b}"); // "00:02:00"
//! ```
//!
//! This is a thin, honest mapping: every method delegates straight to the corresponding managed
//! member, with no added behaviour.

use crate::intrinsics::RustcCLRInteropManagedStruct;
use crate::system::MString;

// `System.TimeSpan` physically lives in `System.Private.CoreLib` (it is only type-*forwarded* from
// `System.Runtime`), so — like `System.String` — method/ctor refs must name the defining assembly,
// or the JIT rejects the emitted IL once a real CoreLib `TimeSpan` flows through it.
const CORELIB: &str = "System.Private.CoreLib";
const TIMESPAN: &str = "System.TimeSpan";

/// The size (in bytes) of a managed `System.TimeSpan`: a single `long _ticks` field.
const TIMESPAN_SIZE: usize = core::mem::size_of::<i64>();

/// The raw managed-value-type handle for `System.TimeSpan`.
type Handle = RustcCLRInteropManagedStruct<{ CORELIB }, { TIMESPAN }, TIMESPAN_SIZE>;

/// A managed `System.TimeSpan` — a time interval, stored inline as a value type (`Copy`, no GC).
///
/// See the [module docs](self) for the full member map.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct DotNetTimeSpan(Handle);

impl DotNetTimeSpan {
    // --- constructors (static factory methods -> a fresh TimeSpan value) --------------------------

    /// `TimeSpan.FromTicks(ticks)` — a `TimeSpan` of `ticks` 100-nanosecond ticks.
    #[inline(always)]
    pub fn from_ticks(ticks: i64) -> Self {
        DotNetTimeSpan(Handle::vt_static1::<"FromTicks", i64, Handle>(ticks))
    }
    /// `TimeSpan.FromDays(days)` — an interval of `days` days (fractional allowed).
    #[inline(always)]
    pub fn from_days(days: f64) -> Self {
        DotNetTimeSpan(Handle::vt_static1::<"FromDays", f64, Handle>(days))
    }
    /// `TimeSpan.FromHours(hours)` — an interval of `hours` hours (fractional allowed).
    #[inline(always)]
    pub fn from_hours(hours: f64) -> Self {
        DotNetTimeSpan(Handle::vt_static1::<"FromHours", f64, Handle>(hours))
    }
    /// `TimeSpan.FromMinutes(minutes)` — an interval of `minutes` minutes (fractional allowed).
    #[inline(always)]
    pub fn from_minutes(minutes: f64) -> Self {
        DotNetTimeSpan(Handle::vt_static1::<"FromMinutes", f64, Handle>(minutes))
    }
    /// `TimeSpan.FromSeconds(seconds)` — an interval of `seconds` seconds (fractional allowed).
    #[inline(always)]
    pub fn from_seconds(seconds: f64) -> Self {
        DotNetTimeSpan(Handle::vt_static1::<"FromSeconds", f64, Handle>(seconds))
    }
    /// `TimeSpan.FromMilliseconds(ms)` — an interval of `ms` milliseconds (fractional allowed).
    #[inline(always)]
    pub fn from_milliseconds(ms: f64) -> Self {
        DotNetTimeSpan(Handle::vt_static1::<"FromMilliseconds", f64, Handle>(ms))
    }
    /// The zero interval. (`TimeSpan.Zero` is a static readonly *field*, not a method, so the
    /// method-based interop machinery cannot read it; `FromTicks(0)` yields the identical value.)
    #[inline(always)]
    pub fn zero() -> Self {
        Self::from_ticks(0)
    }

    // --- whole-value component getters ------------------------------------------------------------

    /// `TimeSpan.Ticks` — the total number of 100-nanosecond ticks.
    #[inline(always)]
    pub fn ticks(self) -> i64 {
        self.0.vt_instance0::<"get_Ticks", i64>()
    }
    /// `TimeSpan.Days` — the whole-days component.
    #[inline(always)]
    pub fn days(self) -> i32 {
        self.0.vt_instance0::<"get_Days", i32>()
    }
    /// `TimeSpan.Hours` — the hours component (0..=23), not the total hours.
    #[inline(always)]
    pub fn hours(self) -> i32 {
        self.0.vt_instance0::<"get_Hours", i32>()
    }
    /// `TimeSpan.Minutes` — the minutes component (0..=59), not the total minutes.
    #[inline(always)]
    pub fn minutes(self) -> i32 {
        self.0.vt_instance0::<"get_Minutes", i32>()
    }
    /// `TimeSpan.Seconds` — the seconds component (0..=59), not the total seconds.
    #[inline(always)]
    pub fn seconds(self) -> i32 {
        self.0.vt_instance0::<"get_Seconds", i32>()
    }
    /// `TimeSpan.Milliseconds` — the milliseconds component (0..=999).
    #[inline(always)]
    pub fn milliseconds(self) -> i32 {
        self.0.vt_instance0::<"get_Milliseconds", i32>()
    }

    // --- fractional totals ------------------------------------------------------------------------

    /// `TimeSpan.TotalDays` — the whole interval expressed in fractional days.
    #[inline(always)]
    pub fn total_days(self) -> f64 {
        self.0.vt_instance0::<"get_TotalDays", f64>()
    }
    /// `TimeSpan.TotalHours` — the whole interval expressed in fractional hours.
    #[inline(always)]
    pub fn total_hours(self) -> f64 {
        self.0.vt_instance0::<"get_TotalHours", f64>()
    }
    /// `TimeSpan.TotalMinutes` — the whole interval expressed in fractional minutes.
    #[inline(always)]
    pub fn total_minutes(self) -> f64 {
        self.0.vt_instance0::<"get_TotalMinutes", f64>()
    }
    /// `TimeSpan.TotalSeconds` — the whole interval expressed in fractional seconds.
    #[inline(always)]
    pub fn total_seconds(self) -> f64 {
        self.0.vt_instance0::<"get_TotalSeconds", f64>()
    }
    /// `TimeSpan.TotalMilliseconds` — the whole interval expressed in fractional milliseconds.
    #[inline(always)]
    pub fn total_milliseconds(self) -> f64 {
        self.0.vt_instance0::<"get_TotalMilliseconds", f64>()
    }

    // --- arithmetic -------------------------------------------------------------------------------

    /// `TimeSpan.Add(other)` — the sum of the two intervals.
    #[inline(always)]
    pub fn add(self, other: Self) -> Self {
        DotNetTimeSpan(self.instance1_ts::<"Add">(other))
    }
    /// `TimeSpan.Subtract(other)` — this interval minus `other`.
    #[inline(always)]
    pub fn subtract(self, other: Self) -> Self {
        DotNetTimeSpan(self.instance1_ts::<"Subtract">(other))
    }
    /// `TimeSpan.Negate()` — the interval with its sign flipped.
    #[inline(always)]
    pub fn negate(self) -> Self {
        DotNetTimeSpan(self.0.vt_instance0::<"Negate", Handle>())
    }
    /// `TimeSpan.Duration()` — the absolute value of the interval (always non-negative).
    #[inline(always)]
    pub fn duration(self) -> Self {
        DotNetTimeSpan(self.0.vt_instance0::<"Duration", Handle>())
    }

    /// `TimeSpan.CompareTo(other)` — `-1`/`0`/`1` as this interval is shorter/equal/longer.
    #[inline(always)]
    pub fn compare_to(self, other: Self) -> i32 {
        // A value-type instance method taking a `TimeSpan` argument: `call instance` on the
        // `valuetype` receiver, argument by value.
        self.0.vt_instance1::<"CompareTo", Handle, i32>(other.0)
    }

    /// The raw managed value-type handle, for lower-level BCL calls.
    #[inline(always)]
    pub fn handle(self) -> Handle {
        self.0
    }

    /// Wrap a raw managed `System.TimeSpan` value returned by another BCL API.
    pub fn from_raw(handle: Handle) -> Self {
        Self(handle)
    }

    /// Shared spelling of a `TimeSpan`-in, `TimeSpan`-out value-type instance method
    /// (`Add`/`Subtract`): `call instance` on the `valuetype` receiver, argument by value.
    #[inline(always)]
    fn instance1_ts<const METHOD: &'static str>(self, other: Self) -> Handle {
        self.0.vt_instance1::<METHOD, Handle, Handle>(other.0)
    }
}

impl Default for DotNetTimeSpan {
    /// The zero interval (`TimeSpan.Zero`).
    #[inline(always)]
    fn default() -> Self {
        Self::zero()
    }
}

impl core::fmt::Display for DotNetTimeSpan {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `TimeSpan.ToString()` yields the invariant `[-][d.]hh:mm:ss[.fffffff]` form.
        let s =
            crate::system::DotNetString::from_handle(self.0.vt_instance0::<"ToString", MString>());
        core::fmt::Display::fmt(&s, f)
    }
}

impl core::fmt::Debug for DotNetTimeSpan {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl PartialEq for DotNetTimeSpan {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.compare_to(*other) == 0
    }
}
impl Eq for DotNetTimeSpan {}

impl PartialOrd for DotNetTimeSpan {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DotNetTimeSpan {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // `CompareTo` already returns the total-order sign, so map it straight onto `Ordering`.
        self.compare_to(*other).cmp(&0)
    }
}
