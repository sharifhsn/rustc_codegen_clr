//! End-to-end validation of the panic → managed-exception throw-bridge (WF-6).
//!
//! `std::panic::catch_unwind` exercises the whole chain: `panic!` → the std panic runtime →
//! `__rust_start_panic` → `_Unwind_RaiseException` (overridden to throw a `RustException`) → the CIL
//! `try`/`catch` in the `catch_unwind` builtin → `__rust_panic_cleanup` decodes the payload → `Err`.
//! Before the bridge, the panic hit the "missing method" stub (a plain `System.Exception`), the
//! `IsInst RustException` filter rejected it, and it escaped uncaught.

use std::panic::catch_unwind;

pub fn main() {
    // Silence the default hook: we only care about catch_unwind's control flow, not the panic
    // message/backtrace machinery (which would also pollute stdout-vs-native comparison).
    std::panic::set_hook(Box::new(|_| {}));

    // Happy path: the closure returns normally -> Ok(4).
    let ok = catch_unwind(|| 2 + 2);
    let ok_pass = matches!(ok, Ok(4));

    // Throw path: the closure panics -> the panic must be thrown as a RustException by the bridge
    // and caught here -> Err.
    let caught = catch_unwind(|| -> i32 { panic!("boom from inside catch_unwind") });
    let caught_pass = caught.is_err();

    // A second round-trip, to confirm catch_unwind is reusable after a caught panic.
    let again = catch_unwind(|| -> i32 { panic!("second boom") }).is_err();

    if ok_pass && caught_pass && again {
        println!("PASS: catch_unwind caught the panic (and recovered to keep running)");
    } else {
        println!("FAIL: ok_pass={ok_pass} caught_pass={caught_pass} again={again}");
    }
}
