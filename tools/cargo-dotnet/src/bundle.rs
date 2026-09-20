//! Versioned, checksummed `CARGO_DOTNET_HOME` bundles.
//!
//! A bundle contains the source-derived SDK/runtime inputs that setup normally copies from a
//! checkout: backend + linker, target spec, PAL/overlays, MSBuild integration, SDK crates, helper
//! sources, legacy launchers, and the current `cargo-dotnet` executable. It deliberately does not
//! claim to contain rustup or .NET; those remain host prerequisites and are recorded in the docs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use rust_dotnet_sdk_core::safe_fs::{DirectoryCapability, TreeWalkNode};
use rust_dotnet_sdk_core::sdk::{
    CURRENT_SDK_MANIFEST_SCHEMA, SDK_MANIFEST_FILE, SdkFile, SdkLayout, SdkManifest,
};

use crate::cli::{BundleArgs, BundleCommand};

const MANIFEST: &str = "bundle-manifest.json";
const PAYLOAD_PREFIX: &str = "payload/";
const MAX_BUNDLE_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BUNDLE_PAYLOAD_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_BUNDLE_ARCHIVE_BYTES: u64 = MAX_BUNDLE_PAYLOAD_BYTES + 256 * 1024 * 1024;
const MAX_BUNDLE_CHECKSUM_BYTES: u64 = 8 * 1024;
const MAX_BUNDLE_ENTRIES: usize = 100_000;
const COMPRESSION_RATIO_MIN_BYTES: u64 = 1024 * 1024;
const MAX_BUNDLE_COMPRESSION_RATIO: u64 = 200;

#[derive(Debug)]
struct SourceFile {
    path: String,
    bytes: Vec<u8>,
    executable: bool,
}

/// One immutable archive authority used by checksum validation, ZIP validation, and extraction.
///
/// The caller-supplied pathname is opened exactly once without following a final link. Its bytes
/// are copied into a private, retained file while that original handle remains alive. Every ZIP
/// phase then clones the private file handle instead of resolving the pathname again, so replacing
/// the path between phases cannot produce a checksum/verification/extraction split-brain.
struct OpenedBundleArchive {
    source_path: PathBuf,
    _source: Option<File>,
    snapshot: tempfile::NamedTempFile,
    sha256: String,
}

impl OpenedBundleArchive {
    fn open(path: &Path) -> Result<Self> {
        Self::open_bounded(path, MAX_BUNDLE_ARCHIVE_BYTES)
    }

    fn open_bounded(path: &Path, max_bytes: u64) -> Result<Self> {
        let mut source = rust_dotnet_sdk_core::safe_fs::open_regular_nofollow(path)
            .with_context(|| format!("opening bundle {}", path.display()))?;
        let declared_bytes = source.metadata()?.len();
        if declared_bytes > max_bytes {
            bail!(
                "bundle archive exceeds the configured byte limit ({} > {})",
                declared_bytes,
                max_bytes
            );
        }
        let mut snapshot = tempfile::Builder::new()
            .prefix("cargo-dotnet-bundle-snapshot-")
            .tempfile()?;
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 128 * 1024];
        let mut copied_bytes = 0_u64;
        loop {
            let read = source
                .read(&mut buffer)
                .with_context(|| format!("reading bundle {}", path.display()))?;
            if read == 0 {
                break;
            }
            copied_bytes = copied_bytes
                .checked_add(read as u64)
                .context("bundle archive byte count overflow")?;
            if copied_bytes > max_bytes {
                bail!("bundle archive grew beyond the configured byte limit while being copied");
            }
            hash.update(&buffer[..read]);
            snapshot.write_all(&buffer[..read])?;
        }
        if copied_bytes != declared_bytes {
            bail!("bundle archive changed size while being copied");
        }
        snapshot.as_file().sync_all()?;
        snapshot.as_file_mut().seek(SeekFrom::Start(0))?;
        Ok(Self {
            source_path: path.to_path_buf(),
            _source: Some(source),
            snapshot,
            sha256: format!("{:x}", hash.finalize()),
        })
    }

    fn from_bytes(source_path: &Path, bytes: &[u8]) -> Result<Self> {
        let mut snapshot = tempfile::Builder::new()
            .prefix("cargo-dotnet-bundle-snapshot-")
            .tempfile()?;
        snapshot.write_all(bytes)?;
        snapshot.as_file().sync_all()?;
        snapshot.as_file_mut().seek(SeekFrom::Start(0))?;
        Ok(Self {
            source_path: source_path.to_path_buf(),
            _source: None,
            snapshot,
            sha256: hex_sha256(bytes),
        })
    }

    fn file(&self) -> Result<File> {
        let mut file = self.snapshot.as_file().try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        Ok(file)
    }
}

pub fn run(args: &BundleArgs) -> Result<i32> {
    match &args.command {
        BundleCommand::Create { home, out } => {
            let home = resolve_home(home)?;
            create(&home, out)?;
            Ok(0)
        }
        BundleCommand::Verify { archive } => {
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
    let source = DirectoryCapability::open(home)
        .with_context(|| format!("opening sealed bundle source {}", home.display()))?;
    let sealed_manifest = verified_sealed_source_manifest(&source)?;
    let running_cli =
        std::env::current_exe().context("locating the running cargo-dotnet executable")?;
    let (manifest, sources) = inventory_from_capability(&source, home, Some(&running_cli), true)?;
    if let Some(sealed_manifest) = sealed_manifest
        && manifest != sealed_manifest
    {
        bail!(
            "sealed SDK source changed while bundle creation captured it; refusing to re-bless bytes that differ from {SDK_MANIFEST_FILE}"
        );
    }
    let version_text = sources
        .iter()
        .find(|source| source.path == "VERSION")
        .context("bundle source snapshot has no VERSION")?;
    let version_text = std::str::from_utf8(&version_text.bytes).context("VERSION is not UTF-8")?;
    reject_dirty_bundle_provenance(&version_text)?;

    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating bundle output directory {}", parent.display()))?;
    }
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
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
        zip.write_all(&source.bytes)?;
    }
    let archive_bytes = zip.finish()?.into_inner();
    let generated = OpenedBundleArchive::from_bytes(out, &archive_bytes)?;
    verify_opened(&generated, false).context("self-verifying generated bundle")?;
    let checksum = format!("{}  {}\n", hex_sha256(&archive_bytes), file_name(out)?);
    publish_bundle_pair(out, &archive_bytes, checksum.as_bytes())?;
    println!("created cargo-dotnet bundle: {}", out.display());
    println!("bundle checksum: {}", checksum_path(out).display());
    Ok(())
}

fn verified_sealed_source_manifest(source: &DirectoryCapability) -> Result<Option<SdkManifest>> {
    let home = source.root();
    let (_, version) = source.snapshot_regular(Path::new("VERSION"))?;
    let version = String::from_utf8(version).context("installed VERSION is not UTF-8")?;
    let lock = home.join(SDK_MANIFEST_FILE);
    let metadata = match fs::symlink_metadata(&lock) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            source.ensure_path_still_bound()?;
            if exact_legacy_0_0_1_version(&version) {
                return Ok(None);
            }
            bail!(
                "installed SDK requires {SDK_MANIFEST_FILE}, but the integrity inventory is missing; refusing to create a bundle from an unsealed current SDK"
            );
        }
        Err(error) => return Err(error.into()),
    };
    if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) || !metadata.is_file()
    {
        bail!(
            "sealed SDK inventory is not a regular file: {}",
            lock.display()
        );
    }
    let (_, bytes) = source.snapshot_regular(Path::new(SDK_MANIFEST_FILE))?;
    let manifest: SdkManifest =
        serde_json::from_slice(&bytes).context("parsing sealed SDK source inventory")?;
    validate_manifest(&manifest, true)?;
    validate_version_identity(&manifest, &version)?;
    verify_tree_from_capability(source, &manifest, &bytes)
        .context("installed cargo-dotnet bundle integrity check failed")?;
    Ok(Some(manifest))
}

fn inventory(
    home: &Path,
    cli_override: Option<&Path>,
    require_running_cli: bool,
) -> Result<(SdkManifest, Vec<SourceFile>)> {
    let capability = DirectoryCapability::open(home)
        .with_context(|| format!("opening install home capability {}", home.display()))?;
    inventory_from_capability(&capability, home, cli_override, require_running_cli)
}

fn inventory_from_capability(
    capability: &DirectoryCapability,
    home: &Path,
    cli_override: Option<&Path>,
    require_running_cli: bool,
) -> Result<(SdkManifest, Vec<SourceFile>)> {
    let facts = crate::host::HostFacts::detect();
    let layout = SdkLayout::for_host(&facts);
    let mut sources = Vec::new();
    for name in layout.inventory_roots() {
        let path = home.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) {
            bail!("bundle inputs may not contain links: {}", path.display());
        }
        if metadata.is_dir() {
            let tree = capability.subdirectory(Path::new(name))?;
            collect_sources(&tree, Path::new(name), &mut sources)?;
        } else if metadata.is_file() {
            let (_, mut file) = capability.open_regular(Path::new(name))?;
            let bytes = rust_dotnet_sdk_core::safe_fs::read_opened_regular(&mut file, &path)?;
            sources.push(SourceFile {
                path: name.to_string(),
                bytes,
                executable: is_executable(&path, &file.metadata()?),
            });
        } else {
            bail!("bundle input has an unsupported type: {}", path.display());
        }
    }

    if let Some(cli_override) = cli_override {
        let executable_name = layout.cargo_dotnet.as_str();
        sources.retain(|source| source.path != executable_name);
        let mut file = rust_dotnet_sdk_core::safe_fs::open_regular_nofollow(cli_override)?;
        let bytes = rust_dotnet_sdk_core::safe_fs::read_opened_regular(&mut file, cli_override)?;
        sources.push(SourceFile {
            path: executable_name.to_string(),
            bytes,
            executable: true,
        });
    }
    sources.sort_by(|left, right| left.path.cmp(&right.path));
    for required in layout.required_leaves(facts.os) {
        let source = sources
            .iter()
            .find(|source| source.path == required.path)
            .with_context(|| {
                format!(
                    "install home is incomplete (missing {}); run `cargo dotnet setup` first",
                    home.join(&required.path).display()
                )
            })?;
        if source.bytes.is_empty() || source.executable != required.executable {
            bail!(
                "install home has an invalid required SDK leaf (regular, non-empty, executable={}): {}",
                required.executable,
                home.join(&required.path).display()
            );
        }
    }

    let mut files = Vec::with_capacity(sources.len());
    for source in &sources {
        files.push(SdkFile {
            path: source.path.clone(),
            bytes: source.bytes.len() as u64,
            sha256: hex_sha256(&source.bytes),
            executable: source.executable,
        });
    }
    let version = sources
        .iter()
        .find(|source| source.path == layout.version)
        .context("bundle inventory has no VERSION source")?;
    let version_text = std::str::from_utf8(&version.bytes).context("VERSION is not UTF-8")?;
    let toolchain = version_value(version_text, "toolchain")
        .context("VERSION is missing toolchain")?
        .to_string();
    let manifest = SdkManifest::new(
        &facts,
        toolchain,
        env!("CARGO_PKG_VERSION").to_string(),
        files,
    );
    validate_manifest(&manifest, false)?;
    let cli = sources
        .iter()
        .find(|source| source.path == layout.cargo_dotnet)
        .context("bundle inventory has no cargo-dotnet front-end")?;
    let binary_build_id = crate::installed_bootstrap::binary_build_id(&cli.bytes)
        .context("reading the staged cargo-dotnet binary build receipt")?;
    let recorded_build_id = version_value(version_text, "driver_build_id")
        .context("VERSION is missing driver_build_id")?;
    if binary_build_id != recorded_build_id {
        bail!(
            "staged cargo-dotnet binary build identity {binary_build_id:?} does not match VERSION.driver_build_id {recorded_build_id:?}"
        );
    }
    if require_running_cli {
        validate_running_manifest_identity(&manifest, &hex_sha256(&cli.bytes))?;
    }
    validate_version_identity(&manifest, version_text)?;
    capability.ensure_path_still_bound()?;
    Ok((manifest, sources))
}

fn collect_sources(
    tree: &DirectoryCapability,
    prefix: &Path,
    out: &mut Vec<SourceFile>,
) -> Result<()> {
    tree.walk_regular_tree(&[], &mut |relative, node| {
        if let TreeWalkNode::File(file) = node {
            let path = tree.root().join(relative);
            let bytes = rust_dotnet_sdk_core::safe_fs::read_opened_regular(file, &path)?;
            out.push(SourceFile {
                path: portable_path(&prefix.join(relative))?,
                bytes,
                executable: is_executable(&path, &file.metadata()?),
            });
        }
        Ok(())
    })?;
    tree.ensure_path_still_bound()
}

fn bundle_publication_lock(path: &Path, shared: bool) -> Result<crate::build_lock::BuildLock> {
    let planned = crate::path_safety::planned_absolute(path)?;
    let scope = format!(
        "bundle-output-{}",
        crate::content_cache::digest_parts([
            b"cargo-dotnet-bundle-output-v1".as_slice(),
            planned.as_os_str().as_encoded_bytes(),
        ])
    );
    if shared {
        crate::build_lock::BuildLock::acquire_scope_shared(&scope)
    } else {
        crate::build_lock::BuildLock::acquire_scope(&scope)
    }
}

fn publish_bundle_pair(out: &Path, archive: &[u8], checksum: &[u8]) -> Result<()> {
    let _publication = bundle_publication_lock(out, false)?;
    let parent = out
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let parent = fs::canonicalize(parent)?;
    let capability = DirectoryCapability::open(&parent)?;
    let archive_leaf = PathBuf::from(out.file_name().context("bundle output has no filename")?);
    let checksum_path = checksum_path(out);
    let checksum_leaf = PathBuf::from(
        checksum_path
            .file_name()
            .context("bundle checksum has no filename")?,
    );
    let previous_archive = snapshot_optional_leaf(&capability, &archive_leaf)?;
    let previous_checksum = snapshot_optional_leaf(&capability, &checksum_leaf)?;
    let publish = (|| -> Result<()> {
        capability.publish_bytes(&archive_leaf, archive)?;
        capability.publish_bytes(&checksum_leaf, checksum)?;
        capability.ensure_path_still_bound()?;
        Ok(())
    })();
    if let Err(error) = publish {
        let rollback = (|| -> Result<()> {
            restore_optional_leaf(&capability, &archive_leaf, previous_archive.as_deref())?;
            restore_optional_leaf(&capability, &checksum_leaf, previous_checksum.as_deref())?;
            capability.ensure_path_still_bound()
        })();
        return match rollback {
            Ok(()) => Err(error).context("bundle archive/checksum publication rolled back"),
            Err(rollback) => bail!(
                "bundle archive/checksum publication failed ({error:#}); rollback also failed: {rollback:#}"
            ),
        };
    }
    Ok(())
}

fn snapshot_optional_leaf(
    capability: &DirectoryCapability,
    relative: &Path,
) -> Result<Option<Vec<u8>>> {
    let path = capability.root().join(relative);
    match fs::symlink_metadata(&path) {
        Ok(metadata)
            if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata)
                || !metadata.is_file() =>
        {
            bail!(
                "bundle output sidecar is not a regular file: {}",
                path.display()
            )
        }
        Ok(_) => Ok(Some(capability.snapshot_regular(relative)?.1)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn restore_optional_leaf(
    capability: &DirectoryCapability,
    relative: &Path,
    previous: Option<&[u8]>,
) -> Result<()> {
    if let Some(previous) = previous {
        capability.publish_bytes(relative, previous)?;
        return Ok(());
    }
    let path = capability.root().join(relative);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            bail!(
                "refusing to remove directory during bundle rollback: {}",
                path.display()
            )
        }
        Ok(_) => fs::remove_file(&path)
            .with_context(|| format!("removing newly-published bundle leaf {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn verify(path: &Path, require_host: bool) -> Result<SdkManifest> {
    let _publication = bundle_publication_lock(path, true)?;
    let archive = OpenedBundleArchive::open(path)?;
    verify_archive_checksum(&archive)?;
    verify_opened(&archive, require_host)
}

fn verify_opened(archive: &OpenedBundleArchive, require_host: bool) -> Result<SdkManifest> {
    let mut zip = ZipArchive::new(archive.file()?).context("opening bundle ZIP")?;
    if zip.len() > MAX_BUNDLE_ENTRIES.saturating_add(1) {
        bail!(
            "bundle contains too many ZIP entries ({} > {})",
            zip.len(),
            MAX_BUNDLE_ENTRIES + 1
        );
    }
    let manifest: SdkManifest = {
        let mut entry = zip
            .by_name(MANIFEST)
            .context("bundle manifest is missing")?;
        let entry_size = entry.size();
        let compressed_size = entry.compressed_size();
        let bytes = read_zip_entry_bounded(
            &mut entry,
            entry_size,
            compressed_size,
            16 * 1024 * 1024,
            "bundle manifest",
        )?;
        serde_json::from_slice(&bytes).context("parsing bundle manifest")?
    };
    validate_manifest(&manifest, require_host)?;

    let expected: BTreeMap<&str, &SdkFile> = manifest
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
    let mut payload_bytes = 0_u64;
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
        payload_bytes = payload_bytes
            .checked_add(entry.size())
            .context("bundle payload size overflow")?;
        if payload_bytes > MAX_BUNDLE_PAYLOAD_BYTES {
            bail!("bundle payload exceeds the aggregate 4 GiB safety limit");
        }
        let compressed_size = entry.compressed_size();
        let bytes = read_zip_entry_bounded(
            &mut entry,
            expected_file.bytes,
            compressed_size,
            MAX_BUNDLE_ENTRY_BYTES,
            relative,
        )?;
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

fn validate_manifest(manifest: &SdkManifest, require_running_cli: bool) -> Result<()> {
    let layout = manifest.validate_schema_and_layout()?;
    if manifest.files.is_empty() {
        bail!("bundle manifest has no files");
    }
    if manifest.host_rid.is_empty()
        || manifest.toolchain.is_empty()
        || manifest.cargo_dotnet_version.is_empty()
    {
        bail!("bundle manifest is missing host/toolchain/front-end identity");
    }
    if manifest.files.len() > MAX_BUNDLE_ENTRIES {
        bail!("bundle manifest declares too many payload entries");
    }
    let mut aggregate_bytes = 0_u64;
    for file in &manifest.files {
        validate_relative(&file.path)?;
        if file.bytes > MAX_BUNDLE_ENTRY_BYTES {
            bail!(
                "bundle manifest entry {} exceeds the 512 MiB safety limit",
                file.path
            );
        }
        aggregate_bytes = aggregate_bytes
            .checked_add(file.bytes)
            .context("bundle manifest payload size overflow")?;
        if aggregate_bytes > MAX_BUNDLE_PAYLOAD_BYTES {
            bail!("bundle manifest payload exceeds the aggregate 4 GiB safety limit");
        }
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("invalid SHA-256 in bundle manifest for {}", file.path);
        }
    }
    if manifest.schema == CURRENT_SDK_MANIFEST_SCHEMA {
        for required in layout.required_leaves(&manifest.host_os) {
            let file = manifest
                .files
                .iter()
                .find(|file| file.path == required.path)
                .with_context(|| {
                    format!("SDK manifest is missing required leaf {}", required.path)
                })?;
            if file.bytes == 0 || file.executable != required.executable {
                bail!(
                    "SDK required leaf {} must be non-empty with executable={}",
                    required.path,
                    required.executable
                );
            }
        }
    } else {
        for required in layout.required_files() {
            if !manifest.files.iter().any(|file| file.path == required) {
                bail!("SDK manifest is missing required file {required}");
            }
        }
        for required in layout.legacy_required_directories() {
            let prefix = format!("{required}/");
            if !manifest
                .files
                .iter()
                .any(|file| file.path.starts_with(&prefix))
            {
                bail!("SDK manifest is missing required directory contents for {required}");
            }
        }
    }
    if !manifest
        .files
        .iter()
        .any(|file| file.path == layout.cargo_dotnet)
    {
        bail!("SDK manifest is missing its cargo-dotnet front-end");
    }
    if manifest.schema == CURRENT_SDK_MANIFEST_SCHEMA {
        for file in &manifest.files {
            if !layout
                .inventory_roots()
                .iter()
                .any(|root| file.path == *root || file.path.starts_with(&format!("{root}/")))
            {
                bail!(
                    "SDK manifest contains a file outside its inventory: {}",
                    file.path
                );
            }
        }
    }
    if require_running_cli {
        let running_cli = std::env::current_exe().context("locating running cargo-dotnet")?;
        let running_hash = hex_sha256(&fs::read(&running_cli)?);
        validate_running_manifest_identity(manifest, &running_hash)?;
    }
    Ok(())
}

fn read_zip_entry_bounded(
    reader: &mut impl Read,
    declared_size: u64,
    compressed_size: u64,
    limit: u64,
    label: &str,
) -> Result<Vec<u8>> {
    if declared_size > limit {
        bail!("{label} exceeds the configured uncompressed safety limit");
    }
    validate_compression_ratio(label, declared_size, compressed_size)?;
    let mut bytes = Vec::with_capacity(declared_size.min(1024 * 1024) as usize);
    let read_limit = declared_size.saturating_add(1).min(limit.saturating_add(1));
    reader.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != declared_size {
        bail!(
            "{label} decompressed size differs from its ZIP declaration (declared {declared_size}, observed at least {})",
            bytes.len()
        );
    }
    Ok(bytes)
}

fn validate_compression_ratio(label: &str, size: u64, compressed_size: u64) -> Result<()> {
    if size >= COMPRESSION_RATIO_MIN_BYTES
        && (compressed_size == 0
            || size > compressed_size.saturating_mul(MAX_BUNDLE_COMPRESSION_RATIO))
    {
        bail!(
            "{label} has a suspicious ZIP compression ratio above {}:1",
            MAX_BUNDLE_COMPRESSION_RATIO
        );
    }
    Ok(())
}

fn validate_running_manifest_identity(manifest: &SdkManifest, running_hash: &str) -> Result<()> {
    if manifest.host_os != std::env::consts::OS || manifest.host_arch != std::env::consts::ARCH {
        bail!(
            "bundle targets {}-{}, but this host is {}-{}",
            manifest.host_os,
            manifest.host_arch,
            std::env::consts::OS,
            std::env::consts::ARCH
        );
    }
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
    let layout = manifest.validate_schema_and_layout()?;
    let bundled_cli = manifest
        .files
        .iter()
        .find(|file| file.path == layout.cargo_dotnet)
        .context("bundle manifest does not contain its cargo-dotnet front-end")?;
    if bundled_cli.sha256 != running_hash {
        bail!("bundle front-end does not match the running cargo-dotnet executable");
    }
    Ok(())
}

fn version_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        (candidate.trim() == key).then(|| value.trim().trim_matches('"'))
    })
}

fn validate_version_identity(manifest: &SdkManifest, text: &str) -> Result<()> {
    let schema = version_value(text, "schema").context("VERSION is missing schema")?;
    let release_tag =
        version_value(text, "release_tag").context("VERSION is missing release_tag")?;
    let recorded_cli_version = version_value(text, "cargo_dotnet_version");
    let host_rid = version_value(text, "host_rid").context("VERSION is missing host_rid")?;
    let toolchain = version_value(text, "toolchain").context("VERSION is missing toolchain")?;
    let inventory_required = version_value(text, "inventory_required");
    let git_rev = version_value(text, "git_rev");
    let source_tree_sha256 = version_value(text, "source_tree_sha256");
    let driver_build_id = version_value(text, "driver_build_id");
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
        || (!matches!(release_tag, "untagged" | "untagged-dirty")
            && release_tag != expected_release_tag)
        || !host_rid_matches
        || toolchain != manifest.toolchain
        || (manifest.schema == CURRENT_SDK_MANIFEST_SCHEMA && inventory_required != Some("true"))
        || (manifest.schema == CURRENT_SDK_MANIFEST_SCHEMA
            && (!source_tree_sha256.is_some_and(valid_sha256)
                || driver_build_id
                    != source_tree_sha256
                        .map(|digest| format!("source-sha256:{digest}"))
                        .as_deref()
                || (release_tag == "untagged-dirty")
                    != git_rev.is_some_and(|revision| revision.ends_with("-dirty"))))
        || (manifest.schema != CURRENT_SDK_MANIFEST_SCHEMA && inventory_required.is_some())
    {
        bail!("bundle VERSION identity does not match its manifest");
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn reject_dirty_bundle_provenance(text: &str) -> Result<()> {
    if version_value(text, "release_tag") == Some("untagged-dirty")
        || version_value(text, "git_rev").is_some_and(|revision| revision.ends_with("-dirty"))
    {
        bail!(
            "bundle creation rejects dirty setup provenance; capture a clean Git revision before producing a release artifact"
        );
    }
    Ok(())
}

/// Bind the already-loaded home-driver image to the currently-active home. The embedded identity
/// catches the old-image/new-home race; the inventory hash catches a replaced on-disk driver.
pub(crate) fn validate_loaded_driver_identity(home: &Path, embedded_build_id: &str) -> Result<()> {
    let capability = rust_dotnet_sdk_core::safe_fs::DirectoryCapability::open(home)
        .context("opening installed SDK identity authority")?;
    let (_, version_bytes) = capability.snapshot_regular(Path::new("VERSION"))?;
    let version = String::from_utf8(version_bytes).context("installed VERSION is not UTF-8")?;
    let recorded_build_id = version_value(&version, "driver_build_id").context(
        "installed VERSION has no driver_build_id; reinstall with the current bootstrap",
    )?;
    if recorded_build_id != embedded_build_id {
        bail!(
            "loaded cargo-dotnet image identity {embedded_build_id:?} does not match active home identity {recorded_build_id:?}; retry through the Cargo-bin bootstrap"
        );
    }
    let (_, lock_bytes) = capability.snapshot_regular(Path::new(SDK_MANIFEST_FILE))?;
    let manifest: SdkManifest =
        serde_json::from_slice(&lock_bytes).context("parsing installed SDK inventory")?;
    validate_manifest(&manifest, false)?;
    validate_version_identity(&manifest, &version)?;
    let layout = manifest.validate_schema_and_layout()?;
    let expected_driver = manifest
        .files
        .iter()
        .find(|file| file.path == layout.cargo_dotnet)
        .context("installed SDK inventory has no home driver")?;
    let (_, driver_bytes) = capability.snapshot_regular(Path::new(&layout.cargo_dotnet))?;
    let binary_build_id = crate::installed_bootstrap::binary_build_id(&driver_bytes)?;
    if binary_build_id != embedded_build_id {
        bail!("active home driver binary receipt does not match its loaded image identity");
    }
    if driver_bytes.len() as u64 != expected_driver.bytes
        || hex_sha256(&driver_bytes) != expected_driver.sha256
    {
        bail!("active installed cargo-dotnet driver does not match BUNDLE-LOCK.json");
    }
    capability.ensure_path_still_bound()?;
    Ok(())
}

fn install(archive: &Path, home: &Path, force: bool, install_cli: bool) -> Result<()> {
    let _publication = bundle_publication_lock(archive, true)?;
    let opened_archive = OpenedBundleArchive::open(archive)?;
    verify_archive_checksum(&opened_archive)?;
    let manifest = verify_opened(&opened_archive, true)?;
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
    let archive_parent = archive
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let archive_authority_parent = fs::canonicalize(archive_parent)?;
    let mut protected = vec![
        ("bundle archive directory", archive_authority_parent),
        ("working directory", std::env::current_dir()?),
        (
            "running cargo-dotnet",
            std::env::current_exe().context("locating running cargo-dotnet")?,
        ),
        ("Cargo home", cargo_home),
    ];
    let repository = match crate::mode::detect()? {
        crate::mode::Mode::Dev { repo_root } => Some(repo_root),
        crate::mode::Mode::Installed { .. } => None,
    };
    let containing_repository = crate::mode::find_repo_ancestor(&planned_home);
    if let Some(repo_root) = &repository {
        protected.push(("repository", repo_root.clone()));
    }
    crate::path_safety::reject_ancestor_of(&planned_home, protected)?;
    if let Some(repo_root) = repository {
        crate::path_safety::reject_overlap_with(&planned_home, [("repository", repo_root)])?;
    }
    if let Some(repo_root) = containing_repository {
        crate::path_safety::reject_overlap_with(
            &planned_home,
            [("containing repository", repo_root)],
        )?;
    }
    let temp = tempfile::Builder::new()
        .prefix(".cargo-dotnet-restore-")
        .tempdir_in(parent)?;
    extract_verified(&opened_archive, temp.path(), &manifest)?;
    verify_tree(temp.path(), &manifest)?;
    fs::write(
        temp.path().join(SDK_MANIFEST_FILE),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    let staged = temp.keep();
    let front_end = install_cli
        .then(|| stage_front_end(&staged, home))
        .transpose()?;
    activate_install(&staged, home, front_end)?;
    println!(
        "installed verified cargo-dotnet bundle -> {} (toolchain {})",
        home.display(),
        manifest.toolchain
    );
    Ok(())
}

fn extract_verified(
    archive: &OpenedBundleArchive,
    destination: &Path,
    manifest: &SdkManifest,
) -> Result<()> {
    let expected: BTreeMap<&str, &SdkFile> = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let mut zip = ZipArchive::new(archive.file()?)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let Some(relative) = entry.name().strip_prefix(PAYLOAD_PREFIX).map(str::to_owned) else {
            continue;
        };
        let metadata = expected
            .get(relative.as_str())
            .ok_or_else(|| anyhow::anyhow!("undeclared payload entry: {relative}"))?;
        if entry.size() != metadata.bytes || entry.size() > MAX_BUNDLE_ENTRY_BYTES {
            bail!("bundle size mismatch for {relative} during extraction");
        }
        validate_compression_ratio(&relative, entry.size(), entry.compressed_size())?;
        let target = destination.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&target)?;
        let copied = std::io::copy(
            &mut entry.by_ref().take(metadata.bytes.saturating_add(1)),
            &mut output,
        )?;
        if copied != metadata.bytes {
            bail!("bundle size mismatch for {relative} during extraction");
        }
        output.sync_all()?;
        set_executable(&target, metadata.executable)?;
    }
    Ok(())
}

fn verify_tree(root: &Path, manifest: &SdkManifest) -> Result<()> {
    let (expected, allowed_dirs) = manifest_tree_shape(manifest)?;
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

/// Verify a sealed SDK tree from the same retained directory authority used to snapshot its lock
/// and capture bundle inputs. This prevents a same-path home replacement, or a lock/tree revision
/// swap between those phases, from turning a successful check of one revision into a bundle of
/// another.
fn verify_tree_from_capability(
    source: &DirectoryCapability,
    manifest: &SdkManifest,
    lock_bytes: &[u8],
) -> Result<()> {
    let (expected, allowed_dirs) = manifest_tree_shape(manifest)?;
    let mut seen = BTreeSet::new();
    source.walk_regular_tree(&[], &mut |relative, node| {
        if relative.as_os_str().is_empty() {
            return Ok(());
        }
        let portable = portable_path(relative)?;
        match node {
            TreeWalkNode::DirectoryEnter(_) | TreeWalkNode::DirectoryLeave(_) => {
                if !allowed_dirs.contains(&portable) {
                    bail!("installed bundle contains an undeclared directory: {portable}");
                }
            }
            TreeWalkNode::File(file) => {
                if portable == SDK_MANIFEST_FILE {
                    let bytes = rust_dotnet_sdk_core::safe_fs::read_opened_regular(
                        file,
                        &source.root().join(relative),
                    )?;
                    if bytes != lock_bytes {
                        bail!("sealed SDK inventory changed while its tree was verified");
                    }
                    return Ok(());
                }
                let expected_file = expected.get(portable.as_str()).with_context(|| {
                    format!("installed bundle contains an undeclared file: {portable}")
                })?;
                let metadata = file.metadata()?;
                if is_executable(&source.root().join(relative), &metadata)
                    != expected_file.executable
                {
                    bail!("installed bundle file mode changed: {portable}");
                }
                let bytes = rust_dotnet_sdk_core::safe_fs::read_opened_regular(
                    file,
                    &source.root().join(relative),
                )?;
                if bytes.len() as u64 != expected_file.bytes
                    || hex_sha256(&bytes) != expected_file.sha256
                {
                    bail!("installed bundle file failed verification: {portable}");
                }
                if !seen.insert(portable) {
                    bail!("installed bundle contains a duplicate file");
                }
            }
        }
        Ok(())
    })?;
    if seen.len() != expected.len() {
        let missing = expected
            .keys()
            .find(|path| !seen.contains(**path))
            .copied()
            .unwrap_or("<unknown>");
        bail!("installed bundle is missing declared file: {missing}");
    }
    source.ensure_path_still_bound()
}

fn manifest_tree_shape<'a>(
    manifest: &'a SdkManifest,
) -> Result<(BTreeMap<&'a str, &'a SdkFile>, BTreeSet<String>)> {
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
    Ok((expected, allowed_dirs))
}

fn verify_tree_entries<'a>(
    root: &Path,
    directory: &Path,
    expected: &BTreeMap<&'a str, &'a SdkFile>,
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
            if relative != SDK_MANIFEST_FILE && !expected.contains_key(relative.as_str()) {
                bail!("installed bundle contains an undeclared file: {relative}");
            }
        } else {
            bail!("installed bundle contains an unsupported file type: {relative}");
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstalledIntegrity {
    Sealed,
    Legacy001,
}

fn exact_legacy_0_0_1_version(text: &str) -> bool {
    version_value(text, "schema") == Some("1")
        && version_value(text, "inventory_required").is_none()
        && (version_value(text, "cargo_dotnet_version") == Some("0.0.1")
            || (version_value(text, "cargo_dotnet_version").is_none()
                && version_value(text, "release_tag") == Some("rust-dotnet-v0.0.1")))
}

fn require_lock_or_exact_legacy(home: &Path) -> Result<InstalledIntegrity> {
    let version_path = home.join("VERSION");
    let metadata = fs::symlink_metadata(&version_path).with_context(|| {
        format!(
            "installed SDK VERSION is missing: {}",
            version_path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("installed SDK VERSION is not a regular file");
    }
    let text = fs::read_to_string(&version_path)?;
    if exact_legacy_0_0_1_version(&text) {
        Ok(InstalledIntegrity::Legacy001)
    } else {
        bail!(
            "installed SDK requires {}, but the integrity inventory is missing; reinstall cargo-dotnet {}",
            SDK_MANIFEST_FILE,
            version_value(&text, "cargo_dotnet_version").unwrap_or("from a verified 0.0.2+ bundle")
        )
    }
}

/// Verify a restored SDK home. A missing lock is accepted only for the immutable, markerless
/// 0.0.1 VERSION identity; every current SDK declares `inventory_required = true` and fails
/// closed when its lock is removed.
fn verify_installed(home: &Path, require_running_cli: bool) -> Result<InstalledIntegrity> {
    let lock = home.join(SDK_MANIFEST_FILE);
    let lock_metadata = match fs::symlink_metadata(&lock) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return require_lock_or_exact_legacy(home);
        }
        Err(error) => return Err(error.into()),
    };
    if lock_metadata.file_type().is_symlink() || !lock_metadata.is_file() {
        bail!(
            "installed SDK inventory is not a regular file: {}",
            lock.display()
        );
    }
    let manifest: SdkManifest = serde_json::from_slice(
        &fs::read(&lock).with_context(|| format!("reading bundle lock {}", lock.display()))?,
    )
    .context("parsing installed bundle lock")?;
    validate_manifest(&manifest, require_running_cli)?;
    validate_version_identity(&manifest, &fs::read_to_string(home.join("VERSION"))?)?;
    verify_tree(home, &manifest).context("installed cargo-dotnet bundle integrity check failed")?;
    Ok(InstalledIntegrity::Sealed)
}

pub(crate) fn verify_installed_if_locked(home: &Path) -> Result<InstalledIntegrity> {
    verify_installed(home, true)
}

/// Verify a just-activated sealed home without binding it to the setup caller's loaded image.
/// The home driver itself was executed against the staged tree before activation; this pass proves
/// that the transactional rename preserved the manifest, VERSION identity, and complete tree.
pub(crate) fn verify_sealed_install_home(home: &Path) -> Result<()> {
    if verify_installed(home, false)? != InstalledIntegrity::Sealed {
        bail!("activated SDK home is not sealed with the current inventory schema");
    }
    Ok(())
}

/// Resolve the installed layout from its immutable inventory. Old source-installed homes without
/// a lock keep working with the compiled host layout, while a locked home must parse and validate.
pub(crate) fn installed_layout(home: &Path) -> Result<SdkLayout> {
    let lock = home.join(SDK_MANIFEST_FILE);
    if fs::symlink_metadata(&lock).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        require_lock_or_exact_legacy(home)?;
        return Ok(SdkLayout::for_host(&crate::host::HostFacts::detect()));
    }
    let metadata = fs::symlink_metadata(&lock)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "installed SDK inventory is not a regular file: {}",
            lock.display()
        );
    }
    let manifest: SdkManifest = serde_json::from_slice(
        &fs::read(&lock).with_context(|| format!("reading SDK inventory {}", lock.display()))?,
    )
    .context("parsing installed SDK inventory")?;
    manifest.validate_schema_and_layout()
}

/// Seal a source-provisioned SDK home with the same inventory consumed by bundles, installed-mode
/// path resolution, and doctor. The front-end is copied into the immutable home before hashing;
/// publication of the lock itself is atomic.
pub(crate) fn seal_install_home(home: &Path, front_end: &Path) -> Result<()> {
    let layout = SdkLayout::for_host(&crate::host::HostFacts::detect());
    let bundled_front_end = home.join(&layout.cargo_dotnet);
    if let Some(parent) = bundled_front_end.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(front_end, &bundled_front_end).with_context(|| {
        format!(
            "copying SDK front-end {} -> {}",
            front_end.display(),
            bundled_front_end.display()
        )
    })?;
    set_executable(&bundled_front_end, true)?;
    let (manifest, _) = inventory(home, None, false)?;
    verify_tree(home, &manifest)?;

    let mut temporary = tempfile::Builder::new()
        .prefix(".sdk-inventory-")
        .tempfile_in(home)?;
    temporary.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(home.join(SDK_MANIFEST_FILE))
        .map_err(|error| error.error)
        .context("publishing SDK inventory")?;
    Ok(())
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

fn activate_install(
    staged_home: &Path,
    home: &Path,
    front_end: Option<StagedFrontEnd>,
) -> Result<()> {
    let (temporary, destination, cli) = match front_end {
        Some(front_end) => {
            let destination = front_end.destination;
            let temporary = front_end.temporary;
            let cli = crate::install_transaction::CliActivation::new(
                temporary.to_path_buf(),
                destination.clone(),
            );
            (Some(temporary), Some(destination), Some(cli))
        }
        None => (None, None, None),
    };
    let validation_destination = destination.clone();
    let result = crate::install_transaction::activate(
        staged_home,
        home,
        cli,
        crate::install_transaction::RollbackDisposition::DiscardInputs,
        || Ok(()),
        || Ok(()),
        || {
            if let Some(destination) = &validation_destination
                && fs::read(
                    home.join("bin").join(
                        destination
                            .file_name()
                            .context("front-end destination has no filename")?,
                    ),
                )? != fs::read(destination)?
            {
                bail!("installed front-end bytes do not match the activated SDK bundle");
            }
            if verify_installed_if_locked(home)? != InstalledIntegrity::Sealed {
                bail!("activated SDK home has no bundle integrity lock");
            }
            Ok(())
        },
    );
    drop(temporary);
    result?;

    if let Some(destination) = &destination {
        println!(
            "installed cargo-dotnet front-end -> {}",
            destination.display()
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
    let mut filename = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    filename.push(".sha256");
    path.with_file_name(filename)
}

fn verify_archive_checksum(archive: &OpenedBundleArchive) -> Result<()> {
    let sidecar = checksum_path(&archive.source_path);
    let text = String::from_utf8(read_regular_bounded(
        &sidecar,
        MAX_BUNDLE_CHECKSUM_BYTES,
        "bundle checksum sidecar",
    )?)
    .context("bundle checksum sidecar is not UTF-8")?;
    let mut fields = text.split_whitespace();
    let expected = fields.next().context("bundle checksum sidecar is empty")?;
    let expected_name = fields
        .next()
        .context("bundle checksum sidecar has no filename")?;
    if fields.next().is_some() || expected_name != file_name(&archive.source_path)? {
        bail!("bundle checksum sidecar has an invalid format or filename");
    }
    if expected != archive.sha256 {
        bail!("bundle archive SHA-256 mismatch");
    }
    Ok(())
}

fn read_regular_bounded(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>> {
    let mut file = rust_dotnet_sdk_core::safe_fs::open_regular_nofollow(path)
        .with_context(|| format!("{label} is missing: {}", path.display()))?;
    let declared_bytes = file.metadata()?.len();
    if declared_bytes > limit {
        bail!("{label} exceeds the configured byte limit");
    }
    let mut bytes = Vec::with_capacity(declared_bytes as usize);
    std::io::Read::by_ref(&mut file)
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("{label} grew beyond the configured byte limit while being read");
    }
    if bytes.len() as u64 != declared_bytes {
        bail!("{label} changed size while being read");
    }
    Ok(bytes)
}

fn file_name(path: &Path) -> Result<String> {
    Ok(path
        .file_name()
        .context("bundle output has no filename")?
        .to_str()
        .context("bundle output filename is not UTF-8")?
        .to_owned())
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
    windows_path_is_executable(path)
}

#[cfg(any(not(unix), test))]
fn windows_path_is_executable(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "cargo-dotnet")
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

    fn fake_unsealed_home(root: &Path) -> PathBuf {
        let home = root.join("home");
        let facts = crate::host::HostFacts::detect();
        let layout = SdkLayout::for_host(&facts);
        for required in layout.required_leaves(facts.os) {
            if required.path == layout.cargo_dotnet {
                continue;
            }
            let path = home.join(&required.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, format!("test payload for {}", required.path)).unwrap();
            set_executable(&path, required.executable).unwrap();
        }
        fs::write(
            home.join("VERSION"),
            format!(
                "schema = 1\ninventory_required = true\ngit_rev = test-clean\nrelease_tag = rust-dotnet-v{}\nsource_tree_sha256 = {}\ndriver_build_id = source-sha256:{}\ncargo_dotnet_version = {}\nhost_rid = {}\ntoolchain = {}\n",
                env!("CARGO_PKG_VERSION"),
                "0".repeat(64),
                "0".repeat(64),
                env!("CARGO_PKG_VERSION"),
                facts.host_rid,
                crate::mode::DEFAULT_TOOLCHAIN
            ),
        )
        .unwrap();
        fs::write(home.join(&layout.pal_root).join("pal.rs"), b"pal").unwrap();
        home
    }

    fn fake_home(root: &Path) -> PathBuf {
        let home = fake_unsealed_home(root);
        seal_install_home(&home, &std::env::current_exe().unwrap()).unwrap();
        home
    }

    #[test]
    fn windows_required_leaf_executable_semantics_accept_extensionless_legacy_launcher() {
        let facts = crate::host::HostFacts::for_target("windows", "x86_64").unwrap();
        let layout = SdkLayout::for_host(&facts);
        let files = layout
            .required_leaves("windows")
            .into_iter()
            .map(|required| SdkFile {
                executable: windows_path_is_executable(Path::new(&required.path)),
                path: required.path,
                bytes: 1,
                sha256: "0".repeat(64),
            })
            .collect();
        let manifest = SdkManifest::new(
            &facts,
            crate::mode::DEFAULT_TOOLCHAIN.into(),
            env!("CARGO_PKG_VERSION").into(),
            files,
        );
        assert!(windows_path_is_executable(Path::new("cargo-dotnet")));
        assert!(validate_manifest(&manifest, false).is_ok());
    }

    #[test]
    fn sealed_home_tamper_cannot_be_reblessed_into_a_new_bundle() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let pal = home.join(SdkLayout::for_host(&crate::host::HostFacts::detect()).pal_root);
        fs::write(pal.join("pal.rs"), b"tampered after sealing").unwrap();
        let archive = temp.path().join("tampered.zip");

        let error = create(&home, &archive).unwrap_err();
        assert!(
            format!("{error:#}").contains("integrity check failed"),
            "{error:#}"
        );
        assert!(!archive.exists());
    }

    #[test]
    fn current_sealed_home_cannot_be_reblessed_after_lock_deletion_and_tamper() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let layout = SdkLayout::for_host(&crate::host::HostFacts::detect());
        fs::remove_file(home.join(SDK_MANIFEST_FILE)).unwrap();
        fs::write(
            home.join(layout.pal_root).join("pal.rs"),
            b"tampered after deleting the seal",
        )
        .unwrap();
        let archive = temp.path().join("reblessed.zip");

        let error = create(&home, &archive).unwrap_err();
        assert!(
            format!("{error:#}").contains("integrity inventory is missing"),
            "{error:#}"
        );
        assert!(!archive.exists());
        assert!(!checksum_path(&archive).exists());
    }

    #[test]
    fn manifest_rejects_oversize_payload_without_allocating_it() {
        let facts = crate::host::HostFacts::detect();
        let layout = SdkLayout::for_host(&facts);
        let manifest = SdkManifest::new(
            &facts,
            crate::mode::DEFAULT_TOOLCHAIN.into(),
            env!("CARGO_PKG_VERSION").into(),
            vec![SdkFile {
                path: layout.version,
                bytes: MAX_BUNDLE_ENTRY_BYTES + 1,
                sha256: "0".repeat(64),
                executable: false,
            }],
        );
        let error = validate_manifest(&manifest, false).unwrap_err();
        assert!(format!("{error:#}").contains("512 MiB safety limit"));
    }

    #[test]
    fn archive_snapshot_rejects_oversize_sparse_input_before_copying() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("oversize.zip");
        let file = File::create(&archive).unwrap();
        file.set_len(1025).unwrap();

        let error = OpenedBundleArchive::open_bounded(&archive, 1024)
            .err()
            .unwrap();
        assert!(
            error
                .to_string()
                .contains("exceeds the configured byte limit"),
            "{error:#}"
        );
    }

    #[test]
    fn checksum_sidecar_is_bounded_before_utf8_or_digest_parsing() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("sdk.zip");
        let archive = OpenedBundleArchive::from_bytes(&archive_path, b"not a zip").unwrap();
        fs::write(
            checksum_path(&archive_path),
            vec![b'a'; MAX_BUNDLE_CHECKSUM_BYTES as usize + 1],
        )
        .unwrap();

        let error = verify_archive_checksum(&archive).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("exceeds the configured byte limit"),
            "{error:#}"
        );
    }

    #[test]
    fn highly_compressible_payload_is_rejected_as_a_small_zip_bomb() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let layout = SdkLayout::for_host(&crate::host::HostFacts::detect());
        fs::write(
            home.join(layout.pal_root).join("compressed-bomb.bin"),
            vec![0_u8; (COMPRESSION_RATIO_MIN_BYTES * 2) as usize],
        )
        .unwrap();
        seal_install_home(&home, &std::env::current_exe().unwrap()).unwrap();
        let archive = temp.path().join("bomb.zip");

        let error = create(&home, &archive).unwrap_err();
        assert!(
            format!("{error:#}").contains("suspicious ZIP compression ratio"),
            "{error:#}"
        );
        assert!(!archive.exists());
    }

    #[test]
    fn dishonest_central_directory_size_stops_after_declared_size_plus_one() {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(
            "payload",
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(&vec![0_u8; 2 * 1024 * 1024]).unwrap();
        let mut archive_bytes = zip.finish().unwrap().into_inner();
        let central = archive_bytes
            .windows(4)
            .rposition(|bytes| bytes == b"PK\x01\x02")
            .expect("ZIP central-directory header");
        archive_bytes[central + 24..central + 28].copy_from_slice(&1_u32.to_le_bytes());

        let mut archive = ZipArchive::new(Cursor::new(archive_bytes)).unwrap();
        let mut entry = archive.by_index(0).unwrap();
        assert_eq!(
            entry.size(),
            1,
            "patched central size was not authoritative"
        );
        let declared_size = entry.size();
        let compressed_size = entry.compressed_size();
        let error = read_zip_entry_bounded(
            &mut entry,
            declared_size,
            compressed_size,
            MAX_BUNDLE_ENTRY_BYTES,
            "dishonest payload",
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("observed at least 2"),
            "bounded reader consumed beyond declared size + 1: {error:#}"
        );
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
        assert_eq!(
            verify_installed_if_locked(&restored).unwrap(),
            InstalledIntegrity::Sealed
        );
        verify_sealed_install_home(&restored).unwrap();
        fs::write(restored.join("dotnet_pal/injected.rs"), b"fn main() {}").unwrap();
        assert!(verify_installed_if_locked(&restored).is_err());
        fs::remove_file(restored.join("dotnet_pal/injected.rs")).unwrap();
        assert_eq!(
            verify_installed_if_locked(&restored).unwrap(),
            InstalledIntegrity::Sealed
        );
        fs::write(restored.join("dotnet_pal/pal.rs"), b"tampered").unwrap();
        assert!(verify_installed_if_locked(&restored).is_err());
        assert!(install(&archive, &restored, false, false).is_err());
        install(&archive, &restored, true, false).unwrap();
        assert_eq!(
            verify_installed_if_locked(&restored).unwrap(),
            InstalledIntegrity::Sealed
        );
    }

    #[test]
    fn loaded_driver_identity_rejects_a_stale_image_even_at_the_same_version() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let archive = temp.path().join("sdk.zip");
        create(&home, &archive).unwrap();
        let restored = temp.path().join("restored");
        install(&archive, &restored, false, false).unwrap();

        let error = validate_loaded_driver_identity(
            &restored,
            "source-sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("loaded cargo-dotnet image identity")
        );
        validate_loaded_driver_identity(
            &restored,
            crate::installed_bootstrap::EMBEDDED_DRIVER_BUILD_ID,
        )
        .unwrap();
    }

    #[test]
    fn sealing_rejects_a_cli_whose_embedded_build_id_differs_from_version() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_unsealed_home(temp.path());
        let version_path = home.join("VERSION");
        let version = fs::read_to_string(&version_path).unwrap();
        let mismatched = "f".repeat(64);
        let version = version.replace(&"0".repeat(64), &mismatched).replace(
            "source-sha256:0000000000000000000000000000000000000000000000000000000000000000",
            &format!("source-sha256:{mismatched}"),
        );
        fs::write(&version_path, version).unwrap();
        let error = seal_install_home(&home, &std::env::current_exe().unwrap()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("staged cargo-dotnet binary build identity"),
            "{error:#}"
        );
        assert!(!home.join(SDK_MANIFEST_FILE).exists());
    }

    #[test]
    fn bundle_install_rejects_a_new_home_nested_below_checkout_markers() {
        let temp = tempfile::tempdir().unwrap();
        let source_home = fake_home(&temp.path().join("source-home"));
        let archive = temp.path().join("sdk.zip");
        create(&source_home, &archive).unwrap();
        let repo = temp.path().join("checkout");
        fs::create_dir_all(repo.join("feasibility")).unwrap();
        fs::write(repo.join("feasibility/_cargo_dotnet_core.sh"), b"core").unwrap();
        fs::write(repo.join("x86_64-unknown-dotnet.json"), b"{}").unwrap();
        let error = install(&archive, &repo.join("nested-sdk"), false, false).unwrap_err();
        assert!(format!("{error:#}").contains("containing repository"));
    }

    #[cfg(unix)]
    #[test]
    fn one_opened_archive_survives_a_path_swap_between_checksum_verify_and_extract() {
        let temp = tempfile::tempdir().unwrap();
        let first_home = fake_home(&temp.path().join("first"));
        let second_home = fake_home(&temp.path().join("second"));
        fs::write(
            first_home.join("dotnet_pal/pal.rs"),
            b"first trusted bundle",
        )
        .unwrap();
        fs::write(second_home.join("dotnet_pal/pal.rs"), b"replacement bundle").unwrap();
        seal_install_home(&first_home, &std::env::current_exe().unwrap()).unwrap();
        seal_install_home(&second_home, &std::env::current_exe().unwrap()).unwrap();
        let selected_path = temp.path().join("selected.zip");
        let replacement_path = temp.path().join("replacement.zip");
        create(&first_home, &selected_path).unwrap();
        create(&second_home, &replacement_path).unwrap();

        let opened = OpenedBundleArchive::open(&selected_path).unwrap();
        verify_archive_checksum(&opened).unwrap();
        fs::rename(&selected_path, temp.path().join("selected.original.zip")).unwrap();
        fs::rename(&replacement_path, &selected_path).unwrap();

        let manifest = verify_opened(&opened, true).unwrap();
        let restored = temp.path().join("restored");
        fs::create_dir(&restored).unwrap();
        extract_verified(&opened, &restored, &manifest).unwrap();
        assert_eq!(
            fs::read(restored.join("dotnet_pal/pal.rs")).unwrap(),
            b"first trusted bundle"
        );
        assert_ne!(
            hex_sha256(&fs::read(&selected_path).unwrap()),
            opened.sha256
        );
    }

    #[test]
    fn current_sdk_cannot_downgrade_by_deleting_its_lock() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let archive = temp.path().join("sdk.zip");
        create(&home, &archive).unwrap();
        let restored = temp.path().join("restored");
        install(&archive, &restored, false, false).unwrap();
        fs::remove_file(restored.join(SDK_MANIFEST_FILE)).unwrap();

        let error = verify_installed_if_locked(&restored).unwrap_err();
        assert!(error.to_string().contains("integrity inventory is missing"));
        assert!(installed_layout(&restored).is_err());

        let legacy = temp.path().join("legacy-0.0.1");
        fs::create_dir(&legacy).unwrap();
        fs::write(
            legacy.join("VERSION"),
            "schema = 1\nrelease_tag = rust-dotnet-v0.0.1\nhost_rid = linux-x64\ntoolchain = nightly-old\n",
        )
        .unwrap();
        assert_eq!(
            verify_installed_if_locked(&legacy).unwrap(),
            InstalledIntegrity::Legacy001
        );
    }

    #[test]
    fn schema_two_rejects_missing_empty_or_wrong_mode_semantic_leaves() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let archive = temp.path().join("sdk.zip");
        create(&home, &archive).unwrap();
        let manifest = verify(&archive, false).unwrap();
        let layout = manifest.validate_schema_and_layout().unwrap();
        let required = layout.required_leaves(&manifest.host_os);

        for leaf in required {
            let mut missing = manifest.clone();
            missing.files.retain(|file| file.path != leaf.path);
            assert!(
                validate_manifest(&missing, false).is_err(),
                "accepted missing required leaf {}",
                leaf.path
            );

            let mut empty = manifest.clone();
            empty
                .files
                .iter_mut()
                .find(|file| file.path == leaf.path)
                .unwrap()
                .bytes = 0;
            assert!(
                validate_manifest(&empty, false).is_err(),
                "accepted empty required leaf {}",
                leaf.path
            );

            let mut wrong_mode = manifest.clone();
            let file = wrong_mode
                .files
                .iter_mut()
                .find(|file| file.path == leaf.path)
                .unwrap();
            file.executable = !file.executable;
            assert!(
                validate_manifest(&wrong_mode, false).is_err(),
                "accepted wrong mode for required leaf {}",
                leaf.path
            );
        }
    }

    #[test]
    fn unsafe_bundle_paths_are_rejected() {
        for path in ["", "../escape", "/absolute", "a/../b", "a\\b"] {
            assert!(validate_relative(path).is_err(), "accepted {path:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn checksum_sidecar_preserves_non_utf_output_filename_bytes() {
        use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};

        let filename = std::ffi::OsString::from_vec(b"sdk-\xff.zip".to_vec());
        let path = Path::new("/tmp").join(filename);
        let sidecar = checksum_path(&path);
        assert_eq!(
            sidecar.file_name().unwrap().as_bytes(),
            b"sdk-\xff.zip.sha256"
        );
    }

    #[test]
    fn manifest_identity_rejects_cross_bound_host_version_and_toolchain() {
        let temp = tempfile::tempdir().unwrap();
        let home = fake_home(temp.path());
        let archive = temp.path().join("sdk.zip");
        create(&home, &archive).unwrap();
        let manifest = verify(&archive, false).unwrap();

        let mut wrong_rid = SdkManifest {
            host_rid: "wrong-rid".into(),
            ..manifest
        };
        assert!(validate_manifest(&wrong_rid, false).is_err());
        wrong_rid.host_rid = crate::host::HostFacts::detect().host_rid.into();
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
        manifest.schema = rust_dotnet_sdk_core::sdk::LEGACY_SDK_MANIFEST_SCHEMA;
        manifest.layout = None;
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

        let error = activate_install(&staged, &home, None).unwrap_err();

        assert!(error.to_string().contains("ownership marker"), "{error:#}");
        assert_eq!(fs::read(home.join("do-not-delete")).unwrap(), b"unrelated");
        assert_eq!(fs::read(staged.join("marker")).unwrap(), b"new-sdk");
    }
}
