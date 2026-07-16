//! Unity Editor environment diagnostics.

use anyhow::{Context as _, Result, bail};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::cli::{
    UnityArgs, UnityAttachArgs, UnityBuildArgs, UnityCommand, UnityDoctorArgs, UnityNativeArgs,
    UnityPackageArgs,
};

#[derive(Debug, Serialize)]
struct Check {
    ok: bool,
    label: &'static str,
    detail: String,
}

pub fn run(args: &UnityArgs) -> Result<i32> {
    match &args.command {
        UnityCommand::Doctor(a) => doctor(a),
        UnityCommand::Build(a) => build(a),
        UnityCommand::Native(a) => crate::unity_native::run(a),
        UnityCommand::Attach(a) => attach(a),
        UnityCommand::Package(a) => package(a),
    }
}

fn attach(args: &UnityAttachArgs) -> Result<i32> {
    if args.native_crate.is_none() && !args.native_exports.is_empty() {
        bail!("--native-export requires --native-crate");
    }
    if args.native_crate.is_some() && args.native_exports.is_empty() {
        bail!("--native-crate requires at least one --native-export to verify");
    }
    let receipt = crate::unity_attach::attach(
        &args.project,
        &args.crate_dir,
        args.native_crate.as_deref(),
        args.force,
    )?;
    build(&UnityBuildArgs {
        project: args.project.clone(),
        crate_dir: Some(args.crate_dir.clone()),
        destination: PathBuf::from("Assets/Plugins/Managed"),
    })?;
    if let Some(native_crate) = &args.native_crate {
        crate::unity_native::run(&UnityNativeArgs {
            project: args.project.clone(),
            crate_dir: Some(native_crate.clone()),
            exports: args.native_exports.clone(),
        })?;
    }
    println!(
        "attached {} ({}) to {}",
        receipt.assembly_name,
        receipt.public_type,
        args.project.display()
    );
    Ok(0)
}

fn package(args: &UnityPackageArgs) -> Result<i32> {
    let summary = crate::unity_package::package(
        &args.project,
        &args.output,
        &args.name,
        &args.version,
        args.force,
    )?;
    println!(
        "materialized Unity package {} ({}) with {} files",
        args.name,
        summary.output.display(),
        summary.files
    );
    Ok(0)
}

fn build(args: &UnityBuildArgs) -> Result<i32> {
    let project = fs::canonicalize(&args.project).context("resolving Unity project")?;
    if !project.join("Assets").is_dir() {
        bail!(
            "Unity project has no Assets directory: {}",
            project.display()
        );
    }
    let crate_arg = match &args.crate_dir {
        Some(path) => path.clone(),
        None => attached_crate_path(&project, "managed_crate")?
            .unwrap_or_else(|| project.join("rustlib")),
    };
    let crate_input = fs::canonicalize(&crate_arg).with_context(|| {
        format!(
            "resolving managed Rust crate {}; pass it explicitly or run `cargo dotnet unity attach`",
            crate_arg.display()
        )
    })?;
    let manifest = if crate_input.is_file() {
        crate_input.clone()
    } else {
        crate_input.join("Cargo.toml")
    };
    let crate_root = manifest
        .parent()
        .context("managed Rust manifest has no parent")?;
    match attached_crate_path(&project, "managed_crate")? {
        Some(attached) => {
            let attached = fs::canonicalize(&attached).with_context(|| {
                format!(
                    "resolving managed crate from Unity attachment receipt {}",
                    attached.display()
                )
            })?;
            anyhow::ensure!(
                attached == crate_root,
                "Unity project is attached to {}, but this build requested {}; run `cargo dotnet unity attach {} {} --force` to change it",
                attached.display(),
                crate_root.display(),
                project.display(),
                crate_root.display()
            );
        }
        None => {
            // A freshly scaffolded project should become a fully diagnosed attachment after its
            // first build; requiring a separate attach command makes `new --unity` and `doctor
            // --project` disagree about what a valid generated project is.
            crate::unity_attach::attach(&project, crate_root, None, false)?;
        }
    }
    let mut metadata_command = cargo_metadata::MetadataCommand::new();
    let metadata = metadata_command
        .manifest_path(&manifest)
        .no_deps()
        .exec()
        .context("reading Rust crate metadata")?;
    let package = metadata
        .root_package()
        .context("Rust crate metadata has no root package")?;
    let assembly = package
        .metadata
        .get("dotnet")
        .and_then(|dotnet| dotnet.get("assembly-name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(package.name.as_str())
        .to_owned();
    let exe = std::env::current_exe()?;
    let status = Command::new(exe)
        .arg("build")
        .arg(manifest.parent().unwrap_or(Path::new(".")))
        .arg("--dotnet")
        .arg("unity-netstandard2.1")
        .arg("--backend")
        .arg("native")
        .status()?;
    anyhow::ensure!(status.success(), "cargo dotnet build failed");
    let target = metadata.target_directory.into_std_path_buf();
    let mut files = Vec::new();
    for ext in ["dll", "pdb", "xml"] {
        let wanted = format!("{assembly}.{ext}");
        if let Some(path) = find_release_artifact(&target, &wanted) {
            files.push(path);
        }
    }
    let mut runtime_assets = Vec::new();
    if let Some(dll) = files
        .iter()
        .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(&format!("{assembly}.dll")))
    {
        let manifest = PathBuf::from(format!("{}.rustdotnet.runtime-assets", dll.display()));
        if let Ok(text) = fs::read_to_string(manifest) {
            for line in text.lines() {
                if let Some((source, relative)) = line.split_once('|') {
                    let source = PathBuf::from(source);
                    let relative = safe_relative_asset(relative)?;
                    if source.is_file() {
                        runtime_assets.push((source, relative));
                    }
                }
            }
        }
    }
    anyhow::ensure!(
        !files.is_empty(),
        "managed assembly {assembly} not found under target"
    );
    let dest = if args.destination.is_absolute() {
        args.destination.clone()
    } else {
        project.join(&args.destination)
    };
    let staging = dest.with_extension("staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    for src in files {
        fs::copy(&src, staging.join(src.file_name().unwrap()))?;
    }
    for (source, relative) in runtime_assets {
        let output = staging.join(relative);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, output)?;
    }
    fs::create_dir_all(&dest)?;
    promote_tree(&staging, &dest)?;
    let _ = fs::remove_dir(&staging);
    write_managed_meta_tree(&dest, &assembly)?;
    let generated = project.join("Assets/RustDotnetGenerated");
    fs::create_dir_all(&generated)?;
    let link = generated.join("link.xml");
    fs::write(
        link,
        format!(
            "<linker>\n  <assembly fullname=\"{assembly}\">\n    <type fullname=\"{assembly}.Exports\" preserve=\"all\" />\n  </assembly>\n</linker>\n"
        ),
    )?;
    write_asset_meta(
        &generated.join("link.xml"),
        &assembly,
        "RustDotnetGenerated/link.xml",
        false,
    )?;
    println!("staged {assembly} into {}", dest.display());
    Ok(0)
}

fn write_managed_meta_tree(root: &Path, identity: &str) -> Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let mut entries: Vec<_> =
            fs::read_dir(&directory)?.collect::<std::result::Result<_, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) == Some("meta") {
                continue;
            }
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            write_asset_meta(&path, identity, &format!("Managed/{relative}"), false)?;
        }
    }
    let mut folder_meta = root.as_os_str().to_os_string();
    folder_meta.push(".meta");
    write_asset_meta(&PathBuf::from(folder_meta), identity, "Managed", true)?;
    Ok(())
}

fn write_asset_meta(path: &Path, identity: &str, logical: &str, folder: bool) -> Result<()> {
    let mut hash = Sha256::new();
    hash.update(identity.as_bytes());
    hash.update([0]);
    hash.update(logical.as_bytes());
    let digest = hash.finalize();
    let guid = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let body = if folder {
        format!(
            "fileFormatVersion: 2\nguid: {guid}\nfolderAsset: yes\nDefaultImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
        )
    } else if path.extension().and_then(|extension| extension.to_str()) == Some("dll") {
        format!(
            "fileFormatVersion: 2\nguid: {guid}\nPluginImporter:\n  externalObjects: {{}}\n  serializedVersion: 2\n  iconMap: {{}}\n  executionOrder: {{}}\n  defineConstraints: []\n  isPreloaded: 0\n  isOverridable: 1\n  isExplicitlyReferenced: 0\n  validateReferences: 1\n  platformData:\n  - first:\n      Any: \n    second:\n      enabled: 1\n      settings: {{}}\n  - first:\n      Editor: Editor\n    second:\n      enabled: 1\n      settings:\n        CPU: AnyCPU\n        DefaultValueInitialized: true\n        OS: AnyOS\n  - first:\n      Standalone: OSXUniversal\n    second:\n      enabled: 1\n      settings:\n        CPU: AnyCPU\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
        )
    } else {
        format!(
            "fileFormatVersion: 2\nguid: {guid}\nTextScriptImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
        )
    };
    let meta = if folder {
        path.to_path_buf()
    } else {
        let mut name = path.as_os_str().to_os_string();
        name.push(".meta");
        PathBuf::from(name)
    };
    fs::write(&meta, body).with_context(|| format!("write {}", meta.display()))?;
    Ok(())
}

fn safe_relative_asset(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("unsafe runtime asset path {value:?}");
    }
    Ok(path.to_owned())
}

pub(crate) fn attached_crate_path(project: &Path, field: &str) -> Result<Option<PathBuf>> {
    let receipt = project.join("Assets/RustDotnetGenerated/rustdotnet.attach.json");
    if !receipt.is_file() {
        return Ok(None);
    }
    let json: serde_json::Value = serde_json::from_slice(
        &fs::read(&receipt).with_context(|| format!("read {}", receipt.display()))?,
    )
    .with_context(|| format!("parse {}", receipt.display()))?;
    let Some(relative) = json.get(field).and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    let path = Path::new(relative);
    if path.is_absolute() {
        bail!("attach receipt contains absolute {field} path; run Unity attach again");
    }
    Ok(Some(project.join(path)))
}

fn promote_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let output = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&output)?;
            promote_tree(&entry.path(), &output)?;
        } else {
            if output.is_file() {
                fs::remove_file(&output)?;
            }
            fs::rename(entry.path(), output)?;
        }
    }
    Ok(())
}

fn find_release_artifact(root: &Path, name: &str) -> Option<PathBuf> {
    for relative in [
        Path::new("x86_64-unknown-dotnet/release").join(name),
        Path::new("dotnet/release").join(name),
        Path::new("release").join(name),
    ] {
        let candidate = root.join(relative);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    walk_find(root, name)
}

fn walk_find(root: &Path, name: &str) -> Option<PathBuf> {
    let rd = fs::read_dir(root).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(x) = walk_find(&p, name) {
                return Some(x);
            }
        } else if p.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(p);
        }
    }
    None
}

fn doctor(args: &UnityDoctorArgs) -> Result<i32> {
    let mut checks = checks(args.editor.as_deref(), args.version, &args.profile);
    if let Some(cli) = find_unity_cli() {
        let output = Command::new(&cli)
            .args(["license", "status", "--json", "--non-interactive"])
            .output();
        let (ok, detail) = match output {
            Ok(output) => {
                let detail = String::from_utf8_lossy(if output.stdout.is_empty() {
                    &output.stderr
                } else {
                    &output.stdout
                })
                .trim()
                .to_owned();
                (output.status.success(), detail)
            }
            Err(error) => (false, error.to_string()),
        };
        checks.push(Check {
            ok,
            label: "Unity license",
            detail: format!("{}: {detail}", cli.display()),
        });
    }
    if let Some(project) = &args.project {
        append_project_checks(&mut checks, project, args.version);
    }
    let failed = checks.iter().any(|c| !c.ok);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&checks)?);
    } else {
        println!("cargo dotnet unity doctor — environment:\n");
        for c in &checks {
            println!(
                "  [{}] {}\n         {}",
                if c.ok { " OK " } else { "FAIL" },
                c.label,
                c.detail
            );
        }
        println!(
            "\n{}",
            if failed {
                "Unity prerequisites are incomplete."
            } else {
                "Unity prerequisites passed."
            }
        );
    }
    Ok(if failed { 1 } else { 0 })
}

fn append_project_checks(checks: &mut Vec<Check>, project: &Path, required_major: u32) {
    let project = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    let project_version = project.join("ProjectSettings/ProjectVersion.txt");
    let version_text = fs::read_to_string(&project_version).unwrap_or_default();
    let version = version_text
        .lines()
        .find_map(|line| line.strip_prefix("m_EditorVersion: "))
        .unwrap_or_default();
    let version_ok = project.join("Assets").is_dir()
        && version
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            == Some(required_major);
    checks.push(Check {
        ok: version_ok,
        label: "Unity project",
        detail: format!("{} ({version})", project.display()),
    });

    let generated = project.join("Assets/RustDotnetGenerated");
    let receipt_path = generated.join("rustdotnet.attach.json");
    let receipt = fs::read(&receipt_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let receipt_ok = receipt
        .as_ref()
        .and_then(|value| value.get("schema"))
        .and_then(serde_json::Value::as_u64)
        == Some(1)
        && ["managed_crate", "native_crate"].iter().all(|field| {
            receipt
                .as_ref()
                .and_then(|value| value.get(field))
                .map(|value| {
                    value.is_null()
                        || value
                            .as_str()
                            .is_some_and(|path| !Path::new(path).is_absolute())
                })
                .unwrap_or(*field == "native_crate")
        });
    checks.push(Check {
        ok: receipt_ok,
        label: "Rust attachment receipt",
        detail: receipt_path.display().to_string(),
    });

    let assembly = receipt
        .as_ref()
        .and_then(|value| value.get("assembly_name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let managed = project
        .join("Assets/Plugins/Managed")
        .join(format!("{assembly}.dll"));
    let helper = project.join("Assets/Plugins/Managed/Mycorrhiza.Interop.Helpers.dll");
    let managed_ok = !assembly.is_empty()
        && managed.is_file()
        && managed.with_extension("dll.meta").is_file()
        && helper.is_file();
    checks.push(Check {
        ok: managed_ok,
        label: "managed Rust assets",
        detail: format!("{}; helper={}", managed.display(), helper.display()),
    });

    let link = generated.join("link.xml");
    let link_ok = fs::read_to_string(&link)
        .ok()
        .is_some_and(|text| !assembly.is_empty() && text.contains(assembly))
        && link.with_extension("xml.meta").is_file();
    checks.push(Check {
        ok: link_ok,
        label: "UnityLinker roots",
        detail: link.display().to_string(),
    });

    let native_expected = receipt
        .as_ref()
        .and_then(|value| value.get("native_crate"))
        .is_some_and(|value| value.is_string());
    let native_receipt = fs::read_dir(&generated).ok().and_then(|entries| {
        entries.flatten().map(|entry| entry.path()).find(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("json")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".native.json"))
        })
    });
    if native_expected || native_receipt.is_some() {
        let library = native_receipt
            .as_ref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.get("library")?.as_str().map(str::to_owned))
            .unwrap_or_default();
        let dylib = project
            .join("Assets/Plugins/macOS")
            .join(format!("lib{library}.dylib"));
        checks.push(Check {
            ok: !library.is_empty()
                && dylib.is_file()
                && PathBuf::from(format!("{}.meta", dylib.display())).is_file(),
            label: "native Rust assets",
            detail: dylib.display().to_string(),
        });
    }
}

fn checks(editor: Option<&Path>, required_major: u32, profile: &str) -> Vec<Check> {
    let path = editor.map(PathBuf::from).or_else(find_editor);
    let Some(path) = path else {
        return vec![Check {
            ok: false,
            label: "Unity Editor",
            detail: "not found (set UNITY_EDITOR or --editor)".into(),
        }];
    };
    let version = Command::new(&path)
        .arg("-version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let major_ok = version
        .split('.')
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        == Some(required_major);
    let mut out = vec![Check {
        ok: major_ok,
        label: "Unity Editor",
        detail: format!(
            "{} ({})",
            path.display(),
            if version.is_empty() {
                "version unknown"
            } else {
                &version
            }
        ),
    }];
    let root = if path.ends_with("Unity") && path.parent().and_then(Path::parent).is_some() {
        path.parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .unwrap_or(&path)
    } else {
        &path
    };
    let netstandard =
        root.join("Contents/Resources/Scripting/NetStandard/ref/2.1.0/netstandard.dll");
    let unity_engine = root.join("Contents/Resources/Scripting/Managed/UnityEngine.dll");
    let profile_ok = profile == "netstandard2.1" && netstandard.is_file() && unity_engine.is_file();
    out.push(Check {
        ok: profile_ok,
        label: "managed compatibility profile",
        detail: format!(
            "{profile} (facade: {}; Unity API: {})",
            netstandard.display(),
            unity_engine.display()
        ),
    });
    if cfg!(target_os = "macos") {
        let module = root.join("Contents/PlaybackEngines/MacStandaloneSupport");
        out.push(Check {
            ok: module.exists(),
            label: "macOS IL2CPP module",
            detail: module.display().to_string(),
        });
    }
    out
}

fn find_editor() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let hub = Path::new("/Applications/Unity/Hub/Editor");
        let mut installed = fs::read_dir(hub)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("Unity.app/Contents/MacOS/Unity"))
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        installed.sort();
        if let Some(path) = installed.pop() {
            return Some(path);
        }
        let standalone = PathBuf::from("/Applications/Unity.app/Contents/MacOS/Unity");
        if standalone.is_file() {
            return Some(standalone);
        }
    }
    None
}

fn find_unity_cli() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let candidate = PathBuf::from(home).join(".unity/bin/unity");
    candidate.is_file().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_editor_is_reported() {
        let c = checks(Some(Path::new("/no/such/unity")), 6000, "netstandard2.1");
        assert!(!c[0].ok);
    }
    #[test]
    fn wrong_profile_is_rejected() {
        let c = checks(Some(Path::new("/no/such/unity")), 6000, "net10.0");
        assert!(!c[0].ok);
    }
    #[test]
    fn runtime_asset_paths_cannot_escape_the_unity_destination() {
        assert_eq!(
            safe_relative_asset("runtimes/osx/native/probe.dylib").unwrap(),
            Path::new("runtimes/osx/native/probe.dylib")
        );
        assert!(safe_relative_asset("../escape.dll").is_err());
        assert!(safe_relative_asset("/absolute.dll").is_err());
    }

    #[test]
    fn attached_project_checks_require_staged_managed_and_linker_assets() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path();
        fs::create_dir_all(project.join("Assets/Plugins/Managed")).unwrap();
        fs::create_dir_all(project.join("Assets/RustDotnetGenerated")).unwrap();
        fs::create_dir_all(project.join("ProjectSettings")).unwrap();
        fs::write(
            project.join("ProjectSettings/ProjectVersion.txt"),
            "m_EditorVersion: 6000.3.19f1\n",
        )
        .unwrap();
        fs::write(
            project.join("Assets/RustDotnetGenerated/rustdotnet.attach.json"),
            r#"{"schema":1,"assembly_name":"Game.Rust","managed_crate":"../rust"}"#,
        )
        .unwrap();
        for path in [
            "Assets/Plugins/Managed/Game.Rust.dll",
            "Assets/Plugins/Managed/Game.Rust.dll.meta",
            "Assets/Plugins/Managed/Mycorrhiza.Interop.Helpers.dll",
            "Assets/RustDotnetGenerated/link.xml.meta",
        ] {
            fs::write(project.join(path), b"x").unwrap();
        }
        fs::write(
            project.join("Assets/RustDotnetGenerated/link.xml"),
            "<assembly fullname=\"Game.Rust\" preserve=\"all\" />",
        )
        .unwrap();
        let mut checks = Vec::new();
        append_project_checks(&mut checks, project, 6000);
        assert!(checks.iter().all(|check| check.ok), "{checks:?}");
    }
}
