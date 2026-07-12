// The END-USER experience of Theme-2 idiomatic error/text ergonomics:
//   * managed `null`      -> `Option`   (`Nullable::to_option` / `from_nullable`)
//   * a thrown exception  -> `Result`   (`try_managed` / the `.try_()` combinator)
//   * `DotNetString`      -> feels like `std::string::String` (Display, From<&str>, ==, concat, the
//                            common `String` methods, ordering).
//
// No `is_null` boilerplate, no `interop_try_catch` fn-pointer dance, no raw `System.String` method
// refs at the call site. Every result is checked in-Rust; `main` prints `pass` then `total` (a
// `9000000xx` marker flags any failing check) and returns non-zero on any mismatch.
#![allow(dead_code)]

use mycorrhiza::bcl::guid::Guid;
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

    // ---------- null <-> Option ----------
    // A managed `null` reference becomes `None`; a live reference maps into an `Option` of a *Rust*
    // value (a managed ref itself can't sit in an `Option` — the overlapping-field wall).
    let null_str: MString = MString::null();
    chk!(null_str.present().is_none(), true);
    chk!(null_str.is_present(), false);
    chk!(
        null_str
            .map_present(|| DotNetString::from_handle(null_str).to_rust_string())
            .is_none(),
        true
    );
    chk!(from_nullable(null_str, || 1i32).is_none(), true);

    let live: MString = DotNetString::from("hi").handle();
    chk!(live.is_present(), true);
    chk!(live.present().is_some(), true);
    // The mapped value is the marshalled content of the (captured) live reference.
    let mapped = live.map_present(|| DotNetString::from_handle(live).to_rust_string());
    chk!(mapped.as_deref(), Some("hi"));
    chk!(
        from_nullable(live, || DotNetString::from_handle(live).len_utf16()),
        Some(2)
    );

    // ---------- throwing call -> Err ----------
    // `Guid.Parse` throws `FormatException` on malformed input — a *foreign* (.NET/BCL) exception,
    // the kind `catch_unwind` refuses to absorb. `try_managed` catches it and yields `Err`.
    let bad = DotNetString::from("definitely-not-a-guid").handle();
    let parsed = try_managed(|| Guid::parse(bad));
    chk!(parsed.is_err(), true);

    // ---------- non-throwing call -> Ok ----------
    // A pure computation: no exception, `Ok(value)`.
    let sum = try_managed(|| 2 + 2);
    chk!(sum, Ok(4));
    // A real (non-throwing) BCL call: parsing a valid GUID succeeds.
    let good = DotNetString::from("00000000-0000-0000-0000-000000000000").handle();
    let g = try_managed(|| Guid::parse(good));
    chk!(g.is_ok(), true);
    chk!(g.unwrap().is_empty(), true); // the all-zero GUID

    // The `.try_()` combinator form reads left-to-right and returns the same `Result`.
    chk!((|| 7i32).try_(), Ok(7));
    chk!((|| Guid::parse(bad)).try_().is_err(), true);

    // `?` interop: a helper that bubbles a managed exception as an `Err`.
    fn checked() -> Result<i32, ManagedException> {
        let a = try_managed(|| 10)?;
        let b = try_managed(|| 5)?;
        Ok(a + b)
    }
    chk!(checked(), Ok(15));

    // ---------- DotNetString: idiomatic String-like surface ----------
    let a = DotNetString::from("Hello");
    let b = DotNetString::from("Hello");
    let c = DotNetString::from("World");

    // equality / ordering / &str comparison
    chk!((a == b), true);
    chk!((a == c), false);
    chk!((a == "Hello"), true); // PartialEq<&str>
    chk!(("Hello" == a), true); // and the reflexive impl
    chk!((a < c), true); // ordinal ordering ('H' < 'W')

    // Display / Debug / marshal-back
    chk!(std::format!("{}", a).as_str(), "Hello");
    chk!(std::format!("{:?}", a).as_str(), "\"Hello\"");
    chk!(a.to_rust_string().as_str(), "Hello");
    chk!(std::string::String::from(a).as_str(), "Hello"); // From<DotNetString>

    // From<&String> / FromStr / Default
    let owned = std::string::String::from("Hello");
    chk!((DotNetString::from(&owned) == a), true);
    chk!(("Hello".parse::<DotNetString>().unwrap() == a), true);
    chk!(DotNetString::default().is_empty(), true);
    chk!(DotNetString::empty().is_empty(), true);
    chk!(a.is_empty(), false);

    // the common `System.String` methods
    chk!(a.len_utf16(), 5);
    chk!(a.contains(DotNetString::from("ell")), true);
    chk!(a.contains(DotNetString::from("xyz")), false);
    chk!(a.starts_with(DotNetString::from("He")), true);
    chk!(a.ends_with(DotNetString::from("lo")), true);
    chk!(a.index_of(DotNetString::from("l")), 2);
    chk!((a.to_upper() == "HELLO"), true);
    chk!((a.to_lower() == "hello"), true);
    chk!((DotNetString::from("  pad  ").trim() == "pad"), true);
    chk!((a.substring(3) == "lo"), true);
    chk!(
        (a.replace(DotNetString::from("l"), DotNetString::from("L")) == "HeLLo"),
        true
    );

    // concatenation via `concat`, `+`, and `+=`
    let sp = DotNetString::from(" ");
    let joined = a.concat(sp).concat(c);
    chk!((joined == "Hello World"), true);
    chk!((a + c == "HelloWorld"), true); // `+`
    let mut acc = DotNetString::from("Hello");
    acc += c;
    chk!((acc == "HelloWorld"), true); // `+=`

    // non-ASCII round-trip (multi-byte UTF-8 -> UTF-16 -> back)
    let u = DotNetString::from("héllo");
    chk!(u.to_rust_string().as_str(), "héllo");
    chk!(std::format!("{}", u).as_str(), "héllo");

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        println!("== cd_idiomatic done ==");
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
