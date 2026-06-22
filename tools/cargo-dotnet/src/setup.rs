//! `setup` — provision the toolchain + install home, then warm the PAL natively.
//!
//! The HEAVY provisioning (rustup nightly + components, .NET 8 SDK via dotnet-install.sh,
//! the CoreCLR ilasm NuGet, building the backend, populating CARGO_DOTNET_HOME) shells
//! out to the dev-only bash front-end (`feasibility/cargo-dotnet` `cd_setup`, :170-382).
//! That is idiomatic — rustup/curl/cargo are external tools, NOT "the bash CORE" — and
//! is a dev-only `--from-repo` step that does not touch the build/run/pack proof.
//!
//! Two parts ARE native: (1) `cargo install --path tools/cargo-dotnet` installs THIS
//! Rust binary to `~/.cargo/bin` (the real clap front-end, not a bash copy); and (2)
//! the rust-src PAL warm now runs the Rust [`palinject::inject_all`] engine directly
//! (no `CD_INJECT_ONLY` bash hook), so the same fail-fast injection the build uses is
//! verified once at setup.
//!
//! Ports `feasibility/cargo-dotnet:170-382`, with the front-end install + PAL warm native.

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

    // ---- native PAL warm: run the Rust injection engine once, fail-fast ----
    // Replaces the bash `CD_INJECT_ONLY=1` core hook. We build an installed Context
    // against the freshly-populated home and run the same `inject_all` the build uses,
    // so a broken rust-src / drifted anchor surfaces at setup, not on first build.
    if let Err(e) = warm_pal(args) {
        eprintln!("!! PAL warm skipped/failed (non-fatal at setup; will retry on first build): {e:#}");
    }

    Ok(0)
}

/// Run the native PAL injection once (the warm step). Setup runs from a repo checkout,
/// so the Context resolves in Dev mode against the just-built backend + the repo's
/// `dotnet_pal/` tree — which injects into the SAME toolchain rust-src the installed
/// build later uses (the injection is per-toolchain, not per-mode), and is idempotent.
fn warm_pal(_args: &SetupArgs) -> Result<()> {
    use crate::context::Context;

    // A throwaway crate shell so `resolve_crate_dir`'s Cargo.toml check passes;
    // `inject_all` never reads `crate_dir`.
    let shell = std::env::temp_dir().join("cd_setup_warm_shell");
    std::fs::create_dir_all(&shell).ok();
    std::fs::write(
        shell.join("Cargo.toml"),
        "[package]\nname = \"warm\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n",
    )
    .ok();

    let build_args = crate::cli::BuildArgs {
        path: Some(shell),
        release: true,
        debug: false,
        clean: false,
        verbose: false,
        backend: Some("native".to_string()),
        dotnet: "8".to_string(), // PAL warm targets the default runtime; version-agnostic enough.
        features: clap_cargo::Features::default(),
        manifest: clap_cargo::Manifest::default(),
        workspace: clap_cargo::Workspace::default(),
        extra: Vec::new(),
        prog_args: Vec::new(),
    };
    let ctx = Context::resolve(&build_args, false)?;
    crate::palinject::inject_all(&ctx)?;
    eprintln!("== PAL injection warmed (native) ==");
    Ok(())
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
