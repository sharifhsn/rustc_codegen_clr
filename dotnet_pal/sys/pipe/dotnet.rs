//! `sys::pipe::{Pipe, pipe}` for the .NET ("dotnet") platform — Cap-1 arm.
//!
//! Injected as the FIRST `cfg_select!` arm of `sys/pipe/mod.rs` so the unix arm
//! (bare `unix =>`) never wins at the Cap-2 `families=["unix"]` flip.
//!
//! PRESENT-but-Unsupported (honest, per LIBC_SHIM_SCOPE §6): `System.IO.Pipes`
//! are `Stream`s, not `Socket`s, so they cannot ride the per-fd `Socket.Poll`
//! readiness loop the dotnet net PAL uses; an anonymous pipe that participates in
//! the readiness loop is IMPOSSIBLE on stock CoreCLR. So this re-uses the verbatim
//! `unsupported.rs` source (`Pipe(!)`, `pipe()->Err(UNSUPPORTED_PLATFORM)`),
//! including its `#[cfg(any(unix, …))] mod unix_traits` (compiled OUT for
//! os=dotnet today, present so the Cap-2 family flip stays additive).

#[path = "unsupported.rs"]
mod imp;
pub use imp::{Pipe, pipe};
