//! `build-std` invocation — typed port of the bash build block (core 626-744).
//!
//! Sets the backend RUSTFLAGS + the dotnet/ilasm env, runs `cargo fetch` then patches
//! the libc REGISTRY copy (the post-fetch second pass), runs the filtered/verbose
//! build, then a final `--message-format=json` pass whose stdout it returns for
//! `artifact::locate`. This is the ONE place a child env is constructed on the native
//! path (the inner cargo); everything else is typed Rust.

use std::process::{Command, Stdio};

use anyhow::{bail, Context as _, Result};

use crate::context::Context;
use crate::{palinject, rustflags};

/// Build the crate under build-std with the dotnet backend. Returns the raw JSON stdout
/// of the `--message-format=json` pass for the artifact locator.
pub fn build(ctx: &Context) -> Result<String> {
    if !ctx.crate_dir.join("Cargo.toml").is_file() {
        bail!("not a crate dir (no Cargo.toml): {}", ctx.crate_dir.display());
    }
    eprintln!(
        "==> cargo dotnet: building {} (profile={})",
        ctx.crate_dir.display(),
        ctx.profile.dir()
    );

    if ctx.flags.clean {
        eprintln!("==> cargo clean (full, bulletproof)");
        let _ = base_cargo(ctx).arg("clean").status();
    }

    // `cargo fetch` materialises registry sources WITHOUT compiling, so we can patch the
    // registry libc copy before it is compiled (the std::os::fd `libc::` refs fail on an
    // unpatched registry libc). `-Zjson-target-spec` is the unstable flag the dotnet
    // target spec (a JSON file) needs — it must NOT be dropped.
    let _ = base_cargo(ctx)
        .arg("-Zjson-target-spec")
        .arg("fetch")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    patch_registry_libc(ctx)?;

    // The build pass. Capture combined output; tee it to lastbuild_log; print it FULL
    // when --verbose, else a filtered tail (errors/warnings/std-compile lines) like
    // dev.sh. Errors are always preserved in the log + the filtered view.
    let mut build_cmd = base_cargo(ctx);
    build_cmd.arg("-Zjson-target-spec").arg("build");
    if let Some(flag) = ctx.profile.cargo_flag() {
        build_cmd.arg(flag);
    }
    for f in &ctx.flags.extra_cargo {
        build_cmd.arg(f);
    }
    let out = build_cmd
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .context("failed to launch the inner build-std cargo")?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::write(&ctx.paths.lastbuild_log, &log);
    if ctx.flags.verbose {
        eprint!("{log}");
    } else {
        for line in log.lines().filter(|l| is_interesting(l)).take(60) {
            eprintln!("{line}");
        }
    }
    eprintln!("== build exit: {} ==", out.status.code().unwrap_or(-1));
    if !out.status.success() {
        bail!("inner cargo build failed (exit {})", out.status.code().unwrap_or(-1));
    }

    // The JSON pass: same flags + --message-format=json; capture stdout for the locator.
    let mut json_cmd = base_cargo(ctx);
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
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The dev.sh build-log filter: keep errors, "could not compile", unused-warnings, the
/// std/core/alloc compile lines, and Finished. Everything else is noise.
fn is_interesting(line: &str) -> bool {
    let l = line.trim_start();
    l.starts_with("error")
        || l.contains("error[")
        || l.contains("could not compile")
        || l.starts_with("warning: unused")
        || l.starts_with("Compiling std ")
        || l.starts_with("Compiling core ")
        || l.starts_with("Compiling alloc ")
        || l.starts_with("Finished")
}

/// Patch the libc REGISTRY copies (post-`cargo fetch`). build-std resolves libc from the
/// registry, not the rust-src vendor tree, so this covers whichever copy it picks.
fn patch_registry_libc(ctx: &Context) -> Result<()> {
    let pal_libc = ctx.paths.pal_root.join("libc/dotnet.rs");
    if !pal_libc.is_file() {
        return Ok(());
    }
    for d in palinject::find_libc_dirs(&ctx.paths.registry_src) {
        if palinject::patch_libc(&d, &pal_libc)? {
            eprintln!("==> patched registry libc: {}", d.display());
        }
    }
    Ok(())
}

/// A cargo Command pre-loaded with the backend RUSTFLAGS + the dotnet/ilasm env + the
/// pinned toolchain (installed only) + quiet/deterministic dotnet knobs. Runs in the
/// crate dir.
fn base_cargo(ctx: &Context) -> Command {
    let mut cmd = Command::new(&ctx.cargo);
    cmd.current_dir(&ctx.crate_dir);

    // The backend RUSTFLAGS (verbatim incl. the getrandom custom-backend embedded quotes).
    cmd.env(
        "RUSTFLAGS",
        rustflags::assemble(&ctx.paths.backend_dylib, &ctx.paths.linker),
    );

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
