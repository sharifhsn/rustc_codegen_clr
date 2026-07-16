//! `build-std` invocation — typed port of the bash build block (core 626-744).
//!
//! Sets the backend RUSTFLAGS + the dotnet/ilasm env, runs `cargo fetch` then patches
//! the libc REGISTRY copy (the post-fetch second pass), runs the filtered/verbose
//! build, then a final `--message-format=json` pass whose stdout it returns for
//! `artifact::locate`. This is the ONE place a child env is constructed on the native
//! path (the inner cargo); everything else is typed Rust.

use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use anyhow::{Context as _, Result, bail};

use crate::context::{Context, DotnetVersion};
use crate::private_sysroot::PrivateSysroot;
use crate::{palinject, rustflags};

/// Build against a provisioned private sysroot. This is the ordinary native pipeline path.
pub fn build_with_sysroot(ctx: &Context, sysroot: &PrivateSysroot) -> Result<String> {
    if !ctx.crate_dir.join("Cargo.toml").is_file() {
        bail!(
            "not a crate dir (no Cargo.toml): {}",
            ctx.crate_dir.display()
        );
    }
    eprintln!(
        "==> cargo dotnet: building {} (profile={})",
        ctx.crate_dir.display(),
        ctx.profile.dir()
    );

    let target_dir = cargo_target_dir(ctx, sysroot)?;
    let sysroot_changed = target_uses_other_sysroot(&target_dir, &sysroot.root)?;
    if ctx.flags.clean || sysroot_changed {
        if sysroot_changed && !ctx.flags.clean {
            eprintln!("==> private sysroot changed; invalidating stale Cargo target fingerprints");
        }
        eprintln!("==> cargo clean (full, bulletproof)");
        let _ = base_cargo(ctx, sysroot).arg("clean").status();
    }

    // `cargo fetch` materialises registry sources WITHOUT compiling, so we can patch the
    // registry libc copy before it is compiled (the std::os::fd `libc::` refs fail on an
    // unpatched registry libc). `-Zjson-target-spec` is the unstable flag the dotnet
    // target spec (a JSON file) needs — it must NOT be dropped.
    fetch_dependencies(ctx, sysroot)?;

    // The build pass. Stream combined output while also preserving it in lastbuild_log.
    // A previous `.output()` implementation waited until rustc + the linker had both
    // finished before printing anything, which made an ordinary build look hung for
    // long stretches. Keep the concise default view, but emit its progress lines as
    // they happen; --verbose still emits every line.
    let mut build_cmd = base_cargo(ctx, sysroot);
    build_cmd.arg("-Zjson-target-spec").arg("build");
    if let Some(flag) = ctx.profile.cargo_flag() {
        build_cmd.arg(flag);
    }
    for f in &ctx.flags.extra_cargo {
        build_cmd.arg(f);
    }
    let mut child = build_cmd
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to launch the inner build-std cargo")?;

    let (tx, rx) = mpsc::channel();
    let stdout_reader = spawn_line_reader(
        child
            .stdout
            .take()
            .context("inner cargo stdout was not piped")?,
        tx.clone(),
    );
    let stderr_reader = spawn_line_reader(
        child
            .stderr
            .take()
            .context("inner cargo stderr was not piped")?,
        tx,
    );
    let mut log = String::new();
    for line in rx {
        log.push_str(&line);
        log.push('\n');
        if ctx.flags.verbose || is_interesting(&line) {
            eprintln!("{line}");
        }
    }
    let status = child.wait().context("wait for the inner build-std cargo")?;
    stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("inner cargo stdout reader panicked"))?;
    stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("inner cargo stderr reader panicked"))?;
    if let Some(parent) = ctx.paths.lastbuild_log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&ctx.paths.lastbuild_log, &log);
    if !status.success() && !ctx.flags.verbose {
        eprintln!("== full inner build log after failure ==");
        eprint!("{log}");
    }
    if ctx.flags.verbose || !status.success() {
        eprintln!("== build exit: {} ==", status.code().unwrap_or(-1));
    }
    if !status.success() {
        bail!(
            "inner cargo build failed (exit {})",
            status.code().unwrap_or(-1)
        );
    }
    record_target_sysroot(&target_dir, &sysroot.root)?;

    // The JSON pass: same flags + --message-format=json; capture stdout for the locator.
    let mut json_cmd = base_cargo(ctx, sysroot);
    json_cmd.arg("-Zjson-target-spec").arg("build");
    if let Some(flag) = ctx.profile.cargo_flag() {
        json_cmd.arg(flag);
    }
    for f in &ctx.flags.extra_cargo {
        json_cmd.arg(f);
    }
    json_cmd.arg("--message-format=json");
    let out = json_cmd
        .stderr(Stdio::null())
        .output()
        .context("failed to run the --message-format=json build pass")?;
    if !out.status.success() {
        bail!(
            "inner cargo JSON build failed (exit {})",
            out.status.code().unwrap_or(-1)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

const TARGET_SYSROOT_MARKER: &str = ".rustdotnet-private-sysroot";

fn cargo_target_dir(ctx: &Context, sysroot: &PrivateSysroot) -> Result<PathBuf> {
    let output = base_cargo(ctx, sysroot)
        .arg("-Zjson-target-spec")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version=1")
        .output()
        .context("query Cargo target directory")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed while locating the target directory:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let path = metadata
        .get("target_directory")
        .and_then(serde_json::Value::as_str)
        .context("cargo metadata omitted target_directory")?;
    Ok(PathBuf::from(path))
}

fn target_uses_other_sysroot(target_dir: &Path, sysroot: &Path) -> Result<bool> {
    if !target_dir.exists() {
        return Ok(false);
    }
    let marker = target_dir.join(TARGET_SYSROOT_MARKER);
    if !marker.is_file() {
        // A pre-marker target can contain build-std fingerprints with an arbitrary private path.
        return Ok(true);
    }
    Ok(std::fs::read_to_string(marker)?.trim() != sysroot.to_string_lossy())
}

fn record_target_sysroot(target_dir: &Path, sysroot: &Path) -> Result<()> {
    std::fs::create_dir_all(target_dir)?;
    std::fs::write(
        target_dir.join(TARGET_SYSROOT_MARKER),
        format!("{}\n", sysroot.display()),
    )?;
    Ok(())
}

fn spawn_line_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    tx: mpsc::Sender<String>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    })
}

/// The default build-log filter: keep failures, actionable warnings, normal Cargo
/// compilation progress, linker stage progress, and the final Cargo summary.
fn is_interesting(line: &str) -> bool {
    let l = line.trim_start();
    l.starts_with("error")
        || l.contains("error[")
        || l.contains("could not compile")
        || l.starts_with("warning: unused")
        || l.starts_with("Compiling ")
        || l.starts_with("Linking ")
        || l.starts_with("==>")
        || l.starts_with("Finished")
}

/// Patch the libc REGISTRY copies (post-`cargo fetch`). build-std resolves libc from the
/// registry, not the rust-src vendor tree, so this covers whichever copy it picks.
pub(crate) fn fetch_dependencies(ctx: &Context, sysroot: &PrivateSysroot) -> Result<()> {
    let mut command = base_cargo(ctx, sysroot);
    command.arg("-Zjson-target-spec").arg("fetch");
    for flag in dependency_fetch_flags(ctx) {
        command.arg(flag);
    }
    let status = command.status().context("run cargo fetch")?;
    if !status.success() {
        bail!(
            "cargo fetch failed (exit {}); run `cargo dotnet restore {}` while network access is available",
            status.code().unwrap_or(-1),
            ctx.crate_dir.display()
        );
    }
    patch_registry_libc(ctx)
}

fn dependency_fetch_flags(ctx: &Context) -> Vec<String> {
    let mut selected = Vec::new();
    let mut flags = ctx.flags.extra_cargo.iter();
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--offline" | "--locked" | "--frozen" => selected.push(flag.clone()),
            "--manifest-path" | "--target" => {
                selected.push(flag.clone());
                if let Some(value) = flags.next() {
                    selected.push(value.clone());
                }
            }
            value if value.starts_with("--manifest-path=") || value.starts_with("--target=") => {
                selected.push(flag.clone());
            }
            _ => {}
        }
    }
    selected
}

fn dependency_metadata_flags(ctx: &Context) -> Vec<String> {
    let mut selected = Vec::new();
    let mut flags = ctx.flags.extra_cargo.iter();
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--offline" | "--locked" | "--frozen" | "--all-features" | "--no-default-features" => {
                selected.push(flag.clone())
            }
            "--manifest-path" | "--features" => {
                selected.push(flag.clone());
                if let Some(value) = flags.next() {
                    selected.push(value.clone());
                }
            }
            value if value.starts_with("--manifest-path=") || value.starts_with("--features=") => {
                selected.push(flag.clone());
            }
            _ => {}
        }
    }
    selected
}

pub(crate) fn local_manifest_paths(
    ctx: &Context,
    sysroot: &PrivateSysroot,
) -> Result<Vec<PathBuf>> {
    let mut command = base_cargo(ctx, sysroot);
    command
        .arg("-Zjson-target-spec")
        .arg("metadata")
        .arg("--format-version=1");
    for flag in dependency_metadata_flags(ctx) {
        command.arg(flag);
    }
    let output = command
        .output()
        .context("query resolved Cargo manifests for restore receipt")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed while writing restore receipt:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let mut manifests = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|package| package["source"].is_null())
        .filter_map(|package| package["manifest_path"].as_str())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    manifests.sort();
    manifests.dedup();
    Ok(manifests)
}

fn patch_registry_libc(ctx: &Context) -> Result<()> {
    for d in palinject::find_libc_dirs(&ctx.paths.registry_src) {
        if palinject::patch_libc(&d)? && ctx.flags.verbose {
            eprintln!("==> patched registry libc: {}", d.display());
        }
    }
    Ok(())
}

/// A cargo Command pre-loaded with the backend RUSTFLAGS + the dotnet/ilasm env + the
/// pinned toolchain (installed only) + quiet/deterministic dotnet knobs. Runs in the
/// crate dir.
fn base_cargo(ctx: &Context, sysroot: &PrivateSysroot) -> Command {
    let mut cmd = Command::new(&ctx.cargo);
    cmd.current_dir(&ctx.crate_dir);
    cmd.env("CARGO_HOME", &ctx.paths.cargo_home);
    if let Some(config) = crate::overlays::ambient_cargo_config(ctx) {
        cmd.arg("--config").arg(config);
    }
    cmd.arg("--config")
        .arg(crate::overlays::generated_config_path(ctx));

    // The backend RUSTFLAGS (verbatim incl. the getrandom custom-backend embedded quotes).
    cmd.env(
        "RUSTFLAGS",
        rustflags::assemble(
            &ctx.paths.backend_dylib,
            &ctx.paths.linker,
            &ctx.paths.sdk_crates_root,
            ctx.dotnet.as_env(),
            &[
                (&ctx.paths.sdk_crates_root, "/_/rust-dotnet-sdk"),
                (&ctx.crate_dir, "/_/consumer"),
                (&ctx.paths.cargo_home, "/_/cargo-home"),
                (&sysroot.root, "/_/rust-sysroot"),
            ],
            ctx.source_link_url.as_deref(),
        ),
    );
    match &ctx.source_link_url {
        Some(url) => {
            let json = serde_json::json!({
                "documents": {
                    "/_/consumer/*": url,
                }
            });
            cmd.env(
                "RCL_SOURCE_LINK_JSON",
                serde_json::to_string(&json).unwrap(),
            );
        }
        None => {
            cmd.env_remove("RCL_SOURCE_LINK_JSON");
        }
    }
    cmd.env("RUST_LIB_SRC", &sysroot.library);
    // RUSTFLAGS does not affect Cargo's rustc discovery calls. Using this executable as
    // RUSTC_WRAPPER makes `rustc --print sysroot` return the private snapshot too, so
    // build-std discovers and compiles the injected copy rather than ambient rust-src.
    if let Ok(wrapper) = std::env::current_exe() {
        cmd.env("RUSTC_WRAPPER", wrapper);
        cmd.env("CARGO_DOTNET_PRIVATE_SYSROOT", &sysroot.root);
    }

    // Pin the toolchain when installed (no rustup dir-override for an external crate).
    if let Some(tc) = &ctx.toolchain {
        cmd.env("RUSTUP_TOOLCHAIN", tc);
    }

    // ilasm (CoreCLR, exported for the cilly linker; version-matched in `host::resolve_ilasm`).
    if let Some(ilasm) = &ctx.ilasm {
        cmd.env("ILASM_PATH", ilasm);
    }

    // Target .NET version — the SINGLE seam: exported so BOTH the codegen backend (rustc, which
    // reads it via cilly) AND the cilly linker (a separate process: runtimeconfig + `.ver` stamps)
    // target the same runtime. Pairs with the version-matched ILASM_PATH above.
    cmd.env("DOTNET_VERSION", ctx.dotnet.as_env());
    if matches!(ctx.dotnet, DotnetVersion::UnityNetStandard21) {
        // Keep the serialized codegen shards and final linker on one AOT-safe ABI contract.
        // The Unity-specific export shim avoids catch_unwind; the backend's existing NO_UNWIND
        // lowering remains preferable to `-C panic=abort`, which changes MIR layouts in ways the
        // backend does not yet support for build-std.
        cmd.env("NO_UNWIND", "1");
    }

    configure_managed_identity_env(&mut cmd, ctx.managed_identity());

    // dotnet self-heal from $HOME/.dotnet.
    if let Some((path_add, dotnet_root)) = &ctx.dotnet_heal {
        let cur = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{}", path_add.display(), cur));
        cmd.env("DOTNET_ROOT", dotnet_root);
    }

    // Quieter, deterministic dotnet + cargo.
    cmd.env("DOTNET_CLI_TELEMETRY_OPTOUT", "1");
    cmd.env("DOTNET_NOLOGO", "1");
    cmd.env("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1");
    cmd.env("CARGO_TERM_COLOR", "never");
    cmd
}

/// Scrub every identity field before applying this build's one explicit identity. Cargo runs
/// build scripts and dependencies in child processes, so inheriting an old shell identity would
/// otherwise turn a legacy crate into a partially stamped release artifact.
fn configure_managed_identity_env(
    cmd: &mut Command,
    identity: Option<&crate::context::ManagedIdentity>,
) {
    const IDENTITY_ENV: &[&str] = &[
        "RCL_MANAGED_IDENTITY_SCHEMA",
        "RCL_MANAGED_PACKAGE_ID",
        "RCL_MANAGED_ASSEMBLY_NAME",
        "RCL_MANAGED_ROOT_NAMESPACE",
        "RCL_MANAGED_MODULE_TYPE",
        "RCL_LEGACY_MAIN_MODULE",
    ];
    for key in IDENTITY_ENV {
        cmd.env_remove(key);
    }

    // Link-time projection is intentionally process-local: the serialized CIL still uses the
    // historical MainModule sentinel, and only an opted-in package's final managed artifact is
    // given a public namespace/type identity.
    if let Some(identity) = identity {
        cmd.env("RCL_MANAGED_IDENTITY_SCHEMA", identity.schema.to_string());
        cmd.env("RCL_MANAGED_PACKAGE_ID", &identity.package_id);
        cmd.env("RCL_MANAGED_ASSEMBLY_NAME", &identity.assembly_name);
        cmd.env("RCL_MANAGED_ROOT_NAMESPACE", &identity.root_namespace);
        cmd.env("RCL_MANAGED_MODULE_TYPE", &identity.module_type);
        cmd.env(
            "RCL_LEGACY_MAIN_MODULE",
            if identity.legacy_main_module {
                "1"
            } else {
                "0"
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ManagedIdentity;

    #[test]
    fn legacy_build_scrubs_ambient_managed_identity() {
        let mut command = Command::new("cargo");
        configure_managed_identity_env(&mut command, None);
        let envs: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        for key in [
            "RCL_MANAGED_IDENTITY_SCHEMA",
            "RCL_MANAGED_PACKAGE_ID",
            "RCL_MANAGED_ASSEMBLY_NAME",
            "RCL_MANAGED_ROOT_NAMESPACE",
            "RCL_MANAGED_MODULE_TYPE",
            "RCL_LEGACY_MAIN_MODULE",
        ] {
            assert_eq!(envs.get(std::ffi::OsStr::new(key)), Some(&None));
        }
    }

    #[test]
    fn identity_build_replaces_ambient_identity_without_overriding_exporter_selection() {
        let mut command = Command::new("cargo");
        let identity = ManagedIdentity {
            schema: 1,
            package_id: "Example.Widget".into(),
            assembly_name: "example_widget".into(),
            root_namespace: "Example.Widget".into(),
            module_type: "Exports".into(),
            legacy_main_module: false,
        };
        configure_managed_identity_env(&mut command, Some(&identity));
        let envs: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            envs.get(std::ffi::OsStr::new("RCL_MANAGED_ASSEMBLY_NAME")),
            Some(&Some(std::ffi::OsStr::new("example_widget")))
        );
        assert!(!envs.contains_key(std::ffi::OsStr::new("DIRECT_PE")));
    }

    #[test]
    fn default_build_output_keeps_consumer_and_linker_progress() {
        assert!(is_interesting(
            "   Compiling customer_app v0.1.0 (/tmp/customer_app)"
        ));
        assert!(is_interesting("==> Optimizing in 1.2s"));
        assert!(is_interesting("    Finished `dev` profile"));
        assert!(!is_interesting("    Checking serde v1.0.0"));
    }
}
