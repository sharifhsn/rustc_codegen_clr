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
use crate::{artifact, buildstd, docker, interop_helpers, nuget, overlays, palinject, run, xmldoc};

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
    // 2.5. Clear stale `#[dotnet_export]` XML-doc scratch entries. `dotnet_macros` APPENDS one
    // entry per fn at proc-macro-expansion time (it can only ever append, never knows about
    // previous runs), so a stale file from an earlier build would silently accumulate duplicate
    // entries forever if cargo's incremental build skips re-expanding an unchanged fn. Deleting
    // it up front means the sidecar XML always reflects exactly this build's expansion (or, if
    // incremental compilation skips re-expanding untouched fns, the same non-duplicated set from
    // last time — never a duplicated one).
    xmldoc::clear_scratch(ctx);
    // 3. build-std with the backend; returns the JSON message stream.
    let json = buildstd::build(ctx)?;
    // 4. Locate the produced artifact.
    let art = artifact::locate(&json, ctx)?;
    // 4.5. Copy any `add-nuget`-fetched runtime dlls alongside the output (a no-op for crates
    // that never ran `add-nuget` — see `nuget::copy_assets`'s doc for why this is the
    // only wiring a third-party NuGet dependency needs at consumer build time).
    let out_dir = match &art {
        Artifact::Executable(exe) => exe.parent(),
        Artifact::Library { so, .. } => so.parent(),
        Artifact::None => None,
    };
    if let Some(out_dir) = out_dir {
        nuget::copy_assets(&ctx.crate_dir, out_dir)?;
        // 4.6. Copy the bundled `Mycorrhiza.Interop.Helpers` companion dll (building it first if
        // needed) for any crate that depends on `mycorrhiza` — see `interop_helpers`'s doc comment
        // for why this is unconditional rather than gated on a marker directory like 4.5 above.
        interop_helpers::ensure_and_copy(ctx, out_dir)?;
    }
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
