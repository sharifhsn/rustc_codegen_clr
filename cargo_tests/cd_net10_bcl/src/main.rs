//! Executable acceptance for BCL members introduced in .NET 10.

use mycorrhiza::bcl::isoweek::{DayOfWeek, ISOWeek};

fn main() {
    // ISO week 2020-W53 began on 2020-12-28 and ended in calendar year 2021. This checks both the
    // new DateOnly-producing overload and the ISO week-year boundary semantics.
    let monday = ISOWeek::to_date_only(2020, 53, DayOfWeek::Monday);
    assert_eq!(monday.vt_instance0::<"get_Year", i32>(), 2020);
    assert_eq!(monday.vt_instance0::<"get_Month", i32>(), 12);
    assert_eq!(monday.vt_instance0::<"get_Day", i32>(), 28);
    assert_eq!(ISOWeek::get_week_of_year(monday), 53);
    assert_eq!(ISOWeek::get_year(monday), 2020);

    let friday = ISOWeek::to_date_only(2020, 53, DayOfWeek::Friday);
    assert_eq!(friday.vt_instance0::<"get_Year", i32>(), 2021);
    assert_eq!(friday.vt_instance0::<"get_Month", i32>(), 1);
    assert_eq!(friday.vt_instance0::<"get_Day", i32>(), 1);
    assert_eq!(ISOWeek::get_year(friday), 2020);

    println!("cd_net10_bcl: all checks passed");
    println!("== cd_net10_bcl done ==");
}
