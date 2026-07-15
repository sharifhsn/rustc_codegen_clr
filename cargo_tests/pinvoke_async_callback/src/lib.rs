//! Product-shaped safe facade used by both the native-only and managed-job acceptances.

mod async_callback;
mod native;

pub use async_callback::{Registration, StopFailure, copy_utf16, live_workers};
