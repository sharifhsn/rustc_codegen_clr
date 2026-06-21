//! `sys::process` for the .NET ("dotnet") platform — Cap-1 foundation arm.
//!
//! Injected as the FIRST `cfg_select!` arm of `sys/process/mod.rs`
//! (`mod dotnet; use dotnet as imp;`) so the unix arm (gated on
//! `target_family="unix"`, which pulls libc `fork`/`execvp`/`posix_spawn`) never
//! wins at the Cap-2 `families=["unix"]` flip.
//!
//! Cap-1 mirrors `unsupported.rs` item-for-item (spawn is genuinely IMPOSSIBLE on
//! stock CoreCLR — no `fork`/`execve`, and a `Process.Start` pid would be
//! synthetic — LIBC_SHIM_SCOPE §2.7/§6, deferred), with ONE cheap REAL upgrade:
//! `getpid()` → `System.Environment.ProcessId` via the `rcl_dotnet_getpid` hook
//! (cilly/src/ir/builtins/dotnet.rs), instead of the `unsupported` `panic!`.
//!
//! Implementation: re-use the verbatim `unsupported.rs` source as the inner `imp`
//! module and re-export everything from it EXCEPT `getpid`, which we shadow with
//! the real hook. The inner module references `super::env` / `super::output`'s
//! siblings; `super` of `imp` is THIS `dotnet` module, so we bring `env` into
//! scope here with a `use` so `imp`'s `super::env::…` paths resolve.
#![forbid(unsafe_op_in_unsafe_fn)]

// Make `super::env` (referenced by the included unsupported source as
// `super::env::{CommandEnv, …}`) resolve: `super` of `imp` is this `dotnet`
// module, so re-bind `env` here from the real `process::env`.
pub(super) use super::env;

#[path = "unsupported.rs"]
mod imp;

// Re-export the full item set `sys/process/mod.rs` consumes, EXCEPT `getpid`
// (shadowed below with the real hook).
pub use imp::{
    ChildPipe, Command, CommandArgs, EnvKey, ExitCode, ExitStatus, ExitStatusError, Process, Stdio,
    output, read_output,
};

unsafe extern "C" {
    fn rcl_dotnet_getpid() -> u32;
}

/// `getpid()` → `System.Environment.ProcessId`. The one REAL upgrade over the
/// `unsupported` arm (which `panic!`s); a genuine process id, unlike spawn's
/// synthetic-pid wall.
pub fn getpid() -> u32 {
    // SAFETY: the hook reads `Environment.ProcessId` (a static i32 getter).
    unsafe { rcl_dotnet_getpid() }
}
