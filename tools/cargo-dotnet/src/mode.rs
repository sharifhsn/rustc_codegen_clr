//! Dual-mode detection: DEV (in-repo checkout) vs INSTALLED (CARGO_DOTNET_HOME).
//!
//! Ports `feasibility/cargo-dotnet:60-89,152-162`. The bash decides mode by whether
//! the sibling `_cargo_dotnet_core.sh` is next to the script. The Rust binary uses
//! the same idea on `current_exe()`: if a repo checkout can be located relative to
//! the binary (it lives at `<repo>/target/.../cargo-dotnet`, or a
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

/// The execution backend. `Native` runs the whole pipeline on the host in pure Rust
/// (the self-contained user journey); `Docker` shells the in-repo bash front-end (the
/// dev-only path, which legitimately owns the container mount model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Native,
    Docker,
}

impl Backend {
    /// Resolve the backend: an explicit `--backend`/`CARGO_DOTNET_BACKEND` value wins;
    /// otherwise installed defaults to native, dev defaults to docker.
    pub fn resolve(flag: Option<&str>, mode: &Mode) -> anyhow::Result<Self> {
        match flag {
            Some("native") => Ok(Backend::Native),
            Some("docker") => Ok(Backend::Docker),
            Some(other) => {
                anyhow::bail!("unknown CARGO_DOTNET_BACKEND='{other}' (expected: native | docker)")
            }
            None => Ok(match mode {
                Mode::Installed { .. } => Backend::Native,
                Mode::Dev { .. } => Backend::Docker,
            }),
        }
    }
}

/// The install home: `$CARGO_DOTNET_HOME` or `$HOME/.cargo-dotnet`.
pub fn cargo_dotnet_home() -> Result<PathBuf> {
    if let Ok(h) = env::var("CARGO_DOTNET_HOME") {
        if !h.is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    let home = crate::host::home_dir()
        .context("neither HOME nor USERPROFILE is set (needed to locate cargo-dotnet home)")?;
    Ok(home.join(".cargo-dotnet"))
}

/// Locate an in-repo checkout relative to the running binary, if any. Recognised by
/// the presence of `feasibility/_cargo_dotnet_core.sh` (the bash mode signal) AND
/// `x86_64-unknown-dotnet.json` (the target spec) at a repo root walked up from the
/// binary's location.
fn find_dev_repo_from(exe: &Path) -> Option<PathBuf> {
    // Walk up from the binary: a `cargo run`/`cargo install --path` build lives under
    // `<repo>/target/<profile>/cargo-dotnet`, so the repo root is a
    // few levels up. Probe every ancestor.
    find_repo_ancestor(exe.parent()?)
}

/// Find checkout markers in `path` or one of its ancestors. The path need not exist: bundle
/// installation uses this to reject creating a new installed home below a checkout even when the
/// currently-running driver is installed elsewhere.
pub(crate) fn find_repo_ancestor(path: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(path);
    while let Some(dir) = cur {
        if is_repo_root(dir) {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

fn installed_home_for_executable(exe: &Path, home: &Path) -> Result<Option<PathBuf>> {
    let metadata = match std::fs::symlink_metadata(home) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        anyhow::bail!(
            "configured cargo-dotnet home is not a regular directory: {}",
            home.display()
        );
    }
    let canonical_home = std::fs::canonicalize(home)
        .with_context(|| format!("resolving configured cargo-dotnet home {}", home.display()))?;
    let canonical_exe = std::fs::canonicalize(exe)
        .with_context(|| format!("resolving running cargo-dotnet {}", exe.display()))?;
    let Ok(relative_exe) = canonical_exe.strip_prefix(&canonical_home) else {
        return Ok(None);
    };
    if relative_exe.as_os_str().is_empty() {
        anyhow::bail!("running executable resolved to the cargo-dotnet home directory");
    }

    // Retain one home authority while proving both the ownership marker and executable are
    // ordinary descendants. A home nested below a source checkout is still installed: its own
    // explicit authority always outranks an incidental repository marker in an ancestor.
    let home_capability = rust_dotnet_sdk_core::safe_fs::DirectoryCapability::open(home)?;
    let (_, version) = home_capability
        .snapshot_regular(Path::new("VERSION"))
        .with_context(|| {
            format!(
                "installed cargo-dotnet home has no safe VERSION: {}",
                home.display()
            )
        })?;
    let version = std::str::from_utf8(&version).context("installed VERSION is not UTF-8")?;
    if !version.lines().any(|line| {
        line.trim().strip_prefix("schema").is_some_and(|value| {
            value
                .trim_start()
                .strip_prefix('=')
                .is_some_and(|value| value.trim() == "1")
        })
    }) {
        anyhow::bail!("installed cargo-dotnet VERSION has unsupported or missing schema = 1");
    }
    let _ = home_capability.open_regular(relative_exe).with_context(
        || "running cargo-dotnet is not a safe regular file in its configured home",
    )?;
    home_capability.ensure_path_still_bound()?;
    Ok(Some(canonical_home))
}

/// Recognize a directly invoked home driver at `<home>/bin/cargo-dotnet` without requiring the
/// caller to repeat `<home>` through `CARGO_DOTNET_HOME`. The candidate is accepted only through
/// the same retained-capability VERSION/executable proof as an explicitly configured home.
fn installed_home_from_executable(exe: &Path) -> Result<Option<PathBuf>> {
    let Some(bin) = exe.parent() else {
        return Ok(None);
    };
    if bin.file_name().is_none_or(|name| name != "bin") {
        return Ok(None);
    }
    let Some(home) = bin.parent() else {
        return Ok(None);
    };
    // `cargo install --root <temporary>` also produces `<temporary>/bin/cargo-dotnet` while
    // setup is building the driver that will later be sealed into the real SDK home.  The path
    // shape alone is therefore not authority.  Absence of the installed-home marker means this
    // optional inference does not apply; an existing but malformed/link-backed marker still
    // reaches `installed_home_for_executable` and fails closed.
    match std::fs::symlink_metadata(home.join("VERSION")) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    installed_home_for_executable(exe, home)
}

fn is_repo_root(dir: &Path) -> bool {
    dir.join("feasibility/_cargo_dotnet_core.sh").is_file()
        && dir.join("x86_64-unknown-dotnet.json").is_file()
}

/// Detect the run mode. A safely opened executable beneath the configured, provisioned home is
/// unconditionally installed, even when that home happens to sit below checkout markers. Only an
/// executable outside the home is eligible for the ancestor-based development probe.
pub fn detect() -> Result<Mode> {
    let exe = env::current_exe().context("locating running cargo-dotnet")?;
    let home = cargo_dotnet_home()?;
    detect_from(&exe, &home)
}

fn detect_from(exe: &Path, home: &Path) -> Result<Mode> {
    if let Some(home) = installed_home_for_executable(exe, home)? {
        return Ok(Mode::Installed { home });
    }
    if let Some(home) = installed_home_from_executable(exe)? {
        return Ok(Mode::Installed { home });
    }
    if let Some(repo_root) = find_dev_repo_from(exe) {
        return Ok(Mode::Dev { repo_root });
    }
    Ok(Mode::Installed {
        home: home.to_path_buf(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_installed_home_nested_under_repo_outweighs_dev_markers() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("rustc_codegen_clr");
        let home = repo.join("nested-sdk");
        let driver = home
            .join("bin")
            .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        std::fs::create_dir_all(driver.parent().unwrap()).unwrap();
        std::fs::create_dir_all(repo.join("feasibility")).unwrap();
        std::fs::write(repo.join("feasibility/_cargo_dotnet_core.sh"), b"core").unwrap();
        std::fs::write(repo.join("x86_64-unknown-dotnet.json"), b"{}").unwrap();
        std::fs::write(home.join("VERSION"), b"schema = 1\n").unwrap();
        std::fs::write(&driver, b"driver").unwrap();

        let mode = detect_from(&driver, &home).unwrap();
        match mode {
            Mode::Installed { home: detected } => {
                assert_eq!(detected, std::fs::canonicalize(&home).unwrap())
            }
            Mode::Dev { repo_root } => panic!(
                "installed driver beneath repo was misclassified as dev: {}",
                repo_root.display()
            ),
        }
        assert_eq!(find_repo_ancestor(&home), Some(repo));
    }

    #[test]
    fn direct_home_driver_infers_its_sealed_home_without_environment() {
        let temp = tempfile::tempdir().unwrap();
        let configured_elsewhere = temp.path().join("default-home");
        let home = temp.path().join("installed-sdk");
        let driver = home
            .join("bin")
            .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        std::fs::create_dir_all(driver.parent().unwrap()).unwrap();
        std::fs::write(home.join("VERSION"), b"schema = 1\n").unwrap();
        std::fs::write(&driver, b"driver").unwrap();

        let mode = detect_from(&driver, &configured_elsewhere).unwrap();
        match mode {
            Mode::Installed { home: detected } => {
                assert_eq!(detected, std::fs::canonicalize(&home).unwrap())
            }
            Mode::Dev { repo_root } => panic!(
                "direct installed driver was misclassified as dev: {}",
                repo_root.display()
            ),
        }
    }

    #[test]
    fn temporary_cargo_install_root_does_not_override_configured_home() {
        let temp = tempfile::tempdir().unwrap();
        let configured_home = temp.path().join("staged-sdk");
        let temporary_install = temp.path().join("cargo-install-root");
        let driver = temporary_install
            .join("bin")
            .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        std::fs::create_dir_all(&configured_home).unwrap();
        std::fs::write(configured_home.join("VERSION"), b"schema = 1\n").unwrap();
        std::fs::create_dir_all(driver.parent().unwrap()).unwrap();
        std::fs::write(&driver, b"driver").unwrap();

        let mode = detect_from(&driver, &configured_home).unwrap();
        match mode {
            Mode::Installed { home: detected } => assert_eq!(detected, configured_home),
            Mode::Dev { repo_root } => panic!(
                "temporary cargo install root was misclassified as dev: {}",
                repo_root.display()
            ),
        }
    }
}
