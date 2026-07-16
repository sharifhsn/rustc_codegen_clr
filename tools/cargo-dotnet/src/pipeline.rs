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

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::artifact::Artifact;
use crate::cli::BuildArgs;
use crate::context::Context;
use crate::mode::Backend;
use crate::{
    artifact, buildstd, docker, interop_helpers, nuget, overlays, private_sysroot, run, xmldoc,
};

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
    // Same-crate invocations share target output, XML scratch, and receipts. Distinct crates have
    // isolated mutable Cargo homes and may execute this pipeline concurrently.
    let _crate_lock = crate::build_lock::BuildLock::acquire_crate(ctx)?;
    // 1. PAL inject into a private snapshot; rustup's rust-src remains immutable.
    let private_sysroot = private_sysroot::prepare(ctx)?;
    // 2. Apply the dotnet_overlays paths override through a build-local Cargo config.
    overlays::apply(ctx)?;
    if ctx.is_offline() {
        crate::restore::verify(ctx, &private_sysroot)?;
    }
    // 2.1. Re-stage any `add-nuget` runtime closure missing from a fresh clone (the assets dir
    // is gitignored, the deps manifest is checked in) before spending a full build on a crate
    // that would fail at runtime with `FileNotFoundException` anyway. A no-op for crates that
    // never ran `add-nuget`; fails fast with an actionable error under `--offline`/`--frozen`.
    nuget::ensure_staged(ctx)?;
    // 2.5. Clear stale managed-API XML-doc scratch entries. `dotnet_macros` APPENDS one
    // entry per member at proc-macro-expansion time (it can only ever append, never knows about
    // previous runs), so a stale file from an earlier build would silently accumulate duplicate
    // and removed entries. `RCL_XMLDOC_BUILD_ID` is a per-driver-process Cargo input on every
    // documentation-producing expansion, so deleting the directory forces one fresh inventory;
    // the normal and JSON locator passes share the token and do not rebuild each other.
    xmldoc::clear_scratch(ctx);
    // 3. build-std with the backend; returns the JSON message stream.
    let build_trace = crate::parallel_trace::StageGuard::enter(ctx, "build")?;
    let json = buildstd::build_with_sysroot(ctx, &private_sysroot)?;
    drop(build_trace);
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
        let mut runtime_assets = nuget::copy_assets(&ctx.crate_dir, out_dir)?;
        // 4.6. Copy the bundled `Mycorrhiza.Interop.Helpers` companion dll (building it first if
        // needed) for any crate that depends on `mycorrhiza` — see `interop_helpers`'s doc comment
        // for why this is unconditional rather than gated on a marker directory like 4.5 above.
        if let Some(helper) = interop_helpers::ensure_and_copy(ctx, out_dir)? {
            runtime_assets.push(helper);
        }
        write_runtime_asset_manifest(&art, &runtime_assets)?;
    }
    if let Some(receipt) = crate::receipt::write(ctx, &art, &private_sysroot)? {
        eprintln!("== artifact receipt: {} ==", receipt.display());
    }
    // 5. Run it, or report.
    if ctx.flags.run {
        run::run(&art, prog_args, ctx)
    } else {
        report(&art);
        Ok(0)
    }
}

/// Record the exact runtime sidecars materialized beside a managed Rust library. Each line is
/// `absolute-source|relative-destination`: MSBuild can therefore combine assets from several Rust
/// crates while preserving culture-resource subdirectories instead of flattening every file into
/// the host output root.
fn write_runtime_asset_manifest(artifact: &Artifact, assets: &[PathBuf]) -> Result<()> {
    let Artifact::Library { dll, .. } = artifact else {
        return Ok(());
    };
    let root = dll
        .parent()
        .context("managed Rust library has no artifact directory")?;
    let root = fs::canonicalize(root)
        .with_context(|| format!("resolving artifact directory {}", root.display()))?;
    let mut assets = assets
        .iter()
        .map(|path| {
            let source = fs::canonicalize(path)
                .with_context(|| format!("resolving runtime asset {}", path.display()))?;
            let relative = source
                .strip_prefix(&root)
                .with_context(|| {
                    format!(
                        "runtime asset {} is outside artifact directory {}",
                        source.display(),
                        root.display()
                    )
                })?
                .to_path_buf();
            if relative.as_os_str().is_empty() {
                bail!("runtime asset cannot be the artifact directory itself");
            }
            Ok((source, relative))
        })
        .collect::<Result<Vec<_>>>()?;
    assets.sort();
    assets.dedup();
    let mut text = String::new();
    for (source, relative) in assets {
        let source_value = source.to_string_lossy();
        let relative_value = relative.to_string_lossy();
        if source_value.contains(['\r', '\n', '|', ';'])
            || relative_value.contains(['\r', '\n', '|', ';'])
        {
            bail!(
                "runtime asset path contains a manifest-reserved character: {}",
                source.display()
            );
        }
        text.push_str(&source_value);
        text.push('|');
        text.push_str(&relative_value);
        text.push('\n');
    }
    let manifest = runtime_asset_manifest_path(dll);
    if fs::read_to_string(&manifest).ok().as_deref() != Some(text.as_str()) {
        fs::write(&manifest, text)
            .with_context(|| format!("writing runtime asset manifest {}", manifest.display()))?;
    }
    Ok(())
}

fn runtime_asset_manifest_path(dll: &Path) -> PathBuf {
    PathBuf::from(format!("{}.rustdotnet.runtime-assets", dll.display()))
}

#[cfg(test)]
mod tests {
    use super::{runtime_asset_manifest_path, write_runtime_asset_manifest};
    use crate::artifact::Artifact;
    use std::fs;
    use std::path::Path;

    #[test]
    fn runtime_asset_manifest_preserves_relative_deployment_paths() {
        let root = std::env::temp_dir().join(format!(
            "cargo-dotnet-runtime-manifest-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let culture = root.join("fr");
        fs::create_dir_all(&culture).unwrap();
        let dll = root.join("Backend.dll");
        let resource = culture.join("Backend.resources.dll");
        fs::write(&dll, b"managed").unwrap();
        fs::write(&resource, b"resource").unwrap();
        let artifact = Artifact::Library {
            so: root.join("libbackend.so"),
            dll: dll.clone(),
            stem: "Backend".to_string(),
        };

        write_runtime_asset_manifest(&artifact, std::slice::from_ref(&resource)).unwrap();

        let source = fs::canonicalize(&resource).unwrap();
        assert_eq!(
            fs::read_to_string(runtime_asset_manifest_path(&dll)).unwrap(),
            format!(
                "{}|{}\n",
                source.display(),
                Path::new("fr").join("Backend.resources.dll").display()
            )
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn report(art: &Artifact) {
    match art {
        Artifact::Executable(exe) => eprintln!("== built: {} ==", exe.display()),
        Artifact::Library { so, dll, .. } => eprintln!(
            "== built lib: {} (referenceable as {}) ==",
            so.display(),
            dll.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("the .dll")
        ),
        Artifact::None => eprintln!("== built: <no bin artifact> =="),
    }
}
