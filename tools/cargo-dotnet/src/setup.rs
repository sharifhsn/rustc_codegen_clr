//! `setup` — STAGED interim.
//!
//! The provisioning logic (rustup nightly + components, .NET 8 SDK, CoreCLR ilasm,
//! building + populating CARGO_DOTNET_HOME, PAL-injecting rust-src) is intricate and
//! lives in the bash front-end (`feasibility/cargo-dotnet` `cd_setup`,
//! :170-382). The Rust binary delegates the heavy provisioning to that bash, then —
//! the one native upgrade over the old "copy a script" step — installs THIS Rust
//! binary to `~/.cargo/bin/cargo-dotnet` via `cargo install --path`, so the installed
//! front-end is the real clap binary (not the bash copy).
//!
//! Ports `feasibility/cargo-dotnet:170-382`, with the front-end install changed.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::cli::SetupArgs;
use crate::mode::Mode;

pub fn run(args: &SetupArgs) -> Result<i32> {
    let mode = crate::mode::detect()?;

    // Resolve the repo to provision from: explicit --from-repo, else the dev checkout.
    let from_repo = resolve_from_repo(args, &mode)?;
    let front_end = from_repo.join("feasibility/cargo-dotnet");
    if !front_end.is_file() {
        bail!(
            "'{}' is not a rustc_codegen_clr checkout (no feasibility/cargo-dotnet)",
            from_repo.display()
        );
    }

    // ---- delegate the provisioning to the bash setup (STAGED) ----
    let mut cmd = Command::new(&front_end);
    cmd.arg("setup");
    cmd.arg("--from-repo").arg(&from_repo);
    if let Some(home) = &args.home {
        cmd.arg("--home").arg(home);
    }
    if let Some(tc) = &args.toolchain {
        cmd.arg("--toolchain").arg(tc);
    }
    if args.skip_toolchain {
        cmd.arg("--skip-toolchain");
    }
    if args.skip_dotnet {
        cmd.arg("--skip-dotnet");
    }
    if args.skip_ilasm {
        cmd.arg("--skip-ilasm");
    }
    if args.force {
        cmd.arg("--force");
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to run bash setup: {}", front_end.display()))?;
    if !status.success() {
        return Ok(status.code().unwrap_or(1));
    }

    // ---- the native upgrade: install THIS Rust binary to ~/.cargo/bin ----
    // Replaces the bash front-end's "copy the cargo-dotnet script to ~/.cargo/bin"
    // with `cargo install --path tools/cargo-dotnet`, so the installed front-end is
    // the real clap binary.
    let crate_dir = from_repo.join("tools/cargo-dotnet");
    if crate_dir.join("Cargo.toml").is_file() {
        println!("==> installing the Rust cargo-dotnet binary (cargo install --path tools/cargo-dotnet)");
        match cargo_install(&crate_dir) {
            Ok(true) => println!("==> installed cargo-dotnet (Rust) -> ~/.cargo/bin/cargo-dotnet"),
            Ok(false) => eprintln!(
                "!! `cargo install --path tools/cargo-dotnet` failed; the bash front-end copy from \
                 setup remains installed. Build/install the Rust binary manually to upgrade."
            ),
            Err(e) => eprintln!("!! could not run cargo install: {e}"),
        }
    } else {
        eprintln!(
            "!! tools/cargo-dotnet not found in the checkout; the bash front-end copy from setup \
             remains the installed front-end."
        );
    }

    Ok(0)
}

fn resolve_from_repo(args: &SetupArgs, mode: &Mode) -> Result<PathBuf> {
    if let Some(p) = &args.from_repo {
        let abs = std::fs::canonicalize(p)
            .with_context(|| format!("--from-repo path does not exist: {}", p.display()))?;
        return Ok(abs);
    }
    match mode {
        Mode::Dev { repo_root } => Ok(repo_root.clone()),
        Mode::Installed { .. } => bail!(
            "setup: --from-repo <path> is required when running the installed front-end (no repo to \
             build from). Re-run from a repo checkout, or pass --from-repo."
        ),
    }
}

/// `cargo install --path <crate_dir>` using a host cargo (not the pinned nightly).
/// Returns Ok(true) on success.
fn cargo_install(crate_dir: &Path) -> Result<bool> {
    // Use the host's default cargo; the crate's nested [workspace] keeps it off the
    // rustc_private toolchain. Prefer a stable toolchain if rustup is the driver.
    let cargo = crate::host::inner_cargo();
    let status = Command::new(&cargo)
        .arg("install")
        .arg("--path")
        .arg(crate_dir)
        .arg("--force")
        .status()
        .with_context(|| format!("failed to launch `{cargo} install`"))?;
    Ok(status.success())
}
