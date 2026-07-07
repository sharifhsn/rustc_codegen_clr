// The END-USER experience of `mycorrhiza::bcl` — the common Base Class Library value types and
// static helpers (DateTime / TimeSpan / Guid / Uri / Regex / Random / Stopwatch / StringBuilder /
// Environment / Math) used like normal Rust types: associated-fn constructors, snake_case methods,
// `&str` in / `String` out, and the natural std traits. No `instanceN`, no assembly strings, no
// `System.String` marshalling at the call site.
//
// Every result is checked in-Rust; `main` prints `pass` then `total` (a `9000000xx` marker flags any
// failing check) and returns non-zero on any mismatch — the `cd_collections` convention.
#![allow(dead_code)]

use mycorrhiza::prelude::*;
use mycorrhiza::system::console::Console;

fn main() -> std::process::ExitCode {
    let mut pass: u32 = 0;
    let mut total: u32 = 0;
    macro_rules! chk {
        ($got:expr, $want:expr) => {{
            total += 1;
            if $got == $want {
                pass += 1;
            } else {
                Console::writeln_u64(900_000_000 + total as u64);
            }
        }};
    }

    // ---------- DateTime ----------
    let ymd = DateTime::new(2026, 6, 30); // new DateTime(2026, 6, 30)
    chk!(ymd.year(), 2026);
    chk!(ymd.month(), 6);
    chk!(ymd.day(), 30);
    chk!(ymd.hour(), 0);
    chk!(ymd.minute(), 0);
    chk!(ymd.second(), 0);
    let next = ymd.add_days(1.0); // rolls into July
    chk!(next.month(), 7);
    chk!(next.day(), 1);
    chk!((next > ymd), true); // Ord via CompareTo
    chk!((ymd == DateTime::new(2026, 6, 30)), true); // value equality
    chk!((ymd == next), false);
    let plus2h = ymd.add_hours(2.0).add_minutes(30.0);
    chk!(plus2h.hour(), 2);
    chk!(plus2h.minute(), 30);
    chk!(ymd.add_years(1).year(), 2027);
    chk!(ymd.add_months(1).month(), 7);
    chk!(ymd.day_of_year(), 181); // 2026-06-30 is day 181 (non-leap year)
    // `now()` must be after a fixed past date and have in-range components.
    let now = DateTime::now();
    chk!((now > DateTime::new(2020, 1, 1)), true);
    chk!((now.month() >= 1 && now.month() <= 12), true);
    chk!((now.day() >= 1 && now.day() <= 31), true);
    // Display round-trips through ToString and is non-empty.
    chk!(std::format!("{ymd}").is_empty(), false);

    // ---------- TimeSpan ----------
    let t = DotNetTimeSpan::from_minutes(1.5);
    chk!(t.total_seconds(), 90.0);
    chk!(t.total_minutes(), 1.5);
    let u = t.add(DotNetTimeSpan::from_seconds(30.0));
    chk!(u.total_seconds(), 120.0);
    chk!(u.minutes(), 2); // component (0..=59), not total
    chk!(u.seconds(), 0);
    chk!((u > t), true);
    chk!(DotNetTimeSpan::from_hours(1.0).total_minutes(), 60.0);
    chk!(DotNetTimeSpan::from_days(1.0).total_hours(), 24.0);
    chk!(DotNetTimeSpan::from_ticks(10_000_000).total_seconds(), 1.0);
    chk!(u.subtract(t).total_seconds(), 30.0);
    chk!(DotNetTimeSpan::from_seconds(-5.0).duration().total_seconds(), 5.0);
    chk!(DotNetTimeSpan::from_seconds(5.0).negate().total_seconds(), -5.0);
    chk!(DotNetTimeSpan::zero().total_seconds(), 0.0);
    chk!(std::format!("{u}").is_empty(), false);
    // Ord (via TimeSpan.CompareTo): sort a Vec<TimeSpan> and check ascending numeric order.
    let ts_a = DotNetTimeSpan::from_seconds(5.0);
    let ts_b = DotNetTimeSpan::from_seconds(1.0);
    let ts_c = DotNetTimeSpan::from_minutes(2.0); // 120s
    let mut tss = std::vec![ts_a, ts_b, ts_c];
    tss.sort();
    chk!(tss[0].total_seconds(), 1.0);
    chk!(tss[1].total_seconds(), 5.0);
    chk!(tss[2].total_seconds(), 120.0);

    // ---------- Guid ----------
    let a = Guid::new_v4();
    let b = Guid::new_v4();
    chk!((a == b), false); // (astronomically) distinct
    chk!((a == a), true);
    chk!(Guid::empty().is_empty(), true);
    chk!(a.is_empty(), false);
    chk!((Guid::empty() == Guid::default()), true);
    // ToString round-trips: parse(a.to_string()) == a. A canonical Guid string is 36 chars.
    let a_str = a.to_string();
    chk!(a_str.len(), 36);
    let parsed = Guid::parse(MString::from(a_str.as_str()));
    chk!((parsed == a), true);
    // Ord (via Guid.CompareTo): sorting must produce a stable ascending order matching CompareTo
    // pairwise, and round-trip the same multiset of values.
    let g_lo = Guid::parse(MString::from("00000000-0000-0000-0000-000000000001"));
    let g_mid = Guid::parse(MString::from("00000000-0000-0000-0000-000000000002"));
    let g_hi = Guid::parse(MString::from("ffffffff-0000-0000-0000-000000000000"));
    let mut gs = std::vec![g_hi, g_lo, g_mid];
    gs.sort();
    chk!((gs[0] == g_lo), true);
    chk!((gs[1] == g_mid), true);
    chk!((gs[2] == g_hi), true);
    chk!((g_lo < g_mid && g_mid < g_hi), true);

    // ---------- Uri ----------
    let uri = Uri::new("https://user@example.com:8443/path/page?q=1#frag");
    chk!(uri.scheme().as_str(), "https");
    chk!(uri.host().as_str(), "example.com");
    chk!(uri.port(), 8443);
    chk!(uri.absolute_path().as_str(), "/path/page");
    chk!(uri.query().as_str(), "?q=1");
    chk!(uri.fragment().as_str(), "#frag");
    chk!(uri.user_info().as_str(), "user");
    chk!(uri.is_absolute(), true);
    chk!(uri.is_file(), false);
    chk!(uri.path_and_query().as_str(), "/path/page?q=1");
    chk!(Uri::escape_data_string("a b&c").as_str(), "a%20b%26c");
    chk!(Uri::unescape_data_string("a%20b%26c").as_str(), "a b&c");

    // ---------- Regex ----------
    let re = Regex::new(r"(\d+)-(\d+)");
    chk!(re.is_match("10-20"), true);
    chk!(re.is_match("no digits"), false);
    let m = re.find("x 10-20 y").unwrap();
    chk!(m.value().as_str(), "10-20");
    chk!(m.index(), 2);
    chk!(m.length(), 5);
    chk!(re.find("no match").is_none(), true);
    // Groups: 0 = whole match, 1 & 2 = the two captures.
    let g = m.groups();
    chk!(g.len(), 3);
    chk!(g.get(1).unwrap().value().as_str(), "10");
    chk!(g.get(2).unwrap().value().as_str(), "20");
    // find_all / count.
    let all = re.find_all("1-2 3-4 5-6");
    chk!(all.len(), 3);
    chk!(re.count("1-2 3-4 5-6"), 3);
    let mut concat = std::string::String::new();
    for mt in all.iter() {
        concat.push_str(mt.value().as_str());
        concat.push('|');
    }
    chk!(concat.as_str(), "1-2|3-4|5-6|");
    // replace_all replaces every occurrence by default.
    chk!(Regex::new(r"\d").replace_all("a1b2c3", "#").as_str(), "a#b#c#");
    // statics.
    chk!(Regex::is_match_str("abc123", r"\d+"), true);
    chk!(Regex::escape("a.b").as_str(), r"a\.b");

    // ---------- Random ----------
    // Seeded reproducibility: same seed -> identical sequence (the .NET contract).
    let mut r1 = Random::with_seed(42);
    let mut r2 = Random::with_seed(42);
    chk!((r1.next() == r2.next()), true);
    chk!((r1.next() == r2.next()), true);
    // Range bounds.
    let mut r = Random::with_seed(7);
    for _ in 0..64 {
        let d6 = r.next_range(1, 7);
        chk!((d6 >= 1 && d6 <= 6), true);
        let p = r.next_f64();
        chk!((p >= 0.0 && p < 1.0), true);
        let below = r.next_below(10);
        chk!((below >= 0 && below < 10), true);
    }
    let f = Random::with_seed(1).next_f32();
    chk!((f >= 0.0 && f < 1.0), true);
    let big = Random::with_seed(3).next_i64_range(100, 200);
    chk!((big >= 100 && big < 200), true);
    // Random::shared() is usable from any thread.
    let s = Random::shared().next_below(2);
    chk!((s == 0 || s == 1), true);

    // ---------- Stopwatch ----------
    let sw = Stopwatch::start_new();
    chk!(sw.is_running(), true);
    // Burn some cycles so elapsed advances monotonically.
    let mut acc: u64 = 0;
    for i in 0..2_000_000u64 {
        acc = acc.wrapping_add(i);
    }
    chk!((sw.elapsed_ticks() >= 0), true);
    chk!((sw.elapsed_millis() >= 0), true);
    sw.stop();
    chk!(sw.is_running(), false);
    let after = sw.elapsed_millis();
    // Elapsed does not advance while stopped.
    for i in 0..500_000u64 {
        acc = acc.wrapping_add(i);
    }
    chk!((sw.elapsed_millis() == after), true);
    let fresh = Stopwatch::new();
    chk!(fresh.is_running(), false);
    chk!(fresh.elapsed_millis(), 0);
    fresh.start();
    chk!(fresh.is_running(), true);
    fresh.reset();
    chk!(fresh.is_running(), false);
    chk!(fresh.elapsed_millis(), 0);
    // Keep `acc` alive so the loops are not optimized away.
    chk!((acc > 0), true);

    // ---------- StringBuilder ----------
    let mut sb = StringBuilder::new();
    sb.append("Hello, ");
    sb.append("world");
    sb.append_char('!');
    chk!(sb.len(), 13);
    chk!(sb.is_empty(), false);
    chk!(sb.to_rust_string().as_str(), "Hello, world!");
    chk!(std::format!("{sb}").as_str(), "Hello, world!");
    sb.clear();
    chk!(sb.is_empty(), true);
    let mut sb2 = StringBuilder::from("abc");
    chk!(sb2.to_rust_string().as_str(), "abc");
    sb2.insert(1, "XY");
    chk!(sb2.to_rust_string().as_str(), "aXYbc");
    sb2.remove(1, 2);
    chk!(sb2.to_rust_string().as_str(), "abc");
    sb2.replace("b", "B");
    chk!(sb2.to_rust_string().as_str(), "aBc");
    // write!/writeln! via core::fmt::Write.
    use core::fmt::Write as _;
    let mut sb3 = StringBuilder::new();
    let _ = write!(sb3, "n={}", 42);
    chk!(sb3.to_rust_string().as_str(), "n=42");

    // ---------- Decimal ----------
    // Eq/Ord (via Decimal.op_Equality/Decimal.Compare): exact base-10 numeric comparison, not
    // textual/culture-sensitive, so a real total order. Sort a Vec<DotNetDecimal> and check order.
    let d_a = DotNetDecimal::parse("10.50");
    let d_b = DotNetDecimal::parse("-3.25");
    let d_c = DotNetDecimal::parse("10.5"); // same numeric value as d_a
    chk!((d_a == d_c), true);
    let mut ds = std::vec![d_a, d_b, d_c];
    ds.sort();
    chk!(ds[0].to_f64(), -3.25);
    chk!((ds[1] == d_a && ds[2] == d_a), true); // the two 10.5-valued entries sort together
    chk!((d_b < d_a), true);

    // ---------- Environment ----------
    chk!(Environment::machine_name().is_empty(), false);
    chk!((Environment::processor_count() >= 1), true);
    chk!((Environment::process_id() > 0), true);
    // Round-trip a variable we set ourselves.
    Environment::set_var("MYCORRHIZA_CD_BCL", "hello-bcl");
    chk!(Environment::var("MYCORRHIZA_CD_BCL"), Some("hello-bcl".to_string()));
    chk!(Environment::var("MYCORRHIZA_DEFINITELY_UNSET_XYZ").is_none(), true);
    chk!((Environment::tick_count64() >= 0), true);
    chk!(Environment::is_64bit_process(), true); // native aarch64/x86_64

    // ---------- Math ----------
    chk!(Math::sqrt(16.0), 4.0);
    chk!(Math::pow(2.0, 10.0), 1024.0);
    chk!(Math::abs(-3.5), 3.5);
    chk!(Math::floor(3.9), 3.0);
    chk!(Math::ceil(3.1), 4.0);
    chk!(Math::round(2.5), 2.0); // banker's rounding (ties-to-even)
    chk!(Math::trunc(-3.9), -3.0);
    chk!(Math::max(2.0, 7.0), 7.0);
    chk!(Math::min(2.0, 7.0), 2.0);
    chk!(Math::sign(-9.0), -1);
    chk!(Math::log2(8.0), 3.0);
    chk!(Math::log10(1000.0), 3.0);
    chk!(Math::cbrt(27.0), 3.0);
    chk!(((Math::exp(0.0) - 1.0).abs() < 1e-12), true);
    chk!(((Math::ln(Math::E) - 1.0).abs() < 1e-12), true);
    chk!(((Math::sin(0.0)).abs() < 1e-12), true);
    chk!(((Math::cos(0.0) - 1.0).abs() < 1e-12), true);
    chk!(((Math::atan2(1.0, 1.0) - Math::PI / 4.0).abs() < 1e-12), true);
    chk!(((Math::PI - std::f64::consts::PI).abs() < 1e-12), true);
    chk!(Math::copy_sign(3.0, -1.0), -3.0);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
