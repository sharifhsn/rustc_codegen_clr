//! Existing-project Unity attachment.
//!
//! This module deliberately only writes files owned by cargo-dotnet.  It does not
//! mutate Unity scenes or project settings, making repeated `attach` calls safe.

use anyhow::{Context, Result, bail, ensure};
use cargo_metadata::MetadataCommand;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;

const GENERATED: &str = "Assets/RustDotnetGenerated";
const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AttachReceipt {
    pub schema: u32,
    pub assembly_name: String,
    pub root_namespace: String,
    pub module_type: String,
    pub public_type: String,
    pub managed_crate: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_crate: Option<String>,
}

/// Attach a managed Rust crate (and optionally a native crate) to an existing
/// Unity project.  Paths in generated files are project-relative whenever
/// possible.  Existing generated files are never silently overwritten.
pub fn attach(
    project: &Path,
    rust_crate: &Path,
    native_crate: Option<&Path>,
    force: bool,
) -> Result<AttachReceipt> {
    let project = project.canonicalize().context("resolve Unity project")?;
    ensure!(
        project.join("Assets").is_dir(),
        "Unity project has no Assets directory: {}",
        project.display()
    );
    let rust_input = rust_crate
        .canonicalize()
        .context("resolve managed Rust crate")?;
    let rust_crate = if rust_input.is_file() {
        rust_input
            .parent()
            .context("managed Rust manifest has no parent")?
            .to_path_buf()
    } else {
        rust_input.clone()
    };
    let package = read_package(&rust_input)?;
    let dotnet = package
        .metadata
        .get("dotnet")
        .and_then(|v| v.as_object())
        .context("managed crate must define [package.metadata.dotnet]")?;
    let profile = dotnet
        .get("compatibility-profile")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    ensure!(
        profile == "unity-netstandard2.1",
        "Unity attach requires compatibility-profile = \"unity-netstandard2.1\" (found {profile:?})"
    );
    let schema = dotnet
        .get("identity-schema")
        .and_then(|v| v.as_u64())
        .context("managed crate metadata is missing dotnet.identity-schema = 1")?;
    ensure!(schema == 1, "Unity attach requires identity-schema = 1");
    ensure!(
        dotnet
            .get("package-id")
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.is_empty()),
        "managed crate metadata is missing dotnet.package-id"
    );
    ensure!(
        dotnet
            .get("public-namespaces")
            .and_then(|v| v.as_array())
            .is_some_and(|v| !v.is_empty()),
        "managed crate metadata is missing dotnet.public-namespaces"
    );
    ensure!(
        !dotnet
            .get("legacy-main-module")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "Unity attach requires legacy-main-module = false"
    );
    let assembly_name = dotnet
        .get("assembly-name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .context("managed crate metadata is missing dotnet.assembly-name")?
        .to_owned();
    let module_type = dotnet
        .get("module-type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .context("managed crate metadata is missing dotnet.module-type")?
        .to_owned();
    let root_namespace = dotnet
        .get("root-namespace")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .context("managed crate metadata is missing dotnet.root-namespace")?
        .to_owned();
    ensure!(
        assembly_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
        "invalid managed assembly name {assembly_name:?}"
    );
    let native_crate = if let Some(native) = native_crate {
        let native_input = native.canonicalize().context("resolve native Rust crate")?;
        let native = if native_input.is_file() {
            native_input
                .parent()
                .context("native Rust manifest has no parent")?
                .to_path_buf()
        } else {
            native_input
        };
        ensure!(
            native.join("Cargo.toml").is_file(),
            "native crate has no Cargo.toml: {}",
            native.display()
        );
        Some(native)
    } else {
        None
    };
    let receipt = AttachReceipt {
        schema: SCHEMA,
        assembly_name,
        public_type: format!("{root_namespace}.{module_type}"),
        root_namespace,
        module_type,
        managed_crate: relative_path(&project, &rust_crate)?,
        native_crate: native_crate
            .as_deref()
            .map(|path| relative_path(&project, path))
            .transpose()?,
    };
    let dir = project.join(GENERATED);
    fs::create_dir_all(&dir).context("create Unity generated directory")?;
    let json = serde_json::to_string_pretty(&receipt)? + "\n";
    let desired = vec![
        (
            dir.join("RustDotnetUnityAdapter.cs"),
            adapter_source(&receipt),
        ),
        (
            dir.join("RustDotnetUnityAdapter.asmdef"),
            asmdef_source().to_owned(),
        ),
        (dir.join("rustdotnet.attach.json"), json),
        (
            project.join("Assets/RustDotnetGenerated.meta"),
            meta_source(
                &receipt.assembly_name,
                "RustDotnetGenerated",
                MetaKind::Folder,
            ),
        ),
        (
            dir.join("RustDotnetUnityAdapter.cs.meta"),
            meta_source(
                &receipt.assembly_name,
                "RustDotnetGenerated/RustDotnetUnityAdapter.cs",
                MetaKind::Script,
            ),
        ),
        (
            dir.join("RustDotnetUnityAdapter.asmdef.meta"),
            meta_source(
                &receipt.assembly_name,
                "RustDotnetGenerated/RustDotnetUnityAdapter.asmdef",
                MetaKind::AssemblyDefinition,
            ),
        ),
        (
            dir.join("rustdotnet.attach.json.meta"),
            meta_source(
                &receipt.assembly_name,
                "RustDotnetGenerated/rustdotnet.attach.json",
                MetaKind::Text,
            ),
        ),
    ];
    write_owned_set(&desired, force)?;
    Ok(receipt)
}

fn read_package(crate_dir: &Path) -> Result<cargo_metadata::Package> {
    let manifest = if crate_dir.is_file() {
        crate_dir
    } else {
        &crate_dir.join("Cargo.toml")
    };
    MetadataCommand::new()
        .manifest_path(manifest)
        .no_deps()
        .exec()
        .context("read managed crate Cargo metadata")?
        .root_package()
        .cloned()
        .context("managed crate metadata has no root package")
}

fn relative_path(project: &Path, path: &Path) -> Result<String> {
    Ok(pathdiff::diff_paths(path, project)
        .context("Rust crate and Unity project must be on the same filesystem volume")?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn write_owned_set(files: &[(std::path::PathBuf, String)], force: bool) -> Result<()> {
    for (path, contents) in files {
        if path.exists()
            && !force
            && fs::read_to_string(path)
                .map(|s| s != *contents)
                .unwrap_or(true)
        {
            bail!(
                "refusing to overwrite existing file {}; pass --force",
                path.display()
            );
        }
    }
    for (path, contents) in files {
        if path.exists() && !force {
            continue;
        }
        let parent = path
            .parent()
            .context("generated Unity file has no parent")?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("create temporary file beside {}", path.display()))?;
        temp.write_all(contents.as_bytes())
            .with_context(|| format!("write temporary Unity file for {}", path.display()))?;
        temp.as_file().sync_all()?;
        temp.persist(path)
            .map_err(|error| error.error)
            .with_context(|| format!("atomically replace {}", path.display()))?;
    }
    Ok(())
}

fn asmdef_source() -> &'static str {
    "{\n  \"name\": \"RustDotnetUnityAdapter\",\n  \"references\": [],\n  \"includePlatforms\": [],\n  \"excludePlatforms\": [],\n  \"allowUnsafeCode\": false,\n  \"overrideReferences\": false,\n  \"autoReferenced\": true\n}\n"
}

#[derive(Clone, Copy)]
enum MetaKind {
    Folder,
    Script,
    AssemblyDefinition,
    Text,
}

fn meta_source(assembly: &str, logical_path: &str, kind: MetaKind) -> String {
    let mut hash = Sha256::new();
    hash.update(assembly.as_bytes());
    hash.update([0]);
    hash.update(logical_path.as_bytes());
    let digest = hash.finalize();
    let guid = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let importer = match kind {
        MetaKind::Folder => "folderAsset: yes\nDefaultImporter:",
        MetaKind::Script => {
            "MonoImporter:\n  serializedVersion: 2\n  defaultReferences: []\n  executionOrder: 0\n  icon: {instanceID: 0}"
        }
        MetaKind::AssemblyDefinition => "AssemblyDefinitionImporter:",
        MetaKind::Text => "TextScriptImporter:",
    };
    format!(
        "fileFormatVersion: 2\nguid: {guid}\n{importer}\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
    )
}

fn adapter_source(r: &AttachReceipt) -> String {
    format!(
        "// Generated by cargo-dotnet; edit the Rust crate, not this file.\nnamespace RustDotnetGenerated {{\n    public static class RustDotnetUnityAdapter {{\n        public const string AssemblyName = {0:?};\n        public const string ModuleType = {1:?};\n    }}\n}}\n",
        r.assembly_name, r.public_type
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let t = tempdir().unwrap();
        let project = t.path().join("Game");
        fs::create_dir_all(project.join("Assets")).unwrap();
        let crate_dir = t.path().join("rust");
        fs::create_dir_all(&crate_dir).unwrap();
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname=\"demo\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[package.metadata.dotnet]\nidentity-schema=1\npackage-id=\"Demo.Managed\"\nassembly-name=\"Demo.Managed\"\nroot-namespace=\"Demo.Managed\"\nmodule-type=\"Exports\"\npublic-namespaces=[\"Demo.Managed\"]\ncompatibility-profile=\"unity-netstandard2.1\"\nlegacy-main-module=false\n").unwrap();
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::write(crate_dir.join("src/lib.rs"), "pub fn marker() {}\n").unwrap();
        (t, project, crate_dir)
    }

    #[test]
    fn attach_is_deterministic_and_project_relative() {
        let (_t, project, crate_dir) = fixture();
        let first = attach(&project, &crate_dir, None, false).unwrap();
        let second = attach(&project, &crate_dir, None, false).unwrap();
        assert_eq!(first, second);
        let text =
            fs::read_to_string(project.join(GENERATED).join("rustdotnet.attach.json")).unwrap();
        assert!(text.contains("../rust"));
        let adapter =
            fs::read_to_string(project.join(GENERATED).join("RustDotnetUnityAdapter.cs")).unwrap();
        assert!(adapter.contains("Demo.Managed.Exports"));
        assert!(!adapter.contains("System.Reflection"));
        assert!(!adapter.contains("Invoke("));
    }

    #[test]
    fn attach_refuses_unknown_generated_file_without_force() {
        let (_t, project, crate_dir) = fixture();
        attach(&project, &crate_dir, None, false).unwrap();
        let p = project.join(GENERATED).join("RustDotnetUnityAdapter.cs");
        fs::write(&p, "hand written").unwrap();
        assert!(attach(&project, &crate_dir, None, false).is_err());
        attach(&project, &crate_dir, None, true).unwrap();
    }
}
