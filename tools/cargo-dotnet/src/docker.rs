//! DEV Docker mode. STAGED: delegates to the in-repo bash front-end
//! (`feasibility/cargo-dotnet`), which carries the full docker mount model (the
//! in-repo /work mount, the external /project mount, the rcc-target volume). The
//! Docker path is the in-repo developer flow only — the installed user journey is
//! native — so re-porting the mount logic to Rust is deferred; the bash front-end is
//! kept as the canonical Docker driver. Ports `feasibility/cargo-dotnet:588-639`.

use std::process::Command;

use anyhow::{Result, bail};

use crate::cli::BuildArgs;
use crate::mode::Mode;

pub fn run(args: &BuildArgs, is_run: bool, mode: &Mode) -> Result<i32> {
    if args.source_link_url.is_some() {
        bail!(
            "--source-link-url is supported by the installed/native pipeline; the Docker backend \
             remains a checkout-oriented development path"
        );
    }
    let repo_root = match mode {
        Mode::Dev { repo_root } => repo_root.clone(),
        Mode::Installed { .. } => bail!(
            "the docker backend needs a repo checkout (mounts it at /work); the installed tool runs \
             native. Use CARGO_DOTNET_BACKEND=native, or run the in-repo dev flow."
        ),
    };
    let front_end = repo_root.join("feasibility/cargo-dotnet");
    if !front_end.is_file() {
        bail!(
            "in-repo bash front-end not found at {} (needed for the Docker dev path)",
            front_end.display()
        );
    }

    let sub = if is_run { "run" } else { "build" };

    // Reassemble the user's positional + flags for the bash front-end (it understands
    // path/--release/--debug/--clean/-v + `-- ARGS`).
    let mut cmd = Command::new(&front_end);
    cmd.arg(sub);
    if let Some(path) = &args.path {
        cmd.arg(path);
    }
    if args.debug {
        cmd.arg("--debug");
    } else if args.release {
        cmd.arg("--release");
    }
    if args.clean {
        cmd.arg("--clean");
    }
    if args.verbose {
        cmd.arg("-v");
    }
    // Extra cargo flags (before `--`) — forward verbatim to the bash front-end. (The
    // bash docker path hard-errors on unknown flags; the native path is the primary
    // route for flag passthrough, but we still forward what the bash understands.)
    for f in &args.extra {
        cmd.arg(f);
    }
    // Program args after `--` (run).
    if !args.prog_args.is_empty() {
        cmd.arg("--");
        for a in &args.prog_args {
            cmd.arg(a);
        }
    }
    // Force the docker backend (the bash dev default, but be explicit).
    cmd.env("CARGO_DOTNET_BACKEND", "docker");

    let status = cmd.status()?;
    Ok(status.code().unwrap_or(1))
}
