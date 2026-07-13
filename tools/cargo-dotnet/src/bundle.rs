//! Versioned, checksummed `CARGO_DOTNET_HOME` bundles.
//!
//! A bundle contains the source-derived SDK/runtime inputs that setup normally copies from a
//! checkout: backend + linker, target spec, PAL/overlays, MSBuild integration, SDK crates, helper
//! sources, legacy launchers, and the current `cargo-dotnet` executable. It deliberately does not
//! claim to contain rustup or .NET; those remain host prerequisites and are recorded in the docs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::cli::{BundleArgs, BundleCommand};

const SCHEMA: u32 = 1;
const MANIFEST: &str = "bundle-manifest.json";
const PAYLOAD_PREFIX: &str = "payload/";
const INSTALL_LOCK: &str = "BUNDLE-LOCK.json";

#[derive(Debug, Serialize, Deserialize)]
struct BundleManifest {
    schema: u32,
    kind: String,
    host_os: String,
    host_arch: String,
    host_rid: String,
    toolchain: String,
    cargo_dotnet_version: String,
    files: Vec<BundleFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BundleFile {
    path: String,
    bytes: u64,
    sha256: String,
    executable: bool,
}

#[derive(Debug)]
struct SourceFile {
    path: String,
    source: PathBuf,
    executable: bool,
}

pub fn run(args: &BundleArgs) -> Result<i32> {
    match &args.command {
        BundleCommand::Create { home, out } => {
            let home = resolve_home(home)?;
            create(&home, out)?;
            Ok(0)
        }
        BundleCommand::Verify { archive } => {
            verify_archive_checksum(archive)?;
            let manifest = verify(archive, false)?;
            println!(
                "verified cargo-dotnet bundle schema {}: {} files for {}-{}",
                manifest.schema,
                manifest.files.len(),
                manifest.host_os,
                manifest.host_arch
            );
            Ok(0)
        }
        BundleCommand::Install {
            archive,
            home,
            force,
            no_install_cli,
        } => {
            let home = resolve_home(home)?;
            install(archive, &home, *force, !*no_install_cli)?;
            Ok(0)
        }
    }
}

fn resolve_home(home: &Option<PathBuf>) -> Result<PathBuf> {
    Ok(match home {
        Some(path) => path.clone(),
        None => crate::mode::cargo_dotnet_home()?,
    })
}

fn create(home: &Path, out: &Path) -> Result<()> {
    if !home.is_dir() {
        bail!("install home does not exist: {}", home.display());
    }
    for required in ["VERSION", "bin", "target", "dotnet_pal", "dotnet_overlays"] {
        if !home.join(required).exists() {
            bail!(
                "install home is incomplete (missing {}); run `cargo dotnet setup` first",
                home.join(required).display()
            );
        }
    }

    let mut sources = Vec::new();
    for name in [
        "VERSION",
        "core.sh",
        "cargo-dotnet",
        "bin",
        "target",
        "dotnet_pal",
        "dotnet_overlays",
        "msbuild",
        "crates",
        "mycorrhiza_interop_helpers",
    ] {
        let path = home.join(name);
        if path.exists() {
            collect_sources(home, &path, &mut sources)?;
        }
    }

    let executable_name = if cfg!(windows) {
        "bin/cargo-dotnet.exe"
    } else {
        "bin/cargo-dotnet"
    };
    sources.retain(|source| source.path != executable_name);
    sources.push(SourceFile {
        path: executable_name.to_string(),
        source: std::env::current_exe().context("locating the running cargo-dotnet executable")?,
        executable: true,
    });
    sources.sort_by(|left, right| left.path.cmp(&right.path));

    let mut files = Vec::with_capacity(sources.len());
    for source in &sources {
        let bytes = fs::read(&source.source)
            .with_context(|| format!("reading bundle input {}", source.source.display()))?;
        files.push(BundleFile {
            path: source.path.clone(),
            bytes: bytes.len() as u64,
            sha256: hex_sha256(&bytes),
            executable: source.executable,
        });
    }
    let facts = crate::host::HostFacts::detect();
    let manifest = BundleManifest {
        schema: SCHEMA,
        kind: "cargo-dotnet-install-home".to_string(),
        host_os: std::env::consts::OS.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        host_rid: facts.host_rid.to_string(),
        toolchain: crate::mode::read_home_toolchain(home),
        cargo_dotnet_version: env!("CARGO_PKG_VERSION").to_string(),
        files,
    };

    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating bundle output directory {}", parent.display()))?;
    }
    let temporary = out.with_extension("zip.tmp");
    let file = File::create(&temporary)
        .with_context(|| format!("creating temporary bundle {}", temporary.display()))?;
    let mut zip = ZipWriter::new(file);
    let regular = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let executable = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o755);

    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    zip.start_file(MANIFEST, regular)?;
    zip.write_all(&manifest_bytes)?;
    for source in &sources {
        zip.start_file(
            format!("{PAYLOAD_PREFIX}{}", source.path),
            if source.executable {
                executable
            } else {
                regular
            },
        )?;
        let bytes = fs::read(&source.source)?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?.sync_all()?;

    verify(&temporary, false).context("self-verifying generated bundle")?;
    fs::rename(&temporary, out).with_context(|| format!("publishing bundle {}", out.display()))?;
    let archive_bytes = fs::read(out)?;
    let checksum = format!("{}  {}\n", hex_sha256(&archive_bytes), file_name(out)?);
    fs::write(checksum_path(out), checksum)?;
    println!("created cargo-dotnet bundle: {}", out.display());
    println!("bundle checksum: {}", checksum_path(out).display());
    Ok(())
}

fn collect_sources(root: &Path, path: &Path, out: &mut Vec<SourceFile>) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        bail!("bundle inputs may not contain symlinks: {}", path.display());
    }
    if metadata.is_dir() {
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            collect_sources(root, &entry.path(), out)?;
        }
    } else if metadata.is_file() {
        let relative = path
            .strip_prefix(root)
            .context("bundle source escaped install home")?;
        let relative = portable_path(relative)?;
        out.push(SourceFile {
            path: relative,
            source: path.to_path_buf(),
            executable: is_executable(path, &metadata),
        });
    }
    Ok(())
}

fn verify(path: &Path, require_host: bool) -> Result<BundleManifest> {
    let file = File::open(path).with_context(|| format!("opening bundle {}", path.display()))?;
    let mut zip = ZipArchive::new(file).context("opening bundle ZIP")?;
    let manifest: BundleManifest = {
        let mut entry = zip
            .by_name(MANIFEST)
            .context("bundle manifest is missing")?;
        if entry.size() > 16 * 1024 * 1024 {
            bail!("bundle manifest exceeds the 16 MiB safety limit");
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        serde_json::from_slice(&bytes).context("parsing bundle manifest")?
    };
    validate_manifest(&manifest, require_host)?;

    let expected: BTreeMap<&str, &BundleFile> = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    if expected.len() != manifest.files.len() {
        bail!("bundle manifest contains duplicate paths");
    }
    let mut seen = BTreeSet::new();
    let mut manifest_entries = 0usize;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name().to_string();
        if name == MANIFEST {
            manifest_entries += 1;
            continue;
        }
        let relative = name
            .strip_prefix(PAYLOAD_PREFIX)
            .ok_or_else(|| anyhow::anyhow!("unexpected bundle entry: {name}"))?;
        validate_relative(relative)?;
        let expected_file = expected.get(relative).ok_or_else(|| {
            anyhow::anyhow!("payload entry is not declared in manifest: {relative}")
        })?;
        if !seen.insert(relative.to_string()) {
            bail!("duplicate payload entry: {relative}");
        }
        if entry.size() != expected_file.bytes {
            bail!("bundle size mismatch for {relative}");
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        if bytes.len() as u64 != expected_file.bytes {
            bail!("bundle size mismatch for {relative}");
        }
        if hex_sha256(&bytes) != expected_file.sha256 {
            bail!("bundle SHA-256 mismatch for {relative}");
        }
    }
    if manifest_entries != 1 {
        bail!("bundle must contain exactly one manifest (found {manifest_entries})");
    }
    if seen.len() != expected.len() {
        let missing = expected
            .keys()
            .find(|path| !seen.contains(**path))
            .copied()
            .unwrap_or("<unknown>");
        bail!("manifest payload is missing from archive: {missing}");
    }
    Ok(manifest)
}

fn validate_manifest(manifest: &BundleManifest, require_host: bool) -> Result<()> {
    if manifest.schema != SCHEMA {
        bail!(
            "unsupported cargo-dotnet bundle schema {} (expected {SCHEMA})",
            manifest.schema
        );
    }
    if manifest.kind != "cargo-dotnet-install-home" {
        bail!("unsupported cargo-dotnet bundle kind: {}", manifest.kind);
    }
    if manifest.files.is_empty() {
        bail!("bundle manifest has no files");
    }
    if manifest.host_rid.is_empty()
        || manifest.toolchain.is_empty()
        || manifest.cargo_dotnet_version.is_empty()
    {
        bail!("bundle manifest is missing host/toolchain/front-end identity");
    }
    for file in &manifest.files {
        validate_relative(&file.path)?;
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("invalid SHA-256 in bundle manifest for {}", file.path);
        }
    }
    if require_host
        && (manifest.host_os != std::env::consts::OS
            || manifest.host_arch != std::env::consts::ARCH)
    {
        bail!(
            "bundle targets {}-{}, but this host is {}-{}",
            manifest.host_os,
            manifest.host_arch,
            std::env::consts::OS,
            std::env::consts::ARCH
        );
    }
    Ok(())
}

fn install(archive: &Path, home: &Path, force: bool, install_cli: bool) -> Result<()> {
    verify_archive_checksum(archive)?;
    let manifest = verify(archive, true)?;
    if home.exists() && !force {
        bail!(
            "install home already exists: {} (pass --force to replace it)",
            home.display()
        );
    }
    let parent = home
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = tempfile::Builder::new()
        .prefix(".cargo-dotnet-restore-")
        .tempdir_in(parent)?;
    extract_verified(archive, temp.path(), &manifest)?;
    verify_tree(temp.path(), &manifest)?;
    fs::write(
        temp.path().join(INSTALL_LOCK),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    let backup = parent.join(format!(".cargo-dotnet-backup-{}", std::process::id()));
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    if home.exists() {
        fs::rename(home, &backup).context("moving previous install home aside")?;
    }
    let staged = temp.keep();
    if let Err(error) = fs::rename(&staged, home) {
        if backup.exists() {
            let _ = fs::rename(&backup, home);
        }
        return Err(error).context("activating restored install home");
    }
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }

    if install_cli {
        install_front_end(home)?;
    }
    println!(
        "installed verified cargo-dotnet bundle -> {} (toolchain {})",
        home.display(),
        manifest.toolchain
    );
    Ok(())
}

fn extract_verified(archive: &Path, destination: &Path, manifest: &BundleManifest) -> Result<()> {
    let expected: BTreeMap<&str, &BundleFile> = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut zip = ZipArchive::new(File::open(archive)?)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let Some(relative) = entry.name().strip_prefix(PAYLOAD_PREFIX) else {
            continue;
        };
        let metadata = expected
            .get(relative)
            .ok_or_else(|| anyhow::anyhow!("undeclared payload entry: {relative}"))?;
        let target = destination.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.sync_all()?;
        set_executable(&target, metadata.executable)?;
    }
    Ok(())
}

fn verify_tree(root: &Path, manifest: &BundleManifest) -> Result<()> {
    for file in &manifest.files {
        let path = root.join(&file.path);
        let bytes = fs::read(&path)
            .with_context(|| format!("restored bundle file is missing: {}", path.display()))?;
        if bytes.len() as u64 != file.bytes || hex_sha256(&bytes) != file.sha256 {
            bail!("restored bundle file failed verification: {}", file.path);
        }
    }
    Ok(())
}

/// Verify a restored bundle home when it carries a bundle lock. Homes created directly by the
/// source-checkout setup predate bundles and return `Ok(false)`; they remain supported but do not
/// gain an integrity claim they cannot prove.
pub(crate) fn verify_installed_if_locked(home: &Path) -> Result<bool> {
    let lock = home.join(INSTALL_LOCK);
    if !lock.is_file() {
        return Ok(false);
    }
    let manifest: BundleManifest = serde_json::from_slice(
        &fs::read(&lock).with_context(|| format!("reading bundle lock {}", lock.display()))?,
    )
    .context("parsing installed bundle lock")?;
    validate_manifest(&manifest, true)?;
    verify_tree(home, &manifest).context("installed cargo-dotnet bundle integrity check failed")?;
    Ok(true)
}

fn install_front_end(home: &Path) -> Result<()> {
    let name = if cfg!(windows) {
        "cargo-dotnet.exe"
    } else {
        "cargo-dotnet"
    };
    let source = home.join("bin").join(name);
    let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from).unwrap_or(
        crate::host::home_dir()
            .context("locating home for CARGO_HOME")?
            .join(".cargo"),
    );
    let destination = cargo_home.join("bin").join(name);
    fs::create_dir_all(destination.parent().expect("cargo bin has a parent"))?;
    let temporary = destination.with_extension("tmp");
    fs::copy(&source, &temporary)?;
    set_executable(&temporary, true)?;
    if cfg!(windows) && destination.exists() {
        fs::remove_file(&destination)?;
    }
    fs::rename(&temporary, &destination)?;
    println!(
        "installed cargo-dotnet front-end -> {}",
        destination.display()
    );
    Ok(())
}

fn validate_relative(path: &str) -> Result<()> {
    if path.is_empty() || path.contains('\\') {
        bail!("invalid bundle path: {path:?}");
    }
    let candidate = Path::new(path);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("unsafe bundle path: {path}");
    }
    Ok(())
}

fn portable_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            _ => bail!("bundle path is not relative: {}", path.display()),
        }
    }
    let result = parts.join("/");
    validate_relative(&result)?;
    Ok(result)
}

fn checksum_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.sha256", path.display()))
}

fn verify_archive_checksum(path: &Path) -> Result<()> {
    let sidecar = checksum_path(path);
    let text = fs::read_to_string(&sidecar)
        .with_context(|| format!("bundle checksum sidecar is missing: {}", sidecar.display()))?;
    let mut fields = text.split_whitespace();
    let expected = fields.next().context("bundle checksum sidecar is empty")?;
    let expected_name = fields
        .next()
        .context("bundle checksum sidecar has no filename")?;
    if fields.next().is_some() || expected_name != file_name(path)? {
        bail!("bundle checksum sidecar has an invalid format or filename");
    }
    let actual = hex_sha256(&fs::read(path)?);
    if expected != actual {
        bail!("bundle archive SHA-256 mismatch");
    }
    Ok(())
}

fn file_name(path: &Path) -> Result<String> {
    Ok(path
        .file_name()
        .context("bundle output has no filename")?
        .to_string_lossy()
        .into_owned())
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(unix)]
fn is_executable(_path: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(path: &Path, _metadata: &fs::Metadata) -> bool {
    path.extension().is_some_and(|extension| extension == "exe")
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
    )?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_home(root: &Path) -> PathBuf {
        let home = root.join("home");
        for relative in ["bin", "target", "dotnet_pal", "dotnet_overlays"] {
            fs::create_dir_all(home.join(relative)).unwrap();
        }
        fs::write(
            home.join("VERSION"),
            "schema = 1\ntoolchain = nightly-2026-06-17\n",
        )
        .unwrap();
        fs::write(home.join("bin/linker"), b"linker").unwrap();
        fs::write(home.join("bin/librustc_codegen_clr.so"), b"backend").unwrap();
        fs::write(home.join("target/x86_64-unknown-dotnet.json"), b"{}").unwrap();
        fs::write(home.join("dotnet_pal/pal.rs"), b"pal").unwrap();
        fs::write(home.join("dotnet_overlays/REGISTRY.toml"), b"overlay").unwrap();
        home
    }

    #[test]
    fn bundle_roundtrip_verifies_and_restores() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let archive = temp.path().join("sdk.zip");
        create(&home, &archive).unwrap();
        let manifest = verify(&archive, true).unwrap();
        assert!(manifest.files.iter().any(|file| file.path == "VERSION"));
        assert!(
            manifest
                .files
                .iter()
                .any(|file| file.path.starts_with("bin/cargo-dotnet"))
        );

        let restored = temp.path().join("restored");
        install(&archive, &restored, false, false).unwrap();
        assert_eq!(
            fs::read(restored.join("dotnet_pal/pal.rs")).unwrap(),
            b"pal"
        );
        assert!(verify_installed_if_locked(&restored).unwrap());
        fs::write(restored.join("dotnet_pal/pal.rs"), b"tampered").unwrap();
        assert!(verify_installed_if_locked(&restored).is_err());
        assert!(install(&archive, &restored, false, false).is_err());
        install(&archive, &restored, true, false).unwrap();
        assert!(verify_installed_if_locked(&restored).unwrap());
    }

    #[test]
    fn unsafe_bundle_paths_are_rejected() {
        for path in ["", "../escape", "/absolute", "a/../b", "a\\b"] {
            assert!(validate_relative(path).is_err(), "accepted {path:?}");
        }
    }
}
