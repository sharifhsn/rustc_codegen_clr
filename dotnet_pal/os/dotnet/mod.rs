//! Platform-specific extensions to `std` for the .NET ("dotnet") platform.
//!
//! This is the `platform` module behind `std::os::unix` for `target_os="dotnet"`
//! under the `target-family=["unix"]` flip (PACKAGE A/B). It mirrors the shape of
//! `os/darwin/mod.rs`: it supplies the `fs::MetadataExt` (`st_*`) the cross-unix
//! `os::unix::fs::MetadataExt` impl delegates to, and the deprecated `raw` type
//! aliases re-exported under `std::os::unix::raw`.
//!
//! `os/unix/mod.rs`'s `mod platform { ... #[cfg(target_os="dotnet")] pub use
//! crate::os::dotnet::*; ... }` line (injected by dev.sh) makes `platform::fs` /
//! `platform::raw` resolve here.
#![stable(feature = "os_dotnet", since = "1.0.0")]
#![doc(cfg(target_os = "dotnet"))]

pub mod fs;

// deprecated, but used for the public re-export under `std::os::unix::raw`.
pub(super) mod raw;
