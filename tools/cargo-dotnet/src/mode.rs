//! Dual-mode detection: DEV (in-repo checkout) vs INSTALLED (CARGO_DOTNET_HOME).
//!
//! Ports `feasibility/cargo-dotnet:60-89,152-162`. The bash decides mode by whether
//! the sibling `_cargo_dotnet_core.sh` is next to the script. The Rust binary uses
//! the same idea on `current_exe()`: if a repo checkout can be located relative to
//! the binary (it lives at `<repo>/tools/cargo-dotnet/target/.../cargo-dotnet`, or a
//! repo is found by walking up), we are in DEV mode; otherwise INSTALLED, sourcing
//! everything from `CARGO_DOTNET_HOME`.
//!
//! Because the must-have user journey is the INSTALLED native pipeline, DEV mode here
//! is primarily a convenience: it lets a fresh `cargo run -p cargo-dotnet` work from a
//! checkout without a prior `setup`. The Docker dev path stays in the bash front-end
//! (see `docker.rs`).

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub const DEFAULT_TOOLCHAIN: &str = "nightly-2026-06-17";

#[derive(Debug, Clone)]
pub enum Mode {
    /// In-repo development: artifacts come from `<repo>/target/release`.
    Dev { repo_root: PathBuf },
    /// Installed: artifacts come from `CARGO_DOTNET_HOME`.
    Installed { home: PathBuf },
}

/// The install home: `$CARGO_DOTNET_HOME` or `$HOME/.cargo-dotnet`.
pub fn cargo_dotnet_home() -> Result<PathBuf> {
    if let Ok(h) = env::var("CARGO_DOTNET_HOME") {
        if !h.is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    let home = env::var("HOME").context("HOME is not set (needed to locate ~/.cargo-dotnet)")?;
    Ok(PathBuf::from(home).join(".cargo-dotnet"))
}

/// Locate an in-repo checkout relative to the running binary, if any. Recognised by
/// the presence of `feasibility/_cargo_dotnet_core.sh` (the bash mode signal) AND
/// `x86_64-unknown-dotnet.json` (the target spec) at a repo root walked up from the
/// binary's location.
fn find_dev_repo() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    // Walk up from the binary: a `cargo run`/`cargo install --path` build lives under
    // `<repo>/tools/cargo-dotnet/target/<profile>/cargo-dotnet`, so the repo root is a
    // few levels up. Probe every ancestor.
    let mut cur: Option<&Path> = exe.parent();
    while let Some(dir) = cur {
        if is_repo_root(dir) {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

fn is_repo_root(dir: &Path) -> bool {
    dir.join("feasibility/_cargo_dotnet_core.sh").is_file()
        && dir.join("x86_64-unknown-dotnet.json").is_file()
}

/// Detect the run mode. An installed home (a provisioned `CARGO_DOTNET_HOME`) takes
/// precedence over an in-repo guess only if the user is clearly outside a checkout;
/// to keep behaviour predictable we mirror the bash: DEV iff a sibling repo core is
/// found, else INSTALLED.
pub fn detect() -> Result<Mode> {
    if let Some(repo_root) = find_dev_repo() {
        return Ok(Mode::Dev { repo_root });
    }
    Ok(Mode::Installed {
        home: cargo_dotnet_home()?,
    })
}

/// The pinned toolchain recorded in `CARGO_DOTNET_HOME/VERSION` (key `toolchain = …`),
/// or the literal default. Ports `read_home_toolchain` (cargo-dotnet:152-162).
pub fn read_home_toolchain(home: &Path) -> String {
    let manifest = home.join("VERSION");
    if let Ok(text) = std::fs::read_to_string(&manifest) {
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("toolchain") {
                let rest = rest.trim_start();
                if let Some(val) = rest.strip_prefix('=') {
                    let val = val.trim();
                    if !val.is_empty() {
                        return val.to_string();
                    }
                }
            }
        }
    }
    DEFAULT_TOOLCHAIN.to_string()
}
