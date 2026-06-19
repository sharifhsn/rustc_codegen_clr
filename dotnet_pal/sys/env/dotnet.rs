//! Environment variables for the .NET ("dotnet") platform.
//!
//! Minimal implementation. The fixed extern contract this PAL is built against
//! exposes only allocation and stdio hooks into the BCL — there is no env-var
//! hook yet — so reads return `None` and writes report `Unsupported`. The
//! iterating `Env`, `env()`, `setenv` and `unsetenv` items are reused verbatim
//! from the shared `unsupported` arm (the zkvm arm does the same), and only
//! `getenv` is provided here so it can later be backed by
//! `System.Environment.GetEnvironmentVariable` without touching the rest.

#[expect(dead_code)]
#[path = "unsupported.rs"]
mod unsupported_env;
pub use unsupported_env::{Env, env, setenv, unsetenv};

use crate::ffi::{OsStr, OsString};

pub fn getenv(_: &OsStr) -> Option<OsString> {
    None
}
