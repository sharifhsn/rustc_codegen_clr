//! Installed-process entry protocol.
//!
//! The Cargo-bin executable is a stable bootstrap: it takes the shared activation coordination
//! lock before resolving the versioned home driver and retains that lock until the child exits.
//! The home driver independently takes the same lock as its first installed-mode action, validates
//! its compile-time build identity against VERSION and BUNDLE-LOCK, then acquires the per-home
//! lifetime lease before releasing coordination. Thus an image loaded immediately before a home
//! swap either observes its matching home or fails closed; there is no executable-open/lease gap.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::install_transaction::{InstalledHomeLease, LaunchCoordinationLease};

include!(concat!(env!("OUT_DIR"), "/cargo_dotnet_build_identity.rs"));

const BUILD_ID_RECEIPT_PREFIX: &[u8] = b"CARGO_DOTNET_BUILD_ID_RECEIPT_V1_BEGIN:";
const BUILD_ID_RECEIPT_SUFFIX: &[u8] = b":CARGO_DOTNET_BUILD_ID_RECEIPT_V1_END";

pub(crate) fn binary_build_id(bytes: &[u8]) -> Result<String> {
    let mut identities = std::collections::BTreeSet::new();
    for start in memchr::memmem::find_iter(bytes, BUILD_ID_RECEIPT_PREFIX) {
        let value_start = start + BUILD_ID_RECEIPT_PREFIX.len();
        let limit = bytes
            .len()
            .min(value_start + 256 + BUILD_ID_RECEIPT_SUFFIX.len());
        let Some(suffix_offset) = bytes[value_start..limit]
            .windows(BUILD_ID_RECEIPT_SUFFIX.len())
            .position(|window| window == BUILD_ID_RECEIPT_SUFFIX)
        else {
            continue;
        };
        let value = &bytes[value_start..value_start + suffix_offset];
        if value.is_empty()
            || value.len() > 256
            || !value
                .iter()
                .all(|byte| byte.is_ascii_graphic() && !matches!(*byte, b'"' | b'\''))
        {
            continue;
        }
        identities.insert(String::from_utf8(value.to_vec())?);
    }
    if identities.len() != 1 {
        bail!(
            "cargo-dotnet binary contains {} valid embedded build identity receipts (expected exactly one)",
            identities.len()
        );
    }
    Ok(identities.into_iter().next().expect("one identity exists"))
}

pub(crate) enum Entry {
    Continue(Option<InstalledHomeLease>),
    Exit(i32),
}

pub(crate) fn enter() -> Result<Entry> {
    std::hint::black_box(&CARGO_DOTNET_BUILD_ID_BINARY_RECEIPT);
    let home = match crate::mode::detect()? {
        crate::mode::Mode::Dev { .. } => return Ok(Entry::Continue(None)),
        crate::mode::Mode::Installed { home } => home,
    };
    let current = std::env::current_exe().context("locating running cargo-dotnet")?;
    let coordination = LaunchCoordinationLease::acquire(&home)?;
    pre_main_test_barrier()?;
    let inside_home = executable_is_inside_home(&current, &home)?;
    let mutating = raw_command_mutates_home(std::env::args_os());
    if !inside_home {
        if mutating {
            // Setup and bundle-install are the writers themselves. They must not retain the
            // shared side while acquiring activation's exclusive side.
            drop(coordination);
            return Ok(Entry::Continue(None));
        }
        let driver = home_driver(&home);
        if !driver.is_file() {
            bail!("{}", crate::context::missing_install_home_message(&home));
        }
        // Coordination remains alive across pathname resolution, process creation, and the
        // complete child lifetime. The child also validates its embedded identity.
        let mut child = Command::new(&driver);
        child.args(std::env::args_os().skip(1));
        child.env_remove("CARGO_DOTNET_TEST_PRE_MAIN_READY");
        child.env_remove("CARGO_DOTNET_TEST_PRE_MAIN_RELEASE");
        let status = child
            .status()
            .with_context(|| format!("starting installed SDK driver {}", driver.display()))?;
        drop(coordination);
        return Ok(Entry::Exit(status.code().unwrap_or(1)));
    }

    crate::bundle::validate_loaded_driver_identity(&home, EMBEDDED_DRIVER_BUILD_ID)?;
    if mutating {
        drop(coordination);
        return Ok(Entry::Continue(None));
    }
    // Once the lifetime lease is held, coordination can be released without a deletion gap.
    let lease = InstalledHomeLease::acquire(&home)?;
    drop(coordination);
    Ok(Entry::Continue(Some(lease)))
}

fn executable_is_inside_home(executable: &Path, home: &Path) -> Result<bool> {
    let executable = std::fs::canonicalize(executable)
        .with_context(|| format!("resolving running executable {}", executable.display()))?;
    let home = match std::fs::canonicalize(home) {
        Ok(home) => home,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(executable.starts_with(home))
}

fn home_driver(home: &Path) -> PathBuf {
    home.join("bin")
        .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX))
}

fn raw_command_mutates_home(args: impl IntoIterator<Item = OsString>) -> bool {
    let mut args = args.into_iter();
    let _program = args.next();
    let mut words = args
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if words.first().is_some_and(|word| word == "dotnet") {
        words.remove(0);
    }
    words.first().is_some_and(|word| word == "setup")
        || (words.first().is_some_and(|word| word == "bundle")
            && words.get(1).is_some_and(|word| word == "install"))
}

fn pre_main_test_barrier() -> Result<()> {
    let Some(ready) = std::env::var_os("CARGO_DOTNET_TEST_PRE_MAIN_READY") else {
        return Ok(());
    };
    let ready = PathBuf::from(ready);
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&ready)
        .with_context(|| format!("creating pre-main test barrier {}", ready.display()))?;
    use std::io::Write as _;
    output.write_all(b"locked\n")?;
    output.sync_all()?;
    let release = std::env::var_os("CARGO_DOTNET_TEST_PRE_MAIN_RELEASE")
        .map(PathBuf::from)
        .context("pre-main test barrier has no release path")?;
    while !release.exists() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_build_id_requires_one_valid_receipt() {
        let identity = format!("source-sha256:{}", "a".repeat(64));
        let receipt = [
            BUILD_ID_RECEIPT_PREFIX,
            identity.as_bytes(),
            BUILD_ID_RECEIPT_SUFFIX,
        ]
        .concat();
        let mut image = b"unrelated executable prefix".to_vec();
        image.extend_from_slice(&receipt);
        image.extend_from_slice(b"unrelated executable suffix");
        assert_eq!(binary_build_id(&image).unwrap(), identity);

        let conflicting_identity = format!("source-sha256:{}", "b".repeat(64));
        image.extend_from_slice(BUILD_ID_RECEIPT_PREFIX);
        image.extend_from_slice(conflicting_identity.as_bytes());
        image.extend_from_slice(BUILD_ID_RECEIPT_SUFFIX);
        assert!(binary_build_id(&image).is_err());
        assert!(binary_build_id(b"no receipt").is_err());
    }

    #[test]
    fn raw_writer_detection_handles_cargo_and_direct_forms() {
        let args = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
        assert!(raw_command_mutates_home(args(&["cargo-dotnet", "setup"])));
        assert!(raw_command_mutates_home(args(&[
            "cargo-dotnet",
            "dotnet",
            "bundle",
            "install"
        ])));
        assert!(!raw_command_mutates_home(args(&[
            "cargo-dotnet",
            "bundle",
            "create"
        ])));
    }
}
