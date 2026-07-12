// Proof for `mycorrhiza::dynamic` -- the raw dynamic-reflection invoke escape hatch. Everything else
// `mycorrhiza`/`add-nuget`/`spinacz` does is STATIC binding (the (assembly, type, method) triple is a
// Rust-compile-time const-generic, so the backend emits a real CIL `call`). This proves the OTHER
// case: given a target known only as runtime strings, `invoke_dynamicN`/`invoke_dynamicN_checked`
// resolve it via `System.Reflection` (`Assembly.Load`/`Type.GetMethod`/`MethodInfo.Invoke`) through
// the bundled `Mycorrhiza.Interop.Helpers` C# helper, and the boxed result round-trips correctly.
//
// Every result is checked against a native Rust oracle computed with ordinary Rust arithmetic/string
// ops (never by re-deriving the expected value from the same dynamic-invoke path). `main` prints
// `pass` then `total` (a `9000000xx` marker flags any failing check) and returns non-zero on any
// mismatch -- the `cd_bcl`/`cd_collections` convention.

use mycorrhiza::dynamic::{
    box_arg, invoke_dynamic1, invoke_dynamic1_checked, invoke_dynamic2_checked, str_arg,
};
use mycorrhiza::intrinsics::{
    rustc_clr_interop_managed_checked_cast as checked_cast, RustcCLRInteropManagedClass,
};
use mycorrhiza::system::console::Console;
use mycorrhiza::system::{DotNetString, MObject, MString};

/// `System.Convert`, used only to unbox the `object` results this test gets back.
type CConvert = RustcCLRInteropManagedClass<"System.Private.CoreLib", "System.Convert">;

fn unbox_i32(o: MObject) -> i32 {
    CConvert::static1::<"ToInt32", MObject, i32>(o)
}

fn unbox_string(o: MObject) -> std::string::String {
    DotNetString::from_handle(checked_cast::<MString, MObject>(o)).to_rust_string()
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

    // ---------- System.Math.Abs(int) via the raw, unchecked `unsafe` path ----------
    // Chosen specifically to sidestep overload-resolution ambiguity: `Math.Abs` has overloads for
    // every signed numeric type, but boxing a Rust `i32` gives a boxed value whose runtime type is
    // EXACTLY `System.Int32`, so the helper's exact-type `GetMethod` lookup resolves the `int`
    // overload unambiguously.
    for native in [-7i32, 0, 7, i32::MIN + 1, i32::MAX] {
        // SAFETY: "System.Private.CoreLib"/"System.Math"/"Abs" is a real, known-good target and the
        // single `int` argument matches `Math.Abs(int)` exactly -- this call cannot fail to resolve.
        let result =
            unsafe { invoke_dynamic1("System.Private.CoreLib", "System.Math", "Abs", box_arg(native)) };
        chk!(unbox_i32(result), native.abs());
    }

    // ---------- System.String.Concat(string, string) -- a reference-type overload ----------
    let pairs = [("foo", "bar"), ("Hello, ", "World!"), ("", "x"), ("rust", "")];
    for (a, b) in pairs {
        let result =
            invoke_dynamic2_checked("System.Private.CoreLib", "System.String", "Concat", str_arg(a), str_arg(b))
                .expect("System.String.Concat(string, string) must resolve");
        chk!(unbox_string(result), std::format!("{a}{b}"));
    }

    // ---------- the checked path surfaces a bad target as Err, not a process abort ----------
    let bad_method = invoke_dynamic1_checked(
        "System.Private.CoreLib",
        "System.Math",
        "ThisMethodDoesNotExist",
        box_arg(1i32),
    );
    chk!(bad_method.is_err(), true);

    let bad_type = invoke_dynamic1_checked(
        "System.Private.CoreLib",
        "System.Nonexistent.Bogus.Type",
        "Abs",
        box_arg(1i32),
    );
    chk!(bad_type.is_err(), true);

    // ---------- a second unambiguous static BCL call, to prove this isn't Math.Abs-specific ----------
    // `System.Math.Max(int, int)` -- two boxed `int` args, exact-type overload match.
    for (a, b) in [(3i32, 5i32), (10, -2), (0, 0)] {
        let result = invoke_dynamic2_checked("System.Private.CoreLib", "System.Math", "Max", box_arg(a), box_arg(b))
            .expect("System.Math.Max(int, int) must resolve");
        chk!(unbox_i32(result), a.max(b));
    }

    Console::writeln_u64(pass as u64);
    Console::writeln_u64(total as u64);
    if pass == total {
        println!("== cd_dynamic_invoke done ==");
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
