//! `add-nuget` — fetch a NuGet package, generate Rust bindings for its public API via
//! reflection, and wire the resulting .dll into a consumer crate's runtime output.
//!
//! Reuses TWO existing mechanisms rather than inventing new ones:
//!   * `spinacz`'s reflection core (`cargo_tests/spinacz/src/reflect.rs`), embedded via
//!     `include_str!` and copied verbatim into an EPHEMERAL bindgen crate — the same reason
//!     spinacz itself can't be a normal library dependency of this (native, not
//!     backend-compiled) tool: `reflect_assembly` calls magic-fn intrinsics that only mean
//!     anything compiled BY this backend. The ephemeral crate's only job is
//!     `Assembly.LoadFrom(<the fetched dll>)` then `reflect_assembly(asm, ...)`.
//!   * `pack.rs`'s in-process pipeline reuse (`palinject::inject_all` -> `overlays::apply` ->
//!     `buildstd::build` -> `artifact::locate`) to build that ephemeral crate — no subprocess
//!     re-invocation of `cargo-dotnet` itself, just the same stages the `build`/`run`/`pack`
//!     subcommands already call.
//!
//! Runtime wiring: `RustcCLRInteropManagedClass<AsmName, TypeName>` is a COMPILE-TIME
//! mechanism — the PE writer emits a real ECMA-335 `AssemblyRef` for `AsmName` into the
//! consumer's own compiled assembly, exactly like a BCL binding's `"System.Runtime"`. The CLR
//! resolves that `AssemblyRef` via normal probing when a bound type is first used — for a BCL
//! assembly that's the shared framework; for a third-party one it's whatever sits next to the
//! consumer's own compiled output. So the ONLY wiring this subcommand needs to do at the
//! consumer end is stage the SDK-selected graph under `.cargo-dotnet-nuget-assets/`; its owned
//! manifest lets `pipeline.rs` materialize managed, native, and culture-resource assets beside
//! every subsequent `build`/`run` output — no explicit `Assembly.LoadFrom` call needed in the
//! generated bindings themselves.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

use crate::artifact::{self, Artifact};
use crate::cli::{AddNativeArgs, AddNativeFileArgs, AddNugetArgs, BuildArgs};
use crate::context::Context;
use crate::{buildstd, mode, overlays};

use rust_dotnet_assets as nuget_assets;
pub(crate) use rust_dotnet_assets::{StagedPackageAsset, StagedPackageAssetKind};

/// The filename of the per-crate `add-nuget` dependency manifest — see [`record_dependency`].
const DEPS_MANIFEST_FILE: &str = ".cargo-dotnet-nuget-deps.json";
const LOCAL_NATIVE_MANIFEST_FILE: &str = ".cargo-dotnet-native-files.json";

/// `{package id: version}` for every `add-nuget` package this crate has ever added — read by
/// `pack` to populate the produced `.nuspec`'s real `<dependency>` entries. A plain JSON map
/// (not the `.cargo-dotnet-nuget-assets/` dll dir) so it survives independently of whatever dlls
/// happen to be cached, and so `nuget::copy_assets`'s "copy every file in assets_dir next to the
/// build output" loop never has to know to skip it.
#[derive(Default, Serialize, Deserialize)]
struct DepsManifest {
    #[serde(flatten)]
    deps: BTreeMap<String, String>,
}

#[derive(Default, Serialize, Deserialize)]
struct LocalNativeManifest {
    schema: u32,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
}

/// Upsert `{id: version}` into `<crate_dir>/.cargo-dotnet-nuget-deps.json`, creating it if this is
/// the crate's first `add-nuget` call. Last-write-wins per id, mirroring how re-running `add-nuget
/// <id> <newer-version>` already overwrites that id's cached dll.
fn record_dependency(crate_dir: &Path, id: &str, version: &str) -> Result<()> {
    let path = crate_dir.join(DEPS_MANIFEST_FILE);
    let mut manifest: DepsManifest = if path.is_file() {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).unwrap_or_default()
    } else {
        DepsManifest::default()
    };
    manifest.deps.insert(id.to_string(), version.to_string());
    let text = serde_json::to_string_pretty(&manifest)?;
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Read back every `{id: version}` an `add-nuget` crate has recorded — `Vec::new()` (not an
/// error) if the crate never ran `add-nuget`. Used by `pack` to populate real `.nuspec`
/// `<dependency>` entries instead of bundling raw dlls (see that module's doc for why).
pub fn recorded_dependencies(crate_dir: &Path) -> Result<Vec<(String, String)>> {
    let path = crate_dir.join(DEPS_MANIFEST_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let manifest: DepsManifest =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(manifest.deps.into_iter().collect())
}

/// Return the complete non-compile closure staged by `add-nuget`, preserving its package
/// layout.  In particular, `runtimes/<rid>/native/` and culture-resource subdirectories are
/// deliberately not converted to output-directory filenames here.
pub(crate) fn staged_package_assets(crate_dir: &Path) -> Result<Vec<StagedPackageAsset>> {
    let mut assets = nuget_assets::package_assets(crate_dir)?;
    assets.extend(local_native_assets(crate_dir)?);
    let mut paths = BTreeMap::<String, PathBuf>::new();
    for asset in &assets {
        if let Some(previous) = paths.insert(asset.logical_path.clone(), asset.source.clone())
            && previous != asset.source
        {
            bail!(
                "native asset collision at {}: {} and {}",
                asset.logical_path,
                previous.display(),
                asset.source.display()
            );
        }
    }
    Ok(assets)
}

fn local_native_assets(crate_dir: &Path) -> Result<Vec<StagedPackageAsset>> {
    let manifest_path = crate_dir.join(LOCAL_NATIVE_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Ok(Vec::new());
    }
    let manifest: LocalNativeManifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parsing {}", manifest_path.display()))?;
    if manifest.schema != 1 {
        bail!(
            "unsupported local native manifest schema {} in {}",
            manifest.schema,
            manifest_path.display()
        );
    }
    let mut assets = Vec::new();
    for (library, rid_paths) in manifest.libraries {
        for (rid, relative) in rid_paths {
            let source = crate_dir.join(&relative);
            if !source.is_file() {
                bail!(
                    "vendored native file is missing: {} (recorded in {})",
                    source.display(),
                    manifest_path.display()
                );
            }
            let filename = source
                .file_name()
                .and_then(|name| name.to_str())
                .context("vendored native filename is not UTF-8")?;
            assets.push(StagedPackageAsset {
                owner: format!("local:{library}"),
                logical_path: format!("runtimes/{rid}/native/{filename}"),
                source,
                kind: StagedPackageAssetKind::Native,
                rid: Some(rid),
            });
        }
    }
    Ok(assets)
}

/// `{id: version}` for every `add-nuget` dependency whose staged runtime closure under
/// `.cargo-dotnet-nuget-assets/` is missing or incomplete relative to what
/// `.cargo-dotnet-nuget-deps.json` recorded — including a staged graph whose version no longer
/// matches the recorded one (see `nuget_assets::missing_recorded_roots`'s doc for the version-
/// drift case this catches). `Vec::new()` for a crate that never ran `add-nuget` — cheap and
/// silent, since it never touches `nuget_assets` beyond the deps manifest read. This is the
/// fresh-clone detector: the deps manifest is checked in, the assets dir is gitignored, so
/// cloning the repo and building leaves every recorded id "missing" here until `ensure_staged`
/// re-restores it.
fn missing_assets(crate_dir: &Path) -> Result<Vec<(String, String)>> {
    let recorded = recorded_dependencies(crate_dir)?;
    if recorded.is_empty() {
        return Ok(Vec::new());
    }
    let missing_ids = nuget_assets::missing_recorded_roots(crate_dir, &recorded)?
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    Ok(recorded
        .into_iter()
        .filter(|(id, _)| missing_ids.contains(id))
        .collect())
}

/// Re-restore and re-stage every `add-nuget` dependency whose runtime closure is missing or
/// incomplete, called from the `build`/`run`/`test` pipeline (before `copy_assets`) and from
/// `cargo dotnet restore` — the sanctioned offline-prepare step. A no-op (and silent) for crates
/// that never ran `add-nuget`, and for crates whose staged assets are already complete.
///
/// Limitation: `.cargo-dotnet-nuget-deps.json` records only `{id: version}` — `add-nuget`'s
/// `--rid`/`--source` are NOT recorded, so auto-restore always uses the host RID default and the
/// configured NuGet sources. A crate originally added with `--rid`/`--source` (a custom feed or a
/// cross-target RID) will not auto-restore from that same source/RID; re-run `add-nuget`
/// explicitly with the original flags in that case.
///
/// Offline (`--offline`/`--frozen`) builds must never silently hit the network: if assets are
/// missing while offline, this fails with a clear, actionable error instead of restoring.
pub fn ensure_staged(ctx: &Context) -> Result<()> {
    let missing = missing_assets(&ctx.crate_dir)?;
    if missing.is_empty() {
        return Ok(());
    }
    if ctx.is_offline() {
        let names = missing
            .iter()
            .map(|(id, version)| format!("{id} {version}"))
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "offline build is missing staged NuGet assets for {names} (recorded in {}); run \
             `cargo dotnet restore` while network access is available, or re-run `cargo dotnet \
             add-nuget` online",
            ctx.crate_dir.join(DEPS_MANIFEST_FILE).display()
        );
    }
    eprintln!(
        "==> cargo dotnet: auto-restoring staged NuGet assets for {} (missing or incomplete)",
        missing
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let home = mode::cargo_dotnet_home()?;
    for (id, version) in &missing {
        let cache_root = home
            .join("nuget_cache")
            .join(id.to_lowercase())
            .join(version);
        fs::create_dir_all(&cache_root)?;
        // Restore the current host RID by default, matching `add-native` and avoiding a native-only
        // package's non-RID target graph from staging every platform binary on a fresh clone.
        let resolved = nuget_assets::restore(
            id,
            version,
            &cache_root,
            Some(ctx.host.host_rid),
            ctx.dotnet.tfm(),
            &[],
        )?;
        nuget_assets::stage_assets(&ctx.crate_dir, id, &resolved.assets)?;
    }
    // Re-check rather than trusting the restore loop unconditionally: the recorded version is
    // whatever the user typed to `add-nuget`, while `missing_recorded_roots`'s completeness check
    // matches against the SDK's own NORMALIZED version from `project.assets.json` (e.g. a
    // recorded `1.0.0.0` never equals a resolved `1.0.0`). Without this, a non-normalized
    // recorded version would silently re-restore from the network on every single build forever
    // instead of ever converging, and would permanently break `--offline` with a "run restore"
    // error that re-running restore can never actually fix.
    let still_missing = missing_assets(&ctx.crate_dir)?;
    if !still_missing.is_empty() {
        let names = still_missing
            .iter()
            .map(|(id, version)| format!("{id} {version}"))
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "restored NuGet assets for {names} but they still don't satisfy the recorded \
             version in {} — the recorded version may not be NuGet's normalized form; re-run \
             `cargo dotnet add-nuget` with the exact version NuGet reports for this package",
            ctx.crate_dir.join(DEPS_MANIFEST_FILE).display()
        );
    }
    Ok(())
}

/// spinacz's reflection core, embedded at COMPILE TIME of `cargo-dotnet` itself. Written out
/// verbatim into the ephemeral bindgen crate at RUN time (see the module doc's "why not a
/// normal dependency" note).
const REFLECT_RS: &str = include_str!("../../../cargo_tests/spinacz/src/reflect.rs");

/// The BCL assemblies `mycorrhiza::bindings` (spinacz's own output) already covers — MUST stay
/// in sync with `cargo_tests/spinacz/src/main.rs`'s `BCL_ASSEMBLIES`. Any type a fetched
/// package's public surface references OUTSIDE this set (plus the package's own assembly) is
/// dropped from the generated bindings rather than emitted as a dangling path — see
/// `reflect_assembly`'s doc in `reflect.rs` for why.
const KNOWN_BCL_ASSEMBLIES: &[&str] = &[
    "System.Private.CoreLib",
    "System.Runtime",
    "System.Console",
    "System.Collections",
    "System.Collections.Concurrent",
    "System.Collections.NonGeneric",
    "System.Collections.Specialized",
    "System.Linq",
    "System.Linq.Expressions",
    "System.Memory",
    "System.Text.Encoding.Extensions",
    "System.Text.RegularExpressions",
    "System.Runtime.InteropServices",
    "System.Runtime.Numerics",
    "System.Threading",
    "System.Threading.Tasks",
    "System.Globalization",
    "System.ObjectModel",
    "System.ComponentModel",
    "System.ComponentModel.Primitives",
    "System.Diagnostics.Tracing",
    "System.Reflection.Primitives",
    "System.Private.Uri",
];

pub fn run(args: &AddNugetArgs) -> Result<i32> {
    let dotnet: crate::context::DotnetVersion = args.dotnet.parse().map_err(anyhow::Error::msg)?;
    let crate_dir = args.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let crate_dir = fs::canonicalize(&crate_dir)
        .with_context(|| format!("add-nuget: no such directory: {}", crate_dir.display()))?;
    if !crate_dir.join("Cargo.toml").is_file() {
        bail!(
            "add-nuget: not a crate dir (no Cargo.toml): {}",
            crate_dir.display()
        );
    }

    let home = mode::cargo_dotnet_home()?;
    let cache_root = home
        .join("nuget_cache")
        .join(args.id.to_lowercase())
        .join(&args.version);
    // `--force` promises a real re-fetch, which matters for local/private-feed development where
    // the same exact version may be rebuilt before publication. Merely regenerating `out.rs` is
    // insufficient: NuGet otherwise reuses the old package bytes under RestorePackagesPath and
    // reflection silently sees a stale API surface.
    clear_cache_if_forced(&cache_root, args.force)?;
    let dll_marker = cache_root.join(".dll_path");
    let bindings_marker = cache_root.join("out.rs");

    // Always ask the SDK for the graph. Restore is incremental when inputs and packages are
    // unchanged, while reparsing project.assets.json prevents the old `.dll_path` cache from
    // silently discarding transitive assets on later invocations.
    fs::create_dir_all(&cache_root)?;
    let resolved = nuget_assets::restore(
        &args.id,
        &args.version,
        &cache_root,
        args.rid.as_deref(),
        dotnet.tfm(),
        &args.source,
    )?;
    let (dll, compile_dlls, runtime_dlls) = (
        resolved
            .primary_dll
            .context("add-nuget: restored package has no managed compile/runtime DLL for net8.0")?,
        resolved.compile_dlls,
        resolved.runtime_dlls,
    );
    nuget_assets::stage_assets(&crate_dir, &args.id, &resolved.assets)?;
    if !dll.is_file() {
        bail!("add-nuget: resolved dll does not exist: {}", dll.display());
    }
    fs::write(&dll_marker, dll.to_string_lossy().as_bytes())?;

    // SDK restore owns TFM selection, version negotiation, and the full transitive graph.
    // Runtime assets are used for CLR probing; compile assets remain available as a fallback
    // for reference-only packages.
    let mut extra_dlls = runtime_dlls;
    for compile in compile_dlls {
        if !extra_dlls
            .iter()
            .any(|p| p.file_name() == compile.file_name())
        {
            extra_dlls.push(compile);
        }
    }
    extra_dlls.retain(|path| path != &dll);

    let asm_name = dll
        .file_stem()
        .and_then(|s| s.to_str())
        .context("add-nuget: dll has no file stem")?
        .to_string();

    eprintln!(
        "== cargo dotnet add-nuget: {} {} -> {} (assembly '{asm_name}') ==",
        args.id,
        args.version,
        dll.display()
    );

    let out_rs = if args.force || !bindings_marker.is_file() {
        let bindgen_dir = cache_root.join("bindgen");
        generate_bindings(&dll, &bindgen_dir, args.verbose, &extra_dlls, dotnet)?;
        let produced = bindgen_dir.join("out.rs");
        fs::copy(&produced, &bindings_marker).with_context(|| {
            format!(
                "add-nuget: bindgen ran but produced no out.rs at {}",
                produced.display()
            )
        })?;
        bindings_marker.clone()
    } else {
        eprintln!(
            "== cargo dotnet add-nuget: using cached bindings (pass --force to regenerate) =="
        );
        bindings_marker.clone()
    };

    // ---- wire into the consumer crate ----
    let mod_name = to_snake_ident(&args.id);
    let nuget_dir = crate_dir.join("src").join("nuget");
    fs::create_dir_all(&nuget_dir)?;
    let dest_file = nuget_dir.join(format!("{mod_name}.rs"));
    // The generated module references OTHER assemblies' types (any BCL type appearing in
    // Newtonsoft.Json's own public signatures, e.g. `System::String`/`System::Object`) as bare
    // `System::X` paths — the same convention spinacz's OWN output uses when it becomes
    // mycorrhiza's `bindings.rs` (a SIBLING top-level `System` module in the same file, so the
    // bare path resolves with no `use`). Here the generated file is its OWN separate module, so
    // it needs an explicit re-export of mycorrhiza's existing BCL bindings to resolve those
    // paths — everything Newtonsoft.Json's public surface itself references (String, Object,
    // Xml::*, ...) is already bound there; we don't regenerate it.
    let mut bindings_src = String::from(
        "// Generated by `cargo dotnet add-nuget` — do not hand-edit (re-run add-nuget instead).\n\
         #![allow(non_camel_case_types, unused_imports)]\n\
         #[allow(unused_imports)]\nuse mycorrhiza::bindings::System;\n\n",
    );
    bindings_src.push_str(&fs::read_to_string(&out_rs)?);
    fs::write(&dest_file, bindings_src)?;

    let mod_rs = nuget_dir.join("mod.rs");
    let decl = format!("pub mod {mod_name};\n");
    let existing = fs::read_to_string(&mod_rs).unwrap_or_default();
    if !existing.contains(&decl) {
        let mut f = File::options().create(true).append(true).open(&mod_rs)?;
        f.write_all(decl.as_bytes())?;
    }

    // The runtime-asset marker dir: `pipeline.rs` copies every file here alongside the final
    // build output on every subsequent `build`/`run` of THIS crate (see its doc comment).
    // Record {id: version} so `pack` (which does NOT bundle .cargo-dotnet-nuget-assets/, see its
    // own doc) can instead emit a real `<dependency>` in the produced .nuspec — the idiomatic
    // NuGet path, which gets RID-specific native assets (e.g. SQLitePCLRaw's native SQLite driver)
    // and transitive version negotiation right in a way bundling raw dlls never could. Stored as a
    // SIBLING of assets_dir, not inside it, so `copy_assets`'s "copy every file" loop below doesn't
    // also ship this bookkeeping file next to the compiled build output.
    record_dependency(&crate_dir, &args.id, &args.version)?;

    eprintln!(
        "== cargo dotnet add-nuget: wrote {} ==",
        dest_file.display()
    );
    eprintln!(
        "== cargo dotnet add-nuget: add `mod nuget;` to your crate root if this is the first \
         package added; the generated module is `nuget::{mod_name}` =="
    );
    eprintln!(
        "== cargo dotnet add-nuget: the SDK-selected runtime/native/resource graph is staged \
         under .cargo-dotnet-nuget-assets/ and will be copied next to your build output automatically =="
    );
    Ok(0)
}

/// Restore and stage a native-only package.  This deliberately does not invoke reflection
/// binding: native packages commonly contain no managed compile assembly.
pub fn run_native(args: &AddNativeArgs) -> Result<i32> {
    let dotnet: crate::context::DotnetVersion = args.dotnet.parse().map_err(anyhow::Error::msg)?;
    let crate_dir = fs::canonicalize(args.path.clone().unwrap_or_else(|| PathBuf::from(".")))?;
    if !crate_dir.join("Cargo.toml").is_file() {
        bail!("add-native: not a crate dir: {}", crate_dir.display());
    }
    let cache_root = mode::cargo_dotnet_home()?
        .join("nuget_cache")
        .join(args.id.to_lowercase())
        .join(&args.version);
    fs::create_dir_all(&cache_root)?;
    let host = crate::host::HostFacts::detect();
    let rid = args.rid.as_deref().unwrap_or(host.host_rid);
    let resolved = nuget_assets::restore(
        &args.id,
        &args.version,
        &cache_root,
        Some(rid),
        dotnet.tfm(),
        &[],
    )?;
    let native_assets = resolved
        .assets
        .iter()
        .filter(|asset| asset.kind == rust_dotnet_assets::AssetKind::Native)
        .collect::<Vec<_>>();
    if native_assets.is_empty() {
        bail!(
            "add-native: package {} {} has no native assets",
            args.id,
            args.version
        );
    }
    if !native_assets
        .iter()
        .any(|asset| native_library_matches(&args.library, &asset.source))
    {
        let available = native_assets
            .iter()
            .filter_map(|asset| asset.source.file_name()?.to_str())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "add-native: --library {:?} does not match a selected native file for {rid}; \
             available: {available}",
            args.library
        );
    }
    nuget_assets::stage_assets(&crate_dir, &args.id, &resolved.assets)?;
    // Use the same durable package manifest as managed dependencies so fresh clones auto-restore
    // this native graph and `cargo dotnet pack` retains the real NuGet dependency.
    record_dependency(&crate_dir, &args.id, &args.version)?;
    eprintln!(
        "== cargo dotnet add-native: staged {} {} for {rid}; declare #[link(name = {:?})] ==",
        args.id, args.version, args.library
    );
    Ok(0)
}

/// Vendor a local native library under a RID-qualified project path and record it for every
/// subsequent build, run, test, and pack. Copying rather than retaining an absolute source path
/// keeps the project reproducible for collaborators and CI.
pub fn run_native_file(args: &AddNativeFileArgs) -> Result<i32> {
    let crate_dir = fs::canonicalize(args.path.clone().unwrap_or_else(|| PathBuf::from(".")))?;
    if !crate_dir.join("Cargo.toml").is_file() {
        bail!("add-native-file: not a crate dir: {}", crate_dir.display());
    }
    let source = fs::canonicalize(&args.file)
        .with_context(|| format!("resolving native library {}", args.file.display()))?;
    if !source.is_file() {
        bail!("add-native-file: not a file: {}", source.display());
    }
    if !native_library_matches(&args.library, &source) {
        bail!(
            "add-native-file: --library {:?} does not match native filename {}",
            args.library,
            source.display()
        );
    }
    let rid = args
        .rid
        .as_deref()
        .unwrap_or_else(|| crate::host::HostFacts::detect().host_rid);
    let filename = source
        .file_name()
        .context("native library has no filename")?;
    let relative = PathBuf::from("native").join(rid).join(filename);
    let destination = crate_dir.join(&relative);
    fs::create_dir_all(destination.parent().expect("vendored file has a parent"))?;
    if source != destination {
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "vendoring {} -> {}",
                source.display(),
                destination.display()
            )
        })?;
    }

    let manifest_path = crate_dir.join(LOCAL_NATIVE_MANIFEST_FILE);
    let mut manifest: LocalNativeManifest = if manifest_path.is_file() {
        serde_json::from_str(&fs::read_to_string(&manifest_path)?)
            .with_context(|| format!("parsing {}", manifest_path.display()))?
    } else {
        LocalNativeManifest {
            schema: 1,
            ..Default::default()
        }
    };
    manifest.schema = 1;
    manifest
        .libraries
        .entry(args.library.clone())
        .or_default()
        .insert(
            rid.to_string(),
            relative.to_string_lossy().replace('\\', "/"),
        );
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    eprintln!(
        "== cargo dotnet add-native-file: vendored {} for {rid}; declare #[link(name = {:?})] ==",
        destination.display(),
        args.library
    );
    Ok(0)
}

pub(crate) fn native_library_matches(logical: &str, path: &Path) -> bool {
    fn normalized(name: &str) -> &str {
        let name = name.strip_prefix("lib").unwrap_or(name);
        name.strip_suffix(".dylib")
            .or_else(|| name.strip_suffix(".dll"))
            .or_else(|| name.split_once(".so").map(|(stem, _)| stem))
            .unwrap_or(name)
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|file| normalized(file).eq_ignore_ascii_case(normalized(logical)))
}

fn clear_cache_if_forced(cache_root: &Path, force: bool) -> Result<()> {
    if force && cache_root.exists() {
        fs::remove_dir_all(cache_root).with_context(|| {
            format!(
                "add-nuget: clearing forced package cache {}",
                cache_root.display()
            )
        })?;
    }
    Ok(())
}

/// Materialize the owned SDK-selected NuGet runtime closure into `out_dir` (the directory holding
/// the just-built artifact). Legacy flat marker directories remain readable for older consumers.
/// Called from `pipeline.rs` after `artifact::locate`, before `run`/`report` — a no-op (and silent)
/// for crates that never ran `add-nuget`.
pub fn copy_assets(crate_dir: &Path, out_dir: &Path) -> Result<()> {
    let assets_dir = crate_dir.join(".cargo-dotnet-nuget-assets");
    if assets_dir.is_dir() && !nuget_assets::copy_staged_assets(crate_dir, out_dir)? {
        for entry in fs::read_dir(&assets_dir)
            .with_context(|| format!("reading {}", assets_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let dest = out_dir.join(entry.file_name());
            fs::copy(&path, &dest)
                .with_context(|| format!("cp {} -> {}", path.display(), dest.display()))?;
        }
    }
    let host_rid = crate::host::HostFacts::detect().host_rid;
    for asset in local_native_assets(crate_dir)? {
        if asset.rid.as_deref() != Some(host_rid) {
            continue;
        }
        let filename = asset
            .source
            .file_name()
            .context("vendored native asset has no filename")?;
        let destination = out_dir.join(filename);
        fs::copy(&asset.source, &destination).with_context(|| {
            format!(
                "copying vendored native asset {} -> {}",
                asset.source.display(),
                destination.display()
            )
        })?;
    }
    Ok(())
}

/// Generate + build + run an ephemeral bindgen crate at `bindgen_dir`: copies `reflect.rs`
/// verbatim, writes a one-assembly `main.rs` that `Assembly.LoadFrom(dll)`s + calls
/// `reflect_assembly`, builds it through the SAME native stage pipeline `pack.rs` uses, then
/// runs the produced apphost with `bindgen_dir` as its working directory (so `out.rs` lands
/// where we expect it, mirroring how spinacz writes `out.rs` to its own cwd when run directly).
fn generate_bindings(
    dll: &Path,
    bindgen_dir: &Path,
    verbose: bool,
    extra_dlls: &[PathBuf],
    dotnet: crate::context::DotnetVersion,
) -> Result<()> {
    fs::create_dir_all(bindgen_dir.join("src"))?;
    fs::write(
        bindgen_dir.join("Cargo.toml"),
        "[package]\nname = \"nuget_bindgen\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nmycorrhiza = { path = \"REPO_MYCORRHIZA_PATH\" }\n[workspace]\n"
            .replace("REPO_MYCORRHIZA_PATH", &mycorrhiza_path()?),
    )?;
    fs::write(bindgen_dir.join("src").join("reflect.rs"), REFLECT_RS)?;

    // The dll path is baked in as a compile-time string literal (the same reason spinacz's own
    // BCL list is a compile-time const, not a CLI arg: `std::env::args()` is unusable under
    // this backend's PAL).
    let dll_path_esc = dll
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let asm_name = dll
        .file_stem()
        .and_then(|s| s.to_str())
        .context("add-nuget: dll has no file stem")?
        .replace('"', "\\\"");
    // `known` = the BCL assemblies mycorrhiza's OWN `bindings.rs` already covers (any of those
    // types are safe to reference — the consumer will have them) PLUS the target package's own
    // assembly (its types obviously resolve, they're what we're generating). Anything the
    // package's public surface references OUTSIDE this set (e.g. Newtonsoft.Json's optional
    // `System.Xml`/`System.ComponentModel`/`System.Runtime.Serialization` touchpoints) gets
    // dropped by `reflect_assembly`'s `known`-gate rather than emitting a dangling path — see
    // its doc in reflect.rs. Keep this list in sync with spinacz's own `BCL_ASSEMBLIES`.
    let known_lit = KNOWN_BCL_ASSEMBLIES
        .iter()
        .map(|s| format!("{s:?}.to_string()"))
        .collect::<Vec<_>>()
        .join(", ");
    let main_rs = format!(
        r#"#![feature(adt_const_params, unsized_const_params)]
#![allow(unused_imports, unused_must_use)]
mod reflect;
use mycorrhiza::system::MString;
use mycorrhiza::System::Reflection::Assembly;
use reflect::{{reflect_assembly, Namespace}};
use std::io::Write;

fn main() {{
    let mut known: Vec<String> = vec![{known_lit}];
    known.push("{asm_name}".to_string());

    let path: MString = "{dll_path_esc}".into();
    let asm = Assembly::static1::<"LoadFrom", MString, Assembly>(path);
    let mut root = Namespace::new(String::new(), 0);
    let mut total: i32 = 0;
    reflect_assembly(asm, &mut root, &mut total, &known);
    let mut out = std::fs::File::create("out.rs").unwrap();
    // `false`: this output lives in a SEPARATE consumer crate, not inside `mycorrhiza` itself —
    // `impl From<Derived> for Base` upcasts would violate Rust's orphan rule there (see
    // `Namespace::export`'s doc in reflect.rs). Base-type access still works via
    // `rustc_clr_interop_managed_checked_cast`, just without `.into()`.
    root.export_root(&mut out, false);
    out.flush().unwrap();
    mycorrhiza::system::console::Console::writeln_u64(total as u64);
}}
"#
    );
    fs::write(bindgen_dir.join("src").join("main.rs"), main_rs)?;

    // ---- build via the same native stage pipeline as pack.rs/pipeline.rs ----
    let build_args = BuildArgs {
        path: Some(bindgen_dir.to_path_buf()),
        release: true,
        debug: false,
        clean: false,
        verbose,
        backend: None,
        dotnet: dotnet.as_env().to_string(),
        source_link_url: None,
        features: clap_cargo::Features::default(),
        manifest: clap_cargo::Manifest::default(),
        workspace: clap_cargo::Workspace::default(),
        extra: Vec::new(),
        prog_args: Vec::new(),
    };
    let ctx = Context::resolve(&build_args, true)?;
    let _build_lock = crate::build_lock::BuildLock::acquire_crate(&ctx)?;
    let private_sysroot = crate::private_sysroot::prepare(&ctx)?;
    overlays::apply(&ctx)?;
    let json = buildstd::build_with_sysroot(&ctx, &private_sysroot)?;
    let art = artifact::locate(&json, &ctx)?;
    crate::receipt::write(&ctx, &art, &private_sysroot)?;
    let Artifact::Executable(exe) = art else {
        bail!("add-nuget: bindgen crate did not produce a runnable apphost (got {art:?})");
    };

    // SDK-resolved dependency DLLs must sit next to the apphost itself — that's where the CLR's
    // default probing looks, NOT `bindgen_dir` (the
    // `cmd.current_dir` below is the ephemeral crate's SOURCE dir, unrelated to assembly
    // probing). Without this, `Assembly.LoadFrom(dll)` + `Module.GetTypes()` throws
    // `ReflectionTypeLoadException`/`FileNotFoundException` for any type that references an
    // unresolved dependency assembly, even one reflection never otherwise touches.
    if let Some(exe_dir) = exe.parent() {
        for extra in extra_dlls {
            let dest = exe_dir.join(
                extra
                    .file_name()
                    .context("add-nuget: dependency dll has no filename")?,
            );
            fs::copy(extra, &dest).with_context(|| {
                format!(
                    "add-nuget: copying dependency dll {} -> {}",
                    extra.display(),
                    dest.display()
                )
            })?;
        }
    }

    eprintln!(
        "== cargo dotnet add-nuget: running bindgen (reflecting {}) ==",
        dll.display()
    );
    let mut cmd = Command::new(&exe);
    cmd.current_dir(bindgen_dir);
    if let Some((path_add, dotnet_root)) = &ctx.dotnet_heal {
        let cur = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{}", path_add.display(), cur));
        cmd.env("DOTNET_ROOT", dotnet_root);
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to run bindgen apphost {}", exe.display()))?;
    if !status.success() {
        bail!(
            "add-nuget: bindgen apphost exited with {status} — the target dll may use a \
               shape spinacz's reflect_assembly can't handle (generics-heavy, non-public API \
               surface only, etc.)"
        );
    }
    if !bindgen_dir.join("out.rs").is_file() {
        bail!(
            "add-nuget: bindgen ran but wrote no out.rs in {}",
            bindgen_dir.display()
        );
    }
    Ok(())
}

/// The absolute path to this repo's `mycorrhiza` crate, derived from `cargo-dotnet`'s own
/// compile-time location (`tools/cargo-dotnet` -> `../../mycorrhiza`). Works regardless of the
/// consumer crate's own location, since the ephemeral bindgen crate is generated OUTSIDE the
/// consumer entirely (under `~/.cargo-dotnet/nuget_cache/`).
fn mycorrhiza_path() -> Result<String> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mycorrhiza = here.join("..").join("..").join("mycorrhiza");
    let mycorrhiza = fs::canonicalize(&mycorrhiza).with_context(|| {
        format!(
            "add-nuget: could not locate mycorrhiza relative to cargo-dotnet's own build dir \
             ({})",
            mycorrhiza.display()
        )
    })?;
    Ok(mycorrhiza.to_string_lossy().into_owned())
}

/// `Newtonsoft.Json` -> `newtonsoft_json` (a valid, idiomatic Rust module name).
fn to_snake_ident(id: &str) -> String {
    let mut out = String::new();
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        clear_cache_if_forced, native_library_matches, run_native_file, staged_package_assets,
    };

    #[test]
    fn force_removes_stale_package_bytes_but_normal_restore_preserves_cache() {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("nuget-cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("stale.dll"), b"old package").unwrap();

        clear_cache_if_forced(&cache, false).unwrap();
        assert!(cache.join("stale.dll").is_file());

        clear_cache_if_forced(&cache, true).unwrap();
        assert!(!cache.exists());
    }

    #[test]
    fn logical_pinvoke_name_matches_platform_library_filenames() {
        assert!(native_library_matches(
            "e_sqlite3",
            std::path::Path::new("libe_sqlite3.dylib")
        ));
        assert!(native_library_matches(
            "e_sqlite3",
            std::path::Path::new("libe_sqlite3.so.0")
        ));
        assert!(native_library_matches(
            "e_sqlite3",
            std::path::Path::new("e_sqlite3.dll")
        ));
        assert!(!native_library_matches(
            "sqlite3",
            std::path::Path::new("e_sqlite3.dll")
        ));
    }

    #[test]
    fn local_native_file_is_vendored_and_projected_to_its_rid() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir_all(&crate_dir).unwrap();
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        let source = temp.path().join("libsample.so");
        std::fs::write(&source, b"native").unwrap();
        run_native_file(&crate::cli::AddNativeFileArgs {
            file: source,
            library: "sample".into(),
            path: Some(crate_dir.clone()),
            rid: Some("linux-x64".into()),
        })
        .unwrap();
        let assets = staged_package_assets(&crate_dir).unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(
            assets[0].logical_path,
            "runtimes/linux-x64/native/libsample.so"
        );
        assert_eq!(std::fs::read(&assets[0].source).unwrap(), b"native");
    }
}
