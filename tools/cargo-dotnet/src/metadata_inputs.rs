//! Cargo-derived source closure for MSBuild incremental builds.
//!
//! MSBuild cannot infer Cargo's local dependency graph from a selected crate's
//! `src/**/*.rs` glob.  In particular, a path dependency may itself be a workspace
//! member and a build script can own ordinary data files through
//! `cargo:rerun-if-changed`.  This module asks Cargo for the resolved package graph,
//! keeps only source-less packages (Cargo's representation of local/path packages),
//! and emits every ordinary file beneath those package roots.  The resulting manifest
//! is deterministic and is written only when its contents change, so MSBuild can use
//! both the manifest and the listed files as normal timestamp inputs.

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use cargo_metadata::MetadataCommand;

use crate::cli::MetadataInputsArgs;
use crate::{context, mode, overlays};

/// Write the Cargo-local input closure requested by `cargo-dotnet metadata-inputs`.
pub fn run(args: &MetadataInputsArgs) -> Result<i32> {
    let crate_dir = args
        .path
        .clone()
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()
        .with_context(|| "canonicalizing crate directory for cargo metadata")?;
    let inputs = collect(&crate_dir)?;
    write_if_changed(&args.output, &render(&inputs)?)?;
    Ok(0)
}

fn collect(crate_dir: &Path) -> Result<Vec<PathBuf>> {
    let manifest = crate_dir.join("Cargo.toml");
    if !manifest.is_file() {
        bail!("cargo metadata inputs: missing {}", manifest.display());
    }

    // Old Docker/bash builds wrote a marked, generated config into the crate itself. If metadata
    // later runs natively, Cargo loads that hierarchical config before our explicit private config
    // and tries to resolve container-only `/work/...` paths. Remove only our two recognized
    // generated headers; a user-owned `.cargo/config.toml` is never touched.
    overlays::remove_legacy_generated_config(crate_dir)?;

    // This command is a dependency-resolution path, not just a local file walk. It must use the
    // same private Cargo home, copied credentials, explicit source configuration, and pinned
    // nightly as a native build; otherwise an MSBuild evaluation could mutate or authenticate
    // against ambient Cargo state before the real Rust build begins.
    let cargo_home = context::cargo_home_for_crate(crate_dir)?;
    overlays::prepare_private_cargo_auth_at(&cargo_home)?;
    let mut command = MetadataCommand::new();
    command.manifest_path(&manifest);
    command.cargo_path("cargo");
    command.current_dir(crate_dir);
    command.env("CARGO_HOME", &cargo_home);
    command.env("RUSTUP_TOOLCHAIN", metadata_toolchain()?);
    // Cargo otherwise rewrites Cargo.lock while merely answering metadata, which would
    // make the next MSBuild evaluation stale forever. A pre-existing lock is also the
    // exact resolution contract we need to inspect. If no lock exists, let Cargo create
    // the initial one, then include it in the emitted closure below.
    let mut options = Vec::new();
    if let Some(config) = overlays::ambient_cargo_config_for(&cargo_home) {
        options.push("--config".to_string());
        options.push(config.to_string_lossy().into_owned());
    }
    let sdk_root = match mode::detect()? {
        mode::Mode::Dev { repo_root } => repo_root,
        mode::Mode::Installed { home } => home.join("crates"),
    };
    for name in ["mycorrhiza", "dotnet_macros"] {
        let path = sdk_root.join(name);
        if path.is_dir() {
            options.push("--config".to_string());
            options.push(format!(
                "patch.crates-io.{name}.path={}",
                toml::Value::String(path.to_string_lossy().into_owned())
            ));
        }
    }
    if has_ancestor_lockfile(crate_dir) {
        options.push("--locked".to_string());
    }
    command.other_options(options);
    let metadata = command
        .exec()
        .context("cargo metadata inputs: `cargo metadata` failed")?;

    let mut inputs = BTreeSet::new();
    // The workspace lockfile and config affect dependency resolution even when the
    // selected package itself lives in a nested member directory.
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        ".cargo/config.toml",
        ".cargo/config",
    ] {
        let path = metadata.workspace_root.as_std_path().join(name);
        if path.is_file() {
            inputs.insert(canonical_file(&path)?);
        }
    }

    let resolve = metadata
        .resolve
        .as_ref()
        .context("cargo metadata inputs: resolved dependency graph is missing")?;
    let root_id = metadata
        .root_package()
        .context("cargo metadata inputs: selected manifest has no root package")?
        .id
        .clone();
    let mut reachable = HashSet::from([root_id]);
    loop {
        let before = reachable.len();
        for node in &resolve.nodes {
            if reachable.contains(&node.id) {
                reachable.extend(node.dependencies.iter().cloned());
            }
        }
        if reachable.len() == before {
            break;
        }
    }

    for package in metadata
        .packages
        .iter()
        .filter(|package| package.source.is_none() && reachable.contains(&package.id))
    {
        let root = package
            .manifest_path
            .as_std_path()
            .parent()
            .context("cargo metadata returned a manifest without a parent directory")?;
        collect_regular_files(root, &mut inputs)?;
    }

    if inputs.is_empty() {
        bail!(
            "cargo metadata inputs: no local package files found for {}",
            crate_dir.display()
        );
    }
    Ok(inputs.into_iter().collect())
}

fn metadata_toolchain() -> Result<String> {
    if let Some(toolchain) = std::env::var("CARGO_DOTNET_TOOLCHAIN")
        .ok()
        .filter(|value| !value.is_empty())
    {
        return Ok(toolchain);
    }
    Ok(match mode::detect()? {
        mode::Mode::Dev { .. } => mode::DEFAULT_TOOLCHAIN.to_string(),
        mode::Mode::Installed { home } => mode::read_home_toolchain(&home),
    })
}

fn has_ancestor_lockfile(path: &Path) -> bool {
    for dir in path.ancestors() {
        let manifest = dir.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let is_selected = dir == path;
        let is_workspace = fs::read_to_string(&manifest)
            .is_ok_and(|text| text.lines().any(|line| line.trim() == "[workspace]"));
        if is_selected || is_workspace {
            return dir.join("Cargo.lock").is_file();
        }
    }
    false
}

fn collect_regular_files(root: &Path, inputs: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(root).with_context(|| format!("read input tree {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name == "target" || name == ".git" {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_regular_files(&path, inputs)?;
        } else if kind.is_file() || kind.is_symlink() {
            // MSBuild item lists cannot faithfully represent newlines or semicolons.
            // Refuse them rather than silently dropping part of Cargo's input closure.
            let rendered = path.to_string_lossy();
            if rendered.contains(['\n', '\r', ';']) {
                bail!(
                    "cargo metadata inputs: unsupported newline or semicolon in input path {}",
                    path.display()
                );
            }
            inputs.insert(canonical_file(&path)?);
        }
    }
    Ok(())
}

fn canonical_file(path: &Path) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .with_context(|| format!("canonicalize cargo input {}", path.display()))?;
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!(
            "cargo metadata inputs: non-canonical path {}",
            path.display()
        );
    }
    Ok(path)
}

fn render(inputs: &[PathBuf]) -> Result<Vec<u8>> {
    let mut rendered = String::new();
    for path in inputs {
        let path = path
            .to_str()
            .context("cargo metadata inputs: input path is not UTF-8")?;
        rendered.push_str(path);
        rendered.push('\n');
    }
    Ok(rendered.into_bytes())
}

fn write_if_changed(path: &Path, contents: &[u8]) -> Result<()> {
    if fs::read(path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    let parent = path
        .parent()
        .context("cargo metadata inputs: output path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, contents).with_context(|| format!("write {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("publish {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_is_sorted_and_newline_delimited() {
        let inputs = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        assert_eq!(render(&inputs).unwrap(), b"/a\n/b\n");
    }

    #[test]
    fn collector_excludes_target_but_keeps_build_inputs() {
        let root = std::env::temp_dir().join(format!(
            "cargo-dotnet-metadata-inputs-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("target")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn value() {}\n").unwrap();
        fs::write(root.join("build-input.txt"), "1\n").unwrap();
        fs::write(root.join("target/ignored"), "ignored\n").unwrap();

        let mut inputs = BTreeSet::new();
        collect_regular_files(&root, &mut inputs).unwrap();
        assert!(inputs.iter().any(|path| path.ends_with("build-input.txt")));
        assert!(!inputs.iter().any(|path| path.ends_with("target/ignored")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unchanged_manifest_preserves_mtime() {
        let root = std::env::temp_dir().join(format!(
            "cargo-dotnet-metadata-manifest-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let manifest = root.join("inputs.txt");
        write_if_changed(&manifest, b"/a\n").unwrap();
        let before = fs::metadata(&manifest).unwrap().modified().unwrap();
        write_if_changed(&manifest, b"/a\n").unwrap();
        assert_eq!(before, fs::metadata(&manifest).unwrap().modified().unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
