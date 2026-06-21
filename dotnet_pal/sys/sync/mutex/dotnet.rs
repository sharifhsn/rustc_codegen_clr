//! `sys::sync::Mutex` for the .NET ("dotnet") platform — Cap-1 foundation arm.
//!
//! Injected as the FIRST `cfg_select!` arm of `sys/sync/mutex/mod.rs` so that
//! when `families=["unix"]` is flipped at Cap-2, the `pthread` arm (which depends
//! on `sys::pal::unix`, absent for os=dotnet) never wins. With `families` UNSET
//! today this is a pure no-op defensive arm: dotnet already falls through to
//! `no_threads` (the futex arm keys on an explicit `target_os` allowlist that
//! dotnet misses, and the pthread arm keys on `target_family="unix"` which is
//! off), so this arm must be AT LEAST as complete as the `no_threads` fallback it
//! shadows — and it IS, because it re-uses the verbatim `no_threads.rs` source.
//!
//! Cap-1 is correct-for-single-managed-thread only: the dotnet PAL is one managed
//! thread today and `thread_local` is process-global (per MEMORY). REAL
//! `System.Threading`-backed locks (a `SemaphoreSlim(1,1)` Mutex — NOT re-entrant
//! `Monitor` — etc.) are MEANINGLESS until the `[ThreadStatic]` TLS fix lands and
//! must be bundled with it as a Cap-2 unit. So this deliberately mirrors
//! `no_threads` rather than reaching for `Monitor`.
//
// TODO(Cap-2): swap to a System.Threading SemaphoreSlim(1,1)-backed Mutex,
// bundled with the [ThreadStatic] TLS fix.

#[path = "no_threads.rs"]
mod imp;
pub use imp::Mutex;
