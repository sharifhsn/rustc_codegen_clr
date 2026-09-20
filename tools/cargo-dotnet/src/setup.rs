//! `setup` — provision the toolchain + install home, then warm the PAL natively.
//!
//! The HEAVY provisioning (rustup nightly + components, .NET 10 SDK via dotnet-install.sh,
//! building the backend, populating CARGO_DOTNET_HOME) shells
//! out to the dev-only bash front-end (`feasibility/cargo-dotnet` `cd_setup`, :170-382).
//! That is idiomatic — rustup/curl/cargo are external tools, NOT "the bash CORE" — and
//! is a dev-only `--from-repo` step that does not touch the build/run/pack proof.
//!
//! Two parts ARE native: (1) the matching Rust front-end and staged SDK home are activated as one
//! rollback-capable transaction; and (2) the private-sysroot PAL warm runs the Rust injection
//! engine directly (no `CD_INJECT_ONLY` bash hook), so the same fail-fast injection the build uses
//! is verified before either previous installation backup is discarded.
//!
//! Ports `feasibility/cargo-dotnet:170-382`, with the front-end install + PAL warm native.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

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
    // Bind the installed driver to this checkout revision. Setup intentionally consumes the
    // checkout directly; the legacy shell already owns the copy/build steps and a private mirror
    // would only duplicate the whole source tree in memory and on disk.
    let (source_git_rev, source_tree_sha256) = source_identity(&from_repo)?;
    let driver_build_id = format!("source-sha256:{source_tree_sha256}");

    let home = args
        .home
        .clone()
        .unwrap_or(crate::mode::cargo_dotnet_home()?);
    if home.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) {
        bail!("setup home must be a normalized path: {}", home.display());
    }
    let home_parent = home
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(home_parent)?;
    let planned_home = crate::path_safety::planned_absolute(&home)?;
    crate::path_safety::require_owned_or_empty_sdk_home(&planned_home)?;
    let cargo_home = cargo_home()?;
    let cargo_bin = cargo_home.join("bin");
    std::fs::create_dir_all(&cargo_bin)?;
    let current_exe = std::env::current_exe().context("locating the running cargo-dotnet")?;
    crate::path_safety::reject_ancestor_of(
        &planned_home,
        [
            ("repository", from_repo.clone()),
            ("working directory", std::env::current_dir()?),
            ("running cargo-dotnet", current_exe.clone()),
            ("Cargo home", cargo_home.clone()),
        ],
    )?;
    crate::path_safety::reject_overlap_with(&planned_home, [("repository", from_repo.clone())])?;
    let staged_home_area = tempfile::Builder::new()
        .prefix(".cargo-dotnet-setup-stage-")
        .tempdir_in(home_parent)?;
    let staged_home = staged_home_area.path().join("home");

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
    cmd.env("CARGO_DOTNET_CLI_VERSION", env!("CARGO_PKG_VERSION"));
    cmd.env("CARGO_DOTNET_SOURCE_GIT_REV", &source_git_rev);
    cmd.env("CARGO_DOTNET_SOURCE_TREE_SHA256", &source_tree_sha256);
    cmd.env("CARGO_DOTNET_DRIVER_BUILD_ID", &driver_build_id);
    cmd.arg("--from-repo").arg(&from_repo);
    cmd.arg("--home").arg(&staged_home);
    if let Some(tc) = &args.toolchain {
        cmd.arg("--toolchain").arg(tc);
    }
    if args.skip_toolchain {
        cmd.arg("--skip-toolchain");
    }
    if args.skip_dotnet {
        cmd.arg("--skip-dotnet");
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

    // ---- stage the matching native front-end without touching the active installation ----
    // Build the installed front-end from the same checkout as the backend.
    let crate_dir = from_repo.join("tools/cargo-dotnet");
    if !crate_dir.join("Cargo.toml").is_file() {
        bail!(
            "tools/cargo-dotnet is missing from {}; setup requires the Rust front-end source",
            from_repo.display()
        );
    }
    println!("==> building the matching Rust cargo-dotnet front-end in an isolated root");
    let built_front_end_area = tempfile::Builder::new()
        .prefix("cargo-dotnet-setup-build-")
        .tempdir()?;
    if !cargo_install(&crate_dir, built_front_end_area.path(), &driver_build_id)? {
        bail!(
            "`cargo install --path tools/cargo-dotnet` failed; setup cannot guarantee that \
             the installed command matches the provisioned backend"
        );
    }
    let front_end_source = built_front_end_area
        .path()
        .join("bin")
        .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
    let staged_front_end = stage_running_executable_into(&front_end_source, &cargo_home)?;

    // ---- native PAL warm: run the Rust injection engine once, fail-fast ----
    // Replaces the bash `CD_INJECT_ONLY=1` core hook. We build an installed Context
    // against the freshly-populated home and run the same `inject_all` the build uses,
    // so a broken rust-src / drifted anchor surfaces at setup, not on first build.
    // ---- provision the bundled mycorrhiza_interop_helpers C# project into the staged home ----
    // `interop_helpers::ensure_and_copy` (called on every `cargo dotnet build`/`run`) looks for
    // this project at `Context::paths.interop_helpers_root`, which in Installed mode resolves to
    // `<home>/mycorrhiza_interop_helpers` — but nothing else populates that path, so without this
    // step every real `cargo dotnet` user (anyone NOT running from a dev checkout) silently never
    // gets `Mycorrhiza.Interop.Helpers.dll`, and `mycorrhiza::linq`'s `&`/`|` predicate combinators
    // throw `FileNotFoundException` at runtime. Bash setup already populated `home`, so it's safe
    // to write into it now. If this checkout ships the helper, a copy failure is fatal: reporting
    // success would defer the problem to a runtime-only failure for LINQ users.
    provision_required_assets(&from_repo, &Some(staged_home.clone()))?;
    crate::bundle::seal_install_home(&staged_home, &front_end_source)
        .context("sealing the staged SDK inventory")?;
    // Warm through the staged installed driver before activation.
    let installed_cache_home = crate::context::cache_home_for_sdk_home(&home);
    warm_pal(args, &staged_home, &front_end_source, &installed_cache_home).context(
        "PAL warm failed; setup stopped so the first user build cannot inherit a broken sysroot",
    )?;
    let destination = staged_front_end.destination.clone();
    activate_setup(&staged_home, &home, staged_front_end, || {
        crate::bundle::verify_sealed_install_home(&home)
            .context("validating the activated SDK inventory")
    })?;
    println!(
        "==> activated SDK home {} and cargo-dotnet front-end {}",
        home.display(),
        destination.display()
    );

    Ok(0)
}

fn provision_required_assets(from_repo: &Path, home_override: &Option<PathBuf>) -> Result<()> {
    provision_interop_helpers(from_repo, home_override)
        .context("could not provision the mycorrhiza interop helper project")?;
    provision_sdk_crates(from_repo, home_override).context("could not provision SDK Rust crates")
}

fn provision_sdk_crates(from_repo: &Path, home_override: &Option<PathBuf>) -> Result<()> {
    let home = home_override
        .clone()
        .map_or_else(crate::mode::cargo_dotnet_home, Ok)?;
    let root = home.join("crates");
    for (relative, name) in [
        ("mycorrhiza", "mycorrhiza"),
        ("dotnet_macros", "dotnet_macros"),
        ("crates/rust-dotnet-pinvoke", "rust-dotnet-pinvoke"),
        (
            "crates/rust-dotnet-native-contract-macros",
            "rust-dotnet-native-contract-macros",
        ),
    ] {
        let src = from_repo.join(relative);
        if !src.is_dir() {
            bail!("SDK crate source is missing: {}", src.display());
        }
        copy_dir_overwrite(&src, &root.join(name))
            .with_context(|| format!("provisioning SDK crate {name}"))?;
    }
    println!("==> provisioned SDK Rust crates -> {}", root.display());
    Ok(())
}

fn provision_interop_helpers(from_repo: &Path, home_override: &Option<PathBuf>) -> Result<()> {
    let src = from_repo.join("mycorrhiza_interop_helpers");
    if !src.is_dir() {
        return Ok(());
    }
    let home = home_override
        .clone()
        .map_or_else(crate::mode::cargo_dotnet_home, Ok)?;
    let dest = home.join("mycorrhiza_interop_helpers");
    copy_dir_overwrite(&src, &dest)
        .with_context(|| format!("copying {} -> {}", src.display(), dest.display()))?;
    println!(
        "==> provisioned mycorrhiza_interop_helpers -> {}",
        dest.display()
    );
    Ok(())
}

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
        let target = dest.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir_overwrite(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn source_identity(repo: &Path) -> Result<(String, String)> {
    fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .with_context(|| format!("running git {}", args.join(" ")))?;
        if !output.status.success() {
            bail!("git {} failed", args.join(" "));
        }
        Ok(output.stdout)
    }

    let revision = String::from_utf8(git(repo, &["rev-parse", "HEAD"])?)
        .context("git revision is not UTF-8")?
        .trim()
        .to_owned();
    let status = git(
        repo,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut identity = revision.as_bytes().to_vec();
    identity.push(0);
    identity.extend_from_slice(&status);
    let revision = if status.is_empty() {
        revision
    } else {
        format!("{revision}-dirty")
    };
    Ok((revision, hex_sha256(&identity)))
}

/// Run the native PAL injection once against the promoted installed home. Forcing Installed mode
/// is important when setup itself was launched by `cargo run` from a mutable development checkout:
/// the warm must consume the PAL and backend copied from the private setup snapshot.
fn warm_pal(_args: &SetupArgs, home: &Path, driver: &Path, cache_home: &Path) -> Result<()> {
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

    let status = Command::new(driver)
        .arg("restore")
        .arg(&shell)
        .arg("--release")
        .arg("--backend")
        .arg("native")
        .arg("--dotnet")
        .arg("10")
        .env("CARGO_DOTNET_HOME", home)
        .env("CARGO_DOTNET_CACHE_HOME", cache_home)
        .env("CARGO_DOTNET_BACKEND", "native")
        .status()
        .with_context(|| format!("starting staged SDK driver {}", driver.display()))?;
    if !status.success() {
        bail!("staged SDK driver failed to warm the private PAL sysroot");
    }
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

struct StagedExecutable {
    temporary: tempfile::TempPath,
    destination: PathBuf,
}

/// Copy the selected front-end into a uniquely-created file beside its final destination. The
/// active executable remains untouched until the SDK home is also ready to promote.
fn stage_running_executable_into(source: &Path, cargo_home: &Path) -> Result<StagedExecutable> {
    let bin_dir = cargo_home.join("bin");
    std::fs::create_dir_all(&bin_dir)
        .with_context(|| format!("creating Cargo binary directory {}", bin_dir.display()))?;
    let destination = bin_dir.join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
    if let Ok(metadata) = std::fs::symlink_metadata(&destination)
        && (!metadata.is_file() || metadata.file_type().is_symlink())
    {
        bail!(
            "refusing to replace non-regular cargo-dotnet front-end: {}",
            destination.display()
        );
    }
    let mut temporary = tempfile::Builder::new()
        .prefix(".cargo-dotnet-cli-stage-")
        .tempfile_in(&bin_dir)?;
    let mut input = rust_dotnet_sdk_core::safe_fs::open_regular_nofollow(source)?;
    let permissions = input.metadata()?.permissions();
    std::io::copy(&mut input, temporary.as_file_mut())
        .with_context(|| format!("copying the running cargo-dotnet from {}", source.display()))?;
    temporary.as_file().set_permissions(permissions)?;
    temporary.as_file().sync_all()?;
    Ok(StagedExecutable {
        temporary: temporary.into_temp_path(),
        destination,
    })
}

fn activate_setup<F>(
    staged_home: &Path,
    home: &Path,
    front_end: StagedExecutable,
    validate: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let StagedExecutable {
        temporary,
        destination,
    } = front_end;
    let cli = crate::install_transaction::CliActivation::new(temporary.to_path_buf(), destination);
    let result = crate::install_transaction::activate(
        staged_home,
        home,
        Some(cli),
        crate::install_transaction::RollbackDisposition::RestoreInputs,
        || Ok(()),
        || Ok(()),
        validate,
    );
    drop(temporary);
    result
}

/// `cargo install --path <crate_dir>` into an isolated root using a host cargo.
/// Returns Ok(true) on success.
fn cargo_install(crate_dir: &Path, root: &Path, driver_build_id: &str) -> Result<bool> {
    // Use the host's default cargo; the crate's nested [workspace] keeps it off the
    // rustc_private toolchain. Prefer a stable toolchain if rustup is the driver.
    let cargo = crate::host::inner_cargo();
    let status = Command::new(&cargo)
        .arg("install")
        .arg("--path")
        .arg(crate_dir)
        .arg("--root")
        .arg(root)
        .arg("--force")
        .arg("--locked")
        .env("CARGO_DOTNET_BUILD_ID", driver_build_id)
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

    fn create_required_sources(repo: &Path) {
        for relative in [
            "mycorrhiza",
            "dotnet_macros",
            "crates/rust-dotnet-pinvoke",
            "crates/rust-dotnet-native-contract-macros",
            "mycorrhiza_interop_helpers",
        ] {
            let directory = repo.join(relative);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("sentinel"), relative).unwrap();
        }
    }

    #[test]
    fn running_executable_and_sdk_home_are_promoted_together() {
        let root = temp_root("promote-running-exe");
        let source = root.join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        let cargo_home = root.join("cargo-home");
        let staged_home = root.join("staged-home");
        let home = root.join("active-home");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&staged_home).unwrap();
        std::fs::write(&source, b"already-built-driver").unwrap();
        std::fs::write(staged_home.join("marker"), b"new-sdk").unwrap();

        let staged = stage_running_executable_into(&source, &cargo_home).unwrap();
        let destination = staged.destination.clone();
        assert!(!destination.exists());
        activate_setup(&staged_home, &home, staged, || Ok(())).unwrap();
        assert_eq!(
            destination,
            cargo_home
                .join("bin")
                .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX))
        );
        assert_eq!(std::fs::read(destination).unwrap(), b"already-built-driver");
        assert_eq!(std::fs::read(home.join("marker")).unwrap(), b"new-sdk");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn setup_activation_rolls_back_sdk_home_and_front_end() {
        let root = temp_root("rollback-setup");
        let source = root.join(format!("new-cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        let cargo_home = root.join("cargo-home");
        let destination = cargo_home
            .join("bin")
            .join(format!("cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        let staged_home = root.join("staged-home");
        let home = root.join("active-home");
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&staged_home).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(&source, b"new-cli").unwrap();
        std::fs::write(&destination, b"old-cli").unwrap();
        std::fs::write(staged_home.join("marker"), b"new-sdk").unwrap();
        std::fs::write(home.join("marker"), b"old-sdk").unwrap();
        std::fs::write(
            home.join("VERSION"),
            "schema = 1\nrelease_tag = untagged\nhost_rid = test\ntoolchain = nightly\n",
        )
        .unwrap();
        let staged = stage_running_executable_into(&source, &cargo_home).unwrap();

        let error = activate_setup(&staged_home, &home, staged, || {
            bail!("injected validation failure")
        })
        .unwrap_err();

        assert!(error.to_string().contains("rolled back"), "{error:#}");
        assert_eq!(std::fs::read(home.join("marker")).unwrap(), b"old-sdk");
        assert_eq!(std::fs::read(destination).unwrap(), b"old-cli");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn setup_activation_never_replaces_an_unowned_directory() {
        let root = temp_root("reject-unowned-setup-home");
        let source = root.join(format!("new-cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        let cargo_home = root.join("cargo-home");
        let staged_home = root.join("staged-home");
        let home = root.join("unrelated-home");
        std::fs::create_dir_all(&staged_home).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(&source, b"new-cli").unwrap();
        std::fs::write(staged_home.join("marker"), b"new-sdk").unwrap();
        std::fs::write(home.join("do-not-delete"), b"unrelated").unwrap();
        let staged = stage_running_executable_into(&source, &cargo_home).unwrap();

        let error = activate_setup(&staged_home, &home, staged, || Ok(())).unwrap_err();

        assert!(error.to_string().contains("ownership marker"), "{error:#}");
        assert_eq!(
            std::fs::read(home.join("do-not-delete")).unwrap(),
            b"unrelated"
        );
        assert_eq!(
            std::fs::read(staged_home.join("marker")).unwrap(),
            b"new-sdk"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn required_asset_provisioning_rejects_incomplete_checkout() {
        let repo = temp_root("missing-sdk");
        let home = temp_root("missing-sdk-home");
        std::fs::create_dir_all(&repo).unwrap();

        let error = provision_required_assets(&repo, &Some(home.clone())).unwrap_err();
        assert!(format!("{error:#}").contains("SDK crate source is missing"));

        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn required_asset_provisioning_copies_scaffold_dependencies() {
        let repo = temp_root("sdk-copy");
        let home = temp_root("sdk-copy-home");
        create_required_sources(&repo);

        provision_required_assets(&repo, &Some(home.clone())).unwrap();
        assert!(home.join("crates/mycorrhiza/sentinel").is_file());
        assert!(home.join("crates/dotnet_macros/sentinel").is_file());
        assert!(home.join("crates/rust-dotnet-pinvoke/sentinel").is_file());
        assert!(
            home.join("crates/rust-dotnet-native-contract-macros/sentinel")
                .is_file()
        );
        assert!(home.join("mycorrhiza_interop_helpers/sentinel").is_file());

        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(home);
    }
}
