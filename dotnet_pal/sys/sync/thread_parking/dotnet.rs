//! `sys::sync::thread_parking::Parker` for the .NET ("dotnet") platform — Cap-1.
//!
//! Injected as the FIRST `cfg_select!` arm of `sys/sync/thread_parking/mod.rs`.
//! The `pthread` Parker arm there is gated on `target_family="unix"` and would
//! win at the Cap-2 `families=["unix"]` flip; it depends on `sys::pal::unix`
//! (absent for os=dotnet). This arm pre-empts it. With `families` UNSET it is a
//! pure no-op: dotnet already falls to `unsupported`, whose verbatim source this
//! re-uses. (Also keeps the `queue` rwlock/once impls — which need a `Parker` —
//! buildable should they ever be selected; we route those to `no_threads`, so
//! they are not selected today, but this keeps Cap-2 robust.)
//
// TODO(Cap-2): swap to a System.Threading ManualResetEventSlim-backed Parker,
// bundled with the [ThreadStatic] TLS fix.

#[path = "unsupported.rs"]
mod imp;
pub use imp::Parker;
