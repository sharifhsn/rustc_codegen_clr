//! Materialize staged Unity assets as a deterministic Unity Package Manager directory.
//!
//! This module deliberately does not create archives: a directory is directly consumable by
//! UPM during development, and can be archived by the caller without losing deterministic layout.

use anyhow::{Context, Result, bail};
use object::{Architecture, Object as _};
use semver::Version;
use sha2::{Digest as _, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Result of a package materialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSummary {
    pub output: PathBuf,
    pub files: usize,
}

/// Copy the staged Unity integration from `project` into a UPM package directory.
pub fn package(
    project: &Path,
    output: &Path,
    package_name: &str,
    version: &str,
    force: bool,
) -> Result<PackageSummary> {
    validate_name(package_name)?;
    Version::parse(version).with_context(|| format!("invalid UPM package version {version:?}"))?;
    let project = project
        .canonicalize()
        .with_context(|| format!("resolving Unity project {}", project.display()))?;
    if !project.join("Assets").is_dir() {
        bail!(
            "Unity project has no Assets directory: {}",
            project.display()
        );
    }
    let output = resolve_output(output)?;
    if output == project
        || project.starts_with(&output)
        || output.starts_with(project.join("Assets"))
    {
        bail!(
            "package output must not be the Unity project, one of its ancestors, or under Assets"
        );
    }
    if output.exists() {
        if !output.is_dir() {
            bail!("package output is not a directory: {}", output.display());
        }
        if fs::read_dir(&output)?.next().is_some() {
            if !force {
                bail!("package output is non-empty; pass --force to replace it");
            }
            let existing_name = fs::read_to_string(output.join("package.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .and_then(|json| json.get("name")?.as_str().map(str::to_owned));
            if existing_name.as_deref() != Some(package_name) {
                bail!(
                    "refusing to replace unknown package directory {}; expected package.json name {package_name:?}",
                    output.display()
                );
            }
        }
    }
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".rustdotnet-upm-")
        .tempdir_in(parent)?;
    let package_root = staging.path();

    let managed = project.join("Assets/Plugins/Managed");
    let native = project.join("Assets/Plugins/macOS");
    let link = project.join("Assets/RustDotnetGenerated/link.xml");
    let mut files = Vec::new();
    collect_files(&managed, &managed, &mut files)?;
    if native.is_dir() {
        collect_files(&native, &native, &mut files)?;
    }
    files.sort_by(|a, b| asset_rank(a.0).cmp(&asset_rank(b.0)).then(a.1.cmp(&b.1)));
    if !files.iter().any(|(kind, path, _)| {
        *kind == AssetKind::Managed && path.extension().and_then(|ext| ext.to_str()) == Some("dll")
    }) {
        bail!("no staged managed DLL found; run cargo dotnet unity build first");
    }
    for (kind, relative, source) in files {
        let destination = package_root.join(match kind {
            AssetKind::Managed => Path::new("Runtime/Managed").join(relative),
            AssetKind::Native => Path::new("Runtime/Plugins/macOS").join(relative),
        });
        copy_file(&source, &destination)?;
    }
    if link.is_file() {
        copy_file(&link, &package_root.join("Runtime/RustDotnetPreserve.xml"))?;
    }
    let generated = project.join("Assets/RustDotnetGenerated");
    for name in ["RustDotnetUnityAdapter.cs", "RustDotnetUnityAdapter.asmdef"] {
        let source = generated.join(name);
        if source.is_file() {
            copy_file(&source, &package_root.join("Runtime/Adapters").join(name))?;
        }
    }
    fs::create_dir_all(package_root.join("Editor"))?;
    fs::write(
        package_root.join("Editor/RustDotnetPreserveImporter.cs"),
        preserve_importer_source(package_name),
    )?;
    fs::write(
        package_root.join("Editor/RustDotnetPreserveImporter.asmdef"),
        "{\n  \"name\": \"RustDotnetPreserveImporter\",\n  \"includePlatforms\": [\"Editor\"],\n  \"autoReferenced\": true\n}\n",
    )?;
    let package_json = format!(
        "{{\n  \"name\": \"{package_name}\",\n  \"version\": \"{version}\",\n  \"displayName\": \"{}\",\n  \"description\": \"Rust managed and native integration for Unity.\",\n  \"unity\": \"6000.0\",\n  \"author\": {{\"name\": \"rustc_codegen_clr\"}}\n}}\n",
        package_name
    );
    fs::write(package_root.join("package.json"), package_json)?;
    fs::write(
        package_root.join("README.md"),
        format!(
            "# {package_name}\n\nRust managed and native integration for Unity.\n\nInstall this directory through Unity Package Manager. An Editor-only importer copies Runtime/RustDotnetPreserve.xml to Assets/RustDotnetGenerated/link.xml when content changes, then re-imports it for UnityLinker and IL2CPP preservation.\n"
        ),
    )?;
    write_deterministic_meta(package_root, package_name)?;
    let final_file_count = count_regular_files(package_root)?;
    let staged = staging.keep();
    promote_package(&staged, &output)?;
    Ok(PackageSummary {
        output,
        files: final_file_count,
    })
}

fn resolve_output(output: &Path) -> Result<PathBuf> {
    if output.exists() {
        return output
            .canonicalize()
            .with_context(|| format!("resolving package output {}", output.display()));
    }
    let absolute = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()?.join(output)
    };
    let mut existing = absolute.as_path();
    let mut suffix = Vec::new();
    while !existing.exists() {
        let name = existing
            .file_name()
            .context("package output has no existing ancestor")?;
        suffix.push(name.to_os_string());
        existing = existing
            .parent()
            .context("package output has no existing ancestor")?;
    }
    let mut resolved = existing
        .canonicalize()
        .with_context(|| format!("resolving package output ancestor {}", existing.display()))?;
    for component in suffix.into_iter().rev() {
        if component == "." {
            continue;
        }
        if component == ".." {
            resolved.pop();
        } else {
            resolved.push(component);
        }
    }
    Ok(resolved)
}

fn promote_package(staged: &Path, output: &Path) -> Result<()> {
    if !output.exists() {
        return fs::rename(staged, output)
            .with_context(|| format!("promote Unity package to {}", output.display()));
    }
    let parent = output.parent().context("package output has no parent")?;
    let reservation = tempfile::Builder::new()
        .prefix(".rustdotnet-upm-backup-")
        .tempdir_in(parent)?;
    let backup = reservation.path().to_path_buf();
    reservation.close()?;
    fs::rename(output, &backup).with_context(|| {
        format!(
            "prepare rollback for generated package {}",
            output.display()
        )
    })?;
    if let Err(error) = fs::rename(staged, output) {
        let rollback = fs::rename(&backup, output);
        return match rollback {
            Ok(()) => {
                Err(error).with_context(|| format!("promote Unity package to {}", output.display()))
            }
            Err(rollback_error) => bail!(
                "failed to promote Unity package to {} ({error}) and failed to restore {} ({rollback_error})",
                output.display(),
                backup.display()
            ),
        };
    }
    fs::remove_dir_all(&backup)
        .with_context(|| format!("remove package rollback {}", backup.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssetKind {
    Managed,
    Native,
}

fn asset_rank(kind: AssetKind) -> u8 {
    match kind {
        AssetKind::Managed => 0,
        AssetKind::Native => 1,
    }
}

fn preserve_importer_source(package_name: &str) -> String {
    format!(
        "#if UNITY_EDITOR\nusing System.IO;\nusing UnityEditor;\nusing UnityEngine;\n\n[InitializeOnLoad]\ninternal static class RustDotnetPreserveImporter\n{{\n    static RustDotnetPreserveImporter() => Sync();\n\n    private static void Sync()\n    {{\n        var source = Path.GetFullPath(\"Packages/{package_name}/Runtime/RustDotnetPreserve.xml\");\n        if (!File.Exists(source)) return;\n        var destination = Path.Combine(Application.dataPath, \"RustDotnetGenerated\", \"link.xml\");\n        var content = File.ReadAllText(source);\n        if (File.Exists(destination) && File.ReadAllText(destination) == content) return;\n        Directory.CreateDirectory(Path.GetDirectoryName(destination));\n        var temporary = destination + \".tmp\";\n        File.WriteAllText(temporary, content);\n        if (File.Exists(destination))\n            File.Replace(temporary, destination, null);\n        else\n            File.Move(temporary, destination);\n        AssetDatabase.ImportAsset(\"Assets/RustDotnetGenerated/link.xml\", ImportAssetOptions.ForceUpdate);\n    }}\n}}\n#endif\n"
    )
}

fn write_deterministic_meta(root: &Path, package_name: &str) -> Result<()> {
    let mut paths = Vec::new();
    collect_package_paths(root, root, &mut paths)?;
    paths.sort_by(|a, b| a.1.cmp(&b.1));
    for (is_dir, relative, source) in paths {
        let logical = relative.to_string_lossy().replace('\\', "/");
        let mut hash = Sha256::new();
        hash.update(package_name.as_bytes());
        hash.update([0]);
        hash.update(logical.as_bytes());
        let digest = hash.finalize();
        let guid = digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let body = meta_body(&source, is_dir, &guid)?;
        let mut meta_name = source.as_os_str().to_os_string();
        meta_name.push(".meta");
        fs::write(PathBuf::from(meta_name), body)?;
    }
    Ok(())
}

fn collect_package_paths(
    root: &Path,
    current: &Path,
    out: &mut Vec<(bool, PathBuf, PathBuf)>,
) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(current)?.collect::<std::result::Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            bail!(
                "refusing symlink in generated Unity package: {}",
                source.display()
            );
        }
        let relative = source.strip_prefix(root)?.to_path_buf();
        if file_type.is_dir() {
            out.push((true, relative, source.clone()));
            collect_package_paths(root, &source, out)?;
        } else if file_type.is_file() {
            out.push((false, relative, source));
        }
    }
    Ok(())
}

fn meta_body(source: &Path, is_dir: bool, guid: &str) -> Result<String> {
    if is_dir {
        return Ok(format!(
            "fileFormatVersion: 2\nguid: {guid}\nfolderAsset: yes\nDefaultImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
        ));
    }
    match source.extension().and_then(|ext| ext.to_str()) {
        Some("cs") => Ok(format!(
            "fileFormatVersion: 2\nguid: {guid}\nMonoImporter:\n  externalObjects: {{}}\n  serializedVersion: 2\n  defaultReferences: []\n  executionOrder: 0\n  icon: {{instanceID: 0}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
        )),
        Some("asmdef") => Ok(format!(
            "fileFormatVersion: 2\nguid: {guid}\nAssemblyDefinitionImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
        )),
        Some("dylib") => native_plugin_meta(source, guid),
        Some("dll") => Ok(managed_plugin_meta(guid)),
        _ => Ok(format!(
            "fileFormatVersion: 2\nguid: {guid}\nTextScriptImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
        )),
    }
}

fn managed_plugin_meta(guid: &str) -> String {
    format!(
        "fileFormatVersion: 2\nguid: {guid}\nPluginImporter:\n  externalObjects: {{}}\n  serializedVersion: 2\n  iconMap: {{}}\n  executionOrder: {{}}\n  defineConstraints: []\n  isPreloaded: 0\n  isOverridable: 1\n  isExplicitlyReferenced: 0\n  validateReferences: 1\n  platformData:\n  - first:\n      Any: \n    second:\n      enabled: 1\n      settings: {{}}\n  - first:\n      Editor: Editor\n    second:\n      enabled: 1\n      settings:\n        CPU: AnyCPU\n        DefaultValueInitialized: true\n        OS: AnyOS\n  - first:\n      Standalone: OSXUniversal\n    second:\n      enabled: 1\n      settings:\n        CPU: AnyCPU\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
    )
}

fn native_plugin_meta(source: &Path, guid: &str) -> Result<String> {
    let bytes = fs::read(source)?;
    let file = object::File::parse(bytes.as_slice())
        .with_context(|| format!("inspect native Unity plug-in {}", source.display()))?;
    let cpu = match file.architecture() {
        Architecture::Aarch64 => "ARM64",
        Architecture::X86_64 => "x86_64",
        architecture => bail!(
            "unsupported macOS Unity plug-in architecture {architecture:?}: {}",
            source.display()
        ),
    };
    Ok(format!(
        "fileFormatVersion: 2\nguid: {guid}\nPluginImporter:\n  externalObjects: {{}}\n  serializedVersion: 2\n  iconMap: {{}}\n  executionOrder: {{}}\n  defineConstraints: []\n  isPreloaded: 0\n  isOverridable: 1\n  isExplicitlyReferenced: 0\n  validateReferences: 1\n  platformData:\n  - first:\n      : Any\n    second:\n      enabled: 0\n      settings:\n        Exclude Editor: 0\n        Exclude Linux64: 1\n        Exclude OSXUniversal: 0\n        Exclude Win: 1\n        Exclude Win64: 1\n  - first:\n      Editor: Editor\n    second:\n      enabled: 1\n      settings:\n        CPU: {cpu}\n        DefaultValueInitialized: true\n        OS: OSX\n  - first:\n      Standalone: OSXUniversal\n    second:\n      enabled: 1\n      settings:\n        CPU: AnyCPU\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
    ))
}

fn count_regular_files(root: &Path) -> Result<usize> {
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                stack.push(entry.path());
            } else if entry.file_type()?.is_file() {
                count += 1;
            }
        }
    }
    Ok(count)
}

fn collect_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(AssetKind, PathBuf, PathBuf)>,
) -> Result<()> {
    let kind = if root.ends_with("macOS") {
        AssetKind::Native
    } else {
        AssetKind::Managed
    };
    if !root.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(current)?.collect::<std::result::Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let ty = entry.file_type()?;
        if ty.is_dir() {
            collect_files(root, &path, out)?;
        } else if ty.is_file()
            && path.extension().and_then(|extension| extension.to_str()) != Some("meta")
        {
            out.push((kind, path.strip_prefix(root)?.to_path_buf(), path));
        } else if ty.is_symlink() {
            bail!(
                "refusing symlink in staged Unity assets: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
        bail!("staged asset is not a regular file: {}", source.display());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).with_context(|| format!("copying {}", source.display()))?;
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 214
        || name.starts_with('.')
        || name.ends_with('.')
        || name.contains("..")
        || !name.split('.').all(|s| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
    {
        bail!("invalid Unity package name {name:?}; use lowercase dot-separated identifiers");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn materializes_sorted_assets_and_metadata() {
        let project = tempdir().unwrap();
        fs::create_dir_all(project.path().join("Assets/Plugins/Managed/sub")).unwrap();
        fs::create_dir_all(project.path().join("Assets/Plugins/macOS")).unwrap();
        fs::create_dir_all(project.path().join("Assets/RustDotnetGenerated")).unwrap();
        fs::write(project.path().join("Assets/Plugins/Managed/z.dll"), b"z").unwrap();
        fs::write(
            project.path().join("Assets/Plugins/Managed/sub/a.pdb"),
            b"a",
        )
        .unwrap();
        let mut arm64_macho = Vec::new();
        arm64_macho.extend_from_slice(&0xfeedfacfu32.to_le_bytes());
        arm64_macho.extend_from_slice(&0x0100000cu32.to_le_bytes());
        arm64_macho.extend_from_slice(&0u32.to_le_bytes());
        arm64_macho.extend_from_slice(&6u32.to_le_bytes());
        arm64_macho.extend_from_slice(&0u32.to_le_bytes());
        arm64_macho.extend_from_slice(&0u32.to_le_bytes());
        arm64_macho.extend_from_slice(&0u32.to_le_bytes());
        arm64_macho.extend_from_slice(&0u32.to_le_bytes());
        fs::write(
            project.path().join("Assets/Plugins/macOS/libx.dylib"),
            arm64_macho,
        )
        .unwrap();
        fs::write(
            project.path().join("Assets/RustDotnetGenerated/link.xml"),
            b"<link/>",
        )
        .unwrap();
        let output = project.path().parent().unwrap().join("upm-test-output");
        let result = package(project.path(), &output, "com.example.rust", "0.1.0", true).unwrap();
        assert!(result.files >= 6);
        assert!(output.join("Runtime/Managed/sub/a.pdb").is_file());
        assert!(output.join("Runtime/Plugins/macOS/libx.dylib").is_file());
        assert!(output.join("Runtime/RustDotnetPreserve.xml").is_file());
        let importer =
            fs::read_to_string(output.join("Editor/RustDotnetPreserveImporter.cs")).unwrap();
        assert!(importer.contains("Packages/com.example.rust/Runtime/RustDotnetPreserve.xml"));
        assert!(importer.contains("AssetDatabase.ImportAsset"));
        assert!(importer.contains("File.Replace(temporary, destination, null)"));
        assert!(!importer.contains("File.Move(temporary, destination, true)"));
        assert!(
            output
                .join("Editor/RustDotnetPreserveImporter.asmdef")
                .is_file()
        );
        assert!(
            !output
                .join("Editor/RustDotnetPreserveImporter.cs.meta.meta")
                .exists()
        );
        assert!(output.join("Runtime/Managed/z.dll.meta").is_file());
        assert!(
            output
                .join("Runtime/Plugins/macOS/libx.dylib.meta")
                .is_file()
        );
        assert!(output.join("package.json").is_file());
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn rejects_nonempty_without_force_and_bad_names() {
        let project = tempdir().unwrap();
        fs::create_dir(project.path().join("Assets")).unwrap();
        let output = tempdir().unwrap();
        assert!(package(project.path(), output.path(), "Bad.Name", "1.0.0", false).is_err());
        fs::write(output.path().join("existing"), b"x").unwrap();
        assert!(
            package(
                project.path(),
                output.path(),
                "com.example.rust",
                "1.0.0",
                false
            )
            .is_err()
        );
    }

    #[test]
    fn permits_project_packages_but_rejects_assets_outputs() {
        let project = tempdir().unwrap();
        fs::create_dir_all(project.path().join("Assets/Plugins/Managed")).unwrap();
        fs::write(
            project.path().join("Assets/Plugins/Managed/demo.dll"),
            b"demo",
        )
        .unwrap();

        assert!(
            package(
                project.path(),
                &project.path().join("Assets/GeneratedPackage"),
                "com.example.rust",
                "1.0.0",
                false
            )
            .is_err()
        );

        let packages_output = project.path().join("Packages/com.example.rust");
        package(
            project.path(),
            &packages_output,
            "com.example.rust",
            "1.0.0",
            false,
        )
        .unwrap();
        assert!(packages_output.join("package.json").is_file());
    }
}
