//! `sys::sync::{Once, OnceState}` for the .NET ("dotnet") platform — REAL Once.
//!
//! Routes to std's GENERIC queue-based `Once` (`sys/sync/once/queue.rs`), the same
//! implementation Windows / generic-Unix / SGX / xous use. It is written purely
//! against `crate::thread::{park, unpark}` + atomics — NO `sys::`/`pal::`/`libc`
//! dependency — so with the dotnet `Parker` now real (see
//! `sys/sync/thread_parking/dotnet.rs`) it compiles and runs unmodified on
//! os=dotnet. This is the Class-D keystone payoff: one real Parker lets the
//! generic Once block-until-Complete on `State::Running` instead of panicking,
//! which is exactly what rayon's lazy global-pool init needs (research:
//! `docs/THREADING_PAL_RESEARCH.md`).

#[path = "queue.rs"]
mod imp;
pub use imp::{Once, OnceState};
