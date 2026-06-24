//! Differential regression for `fn main() -> T where T: Termination`.
//!
//! Before P2-S3 the backend ICE'd here: `cilly::entrypoint::wrapper` only handled `() -> ()` and the
//! C-main ABI, so any non-`Void`-returning `main` (`-> Result<_,_>` / `-> ExitCode`) hit its
//! `panic!("Unsuported entrypoint wrapper signature!")`. The fix routes such a `main` through
//! `std::rt::lang_start::<T>` (mirroring rustc's `create_entry_fn`), which runs `main`, converts `T`
//! to an exit code via `Termination::report`, and propagates it via `Environment.Exit`.
//!
//! This crate exercises the `Ok`-returning `Result` path (incl. the `?` operator), which exits 0 —
//! the apphost launcher preserves a 0 exit code, so this FULL-MATCHes under the standard oracle.
//! (The `Err`/`ExitCode` non-zero exit codes are emitted correctly too — verified byte-identical
//! with `dotnet <dll>` directly — but the single-file `cargo dotnet run` apphost drops a non-zero
//! managed exit code, a known harness limitation orthogonal to this codegen.)

use std::num::ParseIntError;

fn parse_and_add(a: &str, b: &str) -> Result<i64, ParseIntError> {
    let x: i64 = a.parse()?;
    let y: i64 = b.parse()?;
    Ok(x + y)
}

fn main() -> Result<(), ParseIntError> {
    let sum = parse_and_add("40", "2")?;
    println!("sum={sum}");
    let doubled: i64 = "21".parse::<i64>()? * 2;
    println!("doubled={doubled}");
    Ok(())
}
