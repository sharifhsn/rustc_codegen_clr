//! Stage a Rust `cdylib` into a Unity project's native plug-in directory.

use anyhow::{Context, Result, bail, ensure};
use cargo_metadata::MetadataCommand;
use object::{Architecture, Object as _, ObjectSymbol as _};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use crate::cli::UnityNativeArgs;

pub fn run(args: &UnityNativeArgs) -> Result<i32> {
    if !cfg!(target_os = "macos") {
        bail!("unity native currently supports only the macOS host");
    }
    let project = fs::canonicalize(&args.project).context("resolving Unity project")?;
    if !project.join("Assets").is_dir() {
        bail!(
            "Unity project has no Assets directory: {}",
            project.display()
        );
    }
    let crate_arg = match &args.crate_dir {
        Some(path) => path.clone(),
        None => crate::unity::attached_crate_path(&project, "native_crate")?
            .context("no native crate in the Unity attach receipt; pass a crate path")?,
    };
    let crate_path = fs::canonicalize(&crate_arg)
        .with_context(|| format!("resolving native Rust crate {}", crate_arg.display()))?;
    let manifest = if crate_path.is_file() {
        crate_path.clone()
    } else {
        crate_path.join("Cargo.toml")
    };
    if !manifest.is_file() {
        bail!("Rust crate has no Cargo.toml: {}", manifest.display());
    }
    let metadata = MetadataCommand::new()
        .manifest_path(&manifest)
        .no_deps()
        .exec()
        .context("read native crate Cargo metadata")?;
    let package = metadata
        .root_package()
        .context("native crate metadata has no root package")?;
    let name = package.name.clone();
    let target_name = name.replace('-', "_");
    let status = Command::new("cargo")
        .args(["build", "--release", "--manifest-path"])
        .arg(&manifest)
        .status()
        .context("running cargo build")?;
    if !status.success() {
        bail!("cargo build --release failed");
    }
    let source = metadata
        .target_directory
        .as_std_path()
        .join("release")
        .join(format!("lib{target_name}.dylib"));
    if !source.is_file() {
        bail!("release cdylib not found: {}", source.display());
    }
    let bytes = fs::read(&source)?;
    let object = object::File::parse(bytes.as_slice())
        .with_context(|| format!("inspect native Unity plug-in {}", source.display()))?;
    let architecture = match object.architecture() {
        Architecture::Aarch64 => "arm64",
        Architecture::X86_64 => "x86_64",
        other => bail!("unsupported macOS Unity plug-in architecture {other:?}"),
    };
    let symbols = object
        .dynamic_symbols()
        .chain(object.symbols())
        .filter_map(|symbol| symbol.name().ok())
        .map(|name| name.trim_start_matches('_').to_owned())
        .collect::<BTreeSet<_>>();
    let expected_exports = args.exports.clone();
    for export in &expected_exports {
        ensure!(
            export
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_'),
            "invalid native export name {export:?}"
        );
        ensure!(
            symbols.contains(export),
            "native export {export:?} was not found in {}",
            source.display()
        );
    }
    let destination = project.join("Assets/Plugins/macOS");
    fs::create_dir_all(&destination)?;
    let staged = destination.join(format!("lib{target_name}.dylib"));
    write_atomic(&staged, &bytes)?;
    let meta = destination.join(format!("lib{target_name}.dylib.meta"));
    write_atomic(&meta, native_meta(&target_name, architecture).as_bytes())?;
    let generated = project.join("Assets/RustDotnetGenerated");
    fs::create_dir_all(&generated)?;
    let receipt = NativeReceipt {
        schema: 1,
        library: target_name.clone(),
        architecture,
        exports: expected_exports,
    };
    write_atomic(
        &generated.join(format!("{target_name}.native.json")),
        (serde_json::to_string_pretty(&receipt)? + "\n").as_bytes(),
    )?;
    println!("staged {} -> {}", source.display(), staged.display());
    Ok(0)
}

#[derive(Serialize)]
struct NativeReceipt {
    schema: u32,
    library: String,
    architecture: &'static str,
    exports: Vec<String>,
}

fn write_atomic(destination: &Path, bytes: &[u8]) -> Result<()> {
    let parent = destination
        .parent()
        .context("native output has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)
        .with_context(|| format!("atomically replace {}", destination.display()))?;
    Ok(())
}

fn native_meta(library: &str, architecture: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"rust-dotnet-unity-native");
    hash.update([0]);
    hash.update(library.as_bytes());
    let digest = hash.finalize();
    let guid = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let cpu = if architecture == "arm64" {
        "ARM64"
    } else {
        "x86_64"
    };
    format!(
        "fileFormatVersion: 2\nguid: {guid}\nPluginImporter:\n  externalObjects: {{}}\n  serializedVersion: 2\n  iconMap: {{}}\n  executionOrder: {{}}\n  defineConstraints: []\n  isPreloaded: 0\n  isOverridable: 1\n  isExplicitlyReferenced: 0\n  validateReferences: 1\n  platformData:\n  - first:\n      : Any\n    second:\n      enabled: 0\n      settings:\n        Exclude Editor: 0\n        Exclude Linux64: 1\n        Exclude OSXUniversal: 0\n        Exclude Win: 1\n        Exclude Win64: 1\n  - first:\n      Editor: Editor\n    second:\n      enabled: 1\n      settings:\n        CPU: {cpu}\n        DefaultValueInitialized: true\n        OS: OSX\n  - first:\n      Standalone: OSXUniversal\n    second:\n      enabled: 1\n      settings:\n        CPU: AnyCPU\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dylib_name_normalizes_hyphens() {
        assert_eq!(
            "libhello_world.dylib",
            format!("lib{}.dylib", "hello-world".replace('-', "_"))
        );
    }
    #[test]
    fn native_meta_is_deterministic_and_platform_specific() {
        let first = native_meta("probe", "arm64");
        let second = native_meta("probe", "arm64");
        assert_eq!(first, second);
        assert!(first.contains("CPU: ARM64"));
        assert!(first.contains("CPU: AnyCPU"));
        assert!(first.contains("OS: OSX"));
    }
}
