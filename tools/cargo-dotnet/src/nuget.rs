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
//! consumer end is copy the resolved dll into a marker directory
//! (`.cargo-dotnet-nuget-assets/`) that `pipeline.rs` copies alongside the final build output
//! on every subsequent `build`/`run` — no explicit `Assembly.LoadFrom` call needed in the
//! generated bindings themselves.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _, Result};

use crate::artifact::{self, Artifact};
use crate::cli::{AddNugetArgs, BuildArgs};
use crate::context::Context;
use crate::{buildstd, mode, overlays, palinject};

/// spinacz's reflection core, embedded at COMPILE TIME of `cargo-dotnet` itself. Written out
/// verbatim into the ephemeral bindgen crate at RUN time (see the module doc's "why not a
/// normal dependency" note).
const REFLECT_RS: &str = include_str!("../../../cargo_tests/spinacz/src/reflect.rs");

/// TFMs to try, most-specific first, matching this tool's default target (`net8.0`) with a
/// reasonable netstandard/older-net fallback chain — most real-world packages ship at least one
/// of these.
const TFM_CANDIDATES: &[&str] = &[
    "net8.0", "net7.0", "net6.0", "net5.0", "netcoreapp3.1", "netstandard2.1", "netstandard2.0",
];

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
    let crate_dir = args.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let crate_dir = fs::canonicalize(&crate_dir)
        .with_context(|| format!("add-nuget: no such directory: {}", crate_dir.display()))?;
    if !crate_dir.join("Cargo.toml").is_file() {
        bail!("add-nuget: not a crate dir (no Cargo.toml): {}", crate_dir.display());
    }

    let home = mode::cargo_dotnet_home()?;
    let cache_root = home.join("nuget_cache").join(args.id.to_lowercase()).join(&args.version);
    let dll_marker = cache_root.join(".dll_path");
    let bindings_marker = cache_root.join("out.rs");

    let dll = if args.force || !dll_marker.is_file() {
        fs::create_dir_all(&cache_root)?;
        fetch_and_extract(&args.id, &args.version, &cache_root)?
    } else {
        PathBuf::from(fs::read_to_string(&dll_marker)?.trim())
    };
    if !dll.is_file() {
        bail!("add-nuget: resolved dll does not exist: {}", dll.display());
    }
    fs::write(&dll_marker, dll.to_string_lossy().as_bytes())?;

    // Fetch (one level of) transitive dependencies BEFORE reflecting — see
    // `fetch_transitive_deps`'s doc for why: reflection throws on any type referencing an
    // unresolved dependency assembly, not just types that use it. A no-op (empty Vec) for
    // packages with no non-framework dependencies (the common case).
    let extra_dlls = fetch_transitive_deps(&args.id, &args.version, &cache_root)?;

    let asm_name = dll
        .file_stem()
        .and_then(|s| s.to_str())
        .context("add-nuget: dll has no file stem")?
        .to_string();

    eprintln!(
        "== cargo dotnet add-nuget: {} {} -> {} (assembly '{asm_name}') ==",
        args.id, args.version, dll.display()
    );

    let out_rs = if args.force || !bindings_marker.is_file() {
        let bindgen_dir = cache_root.join("bindgen");
        generate_bindings(&dll, &bindgen_dir, args.verbose, &extra_dlls)?;
        let produced = bindgen_dir.join("out.rs");
        fs::copy(&produced, &bindings_marker).with_context(|| {
            format!("add-nuget: bindgen ran but produced no out.rs at {}", produced.display())
        })?;
        bindings_marker.clone()
    } else {
        eprintln!("== cargo dotnet add-nuget: using cached bindings (pass --force to regenerate) ==");
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
    let assets_dir = crate_dir.join(".cargo-dotnet-nuget-assets");
    fs::create_dir_all(&assets_dir)?;
    let dest_dll = assets_dir.join(dll.file_name().context("add-nuget: dll has no filename")?);
    fs::copy(&dll, &dest_dll)?;
    // Transitive dependency dlls (see `fetch_transitive_deps`) are just as much a runtime
    // requirement for the CONSUMER program as they were for reflection — copy them alongside
    // the primary dll so `pipeline.rs`'s `copy_assets` ships them next to the final build output.
    for extra in &extra_dlls {
        let dest = assets_dir.join(extra.file_name().context("add-nuget: dependency dll has no filename")?);
        fs::copy(extra, &dest)?;
    }

    eprintln!("== cargo dotnet add-nuget: wrote {} ==", dest_file.display());
    eprintln!(
        "== cargo dotnet add-nuget: add `mod nuget;` to your crate root if this is the first \
         package added; the generated module is `nuget::{mod_name}` =="
    );
    eprintln!(
        "== cargo dotnet add-nuget: {} will be copied next to your build output automatically \
         from now on ==",
        dest_dll.file_name().and_then(|s| s.to_str()).unwrap_or("the dll")
    );
    Ok(0)
}

/// Copy every file in `<crate_dir>/.cargo-dotnet-nuget-assets/` (if any) into `out_dir` (the
/// directory holding the just-built artifact). Called from `pipeline.rs` after `artifact::locate`,
/// before `run`/`report` — a no-op (and silent) for crates that never ran `add-nuget`.
pub fn copy_assets(crate_dir: &Path, out_dir: &Path) -> Result<()> {
    let assets_dir = crate_dir.join(".cargo-dotnet-nuget-assets");
    if !assets_dir.is_dir() {
        return Ok(());
    }
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
    Ok(())
}

/// Download `<id> <version>`'s `.nupkg` from the public NuGet v3 flatcontainer API (read-only,
/// no auth) and extract the best-matching `lib/<tfm>/<Name>.dll` into `dest_dir`. Returns the
/// extracted dll's path.
fn fetch_and_extract(id: &str, version: &str, dest_dir: &Path) -> Result<PathBuf> {
    let id_lower = id.to_lowercase();
    let url = format!(
        "https://api.nuget.org/v3-flatcontainer/{id_lower}/{version}/{id_lower}.{version}.nupkg"
    );
    let nupkg_path = dest_dir.join(format!("{id_lower}.{version}.nupkg"));
    eprintln!("== cargo dotnet add-nuget: fetching {url} ==");
    let status = Command::new("curl")
        .args(["-sL", "-f", "-o"])
        .arg(&nupkg_path)
        .arg(&url)
        .status()
        .context("add-nuget: failed to spawn curl (is it installed?)")?;
    if !status.success() || !nupkg_path.is_file() {
        bail!(
            "add-nuget: failed to fetch {id} {version} from nuget.org (HTTP failure or package/\
             version not found) — check the id/version, e.g. `Newtonsoft.Json` `13.0.3`"
        );
    }

    let file = File::open(&nupkg_path)
        .with_context(|| format!("opening {}", nupkg_path.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("{} is not a valid .nupkg (zip)", nupkg_path.display()))?;

    // Find the best TFM's primary dll: `lib/<tfm>/<Name>.dll` where <Name> matches the package
    // id's simple name convention (most packages' primary dll is named after the package, but we
    // don't assume — just take the FIRST .dll DIRECTLY under the best-matching lib/<tfm>/ dir).
    //
    // Two things a naive "first .dll under the prefix" search gets wrong, both observed on real
    // packages (e.g. `PdfSharpCore`, which ships localized satellite assemblies):
    //   1. Satellite/localization resource assemblies live one level deeper, at
    //      `lib/<tfm>/<locale>/<Name>.resources.dll` (e.g. `lib/net8.0/de/PdfSharpCore.resources.dll`)
    //      — these still match a bare `starts_with("lib/<tfm>/")` prefix check and often sort
    //      BEFORE the real assembly in the zip's physical entry order, so the naive search picked
    //      an (empty, locale-only) resources dll as the "primary" dll, reflecting zero types.
    //      Filtered out here by requiring no further `/` after the `lib/<tfm>/` prefix.
    //   2. Multiple real dlls can sit directly under `lib/<tfm>/` (multi-assembly packages) — prefer
    //      the one whose file stem matches the package id (case-insensitively), which is the
    //      overwhelmingly common convention, before falling back to "first one found".
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).map(|f| f.name().to_string()))
        .collect::<Result<_, _>>()?;

    let mut chosen: Option<(String, &str)> = None;
    for tfm in TFM_CANDIDATES {
        let prefix = format!("lib/{tfm}/");
        let mut candidates = names.iter().filter(|n| {
            n.starts_with(&prefix)
                && n.to_lowercase().ends_with(".dll")
                && !n[prefix.len()..].contains('/')
        });
        let simple_name = id.rsplit('.').next().unwrap_or(id).to_lowercase();
        let best = candidates.clone().find(|n| {
            Path::new(n.as_str())
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase() == simple_name || s.to_lowercase() == id_lower)
                .unwrap_or(false)
        });
        if let Some(name) = best.or_else(|| candidates.next()) {
            chosen = Some((name.clone(), tfm));
            break;
        }
    }
    let Some((entry_name, tfm)) = chosen else {
        bail!(
            "add-nuget: {id} {version} has no `lib/<tfm>/*.dll` for any of the supported TFMs \
             ({TFM_CANDIDATES:?}) — inspect the .nupkg at {} manually (native/analyzer-only or \
             ref-only packages are not supported)",
            nupkg_path.display()
        );
    };
    eprintln!("== cargo dotnet add-nuget: using {entry_name} (TFM {tfm}) ==");

    let dll_name = Path::new(&entry_name)
        .file_name()
        .context("add-nuget: malformed zip entry name")?;
    let dest_dll = dest_dir.join(dll_name);
    let mut src = zip.by_name(&entry_name)?;
    let mut buf = Vec::new();
    src.read_to_end(&mut buf)?;
    fs::write(&dest_dll, &buf)?;
    Ok(dest_dll)
}

/// Package-id prefixes that are always framework-/runtime-provided (already on the shared
/// framework the apphost runs against) — never worth fetching as a separate .nupkg even if a
/// `.nuspec` lists them as a `<dependency>`.
const FRAMEWORK_DEP_PREFIXES: &[&str] =
    &["System.", "Microsoft.NETCore.", "Microsoft.CSharp", "NETStandard.Library", "Microsoft.Win32."];

/// Read the (single, since a `.nupkg` bundles at most one) `.nuspec` out of an already-downloaded
/// `.nupkg` and return the DISTINCT `<dependency id="..." version="...">` pairs it declares,
/// across every `<group>` (TFM-specific dependency groups aren't disambiguated — we just union
/// them and dedupe by id, keeping the first version seen for each; over-fetching a dependency
/// that isn't actually needed for our TFM is harmless, just a wasted download).
///
/// Hand-rolled scanning rather than a real XML parser (no XML dep in this crate, and `.nuspec`
/// `<dependency>` elements are a very regular self-closed-tag shape) — tolerant of attribute
/// order (`id`/`version` can appear in either order) but assumes well-formed, non-CDATA-escaped
/// attribute values, which every published `.nuspec` satisfies in practice.
fn nuspec_dependencies(nupkg_path: &Path) -> Result<Vec<(String, String)>> {
    let file = File::open(nupkg_path)
        .with_context(|| format!("opening {}", nupkg_path.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("{} is not a valid .nupkg (zip)", nupkg_path.display()))?;
    let nuspec_name = (0..zip.len())
        .map(|i| zip.by_index(i).map(|f| f.name().to_string()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .find(|n| !n.contains('/') && n.to_lowercase().ends_with(".nuspec"));
    let Some(nuspec_name) = nuspec_name else {
        return Ok(Vec::new());
    };
    let mut text = String::new();
    zip.by_name(&nuspec_name)?.read_to_string(&mut text)?;

    let mut deps: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for tag_start in find_all(&text, "<dependency ") {
        let Some(tag_end_rel) = text[tag_start..].find('>') else { continue };
        let tag = &text[tag_start..tag_start + tag_end_rel];
        let (Some(id), Some(version)) = (attr_value(tag, "id"), attr_value(tag, "version")) else {
            continue;
        };
        if FRAMEWORK_DEP_PREFIXES.iter().any(|p| id.starts_with(p)) {
            continue;
        }
        let key = id.to_lowercase();
        if seen.insert(key) {
            deps.push((id, version));
        }
    }
    Ok(deps)
}

/// Every byte offset in `haystack` where `needle` starts (non-overlapping, left to right).
fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        out.push(start + pos);
        start += pos + needle.len();
    }
    out
}

/// Extract `name="value"` (or `name='value'`) from a self-closed XML tag's inner text.
fn attr_value(tag: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let pat = format!("{name}={quote}");
        if let Some(start) = tag.find(&pat) {
            let val_start = start + pat.len();
            if let Some(end_rel) = tag[val_start..].find(quote) {
                return Some(tag[val_start..val_start + end_rel].to_string());
            }
        }
    }
    None
}

/// Fetch every (non-framework) transitive dependency listed in `id`/`version`'s `.nuspec` —
/// ONE level deep only (a dependency's own further dependencies aren't followed; good enough for
/// the common case of a package with a shallow, framework-adjacent dependency tree, and avoids
/// building general NuGet dependency-graph resolution). Returns the fetched dlls' paths.
///
/// WHY THIS EXISTS: reflection (`Assembly.LoadFrom` + `Module.GetTypes()`) throws
/// `FileNotFoundException`/`ReflectionTypeLoadException` for ANY type that references a
/// dependency assembly the CLR can't resolve — even types that don't touch that dependency at
/// all can be silently dropped from `GetTypes()`'s result. `PdfSharpCore` (depends on
/// `SixLabors.ImageSharp`/`SixLabors.Fonts`/`SharpZipLib` for its image support) reflects ZERO
/// types without this — see the task report this shipped with for the full diagnosis.
fn fetch_transitive_deps(id: &str, version: &str, cache_root: &Path) -> Result<Vec<PathBuf>> {
    let id_lower = id.to_lowercase();
    let nupkg_path = cache_root.join(format!("{id_lower}.{version}.nupkg"));
    if !nupkg_path.is_file() {
        return Ok(Vec::new());
    }
    let deps = nuspec_dependencies(&nupkg_path)
        .with_context(|| format!("add-nuget: reading dependencies from {}", nupkg_path.display()))?;
    if deps.is_empty() {
        return Ok(Vec::new());
    }
    eprintln!(
        "== cargo dotnet add-nuget: {id} {version} depends on {} — fetching for reflection ==",
        deps.iter().map(|(i, v)| format!("{i} {v}")).collect::<Vec<_>>().join(", ")
    );
    let mut out = Vec::new();
    for (dep_id, dep_version) in deps {
        let dep_dir = cache_root.join("deps").join(dep_id.to_lowercase()).join(&dep_version);
        let marker = dep_dir.join(".dll_path");
        let dll = if marker.is_file() {
            PathBuf::from(fs::read_to_string(&marker)?.trim())
        } else {
            fs::create_dir_all(&dep_dir)?;
            match fetch_and_extract(&dep_id, &dep_version, &dep_dir) {
                Ok(dll) => {
                    fs::write(&marker, dll.to_string_lossy().as_bytes())?;
                    dll
                }
                Err(e) => {
                    // A missing/unfetchable transitive dep shouldn't hard-fail the whole
                    // command — reflection may still succeed for the types that don't touch it.
                    eprintln!(
                        "== cargo dotnet add-nuget: WARNING: could not fetch transitive \
                         dependency {dep_id} {dep_version} ({e}); reflection may be incomplete =="
                    );
                    continue;
                }
            }
        };
        out.push(dll);
    }
    Ok(out)
}

/// Generate + build + run an ephemeral bindgen crate at `bindgen_dir`: copies `reflect.rs`
/// verbatim, writes a one-assembly `main.rs` that `Assembly.LoadFrom(dll)`s + calls
/// `reflect_assembly`, builds it through the SAME native stage pipeline `pack.rs` uses, then
/// runs the produced apphost with `bindgen_dir` as its working directory (so `out.rs` lands
/// where we expect it, mirroring how spinacz writes `out.rs` to its own cwd when run directly).
fn generate_bindings(dll: &Path, bindgen_dir: &Path, verbose: bool, extra_dlls: &[PathBuf]) -> Result<()> {
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
    let dll_path_esc = dll.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
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
        dotnet: "8".to_string(),
        features: clap_cargo::Features::default(),
        manifest: clap_cargo::Manifest::default(),
        workspace: clap_cargo::Workspace::default(),
        extra: Vec::new(),
        prog_args: Vec::new(),
    };
    let ctx = Context::resolve(&build_args, true)?;
    palinject::inject_all(&ctx)?;
    overlays::apply(&ctx)?;
    let json = buildstd::build(&ctx)?;
    let art = artifact::locate(&json, &ctx)?;
    let Artifact::Executable(exe) = art else {
        bail!("add-nuget: bindgen crate did not produce a runnable apphost (got {art:?})");
    };

    // Any transitive dependency dlls (see `fetch_transitive_deps`'s doc) must sit next to the
    // apphost itself — that's where the CLR's default probing looks, NOT `bindgen_dir` (the
    // `cmd.current_dir` below is the ephemeral crate's SOURCE dir, unrelated to assembly
    // probing). Without this, `Assembly.LoadFrom(dll)` + `Module.GetTypes()` throws
    // `ReflectionTypeLoadException`/`FileNotFoundException` for any type that references an
    // unresolved dependency assembly, even one reflection never otherwise touches.
    if let Some(exe_dir) = exe.parent() {
        for extra in extra_dlls {
            let dest = exe_dir.join(extra.file_name().context("add-nuget: dependency dll has no filename")?);
            fs::copy(extra, &dest).with_context(|| {
                format!("add-nuget: copying dependency dll {} -> {}", extra.display(), dest.display())
            })?;
        }
    }

    eprintln!("== cargo dotnet add-nuget: running bindgen (reflecting {}) ==", dll.display());
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
        bail!("add-nuget: bindgen apphost exited with {status} — the target dll may use a \
               shape spinacz's reflect_assembly can't handle (generics-heavy, non-public API \
               surface only, etc.)");
    }
    if !bindgen_dir.join("out.rs").is_file() {
        bail!("add-nuget: bindgen ran but wrote no out.rs in {}", bindgen_dir.display());
    }
    Ok(())
}

/// The absolute path to this repo's `mycorrhiza` crate, derived from `cargo-dotnet`'s own
/// compile-time location (`tools/cargo-dotnet` -> `../../mycorrhiza`). Works regardless of the
/// consumer crate's own location, since the ephemeral bindgen crate is generated OUTSIDE the
/// consumer entirely (under `~/.cargo-dotnet/nuget_cache/`).
fn mycorrhiza_path() -> Result<String> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mycorrhiza = here
        .join("..")
        .join("..")
        .join("mycorrhiza");
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
    if out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        out.insert(0, '_');
    }
    out
}
