//! An idiomatic Rust wrapper over the .NET value type `System.DateTime`
//! (assembly `System.Private.CoreLib`) — an instant in time on the proleptic Gregorian calendar.
//!
//! `DateTime` is a managed **value type** (a `struct` wrapping a single 64-bit field that packs the
//! tick count and `DateTimeKind`), so a [`DateTime`] here is stored inline, is `Copy`, and never
//! touches the GC heap. It maps the most-used members of the BCL type onto Rust names:
//!
//! * **Constructors** → associated fns: [`new`](DateTime::new) (`new DateTime(y, m, d)`),
//!   [`new_time`](DateTime::new_time), [`parse`](DateTime::parse) /
//!   [`parse_str`](DateTime::parse_str) (`DateTime.Parse`), and the clock fns
//!   [`now`](DateTime::now) / [`utc_now`](DateTime::utc_now) / [`today`](DateTime::today).
//! * **Component properties** → getters: [`year`](DateTime::year), [`month`](DateTime::month),
//!   [`day`](DateTime::day), [`hour`](DateTime::hour), [`minute`](DateTime::minute),
//!   [`second`](DateTime::second), [`day_of_year`](DateTime::day_of_year),
//!   [`ticks`](DateTime::ticks), and [`date`](DateTime::date).
//! * **Calendar arithmetic** → [`add_days`](DateTime::add_days), [`add_hours`](DateTime::add_hours),
//!   [`add_minutes`](DateTime::add_minutes), [`add_seconds`](DateTime::add_seconds),
//!   [`add_years`](DateTime::add_years), [`add_months`](DateTime::add_months).
//! * **Std traits** → [`Display`](core::fmt::Display)/[`Debug`](core::fmt::Debug) (via `ToString`),
//!   [`PartialEq`]/[`Eq`] and [`PartialOrd`]/[`Ord`] (via `DateTime.CompareTo`).
//!
//! ```ignore
//! use mycorrhiza::bcl::datetime::DateTime;
//!
//! let ymd  = DateTime::new(2026, 6, 30);    // new DateTime(2026, 6, 30)
//! let next = ymd.add_days(1.0);             // ymd.AddDays(1)
//! assert_eq!(next.day(), 1);                // rolls into July
//! assert!(next > ymd);
//! println!("{}", DateTime::now());          // ToString()
//! ```
//!
//! This is a thin, honest mapping: every method delegates straight to the corresponding managed
//! member, with no added behaviour. The large formatting/parsing/`DateTimeOffset` surface is out of
//! scope — reach for the raw handle via [`DateTime::handle`] for anything not surfaced here.

use crate::intrinsics::RustcCLRInteropManagedStruct;
use crate::system::MString;

// `System.DateTime` physically lives in `System.Private.CoreLib` (it is only type-*forwarded* from
// `System.Runtime`), so — like `System.String`/`System.TimeSpan` — method/ctor refs must name the
// defining assembly, or the JIT rejects the emitted IL once a real CoreLib `DateTime` flows through
// it.
const CORELIB: &str = "System.Private.CoreLib";
const DATETIME: &str = "System.DateTime";

/// The size (in bytes) of a managed `System.DateTime`: a single 64-bit field packing the tick count
/// and `DateTimeKind` (`sizeof(DateTime) == 8`).
const DATETIME_SIZE: usize = core::mem::size_of::<i64>();

/// The raw managed-value-type handle for `System.DateTime`.
type Handle = RustcCLRInteropManagedStruct<{ CORELIB }, { DATETIME }, DATETIME_SIZE>;

/// A managed `System.DateTime` — an instant in time, stored inline as a value type (`Copy`, no GC).
///
/// See the [module docs](self) for the full member map.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct DateTime(Handle);

impl DateTime {
    // --- constructors -----------------------------------------------------------------------------

    /// `new DateTime(year, month, day)` — midnight (00:00:00) on the given calendar date.
    ///
    /// The components are 1-based as in .NET (`month` 1..=12, `day` 1..=days-in-month).
    ///
    /// The `.ctor(int, int, int)` cannot be used directly here: a value-type `newobj` yields the
    /// struct value, but the ctor magic fn is *declared* returning the managed-*class* handle, and
    /// the CIL type-verifier rightly refuses to reinterpret that reference into the value-type
    /// handle. Instead this formats the date as an invariant ISO-8601 string and hands it to the
    /// static `DateTime.Parse`, which returns a `DateTime` *value* directly. The result is the exact
    /// same instant; an out-of-range component throws `FormatException` in managed code.
    #[inline(always)]
    pub fn new(year: i32, month: i32, day: i32) -> Self {
        Self::parse_str(&std::format!("{year:04}-{month:02}-{day:02}T00:00:00"))
    }

    /// `DateTime.Parse(text)` — parse an invariant date/time string, returning a `DateTime` *value*.
    /// A malformed string throws `FormatException` in managed code.
    #[inline(always)]
    pub fn parse(text: MString) -> Self {
        DateTime(Handle::vt_static1::<"Parse", MString, Handle>(text))
    }

    /// Convenience [`parse`](DateTime::parse) taking a Rust `&str`.
    #[inline(always)]
    pub fn parse_str(text: &str) -> Self {
        Self::parse(MString::from(text))
    }

    /// `new DateTime(year, month, day, hour, minute, second)`.
    #[inline(always)]
    pub fn new_time(year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32) -> Self {
        Self::new(year, month, day)
            .add_hours(hour as f64)
            .add_minutes(minute as f64)
            .add_seconds(second as f64)
    }

    /// `DateTime.Now` — the current local date and time.
    #[inline(always)]
    pub fn now() -> Self {
        DateTime(Self::static_get::<"get_Now">())
    }

    /// `DateTime.UtcNow` — the current UTC date and time.
    #[inline(always)]
    pub fn utc_now() -> Self {
        DateTime(Self::static_get::<"get_UtcNow">())
    }

    /// `DateTime.Today` — the current local date with the time set to midnight.
    #[inline(always)]
    pub fn today() -> Self {
        DateTime(Self::static_get::<"get_Today">())
    }

    // --- component getters ------------------------------------------------------------------------

    /// The year component (`DateTime.Year`).
    #[inline(always)]
    pub fn year(self) -> i32 {
        self.0.vt_instance0::<"get_Year", i32>()
    }
    /// The month component, 1..=12 (`DateTime.Month`).
    #[inline(always)]
    pub fn month(self) -> i32 {
        self.0.vt_instance0::<"get_Month", i32>()
    }
    /// The day-of-month component, 1..=31 (`DateTime.Day`).
    #[inline(always)]
    pub fn day(self) -> i32 {
        self.0.vt_instance0::<"get_Day", i32>()
    }
    /// The hour component, 0..=23 (`DateTime.Hour`).
    #[inline(always)]
    pub fn hour(self) -> i32 {
        self.0.vt_instance0::<"get_Hour", i32>()
    }
    /// The minute component, 0..=59 (`DateTime.Minute`).
    #[inline(always)]
    pub fn minute(self) -> i32 {
        self.0.vt_instance0::<"get_Minute", i32>()
    }
    /// The second component, 0..=59 (`DateTime.Second`).
    #[inline(always)]
    pub fn second(self) -> i32 {
        self.0.vt_instance0::<"get_Second", i32>()
    }
    /// The day of the year, 1..=366 (`DateTime.DayOfYear`).
    #[inline(always)]
    pub fn day_of_year(self) -> i32 {
        self.0.vt_instance0::<"get_DayOfYear", i32>()
    }
    /// The number of 100-nanosecond ticks representing this instant (`DateTime.Ticks`).
    #[inline(always)]
    pub fn ticks(self) -> i64 {
        self.0.vt_instance0::<"get_Ticks", i64>()
    }
    /// The date component with the time set to midnight (`DateTime.Date`).
    #[inline(always)]
    pub fn date(self) -> Self {
        DateTime(self.0.vt_instance0::<"get_Date", Handle>())
    }

    // --- calendar arithmetic ----------------------------------------------------------------------

    /// A new `DateTime` this many (fractional) days later (`DateTime.AddDays`).
    #[inline(always)]
    pub fn add_days(self, days: f64) -> Self {
        DateTime(self.add_f64::<"AddDays">(days))
    }
    /// A new `DateTime` this many (fractional) hours later (`DateTime.AddHours`).
    #[inline(always)]
    pub fn add_hours(self, hours: f64) -> Self {
        DateTime(self.add_f64::<"AddHours">(hours))
    }
    /// A new `DateTime` this many (fractional) minutes later (`DateTime.AddMinutes`).
    #[inline(always)]
    pub fn add_minutes(self, minutes: f64) -> Self {
        DateTime(self.add_f64::<"AddMinutes">(minutes))
    }
    /// A new `DateTime` this many (fractional) seconds later (`DateTime.AddSeconds`).
    #[inline(always)]
    pub fn add_seconds(self, seconds: f64) -> Self {
        DateTime(self.add_f64::<"AddSeconds">(seconds))
    }
    /// A new `DateTime` this many whole years later (`DateTime.AddYears`).
    #[inline(always)]
    pub fn add_years(self, years: i32) -> Self {
        DateTime(self.add_i32::<"AddYears">(years))
    }
    /// A new `DateTime` this many whole months later (`DateTime.AddMonths`).
    #[inline(always)]
    pub fn add_months(self, months: i32) -> Self {
        DateTime(self.add_i32::<"AddMonths">(months))
    }

    // --- comparison -------------------------------------------------------------------------------

    /// Value equality (`DateTime.Equals(DateTime)`) — two instants are equal iff they have the same
    /// tick count, matching what a Rust user means by `==` on a timestamp.
    #[inline(always)]
    pub fn equals(self, other: Self) -> bool {
        // A value-type instance method taking a `DateTime` argument: `call instance` on the
        // `valuetype` receiver (`vt_instance1`), argument by value.
        self.0.vt_instance1::<"Equals", Handle, bool>(other.0)
    }
    /// Chronological comparison (`DateTime.CompareTo`): negative if `self` is earlier than `other`,
    /// zero if equal, positive if later.
    #[inline(always)]
    pub fn compare_to(self, other: Self) -> i32 {
        self.0.vt_instance1::<"CompareTo", Handle, i32>(other.0)
    }

    // --- interop escape hatch ---------------------------------------------------------------------

    /// The raw managed value-type handle, for lower-level BCL calls not surfaced here.
    #[inline(always)]
    pub fn handle(self) -> Handle {
        self.0
    }
    /// Wrap a raw `System.DateTime` value handle (e.g. one returned by another BCL call).
    #[inline(always)]
    pub fn from_raw(raw: Handle) -> Self {
        DateTime(raw)
    }

    // --- private helpers --------------------------------------------------------------------------

    /// Read a static `DateTime`-returning property (`get_Now` / `get_UtcNow` / `get_Today` /
    /// `get_MinValue`) — a zero-arg static `call` on the `valuetype`, yielding the struct value.
    #[inline(always)]
    fn static_get<const METHOD: &'static str>() -> Handle {
        Handle::vt_static0::<METHOD, Handle>()
    }

    /// Shared body for the `Add*(double)` family — a single-`f64`-arg value-type instance call
    /// (`call instance`, receiver by reference) returning a fresh `DateTime` value.
    #[inline(always)]
    fn add_f64<const METHOD: &'static str>(self, arg: f64) -> Handle {
        self.0.vt_instance1::<METHOD, f64, Handle>(arg)
    }

    /// Shared body for the `Add*(int)` family (`AddYears`/`AddMonths`) — a single-`i32`-arg value-type
    /// instance call returning a fresh `DateTime` value.
    #[inline(always)]
    fn add_i32<const METHOD: &'static str>(self, arg: i32) -> Handle {
        self.0.vt_instance1::<METHOD, i32, Handle>(arg)
    }
}

impl core::fmt::Display for DateTime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Delegate to the managed `ToString()` (the invariant round-trip form) and print its UTF-16
        // content through the idiomatic string wrapper, which decodes to Rust text.
        let s =
            crate::system::DotNetString::from_handle(self.0.vt_instance0::<"ToString", MString>());
        core::fmt::Display::fmt(&s, f)
    }
}

impl core::fmt::Debug for DateTime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl PartialEq for DateTime {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.equals(*other)
    }
}
impl Eq for DateTime {}

impl PartialOrd for DateTime {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DateTime {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // `CompareTo` already returns the total-order sign, so map it straight onto `Ordering`.
        self.compare_to(*other).cmp(&0)
    }
}
