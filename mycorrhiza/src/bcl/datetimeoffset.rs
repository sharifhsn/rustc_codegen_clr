//! Idiomatic Rust wrapper over the managed `System.DateTimeOffset` value type.

use crate::NativeStorageSafe;
use crate::bcl::datetime::DateTime;
use crate::bcl::timespan::DotNetTimeSpan;
use crate::intrinsics::RustcCLRInteropManagedStruct;
use crate::system::MString;

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
// SAFETY: `System.DateTimeOffset` is a `DateTime` plus a signed offset; both are native scalar data
// and the value contains no GC references.
unsafe impl NativeStorageSafe for DateTimeOffset {}

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

impl_managed_display_value!(DateTimeOffset);
impl_managed_ordering!(DateTimeOffset, compare_to);
