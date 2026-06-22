//! .NET-specific raw type definitions — PACKAGE A/B keystone.
//!
//! `os/unix/raw.rs` (the deprecated `std::os::unix::raw` aliases) re-exports
//! `super::platform::raw::{pthread_t, blkcnt_t, time_t, blksize_t, dev_t, ino_t,
//! mode_t, nlink_t, off_t}`. Under the `target-family=["unix"]` flip, `platform`
//! is `os/unix/mod.rs`'s per-target `mod platform { ... }` list — which has no
//! dotnet arm by default, so these aliases fail to resolve. The dev.sh injection
//! adds `#[cfg(target_os="dotnet")] pub use crate::os::dotnet::*;` to that list,
//! pointing here.
//!
//! These are the cross-unix `raw_ext` aliases (deprecated since 1.8.0; the values
//! are arbitrary on a platform with no real `stat`/`pthread_t` — we use the
//! widest reasonable widths, matching darwin). No `stat` struct is provided
//! (dotnet's `MetadataExt` does not expose `as_raw_stat`).
#![stable(feature = "raw_ext", since = "1.1.0")]
#![allow(non_camel_case_types)]

#[stable(feature = "raw_ext", since = "1.1.0")]
pub type blkcnt_t = u64;
#[stable(feature = "raw_ext", since = "1.1.0")]
pub type blksize_t = u64;
#[stable(feature = "raw_ext", since = "1.1.0")]
pub type dev_t = u64;
#[stable(feature = "raw_ext", since = "1.1.0")]
pub type ino_t = u64;
#[stable(feature = "raw_ext", since = "1.1.0")]
pub type mode_t = u32;
#[stable(feature = "raw_ext", since = "1.1.0")]
pub type nlink_t = u64;
#[stable(feature = "raw_ext", since = "1.1.0")]
pub type off_t = u64;
#[stable(feature = "raw_ext", since = "1.1.0")]
pub type time_t = i64;

#[stable(feature = "pthread_t", since = "1.8.0")]
pub type pthread_t = usize;
