//! `sys::sync::Condvar` for the .NET ("dotnet") platform — Cap-1 foundation arm.
//!
//! See `sys/sync/mutex/dotnet.rs` for the full rationale. Injected as the FIRST
//! `cfg_select!` arm of `sys/sync/condvar/mod.rs` so the `pthread` arm never wins
//! at the Cap-2 `families=["unix"]` flip. With `families` UNSET it is a pure
//! no-op: dotnet already falls to `no_threads`, whose verbatim source this
//! re-uses (`Condvar::wait` panics, `wait_timeout` sleeps — single-thread shape).
//
// TODO(Cap-2): swap to a System.Threading ManualResetEventSlim/Monitor-backed
// Condvar, bundled with the [ThreadStatic] TLS fix.

#[path = "no_threads.rs"]
mod imp;
pub use imp::Condvar;
