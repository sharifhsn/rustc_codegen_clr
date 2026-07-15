//! Idiomatic Rust wrapper over the managed `System.DateTimeOffset` value type.

use crate::bcl::datetime::DateTime;
use crate::bcl::timespan::DotNetTimeSpan;
use crate::intrinsics::RustcCLRInteropManagedStruct;
use crate::system::{DotNetString, MString};

const CORELIB: &str = "System.Private.CoreLib";
const DATETIME_OFFSET: &str = "System.DateTimeOffset";
const DATETIME_OFFSET_SIZE: usize = 16;
const DATETIME_SIZE: usize = 8;
const TIMESPAN_SIZE: usize = 8;

type DateTimeHandle = RustcCLRInteropManagedStruct<{ CORELIB }, "System.DateTime", DATETIME_SIZE>;
type TimeSpanHandle = RustcCLRInteropManagedStruct<{ CORELIB }, "System.TimeSpan", TIMESPAN_SIZE>;

/// A date and time paired with an explicit UTC offset, stored inline as a managed value type.
///
/// This aliases the compiler's managed-value marker directly, so exported signatures and DTO
/// properties retain the genuine CLR `System.DateTimeOffset` identity.
pub type DateTimeOffset =
    RustcCLRInteropManagedStruct<{ CORELIB }, { DATETIME_OFFSET }, DATETIME_OFFSET_SIZE>;

impl DateTimeOffset {
    /// Current local time with its local offset (`DateTimeOffset.Now`).
    pub fn now() -> Self {
        Self::vt_static0::<"get_Now", Self>()
    }

    /// Current UTC time (`DateTimeOffset.UtcNow`).
    pub fn utc_now() -> Self {
        Self::vt_static0::<"get_UtcNow", Self>()
    }

    /// Parse a managed date/time-offset string.
    pub fn parse(value: MString) -> Self {
        Self::vt_static1::<"Parse", MString, Self>(value)
    }

    /// Parse a Rust string through `DateTimeOffset.Parse`.
    pub fn parse_str(value: &str) -> Self {
        Self::parse(MString::from(value))
    }

    /// The UTC-normalized `DateTime` component.
    pub fn utc_datetime(self) -> DateTime {
        DateTime::from_raw(self.vt_instance0::<"get_UtcDateTime", DateTimeHandle>())
    }

    /// The local clock component without applying the offset.
    pub fn datetime(self) -> DateTime {
        DateTime::from_raw(self.vt_instance0::<"get_DateTime", DateTimeHandle>())
    }

    /// The explicit UTC offset.
    pub fn offset(self) -> DotNetTimeSpan {
        DotNetTimeSpan::from_raw(self.vt_instance0::<"get_Offset", TimeSpanHandle>())
    }

    pub fn unix_time_seconds(self) -> i64 {
        self.vt_instance0::<"ToUnixTimeSeconds", i64>()
    }

    pub fn compare_to(self, other: Self) -> i32 {
        self.vt_instance1::<"CompareTo", Self, i32>(other)
    }

    pub fn handle(self) -> Self {
        self
    }

    pub fn from_raw(handle: Self) -> Self {
        handle
    }
}

impl core::fmt::Display for DateTimeOffset {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let value = DotNetString::from_handle((*self).vt_instance0::<"ToString", MString>());
        core::fmt::Display::fmt(&value, f)
    }
}

impl core::fmt::Debug for DateTimeOffset {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl PartialEq for DateTimeOffset {
    fn eq(&self, other: &Self) -> bool {
        self.compare_to(*other) == 0
    }
}
impl Eq for DateTimeOffset {}

impl PartialOrd for DateTimeOffset {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DateTimeOffset {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.compare_to(*other).cmp(&0)
    }
}
