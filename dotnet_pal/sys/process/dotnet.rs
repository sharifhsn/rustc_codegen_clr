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

// PACKAGE A/B — include the dotnet-patched `imp` (a copy of upstream
// `unsupported.rs` plus the os::unix::process ext-trait surface: CommandExt /
// ExitStatusExt / ChildExt method+variant stubs) instead of the verbatim
// `unsupported.rs`. The `target-family=["unix"]` flip activates those public
// ext traits, which call inherent methods/variants the verbatim file lacks.
// `dotnet_imp.rs` is mirrored into rust-src by feasibility/dev.sh's PAL cp loop.
#[path = "dotnet_imp.rs"]
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

/// PACKAGE A — the `target-family="unix"` flip activates a bare
/// `#[cfg(target_family="unix")] pub use imp::getppid;` re-export in
/// `sys/process/mod.rs`; `imp` (= this dotnet arm) must therefore export
/// `getppid`. **LEAKY (L7):** synthetic `0` — CoreCLR has no portable
/// parent-pid API. Not load-bearing (a pure re-export; std::process exposes no
/// `parent_id` consumer of it on this PAL). An `rcl_dotnet_getppid` BCL hook is
/// a future upgrade if a portable parent pid ever becomes available.
pub fn getppid() -> u32 {
    0
}
