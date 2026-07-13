//! `setup` — provision the toolchain + install home, then warm the PAL natively.
//!
//! The HEAVY provisioning (rustup nightly + components, .NET 10 SDK via dotnet-install.sh,
//! the CoreCLR ilasm NuGet, building the backend, populating CARGO_DOTNET_HOME) shells
//! out to the dev-only bash front-end (`feasibility/cargo-dotnet` `cd_setup`, :170-382).
//! That is idiomatic — rustup/curl/cargo are external tools, NOT "the bash CORE" — and
//! is a dev-only `--from-repo` step that does not touch the build/run/pack proof.
//!
//! Two parts ARE native: (1) `cargo install --path tools/cargo-dotnet` installs THIS
//! Rust binary to `~/.cargo/bin` (the real clap front-end, not a bash copy); and (2)
//! the private-sysroot PAL warm runs the Rust injection engine directly (no `CD_INJECT_ONLY`
//! bash hook), so the same fail-fast injection the build uses is verified once at setup without
//! modifying ambient rust-src.
//!
//! Ports `feasibility/cargo-dotnet:170-382`, with the front-end install + PAL warm native.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

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
    // The legacy provisioning script can still be invoked directly, where it must install a
    // front-end of its own. In this path, however, the currently-running native executable is the
    // authoritative front-end and is promoted below. Suppress the script's cargo-install fallback
    // and ambient rust-src warm: this caller builds the executable exactly once and prepares a
    // content-addressed private sysroot below.
    cmd.env("CARGO_DOTNET_SKIP_FRONTEND_INSTALL", "1");
    cmd.env("CARGO_DOTNET_SKIP_LEGACY_PAL_WARM", "1");
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
    // A checkout bootstrap runs this command through `cargo run --release`. Reuse that just-built
    // executable instead of compiling the same crate a second time with `cargo install`. An older
    // already-installed front-end still rebuilds from `crate_dir`, preserving setup's guarantee
    // that the installed command matches the checkout being provisioned.
    let crate_dir = from_repo.join("tools/cargo-dotnet");
    if crate_dir.join("Cargo.toml").is_file() {
        let current_exe = std::env::current_exe().context("locating the running cargo-dotnet")?;
        if executable_is_from_repo(&current_exe, &from_repo) {
            let destination = install_running_executable(&current_exe)?;
            println!(
                "==> installed the already-built cargo-dotnet -> {}",
                destination.display()
            );
        } else {
            println!(
                "==> installing the Rust cargo-dotnet binary (cargo install --path tools/cargo-dotnet)"
            );
            if cargo_install(&crate_dir)? {
                println!("==> installed cargo-dotnet (Rust) -> ~/.cargo/bin/cargo-dotnet");
            } else {
                bail!(
                    "`cargo install --path tools/cargo-dotnet` failed; setup cannot guarantee that \
                     the installed command matches the provisioned backend"
                );
            }
        }
    } else {
        bail!(
            "tools/cargo-dotnet is missing from {}; setup requires the Rust front-end source",
            from_repo.display()
        );
    }

    // ---- native PAL warm: run the Rust injection engine once, fail-fast ----
    // Replaces the bash `CD_INJECT_ONLY=1` core hook. We build an installed Context
    // against the freshly-populated home and run the same `inject_all` the build uses,
    // so a broken rust-src / drifted anchor surfaces at setup, not on first build.
    warm_pal(args).context(
        "PAL warm failed; setup stopped so the first user build cannot inherit a broken sysroot",
    )?;

    // ---- provision the bundled mycorrhiza_interop_helpers C# project into the Installed
    // home ----
    // `interop_helpers::ensure_and_copy` (called on every `cargo dotnet build`/`run`) looks for
    // this project at `Context::paths.interop_helpers_root`, which in Installed mode resolves to
    // `<home>/mycorrhiza_interop_helpers` — but nothing else populates that path, so without this
    // step every real `cargo dotnet` user (anyone NOT running from a dev checkout) silently never
    // gets `Mycorrhiza.Interop.Helpers.dll`, and `mycorrhiza::linq`'s `&`/`|` predicate combinators
    // throw `FileNotFoundException` at runtime. Bash setup already populated `home`, so it's safe
    // to write into it now. If this checkout ships the helper, a copy failure is fatal: reporting
    // success would defer the problem to a runtime-only failure for LINQ users.
    provision_required_assets(&from_repo, &args.home)?;

    Ok(0)
}

fn provision_required_assets(from_repo: &Path, home_override: &Option<PathBuf>) -> Result<()> {
    provision_interop_helpers(from_repo, home_override)
        .context("could not provision the mycorrhiza interop helper project")?;

    // Every generated app/lib/plugin manifest names these versioned crates. The build-local Cargo
    // patch redirects them to this installed copy, so omission makes every scaffold fail later.
    provision_sdk_crates(from_repo, home_override).context("could not provision SDK Rust crates")
}

fn provision_sdk_crates(from_repo: &Path, home_override: &Option<PathBuf>) -> Result<()> {
    let home = match home_override {
        Some(h) => h.clone(),
        None => crate::mode::cargo_dotnet_home()?,
    };
    let root = home.join("crates");
    for name in ["mycorrhiza", "dotnet_macros"] {
        let src = from_repo.join(name);
        if !src.is_dir() {
            bail!("SDK crate source is missing: {}", src.display());
        }
        copy_dir_overwrite(&src, &root.join(name))
            .with_context(|| format!("provisioning SDK crate {name}"))?;
    }
    println!("==> provisioned SDK Rust crates -> {}", root.display());
    Ok(())
}

/// Copy `<from_repo>/mycorrhiza_interop_helpers` to `<home>/mycorrhiza_interop_helpers`,
/// overwriting any existing copy (so re-running `setup --force` picks up C# source changes).
fn provision_interop_helpers(from_repo: &Path, home_override: &Option<PathBuf>) -> Result<()> {
    let src = from_repo.join("mycorrhiza_interop_helpers");
    if !src.is_dir() {
        // An older checkout predating this feature — nothing to provision, not an error.
        return Ok(());
    }
    let home = match home_override {
        Some(h) => h.clone(),
        None => crate::mode::cargo_dotnet_home()?,
    };
    let dest = home.join("mycorrhiza_interop_helpers");
    copy_dir_overwrite(&src, &dest)
        .with_context(|| format!("copying {} -> {}", src.display(), dest.display()))?;
    println!(
        "==> provisioned mycorrhiza_interop_helpers -> {}",
        dest.display()
    );
    Ok(())
}

/// Recursively copy `src` into `dest`, skipping `bin`/`obj` (build artifacts, regenerated on
/// first use) and any existing `dest` contents that would otherwise linger after a source file is
/// removed upstream (`dest` is removed first, then repopulated).
fn copy_dir_overwrite(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        std::fs::remove_dir_all(dest)
            .with_context(|| format!("removing stale {}", dest.display()))?;
    }
    std::fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "bin" || name == "obj" {
            continue;
        }
        let src_path = entry.path();
        let dest_path = dest.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir_overwrite(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)
                .with_context(|| format!("copying {}", src_path.display()))?;
        }
    }
    Ok(())
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
    .context("writing PAL warm Cargo.toml")?;
    std::fs::create_dir_all(shell.join("src")).context("creating PAL warm src directory")?;
    std::fs::write(shell.join("src/main.rs"), "fn main() {}\n")
        .context("writing PAL warm target")?;

    let build_args = crate::cli::BuildArgs {
        path: Some(shell),
        release: true,
        debug: false,
        clean: false,
        verbose: false,
        backend: Some("native".to_string()),
        dotnet: "10".to_string(),
        source_link_url: None,
        features: clap_cargo::Features::default(),
        manifest: clap_cargo::Manifest::default(),
        workspace: clap_cargo::Workspace::default(),
        extra: Vec::new(),
        prog_args: Vec::new(),
    };
    let ctx = Context::resolve(&build_args, false)?;
    let _build_lock = crate::build_lock::BuildLock::acquire_crate(&ctx)?;
    let private_sysroot = crate::private_sysroot::prepare(&ctx)?;
    eprintln!(
        "== private PAL sysroot warmed: {} ==",
        private_sysroot.root.display()
    );
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

fn executable_is_from_repo(executable: &Path, repo: &Path) -> bool {
    executable.starts_with(repo)
        && executable
            .file_stem()
            .is_some_and(|name| name == "cargo-dotnet")
}

fn cargo_home() -> Result<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .map_or_else(
            || {
                crate::host::home_dir()
                    .map(|home| home.join(".cargo"))
                    .context("neither CARGO_HOME nor a user home directory is available")
            },
            Ok,
        )
}

/// Atomically promote the executable `cargo run` just built into Cargo's command directory.
fn install_running_executable(source: &Path) -> Result<PathBuf> {
    install_running_executable_into(source, &cargo_home()?)
}

fn install_running_executable_into(source: &Path, cargo_home: &Path) -> Result<PathBuf> {
    let bin_dir = cargo_home.join("bin");
    std::fs::create_dir_all(&bin_dir)
        .with_context(|| format!("creating Cargo binary directory {}", bin_dir.display()))?;
    let destination = bin_dir.join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
    if source == destination {
        return Ok(destination);
    }
    let staging = bin_dir.join(format!(".cargo-dotnet.installing.{}", std::process::id()));
    std::fs::copy(source, &staging).with_context(|| {
        format!(
            "copying the running cargo-dotnet from {} to {}",
            source.display(),
            staging.display()
        )
    })?;
    if destination.exists() {
        std::fs::remove_file(&destination).with_context(|| {
            format!(
                "replacing installed cargo-dotnet at {}",
                destination.display()
            )
        })?;
    }
    std::fs::rename(&staging, &destination).with_context(|| {
        format!(
            "promoting the running cargo-dotnet into {}",
            destination.display()
        )
    })?;
    Ok(destination)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "cargo-dotnet-setup-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn checkout_executable_is_reused_only_for_the_matching_binary() {
        let repo = Path::new("/tmp/rustc_codegen_clr");
        assert!(executable_is_from_repo(
            Path::new("/tmp/rustc_codegen_clr/target/release/cargo-dotnet"),
            repo
        ));
        assert!(!executable_is_from_repo(
            Path::new("/home/user/.cargo/bin/cargo-dotnet"),
            repo
        ));
        assert!(!executable_is_from_repo(
            Path::new("/tmp/rustc_codegen_clr/target/release/linker"),
            repo
        ));
    }

    #[test]
    fn running_executable_is_promoted_without_recompiling() {
        let root = temp_root("promote-running-exe");
        let source = root.join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        let cargo_home = root.join("cargo-home");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&source, b"already-built-driver").unwrap();

        let destination = install_running_executable_into(&source, &cargo_home).unwrap();
        assert_eq!(
            destination,
            cargo_home
                .join("bin")
                .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX))
        );
        assert_eq!(std::fs::read(destination).unwrap(), b"already-built-driver");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn required_asset_provisioning_rejects_incomplete_checkout() {
        let repo = temp_root("missing-sdk");
        let home = temp_root("missing-sdk-home");
        std::fs::create_dir_all(&repo).unwrap();

        let error = provision_required_assets(&repo, &Some(home.clone())).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("could not provision SDK Rust crates")
        );

        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn required_asset_provisioning_copies_scaffold_dependencies() {
        let repo = temp_root("sdk-copy");
        let home = temp_root("sdk-copy-home");
        for relative in ["mycorrhiza", "dotnet_macros", "mycorrhiza_interop_helpers"] {
            let directory = repo.join(relative);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("sentinel"), relative).unwrap();
        }

        provision_required_assets(&repo, &Some(home.clone())).unwrap();
        assert!(home.join("crates/mycorrhiza/sentinel").is_file());
        assert!(home.join("crates/dotnet_macros/sentinel").is_file());
        assert!(home.join("mycorrhiza_interop_helpers/sentinel").is_file());

        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(home);
    }
}
