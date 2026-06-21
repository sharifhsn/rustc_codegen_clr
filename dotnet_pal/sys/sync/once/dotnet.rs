//! `sys::sync::{Once, OnceState}` for the .NET ("dotnet") platform — Cap-1 arm.
//!
//! See `sys/sync/mutex/dotnet.rs` for the full rationale. Injected as the FIRST
//! `cfg_select!` arm of `sys/sync/once/mod.rs` so the `queue` arm (which pulls
//! thread parking, gated on `target_family="unix"`) never wins at the Cap-2
//! `families=["unix"]` flip. With `families` UNSET it is a pure no-op: dotnet
//! already falls to `no_threads`, whose verbatim source this re-uses.
//
// TODO(Cap-2): swap to a real concurrent Once when threads land.

#[path = "no_threads.rs"]
mod imp;
pub use imp::{Once, OnceState};
