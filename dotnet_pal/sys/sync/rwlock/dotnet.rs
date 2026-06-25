//! `sys::sync::RwLock` for the .NET ("dotnet") platform — REAL reader/writer lock.
//!
//! Routes to std's GENERIC queue-based `RwLock` (`sys/sync/rwlock/queue.rs`), the
//! same implementation generic-Unix / win7 / SGX / xous use. It is written purely
//! against `crate::thread::{park, unpark}` + atomics — NO `sys::`/`pal::`/`libc`
//! dependency — so with the dotnet `Parker` now real (see
//! `sys/sync/thread_parking/dotnet.rs`) it compiles and runs unmodified on
//! os=dotnet. (research: `docs/THREADING_PAL_RESEARCH.md`.)

#[path = "queue.rs"]
mod imp;
pub use imp::RwLock;
