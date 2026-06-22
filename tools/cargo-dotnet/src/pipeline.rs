//! The build/run orchestrator — a THIN dispatcher over a typed [`Context`].
//!
//! `run` resolves the Context (mode + backend + host facts + paths, all preflighted),
//! then dispatches:
//!   * `Backend::Docker` -> the in-repo bash front-end (dev-only; owns the mount model).
//!   * `Backend::Native` -> the ORDERED, pure-Rust stage pipeline below — NO `CD_*` env
//!     map, NO `Command::new("bash")`.
//!
//! The native pipeline maps the bash core's three separable phases onto typed stages:
//!   PAL inject -> overlays apply -> build-std -> locate artifact -> (run | report).

use anyhow::Result;

use crate::artifact::Artifact;
use crate::cli::BuildArgs;
use crate::context::Context;
use crate::mode::Backend;
use crate::{artifact, buildstd, docker, overlays, palinject, run};

/// Run `build` or `run`. `is_run` selects the run-the-apphost behaviour.
pub fn run(args: &BuildArgs, is_run: bool) -> Result<i32> {
    // Docker is dev-only; resolve it BEFORE the native preflight (which would otherwise
    // bail on a host without the native toolchain). We can decide the backend from the
    // mode + flag alone.
    let mode = crate::mode::detect()?;
    let backend = Backend::resolve(args.backend.as_deref(), &mode)?;
    if backend == Backend::Docker {
        return docker::run(args, is_run, &mode);
    }

    let ctx = Context::resolve(args, is_run)?;
    run_native(&ctx, &args.prog_args)
}

/// The ordered, pure-Rust native stage pipeline.
fn run_native(ctx: &Context, prog_args: &[String]) -> Result<i32> {
    // 1. PAL inject into rust-src (idempotent, re-runnable).
    palinject::inject_all(ctx)?;
    // 2. Apply the dotnet_overlays paths-override (regenerates .cargo/config.toml).
    overlays::apply(ctx)?;
    // 3. build-std with the backend; returns the JSON message stream.
    let json = buildstd::build(ctx)?;
    // 4. Locate the produced artifact.
    let art = artifact::locate(&json, ctx)?;
    // 5. Run it, or report.
    if ctx.flags.run {
        run::run(&art, prog_args, ctx)
    } else {
        report(&art);
        Ok(0)
    }
}

fn report(art: &Artifact) {
    match art {
        Artifact::Executable(exe) => eprintln!("== built: {} ==", exe.display()),
        Artifact::Library { so, dll, .. } => eprintln!(
            "== built lib: {} (referenceable as {}) ==",
            so.display(),
            dll.file_name().and_then(|s| s.to_str()).unwrap_or("the .dll")
        ),
        Artifact::None => eprintln!("== built: <no bin artifact> =="),
    }
}
