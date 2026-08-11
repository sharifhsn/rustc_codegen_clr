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
use rust_dotnet_sdk_core::safe_fs::{DirectoryCapability, TreeWalkNode};
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
    // Open and fingerprint every checkout input before handing control to the legacy shell.
    // The resulting bytes are materialized into a private source tree. Every shell build, copy,
    // native front-end build, and bundled-crate publication below consumes that tree rather than
    // resolving the caller's mutable checkout again.
    let source_authority = SetupSourceAuthority::capture(&from_repo)?;
    let source_git = source_authority
        .git_identity
        .as_ref()
        .context("setup source authority has no Git identity")?;
    let source_git_rev = source_git.recorded_revision();
    let source_release_tag = source_git.recorded_release_tag();
    let source_git_tag = source_git.recorded_git_tag();
    let source_tree_sha256 = source_authority.snapshot.digest.clone();
    let driver_build_id = format!("source-sha256:{source_tree_sha256}");
    let source_snapshot_area = tempfile::Builder::new()
        .prefix("cargo-dotnet-setup-source-")
        .tempdir()?;
    let immutable_repo = source_snapshot_area.path().join("repo");
    source_authority.materialize(&immutable_repo)?;
    source_authority.verify_materialized(&immutable_repo)?;
    let immutable_front_end = immutable_repo.join("feasibility/cargo-dotnet");

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
    if crate::install_transaction::recover(&planned_home)? {
        eprintln!(
            "==> recovered an interrupted SDK setup transaction for {}",
            planned_home.display()
        );
    }
    crate::path_safety::require_owned_or_empty_sdk_home(&planned_home)?;
    let cargo_home = cargo_home()?;
    let cargo_bin = cargo_home.join("bin");
    std::fs::create_dir_all(&cargo_bin)?;
    let current_exe = std::env::current_exe().context("locating the running cargo-dotnet")?;
    crate::path_safety::reject_ancestor_of(
        &planned_home,
        [
            ("repository", from_repo.clone()),
            ("private setup source snapshot", immutable_repo.clone()),
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
    let mut cmd = Command::new(&immutable_front_end);
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
    cmd.env("CARGO_DOTNET_SOURCE_RELEASE_TAG", &source_release_tag);
    cmd.env("CARGO_DOTNET_SOURCE_GIT_TAG", &source_git_tag);
    cmd.env("CARGO_DOTNET_SOURCE_TREE_SHA256", &source_tree_sha256);
    cmd.env("CARGO_DOTNET_DRIVER_BUILD_ID", &driver_build_id);
    cmd.arg("--from-repo").arg(&immutable_repo);
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
    let status = cmd.status().with_context(|| {
        format!(
            "failed to run bash setup from private snapshot: {}",
            immutable_front_end.display()
        )
    })?;
    if !status.success() {
        return Ok(status.code().unwrap_or(1));
    }

    // ---- stage the matching native front-end without touching the active installation ----
    // Build the installed front-end from the same private byte snapshot as the backend. Reusing
    // the currently-running executable would be safe only when its build receipt could be bound to
    // these exact source bytes; rebuilding once during setup is the simpler closed proof.
    let crate_dir = immutable_repo.join("tools/cargo-dotnet");
    if !crate_dir.join("Cargo.toml").is_file() {
        bail!(
            "tools/cargo-dotnet is missing from {}; setup requires the Rust front-end source",
            immutable_repo.display()
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
    provision_required_assets_from_authority(&source_authority, &Some(staged_home.clone()))?;
    crate::bundle::seal_install_home(&staged_home, &front_end_source)
        .context("sealing the staged SDK inventory")?;
    // Warm through the exact snapshot-built installed driver before activation. The setup caller
    // may have a different embedded build receipt, so using its in-process Context would either
    // weaken installed identity validation or falsely reject an otherwise coherent staged SDK.
    let installed_cache_home = crate::context::cache_home_for_sdk_home(&home);
    warm_pal(args, &staged_home, &front_end_source, &installed_cache_home).context(
        "PAL warm failed; setup stopped so the first user build cannot inherit a broken sysroot",
    )?;
    source_authority.verify_materialized(&immutable_repo).context(
        "private SDK source snapshot changed while setup warmed the PAL sysroot; rolled back the staged installation",
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

#[cfg(test)]
fn provision_required_assets(from_repo: &Path, home_override: &Option<PathBuf>) -> Result<()> {
    let authority = SetupSourceAuthority::capture_unversioned(from_repo)?;
    provision_required_assets_from_authority(&authority, home_override)
}

const SETUP_SOURCE_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "x86_64-unknown-dotnet.json",
    "feasibility/cargo-dotnet",
    "feasibility/_cargo_dotnet_core.sh",
    // `nuget.rs` embeds this source with include_str!, so it is a compile-time input even though
    // the rest of cargo_tests is intentionally outside the product snapshot.
    "cargo_tests/spinacz/src/reflect.rs",
];

const SETUP_SOURCE_TREES: &[(&str, &[&str])] = &[
    ("src", &[]),
    ("cilly", &["target"]),
    ("dotnet_aot", &["target"]),
    ("crates/rust-dotnet-sdk-core", &["target"]),
    ("crates/rust-dotnet-assets", &["target"]),
    ("crates/rust-dotnet-bindgen", &["target"]),
    ("tools/cargo-dotnet", &["target"]),
    ("dotnet_pal", &[]),
    ("dotnet_overlays", &["target"]),
    ("msbuild", &["bin", "obj"]),
];

const REQUIRED_SOURCE_TREES: [(&str, &str, &[&str]); 5] = [
    (
        "mycorrhiza_interop_helpers",
        "mycorrhiza_interop_helpers",
        &["bin", "obj"],
    ),
    ("mycorrhiza", "crates/mycorrhiza", &["target"]),
    ("dotnet_macros", "crates/dotnet_macros", &["target"]),
    (
        "crates/rust-dotnet-pinvoke",
        "crates/rust-dotnet-pinvoke",
        &["target"],
    ),
    (
        "crates/rust-dotnet-native-contract-macros",
        "crates/rust-dotnet-native-contract-macros",
        &["target"],
    ),
];

#[derive(Debug)]
struct ProvisionedFile {
    relative: PathBuf,
    bytes: Vec<u8>,
    permissions: std::fs::Permissions,
}

#[derive(Debug)]
struct ProvisionedSnapshot {
    directories: Vec<PathBuf>,
    files: Vec<ProvisionedFile>,
    digest: String,
}

#[derive(Debug)]
struct SetupSourceAuthority {
    snapshot: ProvisionedSnapshot,
    git_identity: Option<GitIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GitIdentity {
    head: String,
    head_ref: String,
    exact_release_tag: Option<String>,
    status_sha256: String,
    dirty: bool,
}

impl GitIdentity {
    fn recorded_revision(&self) -> String {
        if self.dirty {
            format!("{}-dirty", self.head)
        } else {
            self.head.clone()
        }
    }

    fn recorded_release_tag(&self) -> String {
        if self.dirty {
            "untagged-dirty".to_string()
        } else {
            // A local tag is useful provenance, but it is not a release attestation. Only the
            // release workflow, which verifies the annotated/signature policy separately, may
            // populate a trusted rust-dotnet-v* release_tag.
            "untagged".to_string()
        }
    }

    fn recorded_git_tag(&self) -> String {
        self.exact_release_tag
            .clone()
            .unwrap_or_else(|| "untagged".to_string())
    }
}

impl SetupSourceAuthority {
    fn capture(from_repo: &Path) -> Result<Self> {
        Self::capture_with_git_hook(from_repo, &mut |_| {})
    }

    #[cfg(test)]
    fn capture_with_hook(
        from_repo: &Path,
        after_source_snapshot: &mut dyn FnMut(&Path),
    ) -> Result<Self> {
        Self::capture_internal(from_repo, false, after_source_snapshot)
    }

    fn capture_with_git_hook(
        from_repo: &Path,
        after_source_snapshot: &mut dyn FnMut(&Path),
    ) -> Result<Self> {
        Self::capture_internal(from_repo, true, after_source_snapshot)
    }

    #[cfg(test)]
    fn capture_unversioned(from_repo: &Path) -> Result<Self> {
        Self::capture_internal(from_repo, false, &mut |_| {})
    }

    fn capture_internal(
        from_repo: &Path,
        require_git: bool,
        after_source_snapshot: &mut dyn FnMut(&Path),
    ) -> Result<Self> {
        let source = DirectoryCapability::open(from_repo)
            .context("opening one retained SDK checkout capability")?;
        let git_before = require_git
            .then(|| read_git_identity(from_repo))
            .transpose()?;
        source
            .ensure_path_still_bound()
            .context("SDK checkout pathname changed while setup read its Git identity")?;
        let before = snapshot_setup_sources(&source, false, &mut |_| {})?;
        source
            .ensure_path_still_bound()
            .context("SDK checkout pathname changed before setup captured its source bytes")?;
        let snapshot = snapshot_setup_sources(&source, true, after_source_snapshot)?;
        after_source_snapshot(Path::new(".git-identity-revalidate"));
        source
            .ensure_path_still_bound()
            .context("SDK checkout pathname changed while setup captured its source authority")?;
        let verified = snapshot_setup_sources(&source, false, &mut |_| {})?;
        source
            .ensure_path_still_bound()
            .context("SDK checkout pathname changed while setup verified its source authority")?;
        if before.digest != snapshot.digest || snapshot.digest != verified.digest {
            bail!(
                "SDK setup sources changed while their immutable snapshot was captured; retry setup from one stable checkout revision"
            );
        }
        let git_after = require_git
            .then(|| read_git_identity(from_repo))
            .transpose()?;
        source
            .ensure_path_still_bound()
            .context("SDK checkout pathname changed while setup revalidated its Git identity")?;
        if git_before != git_after {
            bail!(
                "SDK checkout Git identity changed while its immutable source snapshot was captured; retry setup from one stable checkout revision"
            );
        }
        Ok(Self {
            snapshot,
            git_identity: git_after,
        })
    }

    fn materialize(&self, destination: &Path) -> Result<()> {
        publish_source_snapshot(destination, &self.snapshot)
    }

    fn verify_materialized(&self, destination: &Path) -> Result<()> {
        let capability = DirectoryCapability::open(destination)
            .context("opening materialized setup source snapshot")?;
        let current = snapshot_setup_sources(&capability, false, &mut |_| {})?;
        capability.ensure_path_still_bound()?;
        if current.digest != self.snapshot.digest {
            bail!("private SDK setup source snapshot changed while it was consumed");
        }
        Ok(())
    }
}

fn provision_required_assets_from_authority(
    authority: &SetupSourceAuthority,
    home_override: &Option<PathBuf>,
) -> Result<()> {
    let home = match home_override {
        Some(h) => h.clone(),
        None => crate::mode::cargo_dotnet_home()?,
    };
    publish_required_assets(&home, &authority.snapshot)?;
    println!(
        "==> provisioned mycorrhiza_interop_helpers -> {}",
        home.join("mycorrhiza_interop_helpers").display()
    );
    println!(
        "==> provisioned SDK Rust crates -> {}",
        home.join("crates").display()
    );
    Ok(())
}

fn snapshot_setup_sources(
    source: &DirectoryCapability,
    retain_contents: bool,
    after_source_snapshot: &mut dyn FnMut(&Path),
) -> Result<ProvisionedSnapshot> {
    let mut hash = Sha256::new();
    hash.update(b"cargo-dotnet-setup-source-authority-v3\0");
    let mut directories = Vec::new();
    let mut files = Vec::new();
    for relative in SETUP_SOURCE_FILES {
        let relative = Path::new(relative);
        let (_, mut file) = source.open_regular(relative).with_context(|| {
            format!(
                "required setup source file is missing or unsafe: {}",
                source.root().join(relative).display()
            )
        })?;
        let bytes = rust_dotnet_sdk_core::safe_fs::read_opened_regular(
            &mut file,
            &source.root().join(relative),
        )?;
        hash.update(b"setup-file\0");
        hash_provision_path(relative, &mut hash);
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
        if retain_contents {
            files.push(ProvisionedFile {
                relative: relative.to_path_buf(),
                bytes,
                permissions: file.metadata()?.permissions(),
            });
        }
        after_source_snapshot(relative);
    }
    for (source_relative, excluded) in SETUP_SOURCE_TREES {
        let source_relative = Path::new(source_relative);
        let tree = source.subdirectory(source_relative).with_context(|| {
            format!(
                "required setup source tree is missing or unsafe: {}",
                source.root().join(source_relative).display()
            )
        })?;
        hash.update(b"setup-tree\0");
        hash_provision_path(source_relative, &mut hash);
        let mut tree_directories = Vec::new();
        let mut tree_files = Vec::new();
        hash_source_tree(
            &tree,
            excluded,
            &mut hash,
            retain_contents.then_some((&mut tree_directories, &mut tree_files)),
        )?;
        if retain_contents {
            directories.push(source_relative.to_path_buf());
            directories.extend(
                tree_directories
                    .into_iter()
                    .map(|relative| source_relative.join(relative)),
            );
            files.extend(tree_files.into_iter().map(|file| ProvisionedFile {
                relative: source_relative.join(file.relative),
                bytes: file.bytes,
                permissions: file.permissions,
            }));
        }
        after_source_snapshot(source_relative);
    }

    for (source_relative, _, excluded) in REQUIRED_SOURCE_TREES {
        let source_relative = Path::new(source_relative);
        let tree = source.subdirectory(source_relative).with_context(|| {
            format!(
                "required SDK source tree is missing or unsafe: {}",
                source.root().join(source_relative).display()
            )
        })?;
        hash.update(b"provisioned-tree\0");
        hash_provision_path(source_relative, &mut hash);
        let mut tree_directories = Vec::new();
        let mut tree_files = Vec::new();
        hash_source_tree(
            &tree,
            excluded,
            &mut hash,
            retain_contents.then_some((&mut tree_directories, &mut tree_files)),
        )?;
        if retain_contents {
            directories.push(source_relative.to_path_buf());
            directories.extend(
                tree_directories
                    .into_iter()
                    .map(|relative| source_relative.join(relative)),
            );
            files.extend(tree_files.into_iter().map(|file| ProvisionedFile {
                relative: source_relative.join(file.relative),
                bytes: file.bytes,
                permissions: file.permissions,
            }));
        }
        after_source_snapshot(source_relative);
    }
    Ok(ProvisionedSnapshot {
        directories,
        files,
        digest: format!("{:x}", hash.finalize()),
    })
}

fn hash_source_tree(
    tree: &DirectoryCapability,
    excluded: &[&str],
    hash: &mut Sha256,
    mut retained: Option<(&mut Vec<PathBuf>, &mut Vec<ProvisionedFile>)>,
) -> Result<()> {
    tree.walk_regular_tree(excluded, &mut |relative, node| {
        match node {
            TreeWalkNode::DirectoryEnter(_) => {
                hash.update(b"directory\0");
                hash_provision_path(relative, hash);
                if let Some((directories, _)) = retained.as_mut() {
                    directories.push(relative.to_path_buf());
                }
            }
            TreeWalkNode::File(file) => {
                let bytes = rust_dotnet_sdk_core::safe_fs::read_opened_regular(
                    file,
                    &tree.root().join(relative),
                )?;
                hash.update(b"file\0");
                hash_provision_path(relative, hash);
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(&bytes);
                if let Some((_, files)) = retained.as_mut() {
                    files.push(ProvisionedFile {
                        relative: relative.to_path_buf(),
                        bytes,
                        permissions: file.metadata()?.permissions(),
                    });
                }
            }
            TreeWalkNode::DirectoryLeave(_) => hash.update(b"leave\0"),
        }
        Ok(())
    })
}

fn hash_provision_path(path: &Path, hash: &mut Sha256) {
    let encoded = path.as_os_str().as_encoded_bytes();
    hash.update((encoded.len() as u64).to_le_bytes());
    hash.update(encoded);
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn publish_source_snapshot(destination: &Path, snapshot: &ProvisionedSnapshot) -> Result<()> {
    std::fs::create_dir(destination).with_context(|| {
        format!(
            "creating private SDK setup source snapshot {}",
            destination.display()
        )
    })?;
    for relative in &snapshot.directories {
        std::fs::create_dir_all(destination.join(relative))?;
    }
    let capability = DirectoryCapability::open(destination)?;
    for file in &snapshot.files {
        capability.publish_bytes(&file.relative, &file.bytes)?;
        let (_, published) = capability.open_regular(&file.relative)?;
        published.set_permissions(file.permissions.clone())?;
        published.sync_all()?;
    }
    capability.ensure_path_still_bound()?;
    Ok(())
}

fn publish_required_assets(home: &Path, snapshot: &ProvisionedSnapshot) -> Result<()> {
    std::fs::create_dir_all(home)?;
    let home = std::fs::canonicalize(home)?;
    for (source_relative, destination_relative, _) in REQUIRED_SOURCE_TREES {
        let destination = home.join(destination_relative);
        match std::fs::symlink_metadata(&destination) {
            Ok(metadata)
                if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata)
                    || !metadata.is_dir() =>
            {
                bail!(
                    "SDK copy destination is not a regular directory: {}",
                    destination.display()
                )
            }
            Ok(_) => crate::path_safety::remove_dir_all_within(&home, &destination)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        std::fs::create_dir_all(&destination)?;
        for relative in &snapshot.directories {
            if let Ok(relative) = relative.strip_prefix(Path::new(source_relative)) {
                std::fs::create_dir_all(destination.join(relative))?;
            }
        }
    }
    let destination = DirectoryCapability::open(&home)?;
    for (source_relative, destination_relative, _) in REQUIRED_SOURCE_TREES {
        let source_relative = Path::new(source_relative);
        for file in &snapshot.files {
            let Ok(relative) = file.relative.strip_prefix(source_relative) else {
                continue;
            };
            let destination_relative = Path::new(destination_relative).join(relative);
            destination.publish_bytes(&destination_relative, &file.bytes)?;
            let (_, published) = destination.open_regular(&destination_relative)?;
            published.set_permissions(file.permissions.clone())?;
            published.sync_all()?;
        }
    }
    Ok(())
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

fn read_git_identity(repo: &Path) -> Result<GitIdentity> {
    fn output(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
        let result = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .with_context(|| format!("running git {}", args.join(" ")))?;
        if !result.status.success() {
            bail!(
                "git {} failed while binding setup source identity: {}",
                args.join(" "),
                String::from_utf8_lossy(&result.stderr).trim()
            );
        }
        Ok(result.stdout)
    }

    fn text(repo: &Path, args: &[&str]) -> Result<String> {
        let value = String::from_utf8(output(repo, args)?)
            .with_context(|| format!("git {} returned non-UTF-8 identity", args.join(" ")))?;
        Ok(value.trim().to_string())
    }

    let canonical_repo = std::fs::canonicalize(repo)?;
    let top = std::fs::canonicalize(text(repo, &["rev-parse", "--show-toplevel"])?)?;
    if top != canonical_repo {
        bail!(
            "setup --from-repo must name the Git worktree root (got {}, root is {})",
            canonical_repo.display(),
            top.display()
        );
    }
    let head = text(repo, &["rev-parse", "--verify", "HEAD"])?;
    let head_ref = text(repo, &["symbolic-ref", "-q", "--short", "HEAD"])
        .unwrap_or_else(|_| "DETACHED".to_string());
    let status = output(
        repo,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let dirty = !status.is_empty();
    let status_sha256 = hex_sha256(&status);
    let tags = text(
        repo,
        &["tag", "--points-at", "HEAD", "--list", "rust-dotnet-v*"],
    )?;
    let tags = tags
        .lines()
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if tags.len() > 1 {
        bail!(
            "HEAD has multiple rust-dotnet release tags: {}",
            tags.join(", ")
        );
    }
    Ok(GitIdentity {
        head,
        head_ref,
        exact_release_tag: tags.into_iter().next(),
        status_sha256,
        dirty,
    })
}

#[cfg(test)]
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
    activate_setup_with_hook(staged_home, home, front_end, || Ok(()), validate)
}

fn activate_setup_with_hook<F, G>(
    staged_home: &Path,
    home: &Path,
    front_end: StagedExecutable,
    before_backup: F,
    validate: G,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
    G: FnOnce() -> Result<()>,
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
        before_backup,
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
        for relative in SETUP_SOURCE_FILES {
            let file = repo.join(relative);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, format!("setup source {relative}\n")).unwrap();
        }
        for (relative, _) in SETUP_SOURCE_TREES {
            let directory = repo.join(relative);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                directory.join("setup-source-sentinel"),
                format!("setup tree {relative}\n"),
            )
            .unwrap();
        }
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

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {} failed", args.join(" "));
    }

    fn initialize_git(repo: &Path) {
        git(repo, &["init", "-q"]);
        git(
            repo,
            &["config", "user.email", "cargo-dotnet@example.invalid"],
        );
        git(repo, &["config", "user.name", "cargo-dotnet tests"]);
        git(repo, &["add", "."]);
        git(repo, &["commit", "-q", "-m", "captured revision"]);
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
    fn setup_revalidates_the_exact_home_moved_to_backup() {
        let root = temp_root("setup-home-swap");
        let source = root.join(format!("new-cargo-dotnet{}", std::env::consts::EXE_SUFFIX));
        let cargo_home = root.join("cargo-home");
        let staged_home = root.join("staged-home");
        let home = root.join("active-home");
        std::fs::create_dir_all(&staged_home).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(&source, b"new-cli").unwrap();
        std::fs::write(staged_home.join("marker"), b"new-sdk").unwrap();
        std::fs::write(
            home.join("VERSION"),
            "schema = 1\nrelease_tag = untagged\nhost_rid = test\ntoolchain = nightly\n",
        )
        .unwrap();
        let staged = stage_running_executable_into(&source, &cargo_home).unwrap();
        let swapped_home = home.clone();

        let error = activate_setup_with_hook(
            &staged_home,
            &home,
            staged,
            move || {
                std::fs::remove_dir_all(&swapped_home)?;
                std::fs::create_dir(&swapped_home)?;
                std::fs::write(swapped_home.join("do-not-delete"), b"swapped-unrelated")?;
                Ok(())
            },
            || Ok(()),
        )
        .unwrap_err();

        assert!(error.to_string().contains("rolled back"), "{error:#}");
        assert_eq!(
            std::fs::read(home.join("do-not-delete")).unwrap(),
            b"swapped-unrelated"
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
        assert!(
            error
                .to_string()
                .contains("required setup source file is missing")
        );

        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(unix)]
    #[test]
    fn sdk_copy_rejects_file_symlinks_without_reading_outside_source() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        let outside = temp.path().join("outside-secret");
        create_required_sources(&source);
        std::fs::write(&outside, b"do-not-copy").unwrap();
        symlink(&outside, source.join("mycorrhiza/injected.rs")).unwrap();

        let error = provision_required_assets(&source, &Some(destination.clone())).unwrap_err();
        assert!(
            format!("{error:#}").contains("without following links")
                || format!("{error:#}").contains("symbolic links"),
            "{error:#}"
        );
        assert_eq!(std::fs::read(&outside).unwrap(), b"do-not-copy");
        assert!(!destination.join("crates/mycorrhiza/injected.rs").exists());
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

    #[cfg(unix)]
    #[test]
    fn immutable_snapshot_survives_same_path_checkout_replacement_after_capture() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let replacement = temp.path().join("replacement");
        let original = temp.path().join("repo-original");
        let home = temp.path().join("home");
        create_required_sources(&repo);
        create_required_sources(&replacement);
        std::fs::write(replacement.join("mycorrhiza/sentinel"), b"outside revision").unwrap();
        let authority = SetupSourceAuthority::capture_unversioned(&repo).unwrap();
        std::fs::rename(&repo, &original).unwrap();
        std::fs::rename(&replacement, &repo).unwrap();

        provision_required_assets_from_authority(&authority, &Some(home.clone())).unwrap();
        assert_eq!(
            std::fs::read(home.join("crates/mycorrhiza/sentinel")).unwrap(),
            b"mycorrhiza"
        );
    }

    #[test]
    fn required_asset_snapshot_rejects_between_crate_mixed_revisions() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        create_required_sources(&repo);
        let changed = repo.join("dotnet_macros/sentinel");
        let mut mutated = false;

        let error = SetupSourceAuthority::capture_with_hook(&repo, &mut |finished| {
            if !mutated && finished == Path::new("mycorrhiza") {
                std::fs::write(&changed, b"next revision").unwrap();
                mutated = true;
            }
        })
        .unwrap_err();

        assert!(mutated);
        assert!(
            error.to_string().contains("immutable snapshot"),
            "{error:#}"
        );
        assert!(!home.join("crates/dotnet_macros/sentinel").exists());
    }

    #[cfg(unix)]
    #[test]
    fn setup_source_authority_ignores_checkout_swap_after_snapshot_handoff() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let original = temp.path().join("repo-original");
        let replacement = temp.path().join("replacement");
        let home = temp.path().join("home");
        create_required_sources(&repo);
        create_required_sources(&replacement);
        std::fs::write(
            replacement.join("dotnet_pal/setup-source-sentinel"),
            b"replacement",
        )
        .unwrap();
        let authority = SetupSourceAuthority::capture_unversioned(&repo).unwrap();

        // The shell receives the private snapshot, so rebinding the caller's checkout after capture
        // cannot affect either legacy outputs or the required crate copy.
        std::fs::rename(&repo, &original).unwrap();
        std::fs::rename(&replacement, &repo).unwrap();
        provision_required_assets_from_authority(&authority, &Some(home.clone())).unwrap();

        assert_eq!(
            std::fs::read(home.join("crates/dotnet_macros/sentinel")).unwrap(),
            b"dotnet_macros"
        );
    }

    #[cfg(unix)]
    #[test]
    fn legacy_cargo_wrapper_cannot_taint_outputs_from_mutated_original_checkout() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original");
        let private = temp.path().join("private");
        let staged_home = temp.path().join("staged-home");
        let user_home = temp.path().join("user-home");
        let fake_bin = temp.path().join("fake-bin");
        create_required_sources(&original);
        for name in [
            "RustDotnet.targets",
            "RustDotnet.props",
            "RustDotnet.Containers.cs",
        ] {
            std::fs::write(original.join("msbuild").join(name), name).unwrap();
        }
        let product_launcher =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../feasibility/cargo-dotnet");
        std::fs::copy(&product_launcher, original.join("feasibility/cargo-dotnet")).unwrap();
        std::fs::set_permissions(
            original.join("feasibility/cargo-dotnet"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        let authority = SetupSourceAuthority::capture_unversioned(&original).unwrap();
        authority.materialize(&private).unwrap();
        authority.verify_materialized(&private).unwrap();

        std::fs::create_dir(&fake_bin).unwrap();
        let cargo_wrapper = fake_bin.join("cargo");
        std::fs::write(
            &cargo_wrapper,
            r##"#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' 'transient checkout revision' > "$ORIGINAL_MARKER"
repo="$PWD"
[ "${repo##*/}" = cilly ] && repo="${repo%/*}"
marker="$(cat "$repo/src/setup-source-sentinel")"
mkdir -p "$repo/target/release"
printf '%s' "$marker" > "$repo/target/release/$BACKEND_NAME"
printf '%s' "$marker" > "$repo/target/release/$LINKER_NAME"
printf '%s\n' 'setup tree src' > "$ORIGINAL_MARKER"
"##,
        )
        .unwrap();
        std::fs::set_permissions(&cargo_wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let dotnet = fake_bin.join("dotnet");
        std::fs::write(&dotnet, "#!/usr/bin/env bash\nexit 0\n").unwrap();
        std::fs::set_permissions(&dotnet, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::join_paths(std::iter::once(fake_bin.clone()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .unwrap();
        let facts = crate::host::HostFacts::detect();
        let backend_name = facts.backend_dylib_name();
        let linker_name = format!("linker{}", facts.exe_ext);
        let status = Command::new(private.join("feasibility/cargo-dotnet"))
            .arg("setup")
            .arg("--from-repo")
            .arg(&private)
            .arg("--home")
            .arg(&staged_home)
            .arg("--skip-toolchain")
            .arg("--skip-dotnet")
            .env("HOME", &user_home)
            .env("CARGO_HOME", user_home.join(".cargo"))
            .env("PATH", path)
            .env("CARGO_DOTNET_SKIP_FRONTEND_INSTALL", "1")
            .env("CARGO_DOTNET_SKIP_LEGACY_PAL_WARM", "1")
            .env("CARGO_DOTNET_CLI_VERSION", env!("CARGO_PKG_VERSION"))
            .env(
                "ORIGINAL_MARKER",
                original.join("src/setup-source-sentinel"),
            )
            .env("BACKEND_NAME", &backend_name)
            .env("LINKER_NAME", &linker_name)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            std::fs::read(staged_home.join("bin").join(backend_name)).unwrap(),
            b"setup tree src"
        );
        assert_eq!(
            std::fs::read(staged_home.join("bin").join(linker_name)).unwrap(),
            b"setup tree src"
        );
        assert_eq!(
            std::fs::read(original.join("src/setup-source-sentinel")).unwrap(),
            b"setup tree src\n"
        );
        authority.verify_materialized(&private).unwrap();
    }

    #[test]
    fn setup_source_authority_rejects_pal_or_msbuild_mixed_with_newer_crates() {
        for legacy_tree in ["dotnet_pal", "msbuild"] {
            let temp = tempfile::tempdir().unwrap();
            let repo = temp.path().join("repo");
            let home = temp.path().join("home");
            create_required_sources(&repo);
            let changed = repo.join("mycorrhiza/sentinel");
            let mut mutated = false;

            let error = SetupSourceAuthority::capture_with_hook(&repo, &mut |finished| {
                if !mutated && finished == Path::new(legacy_tree) {
                    std::fs::write(&changed, b"newer crate revision").unwrap();
                    mutated = true;
                }
            })
            .unwrap_err();

            assert!(mutated, "did not snapshot legacy tree {legacy_tree}");
            assert!(
                error.to_string().contains("immutable snapshot"),
                "{error:#}"
            );
            assert!(!home.join("crates/mycorrhiza/sentinel").exists());
        }
    }

    #[test]
    fn git_identity_labels_dirty_setup_without_blessing_its_release_tag() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        create_required_sources(&repo);
        initialize_git(&repo);
        git(&repo, &["tag", "rust-dotnet-v0.0.2"]);

        let clean = SetupSourceAuthority::capture(&repo).unwrap();
        let clean_git = clean.git_identity.unwrap();
        assert_eq!(clean_git.recorded_release_tag(), "untagged");
        assert_eq!(clean_git.recorded_git_tag(), "rust-dotnet-v0.0.2");
        assert!(!clean_git.recorded_revision().ends_with("-dirty"));

        std::fs::write(repo.join("src/setup-source-sentinel"), b"dirty input\n").unwrap();
        let dirty = SetupSourceAuthority::capture(&repo).unwrap();
        let dirty_git = dirty.git_identity.unwrap();
        assert_eq!(dirty_git.recorded_release_tag(), "untagged-dirty");
        assert!(dirty_git.recorded_revision().ends_with("-dirty"));
    }

    #[test]
    fn source_capture_rejects_git_ref_or_dirty_state_changes_during_capture() {
        for change in ["ref", "dirty"] {
            let temp = tempfile::tempdir().unwrap();
            let repo = temp.path().join("repo");
            create_required_sources(&repo);
            initialize_git(&repo);
            let mut changed = false;
            let error = SetupSourceAuthority::capture_with_git_hook(&repo, &mut |finished| {
                if !changed && finished == Path::new(".git-identity-revalidate") {
                    if change == "ref" {
                        git(&repo, &["commit", "-q", "--allow-empty", "-m", "ref moved"]);
                    } else {
                        std::fs::write(repo.join("identity-race-untracked"), b"dirty").unwrap();
                    }
                    changed = true;
                }
            })
            .unwrap_err();
            assert!(changed);
            assert!(
                error.to_string().contains("Git identity changed"),
                "{error:#}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn source_capture_rejects_checkout_rebinding_during_git_revalidation() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let original = temp.path().join("original");
        let replacement = temp.path().join("replacement");
        create_required_sources(&repo);
        create_required_sources(&replacement);
        initialize_git(&repo);
        initialize_git(&replacement);
        let mut swapped = false;
        let error = SetupSourceAuthority::capture_with_git_hook(&repo, &mut |finished| {
            if !swapped && finished == Path::new(".git-identity-revalidate") {
                std::fs::rename(&repo, &original).unwrap();
                std::fs::rename(&replacement, &repo).unwrap();
                swapped = true;
            }
        })
        .unwrap_err();
        assert!(swapped);
        assert!(
            format!("{error:#}").contains("pathname changed"),
            "{error:#}"
        );
    }

    #[test]
    fn real_immutable_snapshot_contains_every_cargo_dotnet_compile_input() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let authority = SetupSourceAuthority::capture_unversioned(&repo).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let snapshot = temp.path().join("snapshot");
        authority.materialize(&snapshot).unwrap();
        authority.verify_materialized(&snapshot).unwrap();
        assert!(
            snapshot
                .join("cargo_tests/spinacz/src/reflect.rs")
                .is_file()
        );

        let status = Command::new(crate::host::inner_cargo())
            .arg("check")
            .arg("--manifest-path")
            .arg(snapshot.join("tools/cargo-dotnet/Cargo.toml"))
            .arg("--package")
            .arg("cargo-dotnet")
            .arg("--locked")
            .arg("--target-dir")
            .arg(temp.path().join("target"))
            .status()
            .unwrap();
        assert!(
            status.success(),
            "cargo-dotnet did not compile from its private snapshot"
        );
    }
}
