// `?`-operator ergonomics for ManagedException (backlog Theme-2 §2 follow-up):
//
//   `try_managed`/`TryManaged::try_` already surface a thrown .NET exception as
//   `Result<T, ManagedException>`, so `?` works out of the box *when the function's own error type
//   already is* `ManagedException`. The remaining friction is a consumer's *own* error enum: without
//   help, every call site needs a hand-rolled `From<ManagedException> for MyError` (illegal as a
//   blanket impl across crates thanks to orphan rules, so it has to be written per-type). This probe
//   exercises `mycorrhiza::impl_from_managed_exception!`, which generates exactly that `From` impl, so
//   `?` bubbles a caught managed exception straight into a custom error type end-to-end.
#![allow(dead_code)]

use mycorrhiza::bcl::guid::Guid;
use mycorrhiza::prelude::*;
use mycorrhiza::system::console::Console;

#[derive(Debug, PartialEq)]
enum MyError {
    Managed(ManagedException),
    Other(&'static str),
}

// The macro under test: generates `impl From<ManagedException> for MyError`.
mycorrhiza::impl_from_managed_exception!(MyError, MyError::Managed);

// A fallible helper that internally makes managed calls via `?` against `ManagedException`, then
// widens (via `?` again, now using the generated `From`) into the caller's own `MyError`.
fn parse_guid(s: MString) -> Result<Guid, MyError> {
    let g = try_managed(|| Guid::parse(s))?;
    Ok(g)
}

// A second consumer, showing the same generated `From` impl composes with other error variants and
// with the `.try_()` combinator form.
fn sum_or_fail(bad: bool) -> Result<i32, MyError> {
    if bad {
        return Err(MyError::Other("bad flag"));
    }
    let a = (|| 3i32).try_().map_err(MyError::from)?;
    let b = try_managed(|| 4i32)?; // implicit `From<ManagedException>` via `?`
    Ok(a + b)
}

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

    // ---------- `?` converts a caught managed exception into the caller's own error type ----------
    let bad = DotNetString::from("definitely-not-a-guid").handle();
    let err = parse_guid(bad);
    chk!(matches!(err, Err(MyError::Managed(_))), true);

    // ---------- `?` still succeeds normally when the managed call does not throw ----------
    let good = DotNetString::from("00000000-0000-0000-0000-000000000000").handle();
    let ok = parse_guid(good);
    chk!(ok.is_ok(), true);
    chk!(ok.unwrap().is_empty(), true);

    // ---------- the generated `From` composes with a hand-written error variant ----------
    chk!(sum_or_fail(true), Err(MyError::Other("bad flag")));
    chk!(sum_or_fail(false), Ok(7));

    // ---------- explicit `.map_err(MyError::from)` also goes through the generated impl ----------
    let mapped: Result<i32, MyError> = try_managed(|| Guid::parse(bad))
        .map(|_| 0)
        .map_err(MyError::from);
    chk!(matches!(mapped, Err(MyError::Managed(_))), true);

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
