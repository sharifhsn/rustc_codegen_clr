//! `pack` — STAGED interim.
//!
//! The OPC `.nupkg` assembly (the nuspec/[Content_Types].xml/_rels/core-properties
//! tree + the `zip -X` ordering) is intricate and lives in the bash front-end
//! (`feasibility/cargo-dotnet` `cd_pack`, :392-535). The Rust binary delegates to it.
//! Native re-port (using `cargo_metadata` for name/version + a zip crate) is deferred.
//!
//! Ports `feasibility/cargo-dotnet:392-535`.

use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::cli::PackArgs;
use crate::mode::Mode;

pub fn run(args: &PackArgs) -> Result<i32> {
    let mode = crate::mode::detect()?;

    // pack uses the bash front-end. In installed mode there is a copy at the home; in
    // dev mode use the in-repo one. Prefer the in-repo (dev) front-end; fall back to
    // the installed home copy.
    let front_end = match &mode {
        Mode::Dev { repo_root } => repo_root.join("feasibility/cargo-dotnet"),
        Mode::Installed { home } => home.join("cargo-dotnet"),
    };
    if !front_end.is_file() {
        bail!(
            "pack: bash front-end not found at {} (pack is staged on the bash core). \
             Run `cargo dotnet setup` to provision the install home.",
            front_end.display()
        );
    }

    let mut cmd = Command::new(&front_end);
    cmd.arg("pack");
    if let Some(path) = &args.path {
        cmd.arg(path);
    }
    if args.debug {
        cmd.arg("--debug");
    } else if args.is_release() {
        cmd.arg("--release");
    }
    if let Some(id) = &args.id {
        cmd.arg("--id").arg(id);
    }
    if let Some(ver) = &args.version {
        cmd.arg("--version").arg(ver);
    }
    if let Some(out) = &args.out {
        cmd.arg("--out").arg(out);
    }

    let status = cmd
        .status()
        .with_context(|| format!("failed to run bash pack: {}", front_end.display()))?;
    Ok(status.code().unwrap_or(1))
}
