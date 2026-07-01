//! `cargo dotnet test` — build a crate's `#[test]`s with the backend and run them on .NET.
//!
//! A thin composition over the existing native pipeline stages (PAL inject → overlays →
//! build-std → locate → run): it adds `--tests` to the inner build so cargo compiles the
//! libtest harness binary with the dotnet backend, locates that binary the same way a
//! bin apphost is located, and runs it on .NET forwarding any libtest args (after `--`).
//!
//! This lets a library author validate their crate on the REAL target — `#[test]`s run
//! through the standard libtest harness, executed by the .NET runtime. Program args after
//! `--` (e.g. a test-name filter, `--nocapture`, `--test-threads=1`) are forwarded to the
//! harness verbatim, exactly like `cargo test -- <args>`.

use anyhow::{bail, Result};

use crate::cli::BuildArgs;
use crate::context::Context;
use crate::mode::Backend;
use crate::{artifact, buildstd, overlays, palinject, run};

/// Run `cargo dotnet test`.
pub fn run(args: &BuildArgs) -> Result<i32> {
    // Test is a native-pipeline feature (it needs to locate + run the harness binary the
    // build produced). The docker dev backend does not model a distinct test verb.
    let mode = crate::mode::detect()?;
    let backend = Backend::resolve(args.backend.as_deref(), &mode)?;
    if backend == Backend::Docker {
        bail!(
            "`cargo dotnet test` runs on the native backend only. \
             Re-run with CARGO_DOTNET_BACKEND=native (or --backend native)."
        );
    }

    let ctx = resolve_test_context(args)?;
    run_native_tests(&ctx, &args.prog_args)
}

/// Resolve a Context for a test build: identical to a `run` Context, but with `--tests`
/// injected into the inner cargo flags so the harness binary is produced.
fn resolve_test_context(args: &BuildArgs) -> Result<Context> {
    let mut ctx = Context::resolve(args, true)?;
    // Inject --tests at the FRONT of the passthrough so a user-supplied --lib/--bin (rare)
    // still applies; cargo accepts repeated target selectors.
    if !ctx.flags.extra_cargo.iter().any(|f| f == "--tests") {
        ctx.flags.extra_cargo.insert(0, "--tests".to_string());
    }
    Ok(ctx)
}

/// The native test pipeline: build with `--tests`, locate the harness binary, run it.
fn run_native_tests(ctx: &Context, libtest_args: &[String]) -> Result<i32> {
    palinject::inject_all(ctx)?;
    overlays::apply(ctx)?;
    let json = buildstd::build(ctx)?;
    let art = artifact::locate(&json, ctx)?;
    match art {
        artifact::Artifact::Executable(exe) => {
            eprintln!("== running #[test] harness on .NET: {} ==", exe.display());
            // The located executable IS the libtest harness; forward the libtest args.
            run::run(&artifact::Artifact::Executable(exe), libtest_args, ctx)
        }
        artifact::Artifact::Library { .. } | artifact::Artifact::None => {
            bail!(
                "no test harness binary was produced — does this crate have any #[test]s? \
                 (a pure cdylib with no test target produces no runnable harness)"
            )
        }
    }
}
