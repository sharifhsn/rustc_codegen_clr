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
    validate_manifest(&manifest, true)?;
    validate_version_identity(&manifest, &fs::read_to_string(home.join("VERSION"))?)?;

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
    let mut version_identity = None;
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
        if relative == "VERSION" {
            version_identity =
                Some(String::from_utf8(bytes).context("bundle VERSION is not valid UTF-8")?);
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
    validate_version_identity(
        &manifest,
        version_identity
            .as_deref()
            .context("bundle payload is missing VERSION identity")?,
    )?;
    Ok(manifest)
}

fn validate_manifest(manifest: &BundleManifest, require_running_cli: bool) -> Result<()> {
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
    let expected_rid =
        expected_host_rid(&manifest.host_os, &manifest.host_arch).ok_or_else(|| {
            anyhow::anyhow!(
                "bundle names an unsupported host tuple: {}-{}",
                manifest.host_os,
                manifest.host_arch
            )
        })?;
    if manifest.host_rid != expected_rid {
        bail!(
            "bundle RID {} does not match host tuple {}-{} (expected {expected_rid})",
            manifest.host_rid,
            manifest.host_os,
            manifest.host_arch
        );
    }
    for file in &manifest.files {
        validate_relative(&file.path)?;
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("invalid SHA-256 in bundle manifest for {}", file.path);
        }
    }
    if require_running_cli
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
    if require_running_cli {
        if manifest.cargo_dotnet_version != env!("CARGO_PKG_VERSION") {
            bail!(
                "bundle cargo-dotnet version {} does not match running CLI {}",
                manifest.cargo_dotnet_version,
                env!("CARGO_PKG_VERSION")
            );
        }
        if manifest.toolchain != crate::mode::DEFAULT_TOOLCHAIN {
            bail!(
                "bundle toolchain {} does not match running CLI toolchain {}",
                manifest.toolchain,
                crate::mode::DEFAULT_TOOLCHAIN
            );
        }
        let cli_path = if cfg!(windows) {
            "bin/cargo-dotnet.exe"
        } else {
            "bin/cargo-dotnet"
        };
        let bundled_cli = manifest
            .files
            .iter()
            .find(|file| file.path == cli_path)
            .context("bundle manifest does not contain its cargo-dotnet front-end")?;
        let running_cli = std::env::current_exe().context("locating running cargo-dotnet")?;
        let running_hash = hex_sha256(&fs::read(&running_cli)?);
        if bundled_cli.sha256 != running_hash {
            bail!(
                "bundle front-end does not match the running cargo-dotnet executable: {}",
                running_cli.display()
            );
        }
    }
    Ok(())
}

fn expected_host_rid(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("linux-x64"),
        ("macos", "aarch64") => Some("osx-arm64"),
        ("windows", "x86_64") => Some("win-x64"),
        _ => None,
    }
}

fn version_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        (candidate.trim() == key).then(|| value.trim().trim_matches('"'))
    })
}

fn validate_version_identity(manifest: &BundleManifest, text: &str) -> Result<()> {
    let schema = version_value(text, "schema").context("VERSION is missing schema")?;
    let release_tag =
        version_value(text, "release_tag").context("VERSION is missing release_tag")?;
    let recorded_cli_version = version_value(text, "cargo_dotnet_version");
    let host_rid = version_value(text, "host_rid").context("VERSION is missing host_rid")?;
    let toolchain = version_value(text, "toolchain").context("VERSION is missing toolchain")?;
    let expected_release_tag = format!("rust-dotnet-v{}", manifest.cargo_dotnet_version);
    let version_matches = recorded_cli_version
        .map(|version| version == manifest.cargo_dotnet_version)
        // Backward-compatible verification for immutable 0.0.1 bundles, whose VERSION file
        // predates the explicit cargo_dotnet_version field but has an exact release tag.
        .unwrap_or(release_tag == expected_release_tag);
    let host_rid_matches = host_rid == manifest.host_rid
        || (manifest.cargo_dotnet_version == "0.0.1"
            && matches!(
                (manifest.host_rid.as_str(), host_rid),
                ("osx-arm64", "macos-arm64") | ("win-x64", "windows-x64")
            ));
    if schema != "1"
        || !version_matches
        || (release_tag != "untagged" && release_tag != expected_release_tag)
        || !host_rid_matches
        || toolchain != manifest.toolchain
    {
        bail!("bundle VERSION identity does not match its manifest");
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
    crate::path_safety::require_owned_or_empty_sdk_home(home)?;
    let planned_home = crate::path_safety::planned_absolute(home)?;
    let cargo_home = cargo_home_path()?;
    fs::create_dir_all(&cargo_home)?;
    let mut protected = vec![
        ("bundle archive", fs::canonicalize(archive)?),
        ("working directory", std::env::current_dir()?),
        (
            "running cargo-dotnet",
            std::env::current_exe().context("locating running cargo-dotnet")?,
        ),
        ("Cargo home", cargo_home),
    ];
    if let crate::mode::Mode::Dev { repo_root } = crate::mode::detect()? {
        protected.push(("repository", repo_root));
    }
    crate::path_safety::reject_ancestor_of(&planned_home, protected)?;
    let temp = tempfile::Builder::new()
        .prefix(".cargo-dotnet-restore-")
        .tempdir_in(parent)?;
    extract_verified(archive, temp.path(), &manifest)?;
    verify_tree(temp.path(), &manifest)?;
    fs::write(
        temp.path().join(INSTALL_LOCK),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    let staged = temp.keep();
    let front_end = install_cli
        .then(|| stage_front_end(&staged, home))
        .transpose()?;
    activate_install(&staged, home, front_end, || Ok(()))?;
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
    let expected = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let mut allowed_dirs = BTreeSet::new();
    for file in &manifest.files {
        let mut parent = Path::new(&file.path).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            allowed_dirs.insert(portable_path(path)?);
            parent = path.parent();
        }
    }
    verify_tree_entries(root, root, &expected, &allowed_dirs)?;
    for file in &manifest.files {
        let path = root.join(&file.path);
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("reading restored bundle metadata: {}", path.display()))?;
        if !metadata.is_file() || is_executable(&path, &metadata) != file.executable {
            bail!(
                "restored bundle file metadata failed verification: {}",
                file.path
            );
        }
        let bytes = fs::read(&path)
            .with_context(|| format!("restored bundle file is missing: {}", path.display()))?;
        if bytes.len() as u64 != file.bytes || hex_sha256(&bytes) != file.sha256 {
            bail!("restored bundle file failed verification: {}", file.path);
        }
    }
    Ok(())
}

fn verify_tree_entries<'a>(
    root: &Path,
    directory: &Path,
    expected: &BTreeMap<&'a str, &'a BundleFile>,
    allowed_dirs: &BTreeSet<String>,
) -> Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = portable_path(
            path.strip_prefix(root)
                .context("bundle path escaped root")?,
        )?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            bail!("installed bundle contains a symlink: {relative}");
        }
        if file_type.is_dir() {
            if !allowed_dirs.contains(&relative) {
                bail!("installed bundle contains an undeclared directory: {relative}");
            }
            verify_tree_entries(root, &path, expected, allowed_dirs)?;
        } else if file_type.is_file() {
            if relative != INSTALL_LOCK && !expected.contains_key(relative.as_str()) {
                bail!("installed bundle contains an undeclared file: {relative}");
            }
        } else {
            bail!("installed bundle contains an unsupported file type: {relative}");
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
    validate_version_identity(&manifest, &fs::read_to_string(home.join("VERSION"))?)?;
    verify_tree(home, &manifest).context("installed cargo-dotnet bundle integrity check failed")?;
    Ok(true)
}

struct StagedFrontEnd {
    temporary: tempfile::TempPath,
    destination: PathBuf,
}

fn cargo_home_path() -> Result<PathBuf> {
    Ok(std::env::var_os("CARGO_HOME").map(PathBuf::from).unwrap_or(
        crate::host::home_dir()
            .context("locating home for CARGO_HOME")?
            .join(".cargo"),
    ))
}

fn stage_front_end(staged_home: &Path, install_home: &Path) -> Result<StagedFrontEnd> {
    let name = if cfg!(windows) {
        "cargo-dotnet.exe"
    } else {
        "cargo-dotnet"
    };
    let source = staged_home.join("bin").join(name);
    let cargo_home = cargo_home_path()?;
    let destination = cargo_home.join("bin").join(name);
    let destination_parent = destination.parent().expect("cargo bin has a parent");
    fs::create_dir_all(destination_parent)?;
    let canonical_destination_parent = fs::canonicalize(destination_parent)?;
    let install_parent = install_home
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_install_home = if install_home.exists() {
        fs::canonicalize(install_home)?
    } else {
        fs::canonicalize(install_parent)?.join(
            install_home
                .file_name()
                .context("install home has no final component")?,
        )
    };
    if canonical_destination_parent.starts_with(&canonical_install_home) {
        bail!("CARGO_HOME must not be located inside the immutable SDK install home");
    }
    let temporary = tempfile::Builder::new()
        .prefix(".cargo-dotnet-cli-stage-")
        .tempfile_in(destination_parent)?
        .into_temp_path();
    fs::copy(&source, &temporary)?;
    set_executable(&temporary, true)?;
    Ok(StagedFrontEnd {
        temporary,
        destination,
    })
}

fn activate_install<F>(
    staged_home: &Path,
    home: &Path,
    front_end: Option<StagedFrontEnd>,
    before_front_end: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    activate_install_with_hook(staged_home, home, front_end, || Ok(()), before_front_end)
}

fn activate_install_with_hook<F, G>(
    staged_home: &Path,
    home: &Path,
    front_end: Option<StagedFrontEnd>,
    before_backup: F,
    before_front_end: G,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
    G: FnOnce() -> Result<()>,
{
    crate::path_safety::require_owned_or_empty_sdk_home(home)?;
    let parent = home
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let home_backup_area = tempfile::Builder::new()
        .prefix(".cargo-dotnet-home-backup-")
        .tempdir_in(parent)?;
    let home_backup = home_backup_area.path().join("previous");
    let home_discard = home_backup_area.path().join("failed-new");
    let cli_backup_area = front_end
        .as_ref()
        .map(|front_end| {
            tempfile::Builder::new()
                .prefix(".cargo-dotnet-cli-backup-")
                .tempdir_in(
                    front_end
                        .destination
                        .parent()
                        .expect("cargo-dotnet destination has a parent"),
                )
        })
        .transpose()?;
    let cli_backup = cli_backup_area
        .as_ref()
        .map(|area| area.path().join("previous"));
    let cli_discard = cli_backup_area
        .as_ref()
        .map(|area| area.path().join("failed-new"));

    before_backup()?;
    let had_home = home.exists();
    let had_cli = front_end
        .as_ref()
        .is_some_and(|front_end| front_end.destination.exists());
    let mut home_backed_up = false;
    let mut cli_backed_up = false;
    let mut home_promoted = false;
    let mut cli_promoted = false;

    let transaction = (|| -> Result<()> {
        if had_cli {
            let front_end = front_end.as_ref().expect("had_cli implies a front-end");
            fs::rename(&front_end.destination, cli_backup.as_ref().unwrap())
                .context("moving previous cargo-dotnet front-end aside")?;
            cli_backed_up = true;
            let metadata = fs::symlink_metadata(cli_backup.as_ref().unwrap())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!("previous cargo-dotnet front-end is not a regular file");
            }
        }
        if had_home {
            fs::rename(home, &home_backup).context("moving previous install home aside")?;
            home_backed_up = true;
            crate::path_safety::require_owned_or_empty_sdk_home(&home_backup)?;
        }
        fs::rename(staged_home, home).context("activating restored install home")?;
        home_promoted = true;
        before_front_end()?;
        if let Some(front_end) = &front_end {
            fs::rename(&front_end.temporary, &front_end.destination)
                .context("activating cargo-dotnet front-end")?;
            cli_promoted = true;
            if fs::read(
                home.join("bin").join(
                    front_end
                        .destination
                        .file_name()
                        .context("front-end destination has no filename")?,
                ),
            )? != fs::read(&front_end.destination)?
            {
                bail!("installed front-end bytes do not match the activated SDK bundle");
            }
        }
        if !verify_installed_if_locked(home)? {
            bail!("activated SDK home has no bundle integrity lock");
        }
        Ok(())
    })();

    if let Err(error) = transaction {
        let mut rollback_errors = Vec::new();
        if cli_promoted
            && let Some(front_end) = &front_end
            && let Some(discard) = &cli_discard
            && let Err(rollback) = fs::rename(&front_end.destination, discard)
        {
            rollback_errors.push(format!("remove failed front-end: {rollback}"));
        }
        if home_promoted && let Err(rollback) = fs::rename(home, &home_discard) {
            rollback_errors.push(format!("remove failed SDK home: {rollback}"));
        }
        if home_backed_up && let Err(rollback) = fs::rename(&home_backup, home) {
            rollback_errors.push(format!("restore previous SDK home: {rollback}"));
        }
        if cli_backed_up
            && let Some(front_end) = &front_end
            && let Some(backup) = &cli_backup
            && let Err(rollback) = fs::rename(backup, &front_end.destination)
        {
            rollback_errors.push(format!("restore previous front-end: {rollback}"));
        }
        if rollback_errors.is_empty() {
            return Err(error).context("SDK/front-end activation rolled back");
        }
        let home_recovery = home_backup_area.keep();
        let cli_recovery = cli_backup_area.map(tempfile::TempDir::keep);
        bail!(
            "SDK/front-end activation failed ({error:#}); rollback also failed: {}; recoverable backups: {}{}",
            rollback_errors.join("; "),
            home_recovery.display(),
            cli_recovery
                .as_ref()
                .map(|path| format!(", {}", path.display()))
                .unwrap_or_default()
        );
    }

    if let Some(front_end) = &front_end {
        println!(
            "installed cargo-dotnet front-end -> {}",
            front_end.destination.display()
        );
    }
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
            format!(
                "schema = 1\nrelease_tag = rust-dotnet-v{}\ncargo_dotnet_version = {}\nhost_rid = {}\ntoolchain = {}\n",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_VERSION"),
                crate::host::HostFacts::detect().host_rid,
                crate::mode::DEFAULT_TOOLCHAIN
            ),
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
        fs::write(restored.join("dotnet_pal/injected.rs"), b"fn main() {}").unwrap();
        assert!(verify_installed_if_locked(&restored).is_err());
        fs::remove_file(restored.join("dotnet_pal/injected.rs")).unwrap();
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

    #[test]
    fn manifest_identity_rejects_cross_bound_host_version_and_toolchain() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let archive = temp.path().join("sdk.zip");
        create(&home, &archive).unwrap();
        let manifest = verify(&archive, false).unwrap();

        let mut wrong_rid = BundleManifest {
            host_rid: "wrong-rid".into(),
            ..manifest
        };
        assert!(validate_manifest(&wrong_rid, false).is_err());
        wrong_rid.host_rid = expected_host_rid(&wrong_rid.host_os, &wrong_rid.host_arch)
            .unwrap()
            .into();
        wrong_rid.cargo_dotnet_version = "9.9.9".into();
        assert!(validate_manifest(&wrong_rid, true).is_err());
        wrong_rid.cargo_dotnet_version = env!("CARGO_PKG_VERSION").into();
        wrong_rid.toolchain = "nightly-cross-bound".into();
        assert!(validate_manifest(&wrong_rid, true).is_err());
        wrong_rid.toolchain = crate::mode::DEFAULT_TOOLCHAIN.into();
        let cli_path = if cfg!(windows) {
            "bin/cargo-dotnet.exe"
        } else {
            "bin/cargo-dotnet"
        };
        wrong_rid
            .files
            .iter_mut()
            .find(|file| file.path == cli_path)
            .unwrap()
            .sha256 = "0".repeat(64);
        let cli_error = validate_manifest(&wrong_rid, true).unwrap_err();
        assert!(cli_error.to_string().contains("running cargo-dotnet"));
        assert!(
            validate_version_identity(
                &wrong_rid,
                "schema = 1\nrelease_tag = rust-dotnet-v0.0.2\nhost_rid = wrong\ntoolchain = wrong\n"
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_release_rid_aliases_are_scoped_to_exactly_0_0_1() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let archive = temp.path().join("sdk.zip");
        create(&home, &archive).unwrap();
        let mut manifest = verify(&archive, false).unwrap();
        manifest.cargo_dotnet_version = "0.0.1".into();

        for (manifest_rid, legacy_rid) in [("osx-arm64", "macos-arm64"), ("win-x64", "windows-x64")]
        {
            manifest.host_rid = manifest_rid.into();
            let legacy = format!(
                "schema = 1\nrelease_tag = rust-dotnet-v0.0.1\nhost_rid = {legacy_rid}\ntoolchain = {}\n",
                manifest.toolchain
            );
            validate_version_identity(&manifest, &legacy).unwrap();

            manifest.cargo_dotnet_version = "0.0.2".into();
            let current = legacy.replace("rust-dotnet-v0.0.1", "rust-dotnet-v0.0.2");
            assert!(validate_version_identity(&manifest, &current).is_err());
            manifest.cargo_dotnet_version = "0.0.1".into();
        }
    }

    #[test]
    fn sdk_and_front_end_activation_roll_back_together() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("active-sdk");
        let staged = temp.path().join("staged-sdk");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&staged).unwrap();
        fs::write(home.join("marker"), b"old-sdk").unwrap();
        fs::write(
            home.join("VERSION"),
            "schema = 1\nrelease_tag = untagged\nhost_rid = test\ntoolchain = nightly\n",
        )
        .unwrap();
        fs::write(staged.join("marker"), b"new-sdk").unwrap();

        let cli_dir = temp.path().join("cargo/bin");
        fs::create_dir_all(&cli_dir).unwrap();
        let destination = cli_dir.join(if cfg!(windows) {
            "cargo-dotnet.exe"
        } else {
            "cargo-dotnet"
        });
        fs::write(&destination, b"old-cli").unwrap();
        let temporary = tempfile::Builder::new()
            .prefix(".cargo-dotnet-cli-stage-")
            .tempfile_in(&cli_dir)
            .unwrap()
            .into_temp_path();
        fs::write(&temporary, b"new-cli").unwrap();

        let error = activate_install(
            &staged,
            &home,
            Some(StagedFrontEnd {
                temporary,
                destination: destination.clone(),
            }),
            || bail!("injected activation failure"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("rolled back"), "{error:#}");
        assert_eq!(fs::read(home.join("marker")).unwrap(), b"old-sdk");
        assert_eq!(fs::read(destination).unwrap(), b"old-cli");
    }

    #[test]
    fn no_cli_bundle_activation_never_replaces_an_unowned_directory() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("unrelated-home");
        let staged = temp.path().join("staged-sdk");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&staged).unwrap();
        fs::write(home.join("do-not-delete"), b"unrelated").unwrap();
        fs::write(staged.join("marker"), b"new-sdk").unwrap();

        let error = activate_install(&staged, &home, None, || Ok(())).unwrap_err();

        assert!(error.to_string().contains("ownership marker"), "{error:#}");
        assert_eq!(fs::read(home.join("do-not-delete")).unwrap(), b"unrelated");
        assert_eq!(fs::read(staged.join("marker")).unwrap(), b"new-sdk");
    }

    #[test]
    fn bundle_revalidates_the_exact_home_moved_to_backup() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("active-home");
        let staged = temp.path().join("staged-sdk");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&staged).unwrap();
        fs::write(
            home.join("VERSION"),
            "schema = 1\nrelease_tag = untagged\nhost_rid = test\ntoolchain = nightly\n",
        )
        .unwrap();
        fs::write(staged.join("marker"), b"new-sdk").unwrap();
        let swapped_home = home.clone();

        let error = activate_install_with_hook(
            &staged,
            &home,
            None,
            move || {
                fs::remove_dir_all(&swapped_home)?;
                fs::create_dir(&swapped_home)?;
                fs::write(swapped_home.join("do-not-delete"), b"swapped-unrelated")?;
                Ok(())
            },
            || Ok(()),
        )
        .unwrap_err();

        assert!(error.to_string().contains("rolled back"), "{error:#}");
        assert_eq!(
            fs::read(home.join("do-not-delete")).unwrap(),
            b"swapped-unrelated"
        );
        assert_eq!(fs::read(staged.join("marker")).unwrap(), b"new-sdk");
    }
}
