//! `sys::sync::RwLock` for the .NET ("dotnet") platform — Cap-1 foundation arm.
//!
//! See `sys/sync/mutex/dotnet.rs` for the full rationale. Injected as the FIRST
//! `cfg_select!` arm of `sys/sync/rwlock/mod.rs` so the `queue` arm (which pulls
//! thread parking, gated on `target_family="unix"`) never wins at the Cap-2
//! `families=["unix"]` flip. With `families` UNSET it is a pure no-op: dotnet
//! already falls to `no_threads`, whose verbatim source this re-uses.
//
// TODO(Cap-2): swap to a System.Threading ReaderWriterLockSlim-backed RwLock,
// bundled with the [ThreadStatic] TLS fix.

#[path = "no_threads.rs"]
mod imp;
pub use imp::RwLock;
