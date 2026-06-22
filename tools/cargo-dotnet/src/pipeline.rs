//! The native build/run pipeline orchestrator.
//!
//! This is the Rust-native orchestration layer for `build`/`run`: it owns mode + host
//! detection, the RUSTFLAGS assembly, the profile logic, the standard-flag passthrough
//! (P2), and the assembly of every `CD_*` env var. It then shells the proven pipeline
//! CORE (`feasibility/_cargo_dotnet_core.sh` in dev, `$CARGO_DOTNET_HOME/core.sh`
//! installed) for the inner steps that remain shell — PAL injection into rust-src, the
//! `dotnet_overlays` apply, the libc-registry patch, build-std, artifact location, and
//! the run — exactly as the bash front-end did. The CLI/convention/passthrough/env are
//! Rust; the inner sed/awk pipeline is reused (not re-implemented).
//!
//! Ports the NATIVE arm of `feasibility/cargo-dotnet:640-721`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::cli::BuildArgs;
use crate::host::{self, HostFacts};
use crate::mode::Mode;
use crate::{docker, passthrough, rustflags};

/// Run `build` or `run`. `is_run` selects the run-the-apphost behaviour.
pub fn run(args: &BuildArgs, is_run: bool) -> Result<i32> {
    let mode = crate::mode::detect()?;

    // Backend selection: explicit flag/env, else native installed / docker dev.
    let backend = args.backend.clone().unwrap_or_else(|| default_backend(&mode));

    match backend.as_str() {
        "docker" => docker::run(args, is_run, &mode),
        "native" => run_native(args, is_run, &mode),
        other => bail!("unknown CARGO_DOTNET_BACKEND='{other}' (expected: native | docker)"),
    }
}

fn default_backend(mode: &Mode) -> String {
    // Installed default is native (the real user journey); dev default stays docker
    // (unchanged), but native is selectable via the flag/env.
    match mode {
        Mode::Installed { .. } => "native".to_string(),
        Mode::Dev { .. } => "docker".to_string(),
    }
}

/// Resolve the four artifact locations + the registry src + the toolchain pin for the
/// active mode.
struct NativeLayout {
    cd_repo: PathBuf,
    backend_dylib: PathBuf,
    linker: PathBuf,
    target_spec: PathBuf,
    core: PathBuf,
    lastbuild_log: PathBuf,
    /// Some only when installed (an external crate has no rustup dir-override).
    toolchain: Option<String>,
}

fn native_layout(mode: &Mode, facts: &HostFacts) -> Result<NativeLayout> {
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
            let core = home.join("core.sh");
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
            if !core.is_file() {
                bail!("installed pipeline core missing: {}", core.display());
            }
            let toolchain = std::env::var("CARGO_DOTNET_TOOLCHAIN")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| crate::mode::read_home_toolchain(home));
            Ok(NativeLayout {
                cd_repo: home.clone(),
                backend_dylib,
                linker,
                target_spec,
                core,
                lastbuild_log: home.join("_lastbuild.log"),
                toolchain: Some(toolchain),
            })
        }
        Mode::Dev { repo_root } => {
            let backend_dylib = repo_root
                .join(format!("target/release/librustc_codegen_clr.{}", facts.dylib_ext));
            let linker = repo_root.join(format!("target/release/linker{}", facts.exe_ext));
            let target_spec = repo_root.join("x86_64-unknown-dotnet.json");
            let core = repo_root.join("feasibility/_cargo_dotnet_core.sh");
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
            Ok(NativeLayout {
                cd_repo: repo_root.clone(),
                backend_dylib,
                linker,
                target_spec,
                core,
                lastbuild_log: repo_root.join("feasibility/_lastbuild.log"),
                toolchain: None,
            })
        }
    }
}

fn run_native(args: &BuildArgs, is_run: bool, mode: &Mode) -> Result<i32> {
    let crate_dir = host::resolve_crate_dir(&args.path)?;
    let facts = HostFacts::detect();

    // ---- host toolchain preflight ----
    host::ensure_rust_toolchain()?;
    let dotnet_add = host::dotnet_env_additions();
    host::ensure_dotnet(&dotnet_add)?;
    let ilasm = host::resolve_ilasm(&facts)?;

    let layout = native_layout(mode, &facts)?;

    // ---- registry src dir build-std extracts libc into ----
    let registry_src = cargo_registry_src()?;

    // ---- standard-flag passthrough (P2) + program args ----
    let cargo_flags = passthrough::assemble_cargo_flags(args);
    let prog_args = &args.prog_args;

    // ---- assemble the child env (the CD_* seam the core reads) ----
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    env.insert(
        "RUSTFLAGS".to_string(),
        rustflags::assemble(&layout.backend_dylib, &layout.linker),
    );
    env.insert("CD_REPO".to_string(), path_str(&layout.cd_repo));
    env.insert("CD_BACKEND_DYLIB".to_string(), path_str(&layout.backend_dylib));
    env.insert("CD_LINKER".to_string(), path_str(&layout.linker));
    env.insert("CD_TARGET_SPEC".to_string(), path_str(&layout.target_spec));
    env.insert("CD_REGISTRY_SRC".to_string(), path_str(&registry_src));
    env.insert("CD_LASTBUILD_LOG".to_string(), path_str(&layout.lastbuild_log));
    env.insert("CD_EXE_EXT".to_string(), facts.exe_ext.to_string());
    env.insert("CD_REL".to_string(), if args.is_release() { "1" } else { "0" }.to_string());
    env.insert("CD_RUN".to_string(), if is_run { "1" } else { "0" }.to_string());
    env.insert("CD_CLEAN".to_string(), if args.clean { "1" } else { "0" }.to_string());
    env.insert("CD_VERBOSE".to_string(), if args.verbose { "1" } else { "0" }.to_string());

    // The P2 passthrough: standard + verbatim cargo flags -> the inner build.
    if !cargo_flags.is_empty() {
        env.insert("CD_EXTRA_CARGO_FLAGS".to_string(), shell_join(&cargo_flags));
    }

    // Honour $CARGO for the inner cargo.
    env.insert("CARGO".to_string(), host::inner_cargo());

    // ilasm (CoreCLR, exported for the cilly linker).
    if let Some(ilasm) = &ilasm {
        env.insert("ILASM_PATH".to_string(), path_str(ilasm));
    }

    // dotnet self-heal from $HOME/.dotnet.
    if let Some((path_add, dotnet_root)) = &dotnet_add {
        let cur = std::env::var("PATH").unwrap_or_default();
        env.insert("PATH".to_string(), format!("{}:{}", path_add.display(), cur));
        env.insert("DOTNET_ROOT".to_string(), path_str(dotnet_root));
    }

    // External crate: pin the toolchain (no rustup dir-override).
    if let Some(tc) = &layout.toolchain {
        env.insert("RUSTUP_TOOLCHAIN".to_string(), tc.clone());
    }

    // Quieter, deterministic dotnet.
    env_default(&mut env, "DOTNET_CLI_TELEMETRY_OPTOUT", "1");
    env_default(&mut env, "DOTNET_NOLOGO", "1");
    env_default(&mut env, "DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1");
    env.insert("CARGO_TERM_COLOR".to_string(), "never".to_string());

    // ---- run the CORE in the crate dir, forwarding program args ----
    let status = Command::new("bash")
        .arg("-o")
        .arg("pipefail")
        .arg(&layout.core)
        .args(prog_args)
        .current_dir(&crate_dir)
        .envs(&env)
        .status()
        .with_context(|| format!("failed to launch the pipeline core: {}", layout.core.display()))?;

    Ok(status.code().unwrap_or(1))
}

fn cargo_registry_src() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("CARGO_HOME") {
        if !home.is_empty() {
            return Ok(PathBuf::from(home).join("registry/src"));
        }
    }
    let home = std::env::var("HOME").context("HOME is not set (needed to locate ~/.cargo)")?;
    Ok(PathBuf::from(home).join(".cargo/registry/src"))
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn env_default(env: &mut BTreeMap<String, String>, key: &str, val: &str) {
    if std::env::var(key).map(|v| v.is_empty()).unwrap_or(true) {
        env.entry(key.to_string()).or_insert_with(|| val.to_string());
    }
}

/// Join flags into a single space-separated string for `CD_EXTRA_CARGO_FLAGS`.
/// The core word-splits this, so individual tokens must not contain spaces; cargo
/// flag values rarely do, and a path with a space would be the user's responsibility
/// (documented). We keep it simple and predictable.
fn shell_join(flags: &[String]) -> String {
    flags.join(" ")
}
