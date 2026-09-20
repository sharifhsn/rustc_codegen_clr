//! `build-std` invocation — typed port of the bash build block (core 626-744).
//!
//! Sets the backend RUSTFLAGS + the dotnet env, runs `cargo fetch` then patches
//! the libc REGISTRY copy (the post-fetch second pass), then runs one JSON-message
//! build whose human diagnostics are streamed while its stdout is retained for
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
    build_with_invocation(ctx, sysroot, CargoInvocation::Build)
}

/// Build test harnesses without running them.
///
/// Cargo's `build --lib` selects the ordinary library artifact, not the package
/// library's unit-test harness. The latter is only produced by `test --no-run
/// --lib`, so the test pipeline must use Cargo's test compilation mode while
/// retaining the same build-std/backend setup as an ordinary product build.
pub fn build_tests_with_sysroot(ctx: &Context, sysroot: &PrivateSysroot) -> Result<String> {
    build_with_invocation(ctx, sysroot, CargoInvocation::TestNoRun)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CargoInvocation {
    Build,
    TestNoRun,
}

impl CargoInvocation {
    fn append_to(self, command: &mut Command) {
        match self {
            Self::Build => {
                command.arg("build");
            }
            Self::TestNoRun => {
                command.arg("test").arg("--no-run");
            }
        }
    }
}

fn build_with_invocation(
    ctx: &Context,
    sysroot: &PrivateSysroot,
    invocation: CargoInvocation,
) -> Result<String> {
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
        eprintln!("==> cargo clean");
        let mut clean = base_cargo(ctx, sysroot)?;
        clean.arg("-Zjson-target-spec").arg("clean");
        for flag in target_dir_routing_flags(&ctx.flags.extra_cargo) {
            clean.arg(flag);
        }
        let _ = clean.status();
    }

    // `cargo fetch` materialises registry sources WITHOUT compiling, so we can patch the
    // registry libc copy before it is compiled (the std::os::fd `libc::` refs fail on an
    // unpatched registry libc). `-Zjson-target-spec` is the unstable flag the dotnet
    // target spec (a JSON file) needs — it must NOT be dropped.
    fetch_dependencies(ctx, sysroot)?;

    // The build pass. Cargo's JSON stdout is the artifact locator's source of truth; human
    // progress remains on stderr, while compiler diagnostics are rendered from JSON messages.
    // Stream both concurrently so a verbose build remains live and a noisy child cannot fill a
    // pipe. This used to be followed by a second, otherwise redundant Cargo build solely to
    // rediscover artifacts.
    // A previous `.output()` implementation waited until rustc + the linker had both
    // finished before printing anything, which made an ordinary build look hung for
    // long stretches. Keep the concise default view, but emit its progress lines as
    // they happen; --verbose still emits every line.
    let mut build_cmd = base_cargo(ctx, sysroot)?;
    build_cmd.arg("-Zjson-target-spec");
    invocation.append_to(&mut build_cmd);
    if invocation == CargoInvocation::TestNoRun {
        // A test harness needs to catch panics even when the consumer workspace's release
        // profile is deliberately `panic = "abort"` (as compiler-builtins/libm are).  Without
        // this per-invocation override, build-std compiles two incompatible copies of `core`
        // (one unwind, one abort), which rustc rejects as duplicate lang items.  Keep ordinary
        // product builds faithful to the consumer profile; only the test compilation gets the
        // harness-required unwind strategy.
        let env_key = format!(
            "CARGO_PROFILE_{}_PANIC",
            ctx.profile.dir().replace('-', "_").to_ascii_uppercase()
        );
        build_cmd.env(env_key, "unwind");
    }
    if let Some(flag) = ctx.profile.cargo_flag() {
        build_cmd.arg(flag);
    }
    for f in &ctx.flags.extra_cargo {
        build_cmd.arg(f);
    }
    build_cmd.arg("--message-format=json-render-diagnostics");
    let mut child = build_cmd
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to launch the inner build-std cargo")?;

    let (tx, rx) = mpsc::channel();
    let stdout_reader = spawn_stream_reader(
        child
            .stdout
            .take()
            .context("inner cargo stdout was not piped")?,
        CargoStream::Stdout,
        tx.clone(),
    );
    let stderr_reader = spawn_stream_reader(
        child
            .stderr
            .take()
            .context("inner cargo stderr was not piped")?,
        CargoStream::Stderr,
        tx,
    );
    let mut log = String::new();
    let mut json_stdout = Vec::new();
    let mut stream_error = None;
    // A misbehaving build descendant that deliberately keeps Cargo's inherited pipes open can
    // still delay EOF here after Cargo exits. We must drain concurrently to avoid the ordinary
    // full-pipe deadlock; imposing a timeout safely would require process-group ownership that
    // Cargo does not currently expose on every supported host.
    for event in rx {
        match event {
            CargoStreamEvent::Chunk(chunk) => match chunk.stream {
                CargoStream::Stdout => {
                    json_stdout.extend_from_slice(&chunk.bytes);
                    let line = String::from_utf8_lossy(&chunk.bytes);
                    if let Some(output) =
                        cargo_json_human_output(line.trim_end_matches(['\r', '\n']))
                    {
                        record_human_output(&mut log, &output, ctx.flags.verbose);
                    }
                }
                CargoStream::Stderr => {
                    let line = String::from_utf8_lossy(&chunk.bytes);
                    record_human_output(&mut log, &line, ctx.flags.verbose);
                }
            },
            CargoStreamEvent::ReadError { stream, error } => {
                stream_error.get_or_insert_with(|| {
                    anyhow::anyhow!("failed reading inner cargo {stream:?}: {error}")
                });
            }
        }
    }
    let status = child.wait().context("wait for the inner build-std cargo")?;
    let stdout_join_error = stdout_reader
        .join()
        .err()
        .map(|_| anyhow::anyhow!("inner cargo stdout reader panicked"));
    let stderr_join_error = stderr_reader
        .join()
        .err()
        .map(|_| anyhow::anyhow!("inner cargo stderr reader panicked"));
    let log_result = persist_build_log(&ctx.paths.lastbuild_log, &log);
    if !status.success() && !ctx.flags.verbose {
        eprintln!("== full inner build log after failure ==");
        eprint!("{log}");
    }
    if ctx.flags.verbose || !status.success() {
        eprintln!("== build exit: {} ==", status.code().unwrap_or(-1));
    }
    if !status.success() {
        if let Err(error) = log_result {
            eprintln!("warning: could not persist failed build log: {error:#}");
        }
        bail!(
            "inner cargo build failed (exit {})",
            status.code().unwrap_or(-1)
        );
    }
    if let Some(error) = stream_error {
        return Err(error);
    }
    if let Some(error) = stdout_join_error.or(stderr_join_error) {
        return Err(error);
    }
    log_result?;
    record_target_sysroot(&target_dir, &sysroot.root)?;
    Ok(String::from_utf8_lossy(&json_stdout).into_owned())
}

const TARGET_SYSROOT_MARKER: &str = ".rustdotnet-private-sysroot";

fn cargo_target_dir(ctx: &Context, sysroot: &PrivateSysroot) -> Result<PathBuf> {
    let mut command = base_cargo(ctx, sysroot)?;
    // Unlike `cargo build` and `cargo clean`, `cargo metadata` does not accept a
    // `--target-dir` option. Route the caller's final explicit value through
    // Cargo's equivalent environment variable so this query observes the same
    // directory as the build without forwarding invalid target selectors.
    if let Some(target_dir) = explicit_target_dir(&ctx.flags.extra_cargo)? {
        command.env("CARGO_TARGET_DIR", target_dir);
    }
    command
        .arg("-Zjson-target-spec")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version=1");
    let output = command.output().context("query Cargo target directory")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed while locating the target directory:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    metadata
        .get("target_directory")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .context("cargo metadata omitted target_directory")
}

fn target_uses_other_sysroot(target_dir: &Path, sysroot: &Path) -> Result<bool> {
    if !target_dir.exists() {
        return Ok(false);
    }
    let marker = target_dir.join(TARGET_SYSROOT_MARKER);
    if !marker.is_file() {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CargoStream {
    Stdout,
    Stderr,
}

struct CargoStreamChunk {
    stream: CargoStream,
    bytes: Vec<u8>,
}

enum CargoStreamEvent {
    Chunk(CargoStreamChunk),
    ReadError {
        stream: CargoStream,
        error: std::io::Error,
    },
}

fn spawn_stream_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    stream: CargoStream,
    tx: mpsc::Sender<CargoStreamEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        loop {
            let mut bytes = Vec::new();
            match reader.read_until(b'\n', &mut bytes) {
                Ok(0) => break,
                Err(error) => {
                    let _ = tx.send(CargoStreamEvent::ReadError { stream, error });
                    break;
                }
                Ok(_) => {}
            }
            if tx
                .send(CargoStreamEvent::Chunk(CargoStreamChunk { stream, bytes }))
                .is_err()
            {
                break;
            }
        }
    })
}

fn cargo_json_human_output(line: &str) -> Option<String> {
    let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
        return Some(line.to_owned());
    };
    match message.get("reason").and_then(serde_json::Value::as_str) {
        Some("compiler-message") => message
            .get("message")
            .and_then(|diagnostic| {
                diagnostic
                    .get("rendered")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| {
                        diagnostic
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                    })
            })
            .map(str::to_owned)
            .or_else(|| Some(line.to_owned())),
        Some("compiler-artifact" | "build-script-executed" | "build-finished") => None,
        _ => Some(line.to_owned()),
    }
}

fn record_human_output(log: &mut String, output: &str, verbose: bool) {
    log.push_str(output);
    if !output.ends_with('\n') {
        log.push('\n');
    }
    for line in output.lines() {
        if verbose || is_interesting(line) {
            eprintln!("{line}");
        }
    }
}

fn persist_build_log(path: &Path, log: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create build log directory {}", parent.display()))?;
    }
    std::fs::write(path, log).with_context(|| format!("write build log {}", path.display()))?;
    Ok(())
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
    let mut command = base_cargo(ctx, sysroot)?;
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
    forward_cargo_flags(
        &ctx.flags.extra_cargo,
        &["--manifest-path", "--target"],
        &["--offline", "--locked", "--frozen"],
    )
}

/// Cargo's target directory is a routing decision shared by build, metadata, clean, and the
/// private-sysroot marker. Forward only this option to commands that cannot accept target
/// selectors such as `--lib`/`--test`; forwarding the whole build flag vector would make `cargo
/// clean` invalid. Preserve repeated and `--target-dir=...` forms exactly so Cargo remains the
/// authority if a caller supplied conflicting values.
fn target_dir_routing_flags(extra_cargo: &[String]) -> Vec<String> {
    forward_cargo_flags(extra_cargo, &["--target-dir"], &[])
}

fn forward_cargo_flags(
    extra_cargo: &[String],
    value_flags: &[&str],
    plain_flags: &[&str],
) -> Vec<String> {
    let mut selected = Vec::new();
    let mut flags = extra_cargo.iter();
    while let Some(flag) = flags.next() {
        let key = flag.split_once('=').map_or(flag.as_str(), |(key, _)| key);
        if plain_flags.iter().any(|candidate| *candidate == key) {
            selected.push(flag.clone());
        } else if value_flags.iter().any(|candidate| *candidate == key) {
            selected.push(flag.clone());
            if !flag.contains('=') {
                if let Some(value) = flags.next() {
                    selected.push(value.clone());
                }
            }
        }
    }
    selected
}

/// Return the final explicit Cargo target directory, matching Cargo's last-value-wins
/// handling when the option is repeated. `cargo metadata` has no `--target-dir` option,
/// so callers use this value through `CARGO_TARGET_DIR` instead.
fn explicit_target_dir(extra_cargo: &[String]) -> Result<Option<String>> {
    let mut selected = None;
    let mut flags = extra_cargo.iter();
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--target-dir" => {
                let value = flags
                    .next()
                    .context("--target-dir requires a directory argument")?;
                if value.is_empty() {
                    bail!("--target-dir requires a non-empty directory argument");
                }
                selected = Some(value.clone());
            }
            value if value.starts_with("--target-dir=") => {
                let value = value.trim_start_matches("--target-dir=");
                if value.is_empty() {
                    bail!("--target-dir requires a non-empty directory argument");
                }
                selected = Some(value.to_owned());
            }
            _ => {}
        }
    }
    Ok(selected)
}

fn dependency_metadata_flags(ctx: &Context) -> Vec<String> {
    forward_cargo_flags(
        &ctx.flags.extra_cargo,
        &["--manifest-path", "--features"],
        &[
            "--offline",
            "--locked",
            "--frozen",
            "--all-features",
            "--no-default-features",
        ],
    )
}

pub(crate) fn local_manifest_paths(
    ctx: &Context,
    sysroot: &PrivateSysroot,
) -> Result<Vec<PathBuf>> {
    let mut command = base_cargo(ctx, sysroot)?;
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

/// A cargo Command pre-loaded with the backend RUSTFLAGS + the dotnet env + the
/// pinned toolchain (installed only) + quiet/deterministic dotnet knobs. Runs in the
/// crate dir.
fn base_cargo(ctx: &Context, sysroot: &PrivateSysroot) -> Result<Command> {
    let mut cmd = Command::new(&ctx.cargo);
    cmd.current_dir(&ctx.crate_dir);
    cmd.env("CARGO_HOME", &ctx.paths.cargo_home);
    if let Some(config) = crate::overlays::ambient_cargo_config(ctx) {
        cmd.arg("--config").arg(config);
    }
    cmd.arg("--config")
        .arg(crate::overlays::generated_config_path(ctx));

    // Use Cargo's unit-separator encoding so spaces and non-ASCII path bytes remain inside their
    // individual rustc arguments. A whitespace-delimited RUSTFLAGS string cannot represent them.
    let flags = rustflags::assemble(
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
    );
    cmd.env_remove("RUSTFLAGS");
    cmd.env("CARGO_ENCODED_RUSTFLAGS", rustflags::encode(&flags));
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

    // Target .NET version — the SINGLE seam: exported so BOTH the codegen backend (rustc, which
    // reads it via cilly) AND the cilly linker (a separate process: runtimeconfig + `.ver` stamps)
    // target the same runtime.
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
        let mut paths = vec![path_add.clone()];
        if let Some(current) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&current));
        }
        cmd.env(
            "PATH",
            std::env::join_paths(paths).context("constructing PATH for the selected dotnet")?,
        );
        cmd.env("DOTNET_ROOT", dotnet_root);
    }

    // Quieter, deterministic dotnet + cargo.
    cmd.env("DOTNET_CLI_TELEMETRY_OPTOUT", "1");
    cmd.env("DOTNET_NOLOGO", "1");
    cmd.env("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1");
    cmd.env("CARGO_TERM_COLOR", "never");
    Ok(cmd)
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
        };
        configure_managed_identity_env(&mut command, Some(&identity));
        let envs: std::collections::BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            envs.get(std::ffi::OsStr::new("RCL_MANAGED_ASSEMBLY_NAME")),
            Some(&Some(std::ffi::OsStr::new("example_widget")))
        );
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

    #[test]
    fn cargo_json_diagnostics_are_rendered_for_human_output() {
        let line = serde_json::json!({
            "reason": "compiler-message",
            "message": {
                "rendered": "error[E0001]: café failed\n  --> src/main.rs:1:1\n"
            }
        })
        .to_string();
        assert_eq!(
            cargo_json_human_output(&line).as_deref(),
            Some("error[E0001]: café failed\n  --> src/main.rs:1:1\n")
        );

        let artifact = serde_json::json!({
            "reason": "compiler-artifact",
            "executable": "/tmp/app"
        })
        .to_string();
        assert_eq!(cargo_json_human_output(&artifact), None);
    }

    #[test]
    fn cargo_json_diagnostics_fall_back_and_unknown_messages_remain_logged() {
        let plain = serde_json::json!({
            "reason": "compiler-message",
            "message": { "message": "plain diagnostic" }
        })
        .to_string();
        assert_eq!(
            cargo_json_human_output(&plain).as_deref(),
            Some("plain diagnostic")
        );

        let custom = serde_json::json!({
            "reason": "rust-dotnet-progress",
            "message": "linking managed metadata"
        })
        .to_string();
        assert_eq!(
            cargo_json_human_output(&custom).as_deref(),
            Some(custom.as_str())
        );
        assert_eq!(
            cargo_json_human_output("{\"custom\":true}").as_deref(),
            Some("{\"custom\":true}")
        );
    }

    #[test]
    fn streamed_json_stdout_is_preserved_byte_for_byte() {
        let expected = b"{\"reason\":\"compiler-artifact\"}\n{\"reason\":\"build-finished\"}";
        let (tx, rx) = mpsc::channel();
        let reader = spawn_stream_reader(
            std::io::Cursor::new(expected.to_vec()),
            CargoStream::Stdout,
            tx,
        );
        let mut captured = Vec::new();
        for event in rx {
            match event {
                CargoStreamEvent::Chunk(chunk) => {
                    assert_eq!(chunk.stream, CargoStream::Stdout);
                    captured.extend_from_slice(&chunk.bytes);
                }
                CargoStreamEvent::ReadError { error, .. } => panic!("unexpected error: {error}"),
            }
        }
        reader.join().unwrap();
        assert_eq!(captured, expected);
    }

    #[test]
    fn stream_reader_reports_an_error_after_delivering_prior_bytes() {
        struct BytesThenError {
            bytes: std::io::Cursor<Vec<u8>>,
        }

        impl std::io::Read for BytesThenError {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.bytes.position() < self.bytes.get_ref().len() as u64 {
                    return self.bytes.read(buf);
                }
                Err(std::io::Error::other("synthetic stream failure"))
            }
        }

        let (tx, rx) = mpsc::channel();
        let reader = spawn_stream_reader(
            BytesThenError {
                bytes: std::io::Cursor::new(b"first line\n".to_vec()),
            },
            CargoStream::Stdout,
            tx,
        );
        let events = rx.into_iter().collect::<Vec<_>>();
        reader.join().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            CargoStreamEvent::Chunk(CargoStreamChunk {
                stream: CargoStream::Stdout,
                bytes,
            }) if bytes == b"first line\n"
        ));
        assert!(matches!(
            &events[1],
            CargoStreamEvent::ReadError {
                stream: CargoStream::Stdout,
                error,
            } if error.to_string() == "synthetic stream failure"
        ));
    }

    #[test]
    fn human_output_log_preserves_multiline_diagnostics() {
        let mut log = String::new();
        record_human_output(&mut log, "error: failed\n  detail", false);
        assert_eq!(log, "error: failed\n  detail\n");
    }

    #[test]
    fn successful_build_log_persistence_reports_directory_errors() {
        let temp = tempfile::tempdir().unwrap();
        let blocker = temp.path().join("not-a-directory");
        std::fs::write(&blocker, "file").unwrap();
        let error = persist_build_log(&blocker.join("lastbuild.log"), "output").unwrap_err();
        assert!(error.to_string().contains("create build log directory"));

        let directory_target = temp.path().join("is-a-directory");
        std::fs::create_dir(&directory_target).unwrap();
        let error = persist_build_log(&directory_target, "output").unwrap_err();
        assert!(error.to_string().contains("write build log"));
    }

    #[test]
    fn test_invocation_builds_harnesses_without_running_them() {
        let mut command = Command::new("cargo");
        CargoInvocation::TestNoRun.append_to(&mut command);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                std::ffi::OsStr::new("test"),
                std::ffi::OsStr::new("--no-run")
            ]
        );
    }

    #[test]
    fn ordinary_invocation_remains_cargo_build() {
        let mut command = Command::new("cargo");
        CargoInvocation::Build.append_to(&mut command);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [std::ffi::OsStr::new("build")]
        );
    }

    #[test]
    fn target_dir_routing_is_identical_for_metadata_clean_and_build() {
        let flags = vec![
            "--lib".to_string(),
            "--locked".to_string(),
            "--target-dir".to_string(),
            "/tmp/target one".to_string(),
            "--test".to_string(),
            "suite".to_string(),
            "--target-dir=/tmp/target-two".to_string(),
        ];
        assert_eq!(
            target_dir_routing_flags(&flags),
            [
                "--target-dir",
                "/tmp/target one",
                "--target-dir=/tmp/target-two"
            ]
        );
        assert_eq!(
            explicit_target_dir(&flags).unwrap(),
            Some("/tmp/target-two".to_string())
        );
    }

    #[test]
    fn target_dir_routing_rejects_missing_or_empty_values() {
        assert!(explicit_target_dir(&["--target-dir".to_string()]).is_err());
        assert!(explicit_target_dir(&["--target-dir=".to_string()]).is_err());
    }
}
