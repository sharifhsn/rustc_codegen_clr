//! Artifact location — typed serde_json port of the bash awk JSON scrape (core 746-817).
//!
//! Two artifact kinds come out of a build-std build:
//!   * an EXECUTABLE (a bin crate) -> cargo's `"executable"` field. Run it.
//!   * a LIBRARY (cdylib/dylib/staticlib) -> a compiler-artifact whose target
//!     crate_types includes one of those; its `filenames` lists the produced `.so`. The
//!     dotnet target is dynamic-linking, so cargo passes `-o …/lib<crate>.so` and the
//!     cilly linker writes a referenceable .NET PE there. We copy it to `<crate>.dll`
//!     beside it for a direct C# `<Reference>` (a pure file copy — the assembly identity
//!     is `<crate>` regardless of the .so filename) and don't try to run it.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::Deserialize;

use crate::context::Context;

#[derive(Debug)]
pub enum Artifact {
    /// A runnable apphost (bin crate).
    Executable(PathBuf),
    /// A C#-referenceable library: the produced `.so` + its `<stem>.dll` copy + stem.
    Library {
        so: PathBuf,
        dll: PathBuf,
        stem: String,
    },
    /// Build succeeded but produced no runnable/referenceable artifact.
    None,
}

#[derive(Deserialize)]
struct Message {
    reason: String,
    #[serde(default)]
    executable: Option<String>,
    #[serde(default)]
    target: Option<Target>,
    #[serde(default)]
    filenames: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Target {
    #[serde(default)]
    crate_types: Vec<String>,
}

/// Locate the produced artifact from cargo's JSON message stream (one message per line).
pub fn locate(json: &str, ctx: &Context) -> Result<Artifact> {
    let messages: Vec<Message> = json
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Message>(l).ok())
        .collect();

    // (1) executable apphost: the LAST non-null, existing `executable` field.
    let exe = messages
        .iter()
        .filter_map(|m| m.executable.as_deref())
        .filter(|s| !s.is_empty() && *s != "null")
        .map(PathBuf::from)
        .rfind(|p| p.is_file());
    if let Some(exe) = exe {
        return Ok(Artifact::Executable(exe));
    }

    // (2) library .so: a compiler-artifact whose target is a cdylib/dylib/staticlib;
    //     take the first .so/.dll/.dylib from its filenames.
    let lib_so = messages
        .iter()
        .filter(|m| m.reason == "compiler-artifact")
        .filter(|m| {
            m.target
                .as_ref()
                .map(|t| {
                    t.crate_types
                        .iter()
                        .any(|c| c == "cdylib" || c == "dylib" || c == "staticlib")
                })
                .unwrap_or(false)
        })
        .filter_map(|m| m.filenames.as_ref())
        .flatten()
        .map(PathBuf::from)
        .rfind(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("so") | Some("dll") | Some("dylib")
            )
        });
    if let Some(so) = lib_so {
        // The .so PE's real .NET assembly identity is `<crate>` regardless of the .so
        // filename: derive the stem (strip dir / the cargo `lib` prefix / the ext) and
        // copy beside the .so so a C# <Reference HintPath=<stem>.dll> resolves it.
        let file = so.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let cargo_stem = file.strip_prefix("lib").unwrap_or(file).to_string();
        let stem = ctx
            .managed_identity()
            .map(|identity| identity.assembly_name.clone())
            .unwrap_or_else(|| cargo_stem.clone());
        let dll = so.with_file_name(format!("{stem}.dll"));
        // Some target/linker combinations (notably the Unity netstandard profile) already
        // report the managed assembly under its public `<assembly>.dll` name.  In that case
        // `dll` and `so` are the same path; attempting `fs::copy` would fail with “same file”
        // even though artifact discovery succeeded.  Keep the existing file and only copy
        // when Cargo gave us an internal/alternate filename.
        copy_library_image(&so, &dll)?;
        if let Some(pdb) = copy_library_pdb(&so, &dll)? {
            eprintln!("== lib PDB: {} ==", pdb.display());
        }
        eprintln!(
            "== lib PE: {} -> {} (assembly '{stem}') ==",
            so.display(),
            dll.display()
        );
        // Best-effort sidecar XML doc for `#[dotnet_export]` doc comments (see `xmldoc.rs`); never
        // fails the build over doc generation.
        if let Err(e) =
            crate::xmldoc::generate(&ctx.crate_dir, &cargo_stem, &dll, ctx.managed_identity())
        {
            eprintln!("== xml docs: skipped ({e}) ==");
        }
        return Ok(Artifact::Library { so, dll, stem });
    }

    // (3) bin fallback: an arbitrary bin crate whose JSON `executable` field cargo left
    //     null. Probe the conventional apphost paths via cargo_metadata's bin name.
    if let Some(bin) = bin_name(ctx) {
        for sub in ["x86_64-unknown-dotnet", "dotnet"] {
            for name in [format!("{bin}{}", ctx.host.exe_ext), bin.clone()] {
                let cand = ctx
                    .crate_dir
                    .join("target")
                    .join(sub)
                    .join(ctx.profile.dir())
                    .join(&name);
                if cand.is_file() {
                    return Ok(Artifact::Executable(cand));
                }
            }
        }
    }

    Ok(Artifact::None)
}

fn copy_library_image(so: &std::path::Path, dll: &std::path::Path) -> Result<()> {
    if so == dll {
        return Ok(());
    }
    fs::copy(so, dll).with_context(|| format!("cp {} -> {}", so.display(), dll.display()))?;
    Ok(())
}

/// Promote the linker's PDB from Cargo's internal library name (`libfoo.pdb`) to the public
/// managed assembly name (`Foo.pdb`) beside the copied DLL. CoreCLR and debugger consumers resolve
/// the CodeView sidecar next to the loaded image; leaving it under `deps/lib*.pdb` makes a library's
/// otherwise-valid sequence points unreachable from C#.
fn copy_library_pdb(so: &std::path::Path, dll: &std::path::Path) -> Result<Option<PathBuf>> {
    let public_candidate = so.with_extension("pdb");
    let managed_candidate = so
        .parent()
        .map(|parent| parent.join("deps"))
        .unwrap_or_default()
        .join(
            dll.file_name()
                .map(PathBuf::from)
                .unwrap_or_default()
                .with_extension("pdb"),
        );
    let deps_candidate = so
        .parent()
        .map(|parent| parent.join("deps"))
        .unwrap_or_default()
        .join(
            so.file_name()
                .map(PathBuf::from)
                .unwrap_or_default()
                .with_extension("pdb"),
        );
    let source = [public_candidate, managed_candidate, deps_candidate]
        .into_iter()
        .find(|candidate| candidate.is_file());
    let Some(source) = source else {
        return Ok(None);
    };
    let destination = dll.with_extension("pdb");
    fs::copy(&source, &destination)
        .with_context(|| format!("cp {} -> {}", source.display(), destination.display()))?;
    Ok(Some(destination))
}

/// The crate's bin target name via cargo_metadata (replaces the bash tr/awk scrape).
fn bin_name(ctx: &Context) -> Option<String> {
    let meta = cargo_metadata::MetadataCommand::new()
        .manifest_path(ctx.crate_dir.join("Cargo.toml"))
        .no_deps()
        .exec()
        .ok()?;
    for pkg in &meta.packages {
        for t in &pkg.targets {
            if t.kind.iter().any(|k| k == "bin") {
                return Some(t.name.clone());
            }
        }
    }
    // last resort: the crate dir basename.
    ctx.crate_dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_pdb_is_promoted_to_the_public_dll_stem() {
        let temp = tempfile::tempdir().unwrap();
        let so = temp.path().join("deps/libprobe.so");
        let dll = temp.path().join("deps/Managed.Probe.dll");
        fs::create_dir_all(so.parent().unwrap()).unwrap();
        fs::write(&so, b"pe").unwrap();
        fs::write(so.with_extension("pdb"), b"portable-pdb").unwrap();

        let promoted = copy_library_pdb(&so, &dll).unwrap().unwrap();
        assert_eq!(promoted, dll.with_extension("pdb"));
        assert_eq!(fs::read(promoted).unwrap(), b"portable-pdb");
    }

    #[test]
    fn library_pdb_is_found_under_cargos_internal_deps_directory() {
        let temp = tempfile::tempdir().unwrap();
        let so = temp.path().join("libprobe.so");
        let dll = temp.path().join("Managed.Probe.dll");
        fs::create_dir_all(temp.path().join("deps")).unwrap();
        fs::write(&so, b"pe").unwrap();
        fs::write(temp.path().join("deps/libprobe.pdb"), b"deps-pdb").unwrap();

        let promoted = copy_library_pdb(&so, &dll).unwrap().unwrap();
        assert_eq!(fs::read(promoted).unwrap(), b"deps-pdb");
    }

    #[test]
    fn library_pdb_prefers_the_managed_assembly_name_under_deps() {
        let temp = tempfile::tempdir().unwrap();
        let so = temp.path().join("libprobe.so");
        let dll = temp.path().join("Managed.Probe.dll");
        fs::create_dir_all(temp.path().join("deps")).unwrap();
        fs::write(&so, b"pe").unwrap();
        fs::write(temp.path().join("deps/Managed.Probe.pdb"), b"managed-pdb").unwrap();
        fs::write(temp.path().join("deps/libprobe.pdb"), b"legacy-pdb").unwrap();

        let promoted = copy_library_pdb(&so, &dll).unwrap().unwrap();
        assert_eq!(fs::read(promoted).unwrap(), b"managed-pdb");
    }

    #[test]
    fn library_image_copy_is_a_noop_when_cargo_already_reports_public_dll() {
        let temp = tempfile::tempdir().unwrap();
        let dll = temp.path().join("Managed.Probe.dll");
        fs::write(&dll, b"pe").unwrap();

        copy_library_image(&dll, &dll).unwrap();
        assert_eq!(fs::read(&dll).unwrap(), b"pe");
    }
}
