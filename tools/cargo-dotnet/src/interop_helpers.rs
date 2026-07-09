//! Bundled `Mycorrhiza.Interop.Helpers` companion assembly — builds and copies it into any
//! consumer of `mycorrhiza` automatically, no per-crate setup step required.
//!
//! Background: `mycorrhiza::linq`'s `TypedPredicate` `&`/`|` combinators (see
//! `mycorrhiza/src/linq.rs`'s `rebind_param`/`PARAMETER_REBINDER_ASSEMBLY` doc comment) call into a
//! small C# `ExpressionVisitor` helper, `Mycorrhiza.Linq.ParameterRebinder`, that the Rust interop
//! bridge resolves by simple assembly name (`Mycorrhiza.Interop.Helpers`) via normal
//! `AssemblyLoadContext` probing next to the consumer's own build output — the same resolution model
//! as any other runtime dll sitting alongside the app. That C# source now lives in this repo at
//! `mycorrhiza_interop_helpers/` (a small standalone `net8.0` class-library project); this module is
//! the delivery mechanism.
//!
//! This deliberately does NOT reuse `nuget::copy_assets`'s `.cargo-dotnet-nuget-assets/` marker-dir
//! pattern: that mechanism is per-crate opt-in (a consumer only gets a dll there after explicitly
//! running `cargo dotnet add-nuget`, or hand-copying one next to a crate like `cd_linq_groupby`'s
//! `LinqGroupHelper.dll`). `Mycorrhiza.Interop.Helpers` is not crate-specific — it's a runtime
//! dependency of `mycorrhiza` itself (specifically its `linq` module), so ANY crate that depends on
//! `mycorrhiza` needs it with zero extra steps. Instead we detect the dependency straight from the
//! crate's locked graph and copy unconditionally when present — same end result (a dll copied next to
//! the build output, resolved by the PE writer's ordinary `AssemblyRef` probing), different trigger.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _, Result};

use crate::context::Context;

/// Must match `mycorrhiza::linq::PARAMETER_REBINDER_ASSEMBLY` and the helper project's
/// `<AssemblyName>` exactly.
const HELPER_DLL_NAME: &str = "Mycorrhiza.Interop.Helpers.dll";

/// Build (if needed) the bundled interop-helpers project and copy its output dll into `out_dir`,
/// IFF this crate's locked dependency graph includes `mycorrhiza`. A silent no-op otherwise, and
/// also a silent no-op if the helper project isn't present at `ctx.paths.interop_helpers_root`
/// (e.g. an older Installed-mode home predating this feature) — mirrors `nuget::copy_assets`'s
/// "never fatal for a crate that doesn't need it" shape.
pub fn ensure_and_copy(ctx: &Context, out_dir: &Path) -> Result<()> {
    let root = &ctx.paths.interop_helpers_root;
    if !root.is_dir() {
        return Ok(());
    }
    if !depends_on_mycorrhiza(ctx) {
        return Ok(());
    }
    let dll = build(root, ctx.flags.verbose)?;
    let dest = out_dir.join(HELPER_DLL_NAME);
    fs::copy(&dll, &dest).with_context(|| format!("cp {} -> {}", dll.display(), dest.display()))?;
    if ctx.flags.verbose {
        eprintln!("==> copied {} -> {}", HELPER_DLL_NAME, dest.display());
    }
    Ok(())
}

/// Cheap dependency check: does this crate's `Cargo.lock` list a `mycorrhiza` package? Good enough
/// to gate the (fast, incremental) `dotnet build` below without parsing the full dep graph — a
/// crate that never depends on `mycorrhiza` (directly or transitively) simply won't have the entry.
fn depends_on_mycorrhiza(ctx: &Context) -> bool {
    let lock_path = ctx.crate_dir.join("Cargo.lock");
    let Ok(text) = fs::read_to_string(&lock_path) else {
        return false;
    };
    text.contains("name = \"mycorrhiza\"")
}

/// `dotnet build -c Release` the helper project — a fast no-op on `dotnet`'s own incremental cache
/// when the source hasn't changed since the last build — and return the produced dll's path.
fn build(root: &Path, verbose: bool) -> Result<PathBuf> {
    let mut cmd = Command::new("dotnet");
    cmd.arg("build").arg(root).arg("-c").arg("Release").arg("--nologo");
    if !verbose {
        cmd.arg("-v").arg("quiet");
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn `dotnet build` for {}", root.display()))?;
    if !status.success() {
        bail!("`dotnet build -c Release` failed for {}", root.display());
    }
    let dll = root.join("bin/Release/net8.0").join(HELPER_DLL_NAME);
    if !dll.is_file() {
        bail!(
            "expected {} to exist after building {} — check the project's <AssemblyName>/TFM",
            dll.display(),
            root.display()
        );
    }
    Ok(dll)
}
