//! Compile-fail UI tests for the clearest, most impactful `dotnet_macros` misuse cases (backlog §4
//! "Proc-macro error message quality"). Each `tests/ui/*.rs` is expected to FAIL to compile with the
//! message in the paired `*.stderr`; `trybuild` diffs the real rustc output against it.
//!
//! These are pure macro-expansion-time failures (bad attribute syntax/shape), so the fixture crates
//! deliberately do NOT depend on `mycorrhiza` — the bad input is rejected before the expansion would
//! ever need to resolve a `mycorrhiza::comptime::*` intrinsic.
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
