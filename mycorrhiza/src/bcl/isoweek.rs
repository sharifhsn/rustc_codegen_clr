//! .NET 10 `System.Globalization.ISOWeek` overloads that operate directly on `DateOnly`.
//!
//! These methods do not exist in the .NET 8 or .NET 9 reference contract. Build callers with the
//! `net10` runtime profile (`cargo dotnet ... --dotnet 10`).

use crate::bcl::dateonly::DateOnly;
use crate::intrinsics::{RustcCLRInteropManagedClass, RustcCLRInteropManagedStruct};

const CORELIB: &str = "System.Private.CoreLib";
const ISO_WEEK: &str = "System.Globalization.ISOWeek";
const DAY_OF_WEEK: &str = "System.DayOfWeek";

type RawISOWeek = RustcCLRInteropManagedClass<{ CORELIB }, { ISO_WEEK }>;
type RawDayOfWeek = RustcCLRInteropManagedStruct<{ CORELIB }, { DAY_OF_WEEK }, 4>;

/// A day in the seven-day week, matching the stable `System.DayOfWeek` discriminants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum DayOfWeek {
    Sunday = 0,
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
}

impl DayOfWeek {
    #[inline(always)]
    fn managed(self) -> RawDayOfWeek {
        // `System.DayOfWeek` is an Int32-backed managed enum.
        unsafe { core::mem::transmute::<i32, RawDayOfWeek>(self as i32) }
    }
}

/// The .NET 10 `ISOWeek` operations over `DateOnly`.
pub struct ISOWeek;

impl ISOWeek {
    /// Return the ISO 8601 week number (1 through 53) containing `date`.
    #[inline(always)]
    pub fn get_week_of_year(date: DateOnly) -> i32 {
        RawISOWeek::static1::<"GetWeekOfYear", DateOnly, i32>(date)
    }

    /// Return the ISO week-numbering year containing `date`.
    #[inline(always)]
    pub fn get_year(date: DateOnly) -> i32 {
        RawISOWeek::static1::<"GetYear", DateOnly, i32>(date)
    }

    /// Convert an ISO week-numbering date into a `System.DateOnly` value.
    #[inline(always)]
    pub fn to_date_only(year: i32, week: i32, day: DayOfWeek) -> DateOnly {
        RawISOWeek::static3::<"ToDateOnly", i32, i32, RawDayOfWeek, DateOnly>(
            year,
            week,
            day.managed(),
        )
    }
}
