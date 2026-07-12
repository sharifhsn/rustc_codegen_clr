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
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use sha2::{Digest, Sha256};

use crate::cli::BuildArgs;
use crate::host::HostFacts;
use crate::mode::Mode;
use crate::passthrough;
use crate::{host, mode};

/// The managed public identity requested by `[package.metadata.dotnet]`.
///
/// Absence deliberately means the legacy `MainModule` surface. This keeps existing crates and
/// serialized artifacts compatible while release-oriented crates can opt into a collision-free
/// projection at final link time.
#[derive(Debug, Clone)]
pub struct ManagedIdentity {
    pub schema: u16,
    pub package_id: String,
    pub assembly_name: String,
    pub root_namespace: String,
    pub module_type: String,
    pub legacy_main_module: bool,
}

impl ManagedIdentity {
    /// The CLR full type name projected from the compiler's internal `MainModule` sentinel.
    #[must_use]
    pub fn module_full_name(&self) -> Option<String> {
        (!self.legacy_main_module).then(|| format!("{}.{}", self.root_namespace, self.module_type))
    }
}

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
                    bail!(
                        "no install home at {} (run `cargo dotnet setup` first, from a repo checkout)",
                        home.display()
                    );
                }
                let backend_dylib =
                    home.join(format!("bin/librustc_codegen_clr.{}", facts.dylib_ext));
                let linker = home.join(format!("bin/linker{}", facts.exe_ext));
                let target_spec = home.join("target/x86_64-unknown-dotnet.json");
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
                    pal_root: home.join("dotnet_pal"),
                    overlays_root: home.join("dotnet_overlays"),
                    interop_helpers_root: home.join("mycorrhiza_interop_helpers"),
                    sdk_crates_root: home.join("crates"),
                    lastbuild_log: cargo_home.join("logs/lastbuild.log"),
                })
            }
            Mode::Dev { repo_root } => {
                let backend_dylib = repo_root.join(format!(
                    "target/release/librustc_codegen_clr.{}",
                    facts.dylib_ext
                ));
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

/// The target .NET runtime version selected by `--dotnet` (env `DOTNET_VERSION`). The front-end is
/// the *producer* of the version: it exports `DOTNET_VERSION` to the inner cargo (so both the codegen
/// backend and the cilly linker see it) and selects the matching CoreCLR ilasm. It deliberately does
/// NOT know per-version BCL tokens / `.ver` strings — those live in cilly (`cilly::DotnetRuntime`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DotnetVersion {
    /// .NET 8.
    Net8,
    /// .NET 9.
    Net9,
    /// .NET 10 (default).
    #[default]
    Net10,
}
impl DotnetVersion {
    /// Target-framework moniker for NuGet packaging (`net8.0` / `net9.0`).
    #[must_use]
    pub fn tfm(self) -> &'static str {
        match self {
            DotnetVersion::Net8 => "net8.0",
            DotnetVersion::Net9 => "net9.0",
            DotnetVersion::Net10 => "net10.0",
        }
    }
    /// The `DOTNET_VERSION` value the cilly/backend parser expects (canonical bare major).
    #[must_use]
    pub fn as_env(self) -> &'static str {
        match self {
            DotnetVersion::Net8 => "8",
            DotnetVersion::Net9 => "9",
            DotnetVersion::Net10 => "10",
        }
    }
    /// The matching CoreCLR ilasm tool dir under `$HOME/.dotnet` (each runtime needs its own — a
    /// net8 ilasm's PE is rejected by the net9 runtime and vice-versa).
    #[must_use]
    pub fn ilasm_tool_dir(self) -> &'static str {
        match self {
            DotnetVersion::Net8 => "ilasm-tool",
            DotnetVersion::Net9 => "ilasm9-tool",
            DotnetVersion::Net10 => "ilasm10-tool",
        }
    }
}
impl std::str::FromStr for DotnetVersion {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "8" | "net8" | "net8.0" => Ok(DotnetVersion::Net8),
            "9" | "net9" | "net9.0" => Ok(DotnetVersion::Net9),
            "10" | "net10" | "net10.0" => Ok(DotnetVersion::Net10),
            other => Err(format!(
                "--dotnet: invalid value {other:?}; expected 8, 9, or 10"
            )),
        }
    }
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
    pub paths: Paths,
    /// The exact toolchain pinned into every inner Cargo/rustc invocation. External crate and
    /// bindgen working directories cannot inherit this repository's rustup directory override.
    pub toolchain: Option<String>,
    /// The inner cargo binary (`$CARGO` or `cargo`).
    pub cargo: String,
    /// A resolved CoreCLR ilasm to export as ILASM_PATH (None lets cilly's default fire).
    pub ilasm: Option<PathBuf>,
    /// The target .NET runtime version (`--dotnet`). Exported as `DOTNET_VERSION` to the inner cargo.
    pub dotnet: DotnetVersion,
    /// `(PATH addition, DOTNET_ROOT)` if dotnet was self-healed from `$HOME/.dotnet`.
    pub dotnet_heal: Option<(PathBuf, PathBuf)>,
    /// Explicit release-package identity, if the crate opted into schema 1.
    pub managed_identity: Option<ManagedIdentity>,
}

impl Context {
    /// Fold mode detection, backend resolution, the path layout, and the host preflight
    /// into ONE typed value. `is_run` selects the run-the-apphost behaviour.
    pub fn resolve(args: &BuildArgs, is_run: bool) -> Result<Self> {
        let mode = mode::detect()?;
        let host = HostFacts::detect();
        let crate_dir = host::resolve_crate_dir(&args.path)?;

        // host preflight (rustc/cargo present; dotnet reachable; ilasm resolved).
        host::ensure_rust_toolchain()?;
        let dotnet_heal = host::dotnet_env_adds();
        host::ensure_dotnet(&dotnet_heal)?;
        let dotnet: DotnetVersion = args.dotnet.parse().map_err(anyhow::Error::msg)?;
        let ilasm = host::resolve_ilasm(&host, dotnet)?;

        let paths = Paths::resolve(&mode, &host, &crate_dir)?;
        let managed_identity = resolve_managed_identity(&crate_dir)?;
        if managed_identity.is_some() {
            validate_managed_identity_build(args, &crate_dir)?;
        }

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

        let profile = if args.is_release() {
            Profile::Release
        } else {
            Profile::Debug
        };

        // `host::inner_cargo()` prefers `$CARGO` (the Book §External Tools convention,
        // right when we want to reinvoke whichever cargo drove `cargo dotnet`). But
        // `$CARGO` is a cargo subcommand's own outer cargo, resolved to a SPECIFIC
        // toolchain's binary (e.g. `~/.rustup/toolchains/stable-.../bin/cargo`) — not
        // the rustup shim. When we're about to pin a different toolchain via
        // `RUSTUP_TOOLCHAIN` (installed mode), that env var only takes effect through
        // the shim; a hardcoded toolchain binary ignores it outright, so `-Z` flags
        // fail with "only accepted on the nightly channel" if `$CARGO` happens to be
        // stable. Use a bare `cargo` (PATH-resolved, i.e. the shim) whenever we are
        // pinning a toolchain; keep the `$CARGO` preference only when we are not.
        let cargo = if toolchain.is_some() {
            "cargo".to_string()
        } else {
            host::inner_cargo()
        };

        Ok(Context {
            host,
            profile,
            flags: Flags {
                clean: args.clean,
                verbose: args.verbose,
                run: is_run,
                extra_cargo: passthrough::assemble_cargo_flags(args),
            },
            crate_dir,
            paths,
            toolchain,
            cargo,
            ilasm,
            dotnet,
            dotnet_heal,
            managed_identity,
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

fn cargo_package(crate_dir: &Path) -> Result<cargo_metadata::Package> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(crate_dir.join("Cargo.toml"))
        .no_deps()
        .exec()
        .context("read Cargo metadata for managed identity")?;
    metadata.root_package().cloned().with_context(|| {
        format!(
            "Cargo metadata has no root package for {}",
            crate_dir.display()
        )
    })
}

fn resolve_managed_identity(crate_dir: &Path) -> Result<Option<ManagedIdentity>> {
    let package = cargo_package(crate_dir)?;
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
        "legacy-main-module",
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
        legacy_main_module: match dotnet.get("legacy-main-module") {
            None => false,
            Some(value) => value
                .as_bool()
                .context("package.metadata.dotnet.legacy-main-module must be a boolean")?,
        },
    };
    validate_identity(&identity)?;
    Ok(Some(identity))
}

/// Print the filename/CLR identity that generic MSBuild integration must reference.
pub fn print_managed_assembly_name(path: Option<&Path>) -> Result<i32> {
    let crate_dir = path.unwrap_or_else(|| Path::new("."));
    let crate_dir = crate_dir
        .canonicalize()
        .with_context(|| format!("canonicalize Rust crate {}", crate_dir.display()))?;
    let package = cargo_package(&crate_dir)?;
    let name = resolve_managed_identity(&crate_dir)?
        .map(|identity| identity.assembly_name)
        .unwrap_or_else(|| package.name.to_string());
    println!("{name}");
    Ok(0)
}

/// Validate the deliberately narrow Wave-1 identity scope before Cargo starts a process whose
/// linker environment is inherited by every final target.  There is no per-artifact identity
/// channel yet, so a release identity may describe exactly one `cdylib`, never a workspace-wide
/// or mixed bin/library invocation.
fn validate_managed_identity_build(args: &BuildArgs, crate_dir: &Path) -> Result<()> {
    let has_package_selection = !args.workspace.package.is_empty()
        || args.workspace.workspace
        || !args.workspace.exclude.is_empty()
        || args.extra.iter().any(|flag| {
            matches!(
                flag.as_str(),
                "--workspace" | "--exclude" | "-p" | "--package"
            ) || flag.starts_with("--package=")
                || flag.starts_with("--exclude=")
        });
    if has_package_selection {
        bail!(
            "managed identity builds support exactly one selected package; remove --workspace, \
             --exclude, and -p/--package (identity is currently process-wide at final link)"
        );
    }

    let package = cargo_package(crate_dir)?;
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
        let identity = resolve_managed_identity(&crate_dir)?;
        let assembly_name = identity
            .as_ref()
            .map(|identity| identity.assembly_name.clone())
            .unwrap_or_else(|| package.name.to_string());
        let public_type = identity
            .as_ref()
            .and_then(ManagedIdentity::module_full_name)
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

fn is_clr_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_identity_projects_a_distinct_public_type() {
        let identity = ManagedIdentity {
            schema: 1,
            package_id: "Collision.Alpha".into(),
            assembly_name: "collision_alpha".into(),
            root_namespace: "Collision.Alpha".into(),
            module_type: "Exports".into(),
            legacy_main_module: false,
        };
        validate_identity(&identity).unwrap();
        assert_eq!(
            identity.module_full_name().as_deref(),
            Some("Collision.Alpha.Exports")
        );
    }

    #[test]
    fn legacy_identity_keeps_main_module() {
        let identity = ManagedIdentity {
            schema: 1,
            package_id: "legacy".into(),
            assembly_name: "legacy".into(),
            root_namespace: "Legacy".into(),
            module_type: "Exports".into(),
            legacy_main_module: true,
        };
        assert_eq!(identity.module_full_name(), None);
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
            legacy_main_module: false,
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
    let home =
        std::env::var_os("HOME").context("HOME is not set (needed for cargo-dotnet cache)")?;
    Ok(PathBuf::from(home).join(".cargo-dotnet/cache"))
}

/// Stable namespace for mutable state owned by one consumer crate. Cargo registry sources are
/// patched for the CLR PAL, so sharing a Cargo home between unrelated builds is unsafe even when
/// their target directories differ.
pub(crate) fn crate_cache_key(crate_dir: &Path) -> Result<String> {
    let canonical = crate_dir
        .canonicalize()
        .with_context(|| format!("canonicalize consumer crate {}", crate_dir.display()))?;
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical.as_os_str().to_string_lossy().as_bytes())
    ))
}

pub(crate) fn cargo_home_for_crate(crate_dir: &Path) -> Result<PathBuf> {
    Ok(cargo_dotnet_cache_home()?
        .join("crates")
        .join(crate_cache_key(crate_dir)?)
        .join("cargo-home"))
}
