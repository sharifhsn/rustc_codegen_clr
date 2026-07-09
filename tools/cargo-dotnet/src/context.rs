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

use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context as _, Result};

use crate::cli::BuildArgs;
use crate::host::HostFacts;
use crate::mode::Mode;
use crate::passthrough;
use crate::{host, mode};

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
    /// `root/dotnet_pal`.
    pub pal_root: PathBuf,
    /// `root/dotnet_overlays`.
    pub overlays_root: PathBuf,
    /// `root/mycorrhiza_interop_helpers` — the bundled `Mycorrhiza.Interop.Helpers` C# companion
    /// project (currently just `ParameterRebinder`, see `mycorrhiza::linq`). Built and copied
    /// alongside any consumer's build output by `interop_helpers::ensure_and_copy`.
    pub interop_helpers_root: PathBuf,
    pub lastbuild_log: PathBuf,
}

impl Paths {
    /// Resolve the layout for a mode (the old `NativeLayout`, kept with its bail!
    /// preflights). Installed and Dev differ ONLY here.
    fn resolve(mode: &Mode, facts: &HostFacts) -> Result<Self> {
        let registry_src = cargo_registry_src()?;
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
                    pal_root: home.join("dotnet_pal"),
                    overlays_root: home.join("dotnet_overlays"),
                    interop_helpers_root: home.join("mycorrhiza_interop_helpers"),
                    lastbuild_log: home.join("_lastbuild.log"),
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
                    pal_root: repo_root.join("dotnet_pal"),
                    overlays_root: repo_root.join("dotnet_overlays"),
                    interop_helpers_root: repo_root.join("mycorrhiza_interop_helpers"),
                    lastbuild_log: repo_root.join("feasibility/_lastbuild.log"),
                })
            }
        }
    }
}

/// The target .NET runtime version selected by `--dotnet` (env `DOTNET_VERSION`). The front-end is
/// the *producer* of the version: it exports `DOTNET_VERSION` to the inner cargo (so both the codegen
/// backend and the cilly linker see it) and selects the matching CoreCLR ilasm. It deliberately does
/// NOT know per-version BCL tokens / `.ver` strings — those live in cilly (`cilly::DotnetVersion`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DotnetVersion {
    /// .NET 8 (default).
    #[default]
    Net8,
    /// .NET 9.
    Net9,
}
impl DotnetVersion {
    /// Target-framework moniker for NuGet packaging (`net8.0` / `net9.0`).
    #[must_use]
    pub fn tfm(self) -> &'static str {
        match self {
            DotnetVersion::Net8 => "net8.0",
            DotnetVersion::Net9 => "net9.0",
        }
    }
    /// The `DOTNET_VERSION` value the cilly/backend parser expects (canonical bare major).
    #[must_use]
    pub fn as_env(self) -> &'static str {
        match self {
            DotnetVersion::Net8 => "8",
            DotnetVersion::Net9 => "9",
        }
    }
    /// The matching CoreCLR ilasm tool dir under `$HOME/.dotnet` (each runtime needs its own — a
    /// net8 ilasm's PE is rejected by the net9 runtime and vice-versa).
    #[must_use]
    pub fn ilasm_tool_dir(self) -> &'static str {
        match self {
            DotnetVersion::Net8 => "ilasm-tool",
            DotnetVersion::Net9 => "ilasm9-tool",
        }
    }
}
impl std::str::FromStr for DotnetVersion {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "8" | "net8" | "net8.0" => Ok(DotnetVersion::Net8),
            "9" | "net9" | "net9.0" => Ok(DotnetVersion::Net9),
            other => Err(format!("--dotnet: invalid value {other:?}; expected 8 or 9")),
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
    /// Some(toolchain) when installed (pinned via RUSTUP_TOOLCHAIN); None in dev (the
    /// rustup dir-override / active toolchain is used).
    pub toolchain: Option<String>,
    /// The inner cargo binary (`$CARGO` or `cargo`).
    pub cargo: String,
    /// A resolved CoreCLR ilasm to export as ILASM_PATH (None lets cilly's default fire).
    pub ilasm: Option<PathBuf>,
    /// The target .NET runtime version (`--dotnet`). Exported as `DOTNET_VERSION` to the inner cargo.
    pub dotnet: DotnetVersion,
    /// `(PATH addition, DOTNET_ROOT)` if dotnet was self-healed from `$HOME/.dotnet`.
    pub dotnet_heal: Option<(PathBuf, PathBuf)>,
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

        let paths = Paths::resolve(&mode, &host)?;

        let toolchain = match &mode {
            Mode::Installed { home } => Some(
                std::env::var("CARGO_DOTNET_TOOLCHAIN")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| mode::read_home_toolchain(home)),
            ),
            Mode::Dev { .. } => None,
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
        let cargo = if toolchain.is_some() { "cargo".to_string() } else { host::inner_cargo() };

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
        })
    }

    /// The toolchain's sysroot (`rustc --print sysroot`), honouring the pinned
    /// `RUSTUP_TOOLCHAIN` when installed. Used to locate rust-src for PAL injection.
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

/// The cargo registry src dir build-std extracts libc into.
fn cargo_registry_src() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("CARGO_HOME") {
        if !home.is_empty() {
            return Ok(PathBuf::from(home).join("registry/src"));
        }
    }
    let home = std::env::var("HOME").context("HOME is not set (needed to locate ~/.cargo)")?;
    Ok(PathBuf::from(home).join(".cargo/registry/src"))
}
