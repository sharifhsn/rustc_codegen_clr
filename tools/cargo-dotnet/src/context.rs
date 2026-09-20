//! The ONE typed `Context` that erases the ~13 `CD_*` env vars.
//!
//! In the old design every `CD_*` var (CD_REPO, CD_BACKEND_DYLIB, CD_LINKER,
//! CD_TARGET_SPEC, CD_REGISTRY_SRC, CD_EXE_EXT, CD_REL, CD_RUN, CD_CLEAN, CD_VERBOSE,
//! CD_EXTRA_CARGO_FLAGS, CD_LASTBUILD_LOG, …) existed PURELY as the Rust→bash seam:
//! `pipeline.rs` assembled them and `_cargo_dotnet_core.sh` read them. Once the stages
//! are pure Rust the seam evaporates — every fact lives here as a typed field, threaded
//! by reference through the stage pipeline. The child env is now built ONLY at the
//! docker delegation boundary (`docker.rs`) and for the inner `cargo` invocation
//! (`buildstd.rs`), never as a thread-through-Rust contract.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use sha2::{Digest, Sha256};

use crate::cli::BuildArgs;
use crate::host::HostFacts;
use crate::mode::Mode;
use crate::passthrough;
use crate::{host, mode};
pub use rust_dotnet_sdk_core::identity::{ManagedIdentity, ManagedProjectConfig};
pub use rust_dotnet_sdk_core::runtime::DotnetVersion;

/// Build profile (replaces the stringly `CD_REL`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Release,
    Debug,
}

impl Profile {
    /// The cargo target-dir profile subdir (`release`/`debug`).
    pub fn dir(self) -> &'static str {
        match self {
            Profile::Release => "release",
            Profile::Debug => "debug",
        }
    }
    /// The cargo profile flag, if any (`--release` for release, none for debug).
    pub fn cargo_flag(self) -> Option<&'static str> {
        match self {
            Profile::Release => Some("--release"),
            Profile::Debug => None,
        }
    }
}

/// Per-build behaviour flags (replaces CD_CLEAN/CD_VERBOSE/CD_RUN + the extra-flags var).
#[derive(Debug, Clone)]
pub struct Flags {
    pub clean: bool,
    pub verbose: bool,
    pub run: bool,
    /// Standard + verbatim cargo flags forwarded to the inner build (the P2 passthrough).
    pub extra_cargo: Vec<String>,
}

/// Every filesystem location the pipeline needs. `root` is the CD_REPO trick: the repo
/// in Dev, the install home in Installed — both layouts resolve identically from it.
#[derive(Debug, Clone)]
pub struct Paths {
    pub backend_dylib: PathBuf,
    pub linker: PathBuf,
    pub target_spec: PathBuf,
    /// cargo registry src dir (where build-std extracts libc).
    pub registry_src: PathBuf,
    /// Private Cargo home used by restore/build-std. Never patch the user's ambient registry.
    pub cargo_home: PathBuf,
    /// `root/dotnet_pal`.
    pub pal_root: PathBuf,
    /// `root/dotnet_overlays`.
    pub overlays_root: PathBuf,
    /// `root/mycorrhiza_interop_helpers` — the bundled `Mycorrhiza.Interop.Helpers` C# companion
    /// project (currently just `ParameterRebinder`, see `mycorrhiza::linq`). Built and copied
    /// alongside any consumer's build output by `interop_helpers::ensure_and_copy`.
    pub interop_helpers_root: PathBuf,
    /// SDK-owned Rust crates used by portable consumer manifests.
    pub sdk_crates_root: PathBuf,
    pub lastbuild_log: PathBuf,
}

impl Paths {
    /// Resolve the layout for a mode (the old `NativeLayout`, kept with its bail!
    /// preflights). Installed and Dev differ ONLY here.
    fn resolve(mode: &Mode, facts: &HostFacts, crate_dir: &Path) -> Result<Self> {
        let cargo_home = cargo_home_for_crate(crate_dir)?;
        let registry_src = cargo_home.join("registry/src");
        match mode {
            Mode::Installed { home } => {
                if !home.is_dir() {
                    bail!(missing_install_home_message(home));
                }
                let _integrity = crate::bundle::verify_installed_if_locked(home)?;
                let layout = crate::bundle::installed_layout(home)?;
                let backend_dylib = home.join(&layout.backend);
                let linker = home.join(&layout.linker);
                let target_spec = home.join(&layout.target_spec);
                if !backend_dylib.is_file() {
                    bail!(
                        "installed backend dylib missing: {} — run `cargo dotnet setup`",
                        backend_dylib.display()
                    );
                }
                if !linker.is_file() {
                    bail!(
                        "installed linker missing: {} — run `cargo dotnet setup`",
                        linker.display()
                    );
                }
                if !target_spec.is_file() {
                    bail!("installed target spec missing — run `cargo dotnet setup`");
                }
                Ok(Paths {
                    backend_dylib,
                    linker,
                    target_spec,
                    registry_src,
                    cargo_home: cargo_home.clone(),
                    pal_root: home.join(&layout.pal_root),
                    overlays_root: home.join(&layout.overlays_root),
                    interop_helpers_root: home.join(&layout.interop_helpers_root),
                    sdk_crates_root: home.join(&layout.crates_root),
                    lastbuild_log: cargo_home.join("logs/lastbuild.log"),
                })
            }
            Mode::Dev { repo_root } => {
                let backend_name = facts.backend_dylib_name();
                let backend_dylib = repo_root.join("target/release").join(backend_name);
                let linker = repo_root.join(format!("target/release/linker{}", facts.exe_ext));
                let target_spec = repo_root.join("x86_64-unknown-dotnet.json");
                if !backend_dylib.is_file() {
                    bail!(
                        "native backend dylib missing: {} — build it first: \
                         (cd cilly && cargo build --release) && cargo build --release -p rustc_codegen_clr",
                        backend_dylib.display()
                    );
                }
                if !linker.is_file() {
                    bail!(
                        "native linker missing: {} — build it: (cd cilly && cargo build --release)",
                        linker.display()
                    );
                }
                if !target_spec.is_file() {
                    bail!("target spec missing: {}", target_spec.display());
                }
                Ok(Paths {
                    backend_dylib,
                    linker,
                    target_spec,
                    registry_src,
                    cargo_home: cargo_home.clone(),
                    pal_root: repo_root.join("dotnet_pal"),
                    overlays_root: repo_root.join("dotnet_overlays"),
                    interop_helpers_root: repo_root.join("mycorrhiza_interop_helpers"),
                    sdk_crates_root: repo_root.clone(),
                    lastbuild_log: cargo_home.join("logs/lastbuild.log"),
                })
            }
        }
    }
}

pub(crate) fn missing_install_home_message(home: &Path) -> String {
    format!(
        "the cargo-dotnet command is installed, but its SDK home does not exist: {}\n\
A bare `cargo install` installs only the command. Complete either supported installation path:\n  \
from a rustc_codegen_clr checkout: cargo dotnet setup --from-repo /path/to/rustc_codegen_clr\n  \
from a release SDK bundle:         cargo dotnet bundle install /path/to/cargo-dotnet-sdk-<host>.zip",
        home.display()
    )
}

/// The single typed config threaded (by reference) through the stage pipeline. It is
/// only ever resolved on the NATIVE backend (the docker backend short-circuits in
/// `pipeline::run` before this), so it carries no `backend` discriminant.
pub struct Context {
    pub host: HostFacts,
    pub profile: Profile,
    pub flags: Flags,
    /// The crate dir to build (absolute; verified to contain Cargo.toml).
    pub crate_dir: PathBuf,
    /// Cargo's effective workspace root. Locks and workspace-scoped configuration live here even
    /// when `crate_dir` is one selected member.
    pub workspace_root: PathBuf,
    /// Cargo's canonical package ID for the one selected manifest. Artifact discovery must match
    /// this identity, not merely a target name that another workspace package can reuse.
    pub selected_package_id: String,
    pub paths: Paths,
    /// The exact toolchain pinned into every inner Cargo/rustc invocation. External crate and
    /// bindgen working directories cannot inherit this repository's rustup directory override.
    pub toolchain: Option<String>,
    /// The inner cargo binary (`$CARGO` or `cargo`).
    pub cargo: String,
    /// The target .NET runtime version (`--dotnet`). Exported as `DOTNET_VERSION` to the inner cargo.
    pub dotnet: DotnetVersion,
    /// `(PATH addition, DOTNET_ROOT)` if dotnet was self-healed from `$HOME/.dotnet`.
    pub dotnet_heal: Option<(PathBuf, PathBuf)>,
    /// Complete release/host contract, resolved from one validated Cargo metadata table.
    pub managed_project: Option<ManagedProjectConfig>,
    /// Optional Source Link URL template for `/_/consumer/*` documents. The validated URL is
    /// retained separately from the deterministic JSON passed to the linker so receipts stay
    /// human-readable.
    pub source_link_url: Option<String>,
}

impl Context {
    pub fn managed_identity(&self) -> Option<&ManagedIdentity> {
        self.managed_project
            .as_ref()
            .map(|project| &project.identity)
    }

    pub fn is_offline(&self) -> bool {
        self.flags
            .extra_cargo
            .iter()
            .any(|flag| flag == "--offline" || flag == "--frozen")
    }

    pub fn requires_existing_lock(&self) -> bool {
        self.flags
            .extra_cargo
            .iter()
            .any(|flag| flag == "--locked" || flag == "--frozen")
    }

    /// Fold mode detection, backend resolution, the path layout, and the host preflight
    /// into ONE typed value. `is_run` selects the run-the-apphost behaviour.
    pub fn resolve(args: &BuildArgs, is_run: bool) -> Result<Self> {
        Self::resolve_with_mode(args, is_run, mode::detect()?)
    }

    pub(crate) fn resolve_with_mode(args: &BuildArgs, is_run: bool, mode: Mode) -> Result<Self> {
        let host = HostFacts::detect();
        let initial_crate_dir = host::resolve_crate_dir(&args.path)?;
        // The original Docker/bash frontend wrote a generated Cargo config into fixtures. Remove
        // every recognized historical header before even the package-selection metadata query;
        // otherwise Cargo can try container-only `/work/...` paths before the native pipeline has
        // a chance to create its private build-local config. User-owned configs are preserved.
        crate::overlays::remove_legacy_generated_config(&initial_crate_dir)?;

        // host preflight (rustc/cargo present; dotnet reachable).
        host::ensure_rust_toolchain()?;
        let dotnet: DotnetVersion = args.dotnet.parse().map_err(anyhow::Error::msg)?;
        let dotnet_heal = match dotnet {
            DotnetVersion::Net10 => host::dotnet_env_adds_for(dotnet.as_env()),
            // The Unity target consumes the SDK's netstandard2.1 reference pack and does not
            // require a matching CoreCLR runtime installation.
            DotnetVersion::UnityNetStandard21 => None,
        };
        host::ensure_dotnet(&dotnet_heal)?;

        let toolchain = match &mode {
            Mode::Installed { home } => Some(
                std::env::var("CARGO_DOTNET_TOOLCHAIN")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| mode::read_home_toolchain(home)),
            ),
            Mode::Dev { .. } => Some(
                std::env::var("CARGO_DOTNET_TOOLCHAIN")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| mode::DEFAULT_TOOLCHAIN.to_string()),
            ),
        };

        // `$CARGO` inside a cargo subcommand may name a specific stable toolchain binary. A bare
        // shim is required for the explicitly-pinned nightly used by the backend.
        let cargo = if toolchain.is_some() {
            "cargo".to_string()
        } else {
            host::inner_cargo()
        };
        let extra_cargo = passthrough::assemble_cargo_flags(args);
        let selection_cargo_home = cargo_home_for_crate(&initial_crate_dir)?;
        let (crate_dir, extra_cargo) = crate::interop_helpers::resolve_single_selected_crate(
            &initial_crate_dir,
            &extra_cargo,
            &cargo,
            Some(&selection_cargo_home),
            toolchain.as_deref(),
        )?;
        let paths = Paths::resolve(&mode, &host, &crate_dir)?;
        let selected_metadata = cargo_metadata_with_environment(
            &crate_dir,
            &cargo,
            &selection_cargo_home,
            toolchain.as_deref(),
        )?;
        let package = package_for_manifest(&selected_metadata, &manifest_path(&crate_dir))?.clone();
        let workspace_root = selected_metadata.workspace_root.into_std_path_buf();
        let selected_package_id = package.id.repr.clone();
        let managed_project = resolve_managed_project(&package)?;
        let source_link_url = validate_source_link_url(args.source_link_url.as_deref())?;
        if managed_project.is_some() {
            validate_managed_identity_build(args, &package)?;
        }

        let profile = if args.is_release() {
            Profile::Release
        } else {
            Profile::Debug
        };

        Ok(Context {
            host,
            profile,
            flags: Flags {
                clean: args.clean,
                verbose: args.verbose,
                run: is_run,
                extra_cargo,
            },
            crate_dir,
            workspace_root,
            selected_package_id,
            paths,
            toolchain,
            cargo,
            dotnet,
            dotnet_heal,
            managed_project,
            source_link_url,
        })
    }

    /// The ambient toolchain sysroot (`rustc --print sysroot`), honouring the pinned
    /// `RUSTUP_TOOLCHAIN`. It is read as the pristine source for a private snapshot, never patched.
    pub fn rustc_sysroot(&self) -> Result<PathBuf> {
        let mut cmd = Command::new("rustc");
        if let Some(tc) = &self.toolchain {
            cmd.env("RUSTUP_TOOLCHAIN", tc);
        }
        let out = cmd
            .arg("--print")
            .arg("sysroot")
            .output()
            .context("failed to run `rustc --print sysroot`")?;
        if !out.status.success() {
            bail!(
                "`rustc --print sysroot` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() {
            bail!("`rustc --print sysroot` returned empty (is the toolchain installed?)");
        }
        Ok(PathBuf::from(s))
    }
}

fn validate_source_link_url(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    if !value.starts_with("https://") {
        bail!("--source-link-url must use https://");
    }
    if value.chars().filter(|&ch| ch == '*').count() != 1 {
        bail!("--source-link-url must contain exactly one `*` placeholder");
    }
    if value.chars().any(char::is_whitespace) {
        bail!("--source-link-url must not contain whitespace");
    }
    let authority = value
        .strip_prefix("https://")
        .unwrap()
        .split('/')
        .next()
        .unwrap_or_default();
    if authority.contains('@') {
        bail!("--source-link-url must not contain embedded credentials");
    }
    if value.contains('?') || value.contains('#') {
        bail!("--source-link-url must not contain query parameters or fragments");
    }
    Ok(Some(value.to_string()))
}

/// Resolve a crate directory or manifest to the manifest Cargo should inspect.
pub(crate) fn manifest_path(crate_path: &Path) -> PathBuf {
    if crate_path.is_file() {
        crate_path.to_path_buf()
    } else {
        crate_path.join("Cargo.toml")
    }
}

/// Read the ordinary root package metadata used by build/attach/pack operations.
///
/// Full-graph callers (provenance, restore receipts, and metadata-input tracking) deliberately
/// keep their own Cargo invocation because they need locked/config/toolchain semantics.
pub(crate) fn cargo_package(crate_path: &Path) -> Result<cargo_metadata::Package> {
    let metadata = cargo_metadata(crate_path)?;
    let manifest = manifest_path(crate_path);
    package_for_manifest(&metadata, &manifest).cloned()
}

pub(crate) fn package_for_manifest<'a>(
    metadata: &'a cargo_metadata::Metadata,
    manifest: &Path,
) -> Result<&'a cargo_metadata::Package> {
    let manifest = manifest
        .canonicalize()
        .with_context(|| format!("canonicalize Cargo manifest {}", manifest.display()))?;
    let mut matches = metadata.packages.iter().filter(|package| {
        package
            .manifest_path
            .as_std_path()
            .canonicalize()
            .is_ok_and(|candidate| candidate == manifest)
    });
    let package = matches.next().with_context(|| {
        format!(
            "Cargo metadata has no package for selected manifest {}",
            manifest.display()
        )
    })?;
    if matches.next().is_some() {
        bail!(
            "Cargo metadata has multiple packages for selected manifest {}",
            manifest.display()
        );
    }
    Ok(package)
}

pub(crate) fn cargo_metadata(crate_path: &Path) -> Result<cargo_metadata::Metadata> {
    let manifest = manifest_path(crate_path);
    cargo_metadata::MetadataCommand::new()
        .manifest_path(manifest)
        .no_deps()
        .exec()
        .context("read Cargo metadata")
}

fn cargo_metadata_with_environment(
    crate_path: &Path,
    cargo: &str,
    cargo_home: &Path,
    toolchain: Option<&str>,
) -> Result<cargo_metadata::Metadata> {
    let manifest = manifest_path(crate_path);
    let mut command = cargo_metadata::MetadataCommand::new();
    command
        .cargo_path(cargo)
        .manifest_path(manifest)
        .current_dir(crate_path)
        .no_deps()
        .env("CARGO_HOME", cargo_home);
    if let Some(toolchain) = toolchain {
        command.env("RUSTUP_TOOLCHAIN", toolchain);
    }
    command
        .exec()
        .context("read Cargo metadata with the selected cargo environment")
}

fn resolve_managed_project(
    package: &cargo_metadata::Package,
) -> Result<Option<ManagedProjectConfig>> {
    let Some(dotnet) = package.metadata.get("dotnet") else {
        return Ok(None);
    };
    let Some(dotnet) = dotnet.as_object() else {
        bail!("package.metadata.dotnet must be a table/object");
    };
    const IDENTITY_KEYS: &[&str] = &[
        "identity-schema",
        "package-id",
        "assembly-name",
        "root-namespace",
        "module-type",
        "public-namespaces",
        "compatibility-profile",
    ];
    for key in dotnet.keys() {
        if !IDENTITY_KEYS.contains(&key.as_str()) {
            bail!("unknown package.metadata.dotnet key {key:?}");
        }
    }

    let string = |key: &str| -> Result<String> {
        dotnet
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .with_context(|| format!("package.metadata.dotnet.{key} must be a non-empty string"))
    };
    let schema = dotnet
        .get("identity-schema")
        .and_then(serde_json::Value::as_u64)
        .context("package.metadata.dotnet.identity-schema must be integer 1")?;
    if schema != 1 {
        bail!("unsupported package.metadata.dotnet.identity-schema {schema}; expected 1");
    }
    let identity = ManagedIdentity {
        schema: schema as u16,
        package_id: string("package-id")?,
        assembly_name: string("assembly-name")?,
        root_namespace: string("root-namespace")?,
        module_type: string("module-type")?,
    };
    validate_identity(&identity)?;
    let public_namespaces = dotnet
        .get("public-namespaces")
        .and_then(serde_json::Value::as_array)
        .context(
            "package.metadata.dotnet.public-namespaces must be a non-empty array of dotted CLR namespaces",
        )?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .context("package.metadata.dotnet.public-namespaces entries must be non-empty strings")
        })
        .collect::<Result<Vec<_>>>()?;
    if public_namespaces.is_empty() {
        bail!("package.metadata.dotnet.public-namespaces must not be empty");
    }
    let compatibility_profile = string("compatibility-profile")?;
    let project = ManagedProjectConfig {
        identity,
        public_namespaces,
        compatibility_profile,
    };
    validate_managed_project(&project)?;
    Ok(Some(project))
}

/// Print the filename/CLR identity that generic MSBuild integration must reference.
pub fn print_managed_assembly_name(path: Option<&Path>) -> Result<i32> {
    let crate_dir = path.unwrap_or_else(|| Path::new("."));
    let crate_dir = crate_dir
        .canonicalize()
        .with_context(|| format!("canonicalize Rust crate {}", crate_dir.display()))?;
    let package = cargo_package(&crate_dir)?;
    let name = resolve_managed_project(&package)?
        .map(|project| project.identity.assembly_name)
        .unwrap_or_else(|| package.name.to_string());
    println!("{name}");
    Ok(0)
}

/// Validate the deliberately narrow Wave-1 identity scope before Cargo starts a process whose
/// linker environment is inherited by every final target.  There is no per-artifact identity
/// channel yet, so a release identity may describe exactly one `cdylib`, never a workspace-wide
/// or mixed bin/library invocation.
fn validate_managed_identity_build(
    _args: &BuildArgs,
    package: &cargo_metadata::Package,
) -> Result<()> {
    let final_targets: Vec<_> = package
        .targets
        .iter()
        .filter(|target| {
            target.kind.iter().any(|kind| kind == "bin")
                || target.crate_types.iter().any(|kind| kind == "cdylib")
        })
        .map(|target| {
            (
                target.name.as_str(),
                target.crate_types.iter().any(|kind| kind == "cdylib"),
            )
        })
        .collect();
    validate_managed_final_targets(&final_targets)
}

fn validate_managed_final_targets(final_targets: &[(&str, bool)]) -> Result<()> {
    let is_single_cdylib = matches!(final_targets, [(_, true)]);
    if !is_single_cdylib {
        let names = final_targets
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "managed identity requires exactly one final cdylib target; found [{}]. \
             Split bin/cdylib or multi-target packages into separate cargo dotnet builds.",
            names
        );
    }
    Ok(())
}

/// Validate the identities of all Rust crates referenced by one managed host before the host
/// builds any of them.  This is the cross-process collision check that Cargo itself cannot make:
/// MSBuild invokes cargo-dotnet once per crate, so each final linker would otherwise see only its
/// own process-local identity.
pub fn validate_managed_identity_set(crate_dirs: &[PathBuf]) -> Result<i32> {
    let mut assembly_owners = BTreeMap::<String, PathBuf>::new();
    let mut public_type_owners = BTreeMap::<String, PathBuf>::new();

    for crate_dir in crate_dirs {
        let crate_dir = crate_dir.canonicalize().with_context(|| {
            format!(
                "managed identity crate path does not exist: {}",
                crate_dir.display()
            )
        })?;
        let package = cargo_package(&crate_dir)?;
        let project = resolve_managed_project(&package)?;
        let assembly_name = project
            .as_ref()
            .map(|project| project.identity.assembly_name.clone())
            .unwrap_or_else(|| package.name.to_string());
        let public_type = project
            .as_ref()
            .map(|project| project.identity.module_full_name())
            .unwrap_or_else(|| "MainModule".to_string());

        if let Some(previous) = assembly_owners.insert(assembly_name.clone(), crate_dir.clone()) {
            bail!(
                "duplicate managed assembly name {assembly_name:?}: {} and {}. \
                 Assign distinct package.metadata.dotnet.assembly-name values.",
                previous.display(),
                crate_dir.display()
            );
        }
        if let Some(previous) = public_type_owners.insert(public_type.clone(), crate_dir.clone()) {
            bail!(
                "duplicate managed public type {public_type:?}: {} and {}. \
                 Assign distinct root-namespace/module-type values or isolate legacy MainModule crates.",
                previous.display(),
                crate_dir.display()
            );
        }
    }
    Ok(0)
}

fn validate_identity(identity: &ManagedIdentity) -> Result<()> {
    for (label, value) in [
        ("assembly-name", identity.assembly_name.as_str()),
        ("root-namespace", identity.root_namespace.as_str()),
        ("module-type", identity.module_type.as_str()),
    ] {
        if !value.split('.').all(is_clr_identifier) {
            bail!("package.metadata.dotnet.{label}={value:?} is not a dotted CLR identifier");
        }
    }
    Ok(())
}

fn validate_managed_project(project: &ManagedProjectConfig) -> Result<()> {
    let mut namespaces = std::collections::BTreeSet::<String>::new();
    for namespace in &project.public_namespaces {
        if !namespace.split('.').all(is_clr_identifier) {
            bail!(
                "package.metadata.dotnet.public-namespaces contains invalid dotted CLR namespace {namespace:?}"
            );
        }
        if !namespaces.insert(namespace.clone()) {
            bail!(
                "package.metadata.dotnet.public-namespaces contains duplicate namespace {namespace:?}"
            );
        }
    }
    if !namespaces.contains(project.identity.root_namespace.as_str()) {
        bail!(
            "package.metadata.dotnet.public-namespaces must include root-namespace {:?}",
            project.identity.root_namespace
        );
    }
    if !crate::profiles::is_known(&project.compatibility_profile) {
        bail!(
            "unknown package.metadata.dotnet.compatibility-profile {:?}; run `cargo dotnet profiles` for valid names",
            project.compatibility_profile
        );
    }
    Ok(())
}

fn is_clr_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    #[test]
    fn configured_metadata_uses_selected_cargo_home_and_toolchain() {
        use std::os::unix::fs::PermissionsExt;

        fn shell_quote(value: &str) -> String {
            format!("'{}'", value.replace('\'', "'\"'\"'"))
        }

        let root = tempfile::tempdir().unwrap();
        let crate_dir = root.path().join("selected");
        let cargo_home = root.path().join("cargo-home");
        let marker = root.path().join("metadata-environment.txt");
        let wrapper = root.path().join("cargo-wrapper.sh");
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::create_dir_all(&cargo_home).unwrap();
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname='configured-metadata'\nversion='0.0.0'\nedition='2021'\n",
        )
        .unwrap();
        fs::write(crate_dir.join("src/lib.rs"), "").unwrap();

        let real_cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\nprintf '%s\\n%s\\n' \"$CARGO_HOME\" \"$RUSTUP_TOOLCHAIN\" > {}\nunset RUSTUP_TOOLCHAIN\nexec {} \"$@\"\n",
                shell_quote(&marker.to_string_lossy()),
                shell_quote(&real_cargo),
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&wrapper).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&wrapper, permissions).unwrap();

        let metadata = cargo_metadata_with_environment(
            &crate_dir,
            wrapper.to_str().unwrap(),
            &cargo_home,
            Some("configured-toolchain-probe"),
        )
        .unwrap();
        assert_eq!(
            package_for_manifest(&metadata, &crate_dir.join("Cargo.toml"))
                .unwrap()
                .name,
            "configured-metadata"
        );
        assert_eq!(
            fs::read_to_string(marker).unwrap(),
            format!("{}\nconfigured-toolchain-probe\n", cargo_home.display())
        );
    }

    #[test]
    fn cargo_package_selects_a_workspace_member_manifest() {
        let root = tempfile::tempdir().unwrap();
        let member = root.path().join("member");
        fs::create_dir_all(member.join("src")).unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"member\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"selected-member\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(member.join("src/lib.rs"), "").unwrap();

        let package = cargo_package(&member).unwrap();
        assert_eq!(package.name, "selected-member");
        assert_eq!(
            package.manifest_path.as_std_path().canonicalize().unwrap(),
            member.join("Cargo.toml").canonicalize().unwrap()
        );
    }

    #[test]
    fn archived_runtime_profile_parses_for_legacy_validation() {
        assert_eq!("10".parse::<DotnetVersion>().unwrap(), DotnetVersion::Net10);
        assert_eq!(
            "unity-netstandard2.1".parse::<DotnetVersion>().unwrap(),
            DotnetVersion::UnityNetStandard21
        );
        for unsupported in ["8", "9", "net8.0", "net9.0", "11"] {
            let error = unsupported.parse::<DotnetVersion>().unwrap_err();
            assert!(error.contains("supports .NET 10"), "{error}");
        }
    }

    #[test]
    fn missing_install_home_explains_both_exact_recovery_paths() {
        let message = missing_install_home_message(Path::new("/tmp/missing-sdk"));
        assert!(message.contains("A bare `cargo install` installs only the command"));
        assert!(message.contains("cargo dotnet setup --from-repo /path/to/rustc_codegen_clr"));
        assert!(
            message.contains("cargo dotnet bundle install /path/to/cargo-dotnet-sdk-<host>.zip")
        );
    }

    #[test]
    fn source_link_url_is_https_single_wildcard_and_credential_free() {
        assert_eq!(
            validate_source_link_url(Some("https://example.invalid/revision/*")).unwrap(),
            Some("https://example.invalid/revision/*".to_string())
        );
        for invalid in [
            "http://example.invalid/*",
            "https://example.invalid/no-wildcard",
            "https://example.invalid/**",
            "https://user:secret@example.invalid/*",
            "https://example.invalid/bad path/*",
            "https://example.invalid/*?token=secret",
            "https://example.invalid/*#fragment",
        ] {
            assert!(
                validate_source_link_url(Some(invalid)).is_err(),
                "accepted invalid Source Link URL {invalid:?}"
            );
        }
    }

    #[test]
    fn managed_identity_projects_a_distinct_public_type() {
        let identity = ManagedIdentity {
            schema: 1,
            package_id: "Collision.Alpha".into(),
            assembly_name: "collision_alpha".into(),
            root_namespace: "Collision.Alpha".into(),
            module_type: "Exports".into(),
        };
        validate_identity(&identity).unwrap();
        assert_eq!(identity.module_full_name(), "Collision.Alpha.Exports");
    }

    fn example_project() -> ManagedProjectConfig {
        ManagedProjectConfig {
            identity: ManagedIdentity {
                schema: 1,
                package_id: "Example.Widget".into(),
                assembly_name: "Example.Widget".into(),
                root_namespace: "Example.Widget".into(),
                module_type: "Exports".into(),
            },
            public_namespaces: vec!["Example.Widget".into(), "Example.Widget.Models".into()],
            compatibility_profile: "net10-coreclr".into(),
        }
    }

    #[test]
    fn managed_project_validates_namespaces_and_known_profile_together() {
        validate_managed_project(&example_project()).unwrap();

        let mut missing_root = example_project();
        missing_root.public_namespaces = vec!["Example.Widget.Models".into()];
        assert!(
            validate_managed_project(&missing_root)
                .unwrap_err()
                .to_string()
                .contains("must include root-namespace")
        );

        let mut unknown_profile = example_project();
        unknown_profile.compatibility_profile = "wishful-future-host".into();
        assert!(
            validate_managed_project(&unknown_profile)
                .unwrap_err()
                .to_string()
                .contains("cargo dotnet profiles")
        );
    }

    #[test]
    fn managed_identity_projects_module_type() {
        let identity = ManagedIdentity {
            schema: 1,
            package_id: "legacy".into(),
            assembly_name: "legacy".into(),
            root_namespace: "Legacy".into(),
            module_type: "Exports".into(),
        };
        assert_eq!(identity.module_full_name(), "Legacy.Exports");
    }

    #[test]
    fn managed_identity_rejects_mixed_final_targets() {
        let error = validate_managed_final_targets(&[("app", false), ("library", true)])
            .unwrap_err()
            .to_string();
        assert!(error.contains("exactly one final cdylib target"), "{error}");
        assert!(error.contains("app, library"), "{error}");
    }

    #[test]
    fn managed_identity_accepts_one_cdylib_target() {
        validate_managed_final_targets(&[("library", true)]).unwrap();
    }

    #[test]
    fn identity_metadata_rejects_non_clr_names() {
        let identity = ManagedIdentity {
            schema: 1,
            package_id: "Example.Widget".into(),
            assembly_name: "example-widget".into(),
            root_namespace: "Example.Widget".into(),
            module_type: "Exports".into(),
        };
        let error = validate_identity(&identity).unwrap_err().to_string();
        assert!(error.contains("assembly-name"), "{error}");
    }
}

pub(crate) fn cargo_dotnet_cache_home() -> Result<PathBuf> {
    if let Some(path) =
        std::env::var_os("CARGO_DOTNET_CACHE_HOME").filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(path));
    }
    if let Some(sdk_home) = std::env::var_os("CARGO_DOTNET_HOME").filter(|value| !value.is_empty())
    {
        return Ok(cache_home_for_sdk_home(&PathBuf::from(sdk_home)));
    }
    let home = crate::host::home_dir()
        .context("neither HOME nor USERPROFILE is set (needed for cargo-dotnet cache)")?;
    Ok(home.join(".cargo-dotnet-cache"))
}

/// Stable namespace for mutable state owned by one consumer crate. Cargo registry sources are
/// patched for the CLR PAL, so sharing a Cargo home between unrelated builds is unsafe even when
/// their target directories differ.
pub(crate) fn crate_cache_key(crate_dir: &Path) -> Result<String> {
    let canonical = crate_dir
        .canonicalize()
        .with_context(|| format!("canonicalize consumer crate {}", crate_dir.display()))?;
    Ok(crate_cache_key_from_canonical(&canonical))
}

fn crate_cache_key_from_canonical(canonical: &Path) -> String {
    raw_os_str_sha256(canonical.as_os_str())
}

pub(crate) fn cache_home_for_sdk_home(sdk_home: &Path) -> PathBuf {
    let parent = sdk_home.parent().unwrap_or_else(|| Path::new("."));
    let digest = raw_os_str_sha256(sdk_home.as_os_str());
    parent.join(format!(".cargo-dotnet-cache-{}", &digest[..24]))
}

/// Hash the operating system's exact path-string identity using an explicit, portable wire
/// encoding. Unix bytes and Windows WTF-16 code units are length-delimited and domain-separated;
/// invalid Unicode can therefore never collapse through replacement-character rendering.
fn raw_os_str_sha256(value: &OsStr) -> String {
    let mut hash = Sha256::new();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        let bytes = value.as_bytes();
        hash.update(b"cargo-dotnet-osstr-v1\0unix-bytes\0");
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        let units = value.encode_wide().collect::<Vec<_>>();
        hash.update(b"cargo-dotnet-osstr-v1\0windows-wtf16le\0");
        hash.update((units.len() as u64).to_le_bytes());
        for unit in units {
            hash.update(unit.to_le_bytes());
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let bytes = value.as_encoded_bytes();
        hash.update(b"cargo-dotnet-osstr-v1\0platform-encoded\0");
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}

pub(crate) fn cargo_home_for_crate(crate_dir: &Path) -> Result<PathBuf> {
    Ok(cargo_dotnet_cache_home()?
        .join("crates")
        .join(crate_cache_key(crate_dir)?)
        .join("cargo-home"))
}

#[cfg(test)]
mod cache_identity_tests {
    use super::*;

    #[test]
    fn raw_os_string_hash_is_stable_and_domain_separated() {
        let first = raw_os_str_sha256(OsStr::new("crate-a"));
        assert_eq!(first, raw_os_str_sha256(OsStr::new("crate-a")));
        assert_ne!(first, raw_os_str_sha256(OsStr::new("crate-b")));
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[cfg(unix)]
    #[test]
    fn invalid_utf8_paths_cannot_collide_in_crate_or_sdk_cache_names() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt as _;

        let first_name = OsString::from_vec(b"consumer-\x80".to_vec());
        let second_name = OsString::from_vec(b"consumer-\x81".to_vec());
        assert_eq!(first_name.to_string_lossy(), second_name.to_string_lossy());
        let first = PathBuf::from(first_name);
        let second = PathBuf::from(second_name);

        assert_ne!(
            crate_cache_key_from_canonical(&first),
            crate_cache_key_from_canonical(&second)
        );
        assert_ne!(
            cache_home_for_sdk_home(&first),
            cache_home_for_sdk_home(&second)
        );
    }
}
