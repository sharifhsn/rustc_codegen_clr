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
//! consumer end is stage the SDK-selected graph under `.cargo-dotnet-nuget-assets/`; its owned
//! manifest lets `pipeline.rs` materialize managed, native, and culture-resource assets beside
//! every subsequent `build`/`run` output — no explicit `Assembly.LoadFrom` call needed in the
//! generated bindings themselves.

use std::collections::BTreeMap;
use std::fs::{self, File};
#[cfg(unix)]
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::artifact::{self, Artifact};
use crate::cli::{AddNativeArgs, AddNativeFileArgs, AddNugetArgs, BuildArgs};
use crate::context::Context;
use crate::{buildstd, overlays};

use rust_dotnet_assets as nuget_assets;
pub(crate) use rust_dotnet_assets::{StagedPackageAsset, StagedPackageAssetKind};
use rust_dotnet_sdk_core::safe_fs::{DirectoryCapability, TreeWalkNode};

/// The filename of the per-crate `add-nuget` dependency manifest — see [`record_dependency`].
const DEPS_MANIFEST_FILE: &str = ".cargo-dotnet-nuget-deps.json";
const LOCAL_NATIVE_MANIFEST_FILE: &str = ".cargo-dotnet-native-files.json";
const BINDGEN_CACHE_SCHEMA: u32 = 3;
const BINDGEN_CACHE_LIMIT: usize = 24;
const NUGET_TRANSACTION_ROOTS: &[&str] = &[
    ".cargo-dotnet-nuget-assets",
    DEPS_MANIFEST_FILE,
    "src/nuget",
];
const NUGET_JOURNAL_DIRECTORY: &str = ".cargo-dotnet-nuget-transaction";
const NUGET_JOURNAL_STAGE_PREFIX: &str = ".cargo-dotnet-nuget-journal-stage-";
const NUGET_JOURNAL_CLEANUP_PREFIX: &str = ".cargo-dotnet-nuget-journal-cleanup-";
const NUGET_JOURNAL_SNAPSHOT: &str = "snapshot";
const NUGET_JOURNAL_RECEIPT: &str = "receipt.json";
const NUGET_JOURNAL_COMMITTED: &str = "COMMITTED";
const NUGET_JOURNAL_SCHEMA: u32 = 1;
const NUGET_RESIDUE_RECEIPT_SUFFIX: &str = ".owner-v1-";
const NUGET_RESIDUE_RECEIPT_END: &str = ".receipt";
const NUGET_QUARANTINE_AUTHORITY_PREFIX: &str = ".cdnqa1.";
const NUGET_BOUND_DIRECTORY_PREFIX: &str = ".cdnq1d.";
const NUGET_BOUND_MARKER_PREFIX: &str = ".cdnq1m.";
const NUGET_CREATION_BINDING_PREFIX: &str = ".cdnb1.";

struct CachedBindings {
    path: PathBuf,
    _lease: crate::content_cache::CacheObject,
}

const DEPS_MANIFEST_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DependencySource {
    Default,
    Url { value: String },
    CratePath { path: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct DependencyRecord {
    version: String,
    rid: Option<String>,
    tfm: String,
    sources: Vec<DependencySource>,
    source_identity_sha256: String,
}

#[derive(Default, Serialize, Deserialize)]
struct DepsManifest {
    schema: u32,
    dependencies: BTreeMap<String, DependencyRecord>,
}

#[derive(Default, Serialize, Deserialize)]
struct LocalNativeManifest {
    schema: u32,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug)]
enum SnapshotRoot {
    Missing,
    File(ProvisionedFile),
    Directory {
        directories: Vec<PathBuf>,
        files: Vec<ProvisionedFile>,
    },
}

#[derive(Debug)]
struct ProvisionedFile {
    relative: PathBuf,
    bytes: Vec<u8>,
    permissions: fs::Permissions,
}

struct NugetProjectSnapshot {
    roots: Vec<(PathBuf, SnapshotRoot)>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NugetJournalReceipt {
    schema: u32,
    crate_key: String,
    roots: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NugetResidueReceipt {
    schema: u32,
    crate_key: String,
    residue_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct NugetQuarantineAuthority {
    schema: u32,
    crate_key: String,
    residue_name: String,
    directory_identity: rust_dotnet_sdk_core::safe_fs::FileIdentity,
    marker_identity: rust_dotnet_sdk_core::safe_fs::FileIdentity,
    binding_identity: rust_dotnet_sdk_core::safe_fs::FileIdentity,
    nonce: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct NugetCreationBinding {
    schema: u32,
    crate_key: String,
    residue_name: String,
    directory_identity: rust_dotnet_sdk_core::safe_fs::FileIdentity,
    marker_identity: rust_dotnet_sdk_core::safe_fs::FileIdentity,
}

struct RetainedCreationBinding {
    binding: NugetCreationBinding,
    path: PathBuf,
    marker: File,
}

struct PreparedQuarantineAuthority {
    authority: NugetQuarantineAuthority,
    path: PathBuf,
    capability: DirectoryCapability,
    marker: File,
}

struct NugetJournal {
    path: PathBuf,
}

struct NugetResidueAuthority {
    path: PathBuf,
    capability: Option<DirectoryCapability>,
    marker: Option<File>,
    armed: bool,
}

/// A directory that is owned only after its complete external receipt has been published.
///
/// The receipt intentionally precedes directory creation: recovery must never infer ownership
/// from a prefix, an empty directory, or a copy of the journal's internal receipt.
struct PreparedNugetResidue {
    crate_dir: PathBuf,
    path: PathBuf,
    authority: Option<NugetResidueAuthority>,
}

impl PreparedNugetResidue {
    fn persist(mut self) -> (PathBuf, PathBuf) {
        let receipt = self
            .authority
            .take()
            .expect("prepared NuGet residue must retain its authority")
            .persist();
        (self.path.clone(), receipt)
    }
}

impl Drop for PreparedNugetResidue {
    fn drop(&mut self) {
        let Some(authority) = self.authority.take() else {
            return;
        };
        let receipt = authority.persist();
        if self.path.exists() {
            let _ = remove_owned_nuget_residue(&self.crate_dir, &self.path, &receipt);
        } else {
            let _ = remove_nuget_residue_receipt(&self.crate_dir, &receipt);
        }
    }
}

impl NugetResidueAuthority {
    fn persist(mut self) -> PathBuf {
        self.armed = false;
        self.path.clone()
    }
}

impl Drop for NugetResidueAuthority {
    fn drop(&mut self) {
        if self.armed {
            if let (Some(capability), Some(marker)) = (self.capability.take(), self.marker.take()) {
                let relative = Path::new(self.path.file_name().unwrap());
                let _ = capability.remove_open_regular(relative, &marker);
            }
        }
    }
}

fn nuget_transaction_lock(crate_dir: &Path) -> Result<crate::build_lock::BuildLock> {
    let scope = format!(
        "nuget-project-{}",
        crate::context::crate_cache_key(crate_dir)?
    );
    crate::build_lock::BuildLock::acquire_scope(&scope)
}

/// A coherent read lease over every crate-local file owned by the NuGet pipeline.  Build-like
/// commands acquire this after [`ensure_staged`] and retain it through compilation and output
/// materialization, so an `add-nuget` transaction cannot replace bindings, dependency metadata,
/// or staged assets halfway through one build.
pub(crate) struct NugetProjectLease {
    crate_dir: PathBuf,
    _lock: crate::build_lock::BuildLock,
}

pub(crate) fn acquire_project_lease(crate_dir: &Path) -> Result<NugetProjectLease> {
    loop {
        let lock = nuget_transaction_lock(crate_dir)?;
        recover_nuget_transaction_locked(crate_dir)?;
        let lock = lock.downgrade_to_shared()?;
        // A writer may have won the downgrade window. Once this shared lock is held no writer can
        // create a new journal, so absence proves the reader sees a committed project revision.
        match fs::symlink_metadata(crate_dir.join(NUGET_JOURNAL_DIRECTORY)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(NugetProjectLease {
                    crate_dir: crate_dir.to_path_buf(),
                    _lock: lock,
                });
            }
            Ok(_) => drop(lock),
            Err(error) => return Err(error.into()),
        }
    }
}

impl NugetProjectLease {
    pub(crate) fn recorded_dependencies(&self) -> Result<Vec<(String, String)>> {
        recorded_dependencies_locked(&self.crate_dir)
    }

    pub(crate) fn staged_package_assets(&self) -> Result<Vec<StagedPackageAsset>> {
        staged_package_assets_locked(&self.crate_dir)
    }

    pub(crate) fn copy_assets(&self, out_dir: &Path) -> Result<Vec<PathBuf>> {
        copy_assets_locked(&self.crate_dir, out_dir)
    }
}

fn with_nuget_transaction<T>(crate_dir: &Path, action: impl FnOnce() -> Result<T>) -> Result<T> {
    let _lock = nuget_transaction_lock(crate_dir)?;
    recover_nuget_transaction_locked(crate_dir)?;
    let snapshot = NugetProjectSnapshot::capture(crate_dir)?;
    let journal = NugetJournal::prepare(crate_dir, &snapshot)?;
    match action() {
        Ok(value) => {
            sync_nuget_project_state(crate_dir)?;
            journal.commit(crate_dir)?;
            Ok(value)
        }
        Err(error) => match snapshot.restore(crate_dir) {
            Ok(()) => {
                sync_nuget_project_state(crate_dir)?;
                journal.retire(crate_dir)?;
                Err(error).context("NuGet project transaction rolled back")
            }
            Err(rollback) => bail!(
                "NuGet project transaction failed ({error:#}); rollback also failed: {rollback:#}"
            ),
        },
    }
}

impl NugetJournal {
    fn prepare(crate_dir: &Path, snapshot: &NugetProjectSnapshot) -> Result<Self> {
        let journal = crate_dir.join(NUGET_JOURNAL_DIRECTORY);
        if fs::symlink_metadata(&journal).is_ok() {
            bail!(
                "NuGet recovery journal already exists after recovery: {}",
                journal.display()
            );
        }
        let receipt = expected_nuget_journal_receipt(crate_dir)?;
        let staging = prepare_nuget_residue(crate_dir, NUGET_JOURNAL_STAGE_PREFIX)?;
        test_nuget_transaction_crash_point("after-residue-receipt");
        let staging_capability = DirectoryCapability::open(&staging.path)?;
        staging_capability.publish_bytes(
            Path::new(NUGET_JOURNAL_RECEIPT),
            &serde_json::to_vec_pretty(&receipt)?,
        )?;
        crate::install_transaction::sync_tree(&staging.path)?;
        let snapshot_root = staging.path.join(NUGET_JOURNAL_SNAPSHOT);
        snapshot.materialize(&snapshot_root)?;
        crate::install_transaction::sync_tree(&staging.path)?;
        test_nuget_transaction_crash_point("after-journal-stage");
        let staging_receipt = nuget_residue_receipt_path(crate_dir, &staging.path)?;
        let (_, owner_marker) =
            validate_nuget_residue_receipt(crate_dir, &staging.path, &staging_receipt)?;
        let creation = load_creation_binding(crate_dir, &staging.path, &owner_marker)?
            .context("staged journal lost its creation binding")?;
        let (staging, residue_receipt) = staging.persist();
        let root = DirectoryCapability::open(crate_dir)?;
        let published = root.quarantine_subdirectory_bound(
            Path::new(
                staging
                    .file_name()
                    .context("staging path has no filename")?,
            ),
            Path::new(
                journal
                    .file_name()
                    .context("journal path has no filename")?,
            ),
            creation.binding.directory_identity,
        )?;
        crate::install_transaction::sync_directory(crate_dir)?;
        remove_nuget_residue_receipt(crate_dir, &residue_receipt)?;
        let binding_relative = Path::new(
            creation
                .path
                .file_name()
                .context("creation binding has no filename")?,
        );
        root.retain_bound_regular(
            binding_relative,
            rust_dotnet_sdk_core::safe_fs::retained_file_identity(&creation.marker)?,
        )?
        .remove()?;
        drop(published);
        Ok(Self { path: journal })
    }

    fn commit(self, crate_dir: &Path) -> Result<()> {
        let capability = DirectoryCapability::open(&self.path)?;
        capability.publish_bytes(Path::new(NUGET_JOURNAL_COMMITTED), b"committed\n")?;
        crate::install_transaction::sync_directory(&self.path)?;
        self.retire(crate_dir)
    }

    fn retire(self, crate_dir: &Path) -> Result<()> {
        retire_nuget_journal(crate_dir, &self.path)
    }
}

fn recover_nuget_transaction_locked(crate_dir: &Path) -> Result<bool> {
    cleanup_nuget_journal_residue_locked(crate_dir)?;
    let journal = crate_dir.join(NUGET_JOURNAL_DIRECTORY);
    let metadata = match fs::symlink_metadata(&journal) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        bail!(
            "NuGet recovery journal is not a regular directory: {}",
            journal.display()
        );
    }
    validate_nuget_journal_receipt(crate_dir, &journal)?;
    let committed = journal.join(NUGET_JOURNAL_COMMITTED);
    match fs::symlink_metadata(&committed) {
        Ok(metadata)
            if !rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata)
                && metadata.is_file() =>
        {
            retire_nuget_journal(crate_dir, &journal)?;
            return Ok(true);
        }
        Ok(_) => bail!("NuGet journal commit marker is not a regular file"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let snapshot_root = journal.join(NUGET_JOURNAL_SNAPSHOT);
    let snapshot = NugetProjectSnapshot::capture(&snapshot_root)
        .context("loading durable NuGet rollback snapshot")?;
    snapshot
        .restore(crate_dir)
        .context("recovering interrupted NuGet project transaction")?;
    sync_nuget_project_state(crate_dir)?;
    retire_nuget_journal(crate_dir, &journal)?;
    Ok(true)
}

fn expected_nuget_journal_receipt(crate_dir: &Path) -> Result<NugetJournalReceipt> {
    Ok(NugetJournalReceipt {
        schema: NUGET_JOURNAL_SCHEMA,
        crate_key: crate::context::crate_cache_key(crate_dir)?,
        roots: NUGET_TRANSACTION_ROOTS
            .iter()
            .map(|root| (*root).to_string())
            .collect(),
    })
}

fn is_nuget_residue_name(name: &str) -> bool {
    [NUGET_JOURNAL_STAGE_PREFIX, NUGET_JOURNAL_CLEANUP_PREFIX]
        .into_iter()
        .find_map(|prefix| name.strip_prefix(prefix))
        .is_some_and(|nonce| is_lower_hex(nonce, 32))
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn nuget_residue_name(path: &Path) -> Result<&str> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("NuGet residue path has no UTF-8 filename")?;
    if !is_nuget_residue_name(name) || name.contains(NUGET_RESIDUE_RECEIPT_SUFFIX) {
        bail!("invalid NuGet residue name: {name:?}");
    }
    Ok(name)
}

fn nuget_residue_receipt_path(crate_dir: &Path, residue: &Path) -> Result<PathBuf> {
    let receipt = expected_nuget_residue_receipt(crate_dir, residue)?;
    let digest = Sha256::digest(serde_json::to_vec(&receipt)?);
    Ok(crate_dir.join(format!(
        "{}{NUGET_RESIDUE_RECEIPT_SUFFIX}{digest:x}{NUGET_RESIDUE_RECEIPT_END}",
        nuget_residue_name(residue)?
    )))
}

fn expected_nuget_residue_receipt(crate_dir: &Path, residue: &Path) -> Result<NugetResidueReceipt> {
    Ok(NugetResidueReceipt {
        schema: NUGET_JOURNAL_SCHEMA,
        crate_key: crate::context::crate_cache_key(crate_dir)?,
        residue_name: nuget_residue_name(residue)?.to_string(),
    })
}

fn quarantine_directory_name(authority: &NugetQuarantineAuthority) -> String {
    format!(
        "{NUGET_BOUND_DIRECTORY_PREFIX}{}.{}",
        authority.residue_name, authority.nonce
    )
}

fn quarantine_marker_name(authority: &NugetQuarantineAuthority) -> String {
    format!(
        "{NUGET_BOUND_MARKER_PREFIX}{}.{}",
        authority.residue_name, authority.nonce
    )
}

fn compact_residue_parts(name: &str) -> Result<(&'static str, &str)> {
    if let Some(nonce) = name.strip_prefix(NUGET_JOURNAL_STAGE_PREFIX) {
        return Ok(("s", nonce));
    }
    if let Some(nonce) = name.strip_prefix(NUGET_JOURNAL_CLEANUP_PREFIX) {
        return Ok(("c", nonce));
    }
    bail!("invalid residue name")
}

fn expand_residue_parts(kind: &str, nonce: &str) -> Result<String> {
    if !is_lower_hex(nonce, 32) {
        bail!("invalid compact residue nonce");
    }
    let prefix = match kind {
        "s" => NUGET_JOURNAL_STAGE_PREFIX,
        "c" => NUGET_JOURNAL_CLEANUP_PREFIX,
        _ => bail!("invalid compact residue kind"),
    };
    Ok(format!("{prefix}{nonce}"))
}

fn quarantine_authority_path(
    crate_dir: &Path,
    authority: &NugetQuarantineAuthority,
) -> Result<PathBuf> {
    let digest = Sha256::digest(serde_json::to_vec(authority)?);
    let (kind, residue_nonce) = compact_residue_parts(&authority.residue_name)?;
    Ok(crate_dir.join(format!(
        "{NUGET_QUARANTINE_AUTHORITY_PREFIX}{kind}~{residue_nonce}~{:016x}~{:016x}~{:016x}~{:016x}~{:016x}~{:016x}~{}~{digest:x}",
        authority.directory_identity.volume,
        authority.directory_identity.file,
        authority.marker_identity.volume,
        authority.marker_identity.file,
        authority.binding_identity.volume,
        authority.binding_identity.file,
        authority.nonce,
    )))
}

fn prepare_quarantine_authority(
    crate_dir: &Path,
    residue: &Path,
    receipt_marker: &File,
) -> Result<PreparedQuarantineAuthority> {
    let capability = DirectoryCapability::open(crate_dir)?;
    let creation = load_creation_binding(crate_dir, residue, receipt_marker)?
        .context("NuGet residue has no valid creation-time identity binding")?;
    let mut nonce = [0_u8; 16];
    fill_system_random(&mut nonce)?;
    let authority = NugetQuarantineAuthority {
        schema: NUGET_JOURNAL_SCHEMA,
        crate_key: crate::context::crate_cache_key(crate_dir)?,
        residue_name: nuget_residue_name(residue)?.to_string(),
        directory_identity: creation.binding.directory_identity,
        marker_identity: creation.binding.marker_identity,
        binding_identity: rust_dotnet_sdk_core::safe_fs::retained_file_identity(&creation.marker)?,
        nonce: hex_bytes(&nonce),
    };
    let path = quarantine_authority_path(crate_dir, &authority)?;
    let marker = capability.create_empty_regular(Path::new(
        path.file_name().context("authority has no filename")?,
    ))?;
    crate::install_transaction::sync_directory(crate_dir)?;
    Ok(PreparedQuarantineAuthority {
        authority,
        path,
        capability,
        marker,
    })
}

fn parse_quarantine_authority(crate_dir: &Path, path: &Path) -> Result<NugetQuarantineAuthority> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("quarantine authority has no UTF-8 filename")?;
    let body = name
        .strip_prefix(NUGET_QUARANTINE_AUTHORITY_PREFIX)
        .context("invalid quarantine authority grammar")?;
    let parts = body.split('~').collect::<Vec<_>>();
    if parts.len() != 10
        || !matches!(parts[0], "s" | "c")
        || !is_lower_hex(parts[1], 32)
        || !parts[2..=7].iter().all(|part| is_lower_hex(part, 16))
        || !is_lower_hex(parts[8], 32)
        || !is_lower_hex(parts[9], 64)
    {
        bail!("invalid quarantine authority grammar");
    }
    let parse_identity = |volume: &str, file: &str| -> Result<_> {
        Ok(rust_dotnet_sdk_core::safe_fs::FileIdentity {
            volume: u64::from_str_radix(volume, 16)?,
            file: u64::from_str_radix(file, 16)?,
        })
    };
    let authority = NugetQuarantineAuthority {
        schema: NUGET_JOURNAL_SCHEMA,
        crate_key: crate::context::crate_cache_key(crate_dir)?,
        residue_name: expand_residue_parts(parts[0], parts[1])?,
        directory_identity: parse_identity(parts[2], parts[3])?,
        marker_identity: parse_identity(parts[4], parts[5])?,
        binding_identity: parse_identity(parts[6], parts[7])?,
        nonce: parts[8].to_string(),
    };
    let expected = quarantine_authority_path(crate_dir, &authority)?;
    if expected != path {
        bail!("quarantine authority digest does not match its typed identity");
    }
    Ok(authority)
}

fn creation_binding_path(crate_dir: &Path, binding: &NugetCreationBinding) -> Result<PathBuf> {
    let (kind, nonce) = compact_residue_parts(&binding.residue_name)?;
    let digest = Sha256::digest(serde_json::to_vec(binding)?);
    Ok(crate_dir.join(format!(
        "{NUGET_CREATION_BINDING_PREFIX}{kind}~{nonce}~{:016x}~{:016x}~{:016x}~{:016x}~{digest:x}",
        binding.directory_identity.volume,
        binding.directory_identity.file,
        binding.marker_identity.volume,
        binding.marker_identity.file,
    )))
}

fn creation_binding_from_authority(authority: &NugetQuarantineAuthority) -> NugetCreationBinding {
    NugetCreationBinding {
        schema: authority.schema,
        crate_key: authority.crate_key.clone(),
        residue_name: authority.residue_name.clone(),
        directory_identity: authority.directory_identity,
        marker_identity: authority.marker_identity,
    }
}

fn parse_creation_binding(crate_dir: &Path, path: &Path) -> Result<NugetCreationBinding> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("creation binding has no UTF-8 filename")?;
    let body = name
        .strip_prefix(NUGET_CREATION_BINDING_PREFIX)
        .context("invalid creation binding grammar")?;
    let parts = body.split('~').collect::<Vec<_>>();
    if parts.len() != 7
        || !matches!(parts[0], "s" | "c")
        || !is_lower_hex(parts[1], 32)
        || !parts[2..=5].iter().all(|part| is_lower_hex(part, 16))
        || !is_lower_hex(parts[6], 64)
    {
        bail!("invalid creation binding grammar");
    }
    let identity = |volume: &str, file: &str| -> Result<_> {
        Ok(rust_dotnet_sdk_core::safe_fs::FileIdentity {
            volume: u64::from_str_radix(volume, 16)?,
            file: u64::from_str_radix(file, 16)?,
        })
    };
    let binding = NugetCreationBinding {
        schema: NUGET_JOURNAL_SCHEMA,
        crate_key: crate::context::crate_cache_key(crate_dir)?,
        residue_name: expand_residue_parts(parts[0], parts[1])?,
        directory_identity: identity(parts[2], parts[3])?,
        marker_identity: identity(parts[4], parts[5])?,
    };
    if creation_binding_path(crate_dir, &binding)? != path {
        bail!("creation binding digest does not match its typed identity");
    }
    Ok(binding)
}

fn publish_creation_binding(
    crate_dir: &Path,
    residue: &Path,
    owner_marker: &File,
) -> Result<RetainedCreationBinding> {
    let capability = DirectoryCapability::open(crate_dir)?;
    let directory_identity = capability.direct_child_directory_identity(Path::new(
        residue.file_name().context("residue has no filename")?,
    ))?;
    publish_creation_binding_for_identity(crate_dir, residue, owner_marker, directory_identity)
}

fn publish_creation_binding_for_identity(
    crate_dir: &Path,
    residue: &Path,
    owner_marker: &File,
    directory_identity: rust_dotnet_sdk_core::safe_fs::FileIdentity,
) -> Result<RetainedCreationBinding> {
    let capability = DirectoryCapability::open(crate_dir)?;
    if capability.direct_child_directory_identity(Path::new(
        residue.file_name().context("residue has no filename")?,
    ))? != directory_identity
    {
        bail!("residue directory changed before creation binding publication");
    }
    publish_creation_binding_for_identity_unchecked(
        crate_dir,
        residue,
        owner_marker,
        directory_identity,
    )
}

fn publish_creation_binding_for_identity_unchecked(
    crate_dir: &Path,
    residue: &Path,
    owner_marker: &File,
    directory_identity: rust_dotnet_sdk_core::safe_fs::FileIdentity,
) -> Result<RetainedCreationBinding> {
    let capability = DirectoryCapability::open(crate_dir)?;
    let binding = NugetCreationBinding {
        schema: NUGET_JOURNAL_SCHEMA,
        crate_key: crate::context::crate_cache_key(crate_dir)?,
        residue_name: nuget_residue_name(residue)?.to_string(),
        directory_identity,
        marker_identity: rust_dotnet_sdk_core::safe_fs::retained_file_identity(owner_marker)?,
    };
    let path = creation_binding_path(crate_dir, &binding)?;
    let marker = capability.create_empty_regular(Path::new(
        path.file_name().context("binding has no filename")?,
    ))?;
    crate::install_transaction::sync_directory(crate_dir)?;
    Ok(RetainedCreationBinding {
        binding,
        path,
        marker,
    })
}

fn load_creation_binding(
    crate_dir: &Path,
    residue: &Path,
    owner_marker: &File,
) -> Result<Option<RetainedCreationBinding>> {
    let owner_identity = rust_dotnet_sdk_core::safe_fs::retained_file_identity(owner_marker)?;
    let residue_name = nuget_residue_name(residue)?;
    let capability = DirectoryCapability::open(crate_dir)?;
    let mut matched = None;
    for entry in fs::read_dir(crate_dir)? {
        let entry = entry?;
        let Ok(binding) = parse_creation_binding(crate_dir, &entry.path()) else {
            continue;
        };
        if binding.residue_name != residue_name || binding.marker_identity != owner_identity {
            continue;
        }
        let file_name = entry.file_name();
        let (_, marker) = match capability.open_regular(Path::new(&file_name)) {
            Ok(opened) => opened,
            Err(_) => continue,
        };
        if marker.metadata()?.len() != 0 || matched.is_some() {
            return Ok(None);
        }
        matched = Some(RetainedCreationBinding {
            binding,
            path: entry.path(),
            marker,
        });
    }
    Ok(matched)
}

fn reserve_nuget_residue_path(crate_dir: &Path, prefix: &str) -> Result<PathBuf> {
    let mut nonce = [0_u8; 16];
    fill_system_random(&mut nonce)?;
    Ok(crate_dir.join(format!("{prefix}{}", hex_bytes(&nonce))))
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(unix)]
fn fill_system_random(bytes: &mut [u8]) -> Result<()> {
    let mut random = File::open("/dev/urandom").context("opening operating-system randomness")?;
    random
        .read_exact(bytes)
        .context("reading operating-system randomness")
}

#[cfg(windows)]
fn fill_system_random(bytes: &mut [u8]) -> Result<()> {
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptGenRandom(
            algorithm: *mut core::ffi::c_void,
            buffer: *mut u8,
            length: u32,
            flags: u32,
        ) -> i32;
    }
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            u32::try_from(bytes.len())?,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        bail!("BCryptGenRandom failed with NTSTATUS {status:#x}");
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn fill_system_random(_bytes: &mut [u8]) -> Result<()> {
    bail!("this host has no supported system randomness provider")
}

fn prepare_nuget_residue(crate_dir: &Path, prefix: &str) -> Result<PreparedNugetResidue> {
    let path = reserve_nuget_residue_path(crate_dir, prefix)?;
    let authority = publish_nuget_residue_receipt(crate_dir, &path)?;
    if let Err(error) = fs::create_dir(&path) {
        drop(authority);
        return Err(error).with_context(|| format!("creating NuGet residue {}", path.display()));
    }
    test_nuget_transaction_crash_point("after-residue-directory-before-binding");
    let owner_marker = authority
        .marker
        .as_ref()
        .context("prepared residue lost its owner marker")?;
    let _binding = publish_creation_binding(crate_dir, &path, owner_marker)?;
    crate::install_transaction::sync_directory(crate_dir)?;
    Ok(PreparedNugetResidue {
        crate_dir: crate_dir.to_path_buf(),
        path,
        authority: Some(authority),
    })
}

fn publish_nuget_residue_receipt(
    crate_dir: &Path,
    residue: &Path,
) -> Result<NugetResidueAuthority> {
    let path = nuget_residue_receipt_path(crate_dir, residue)?;
    test_nuget_transaction_crash_point("before-residue-receipt-publication");
    let capability = DirectoryCapability::open(crate_dir)?;
    let relative = Path::new(path.file_name().context("receipt has no filename")?);
    let marker = capability
        .create_empty_regular(relative)
        .with_context(|| format!("publishing NuGet residue receipt {}", path.display()))?;
    crate::install_transaction::sync_directory(crate_dir)?;
    // Creation of the empty, digest-addressed marker is the entire publication operation. There
    // is no partially-written state and no pre-authority temporary pathname.
    test_nuget_transaction_crash_point("during-residue-receipt-publication");
    Ok(NugetResidueAuthority {
        path,
        capability: Some(capability),
        marker: Some(marker),
        armed: true,
    })
}

fn validate_nuget_residue_receipt(
    crate_dir: &Path,
    residue: &Path,
    receipt_path: &Path,
) -> Result<(DirectoryCapability, File)> {
    if receipt_path != nuget_residue_receipt_path(crate_dir, residue)? {
        bail!("NuGet residue receipt filename does not match its typed authority");
    }
    let capability = DirectoryCapability::open(crate_dir)?;
    let relative = Path::new(
        receipt_path
            .file_name()
            .context("NuGet residue receipt has no filename")?,
    );
    let (_, file) = capability.open_regular(relative)?;
    if file.metadata()?.len() != 0 {
        bail!("NuGet residue receipt marker is not empty");
    }
    capability.ensure_path_still_bound()?;
    Ok((capability, file))
}

fn remove_nuget_residue_receipt(crate_dir: &Path, receipt: &Path) -> Result<()> {
    let Some(residue_name) = residue_name_from_receipt(receipt) else {
        bail!(
            "invalid NuGet residue receipt filename: {}",
            receipt.display()
        );
    };
    let residue = crate_dir.join(residue_name);
    let (capability, marker) = validate_nuget_residue_receipt(crate_dir, &residue, receipt)?;
    let relative = Path::new(receipt.file_name().context("receipt has no filename")?);
    let _ = capability.remove_open_regular(relative, &marker)?;
    Ok(())
}

fn remove_owned_nuget_residue(crate_dir: &Path, residue: &Path, receipt: &Path) -> Result<()> {
    let (root, marker) = validate_nuget_residue_receipt(crate_dir, residue, receipt)?;
    test_nuget_transaction_crash_point("mid-residue-cleanup");
    let prepared = prepare_quarantine_authority(crate_dir, residue, &marker)?;
    let directory_name = quarantine_directory_name(&prepared.authority);
    let marker_name = quarantine_marker_name(&prepared.authority);
    let source_relative = Path::new(residue.file_name().context("residue has no filename")?);
    let quarantined = root.quarantine_subdirectory_bound(
        source_relative,
        Path::new(&directory_name),
        prepared.authority.directory_identity,
    )?;
    test_nuget_transaction_crash_point("after-directory-quarantine");
    quarantined.remove()?;
    test_nuget_transaction_crash_point("after-directory-removal");
    let receipt_relative = Path::new(receipt.file_name().context("receipt has no filename")?);
    let quarantined_marker = root.quarantine_regular_bound(
        receipt_relative,
        &marker,
        Path::new(&marker_name),
        prepared.authority.marker_identity,
    )?;
    test_nuget_transaction_crash_point("after-marker-quarantine");
    quarantined_marker.remove()?;
    let binding_path = creation_binding_path(
        crate_dir,
        &creation_binding_from_authority(&prepared.authority),
    )?;
    let binding_relative = Path::new(
        binding_path
            .file_name()
            .context("creation binding has no filename")?,
    );
    let quarantined_binding =
        root.retain_bound_regular(binding_relative, prepared.authority.binding_identity)?;
    quarantined_binding.remove()?;
    let authority_relative = Path::new(
        prepared
            .path
            .file_name()
            .context("authority has no filename")?,
    );
    let _ = prepared
        .capability
        .remove_retained_regular(authority_relative, &prepared.marker)?;
    Ok(())
}

fn residue_name_from_receipt(receipt: &Path) -> Option<&str> {
    let name = receipt.file_name()?.to_str()?;
    let (residue, suffix) = name.split_once(NUGET_RESIDUE_RECEIPT_SUFFIX)?;
    let digest = suffix.strip_suffix(NUGET_RESIDUE_RECEIPT_END)?;
    if !is_lower_hex(digest, 64) || !is_nuget_residue_name(residue) {
        return None;
    }
    Some(residue)
}

fn recover_quarantine_authority(crate_dir: &Path, path: &Path) -> Result<bool> {
    let authority = match parse_quarantine_authority(crate_dir, path) {
        Ok(authority) => authority,
        Err(_) => return Ok(false),
    };
    let root = DirectoryCapability::open(crate_dir)?;
    let authority_relative = Path::new(path.file_name().context("authority has no filename")?);
    let (_, authority_marker) = match root.open_regular(authority_relative) {
        Ok(opened) => opened,
        Err(_) => return Ok(false),
    };
    if authority_marker.metadata()?.len() != 0 {
        return Ok(false);
    }

    let source = crate_dir.join(&authority.residue_name);
    let directory_name = quarantine_directory_name(&authority);
    let directory_relative = Path::new(&directory_name);
    if fs::symlink_metadata(crate_dir.join(directory_relative)).is_ok() {
        let Ok(quarantined) =
            root.retain_bound_subdirectory(directory_relative, authority.directory_identity)
        else {
            return Ok(false);
        };
        quarantined.remove()?;
    } else if fs::symlink_metadata(&source).is_ok() {
        if root
            .direct_child_directory_identity(Path::new(&authority.residue_name))
            .ok()
            != Some(authority.directory_identity)
        {
            return Ok(false);
        }
        let quarantined = root.quarantine_subdirectory_bound(
            Path::new(&authority.residue_name),
            directory_relative,
            authority.directory_identity,
        )?;
        quarantined.remove()?;
    }

    let receipt = nuget_residue_receipt_path(crate_dir, &source)?;
    let marker_name = quarantine_marker_name(&authority);
    let marker_relative = Path::new(&marker_name);
    if fs::symlink_metadata(crate_dir.join(marker_relative)).is_ok() {
        let Ok(quarantined) = root.retain_bound_regular(marker_relative, authority.marker_identity)
        else {
            return Ok(false);
        };
        quarantined.remove()?;
    } else if fs::symlink_metadata(&receipt).is_ok() {
        let receipt_relative = Path::new(receipt.file_name().unwrap());
        let (_, marker) = match root.open_regular(receipt_relative) {
            Ok(opened) => opened,
            Err(_) => return Ok(false),
        };
        if rust_dotnet_sdk_core::safe_fs::retained_file_identity(&marker)?
            != authority.marker_identity
        {
            return Ok(false);
        }
        let quarantined = root.quarantine_regular_bound(
            receipt_relative,
            &marker,
            marker_relative,
            authority.marker_identity,
        )?;
        quarantined.remove()?;
    }

    let binding_path =
        creation_binding_path(crate_dir, &creation_binding_from_authority(&authority))?;
    if fs::symlink_metadata(&binding_path).is_ok() {
        let binding_relative = Path::new(binding_path.file_name().unwrap());
        let Ok(binding) = root.retain_bound_regular(binding_relative, authority.binding_identity)
        else {
            return Ok(false);
        };
        binding.remove()?;
    }

    let _ = root.remove_retained_regular(authority_relative, &authority_marker)?;
    Ok(true)
}

fn cleanup_nuget_journal_residue_locked(crate_dir: &Path) -> Result<usize> {
    let mut removed = 0usize;
    let mut entries = fs::read_dir(crate_dir)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    let authority_residues = entries
        .iter()
        .filter_map(|entry| parse_quarantine_authority(crate_dir, &entry.path()).ok())
        .map(|authority| authority.residue_name)
        .collect::<std::collections::BTreeSet<_>>();
    for entry in &entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with(NUGET_QUARANTINE_AUTHORITY_PREFIX) {
            if recover_quarantine_authority(crate_dir, &entry.path())? {
                removed += 1;
            }
            continue;
        }
        if !is_nuget_residue_name(name) || name.contains(NUGET_RESIDUE_RECEIPT_SUFFIX) {
            continue;
        }
        if authority_residues.contains(name) {
            continue;
        }
        let path = entry.path();
        let residue_receipt = nuget_residue_receipt_path(crate_dir, &path)?;
        let Ok((root, owner_marker)) =
            validate_nuget_residue_receipt(crate_dir, &path, &residue_receipt)
        else {
            // A matching prefix alone is never ownership. Do not inspect, follow, or delete a
            // lookalike unless its complete external sidecar authorizes this exact path.
            continue;
        };
        let metadata = fs::symlink_metadata(&path)?;
        if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata)
            || !metadata.is_dir()
        {
            bail!(
                "NuGet journal residue is not a regular directory: {}",
                path.display()
            );
        }
        let binding = load_creation_binding(crate_dir, &path, &owner_marker)?;
        if let Some(binding) = binding {
            if root.direct_child_directory_identity(Path::new(name)).ok()
                != Some(binding.binding.directory_identity)
            {
                // The creation-time object was moved away and this pathname was rebound. Its
                // typed owner marker does not bless the replacement.
                continue;
            }
        } else {
            let retained = match root.retain_subdirectory_for_removal(Path::new(name)) {
                Ok(retained) => retained,
                Err(_) => continue,
            };
            if !retained.is_empty()? {
                // A crash before creation binding publication owns only a demonstrably empty
                // directory. Nonempty data is never inferred to be ours.
                continue;
            }
            publish_creation_binding_for_identity(
                crate_dir,
                &path,
                &owner_marker,
                retained.identity()?,
            )?;
        }
        remove_owned_nuget_residue(crate_dir, &path, &residue_receipt)?;
        removed += 1;
    }

    let mut current = fs::read_dir(crate_dir)?.collect::<std::io::Result<Vec<_>>>()?;
    current.sort_by_key(|entry| entry.file_name());
    for entry in current {
        let entry_path = entry.path();
        let Some(residue_name) = residue_name_from_receipt(&entry_path) else {
            continue;
        };
        if !is_nuget_residue_name(residue_name) {
            continue;
        }
        if authority_residues.contains(residue_name) {
            continue;
        }
        let residue = crate_dir.join(residue_name);
        if fs::symlink_metadata(&residue).is_ok() {
            continue;
        }
        if validate_nuget_residue_receipt(crate_dir, &residue, &entry_path).is_ok() {
            // A complete sidecar with no directory proves a crash after authority publication and
            // before directory creation. It owns no project data, so retiring it is safe.
            remove_nuget_residue_receipt(crate_dir, &entry_path)?;
            removed += 1;
        }
    }
    let mut bindings = fs::read_dir(crate_dir)?.collect::<std::io::Result<Vec<_>>>()?;
    bindings.sort_by_key(|entry| entry.file_name());
    let root = DirectoryCapability::open(crate_dir)?;
    for entry in bindings {
        let path = entry.path();
        let Ok(binding) = parse_creation_binding(crate_dir, &path) else {
            continue;
        };
        if authority_residues.contains(&binding.residue_name) {
            continue;
        }
        let residue = crate_dir.join(&binding.residue_name);
        let receipt = nuget_residue_receipt_path(crate_dir, &residue)?;
        if fs::symlink_metadata(&residue).is_ok() || fs::symlink_metadata(&receipt).is_ok() {
            continue;
        }
        let relative = Path::new(path.file_name().unwrap());
        let (_, marker) = match root.open_regular(relative) {
            Ok(opened) => opened,
            Err(_) => continue,
        };
        if marker.metadata()?.len() == 0 && root.remove_retained_regular(relative, &marker)? {
            removed += 1;
        }
    }
    Ok(removed)
}

fn validate_nuget_journal_receipt(crate_dir: &Path, journal: &Path) -> Result<DirectoryCapability> {
    let root = DirectoryCapability::open(crate_dir)?;
    let capability = root.direct_subdirectory_for_move(Path::new(
        journal.file_name().context("journal has no filename")?,
    ))?;
    let receipt: NugetJournalReceipt = serde_json::from_slice(
        &capability
            .snapshot_regular(Path::new(NUGET_JOURNAL_RECEIPT))?
            .1,
    )
    .context("parsing NuGet recovery journal receipt")?;
    let expected = expected_nuget_journal_receipt(crate_dir)?;
    if receipt.schema != expected.schema
        || receipt.crate_key != expected.crate_key
        || receipt.roots != expected.roots
    {
        bail!("NuGet recovery journal identity does not match this project");
    }
    capability.ensure_path_still_bound()?;
    Ok(capability)
}

fn retire_nuget_journal(crate_dir: &Path, journal: &Path) -> Result<()> {
    retire_nuget_journal_with_hook(crate_dir, journal, &mut |_, _| Ok(()))
}

fn retire_nuget_journal_with_hook(
    crate_dir: &Path,
    journal: &Path,
    before_move: &mut dyn FnMut(&Path, &Path) -> Result<()>,
) -> Result<()> {
    retire_nuget_journal_with_hooks(crate_dir, journal, &mut |_| Ok(()), before_move)
}

fn retire_nuget_journal_with_hooks(
    crate_dir: &Path,
    journal: &Path,
    after_validation: &mut dyn FnMut(&Path) -> Result<()>,
    before_move: &mut dyn FnMut(&Path, &Path) -> Result<()>,
) -> Result<()> {
    let validated_journal = validate_nuget_journal_receipt(crate_dir, journal)?;
    after_validation(journal)?;
    let cleanup_path = reserve_nuget_residue_path(crate_dir, NUGET_JOURNAL_CLEANUP_PREFIX)?;
    let authority = publish_nuget_residue_receipt(crate_dir, &cleanup_path)?;
    let root = DirectoryCapability::open(crate_dir)?;
    let journal_identity = validated_journal.identity()?;
    let _binding = publish_creation_binding_for_identity_unchecked(
        crate_dir,
        &cleanup_path,
        authority.marker.as_ref().context("owner marker missing")?,
        journal_identity,
    )?;
    before_move(journal, &cleanup_path)?;
    let moved = root.quarantine_retained_subdirectory_bound(
        Path::new(journal.file_name().context("journal has no filename")?),
        &validated_journal,
        Path::new(
            cleanup_path
                .file_name()
                .context("cleanup path has no filename")?,
        ),
        journal_identity,
    )?;
    let residue_receipt = authority.persist();
    crate::install_transaction::sync_directory(crate_dir)?;
    test_nuget_transaction_crash_point("after-journal-retire");
    drop(moved);
    remove_owned_nuget_residue(crate_dir, &cleanup_path, &residue_receipt)
}

fn sync_nuget_project_state(crate_dir: &Path) -> Result<()> {
    let capability = DirectoryCapability::open(crate_dir)?;
    for root in NUGET_TRANSACTION_ROOTS {
        let relative = Path::new(root);
        let path = crate_dir.join(relative);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(metadata)
                if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) =>
            {
                bail!("NuGet project state contains a link: {}", path.display())
            }
            Ok(metadata) if metadata.is_file() => {
                let (_, file) = capability.open_regular(relative)?;
                file.sync_all()?;
            }
            Ok(metadata) if metadata.is_dir() => {
                crate::install_transaction::sync_tree(&path)?;
            }
            Ok(_) => bail!(
                "NuGet project state has an unsupported type: {}",
                path.display()
            ),
        }
    }
    capability.ensure_path_still_bound()?;
    crate::install_transaction::sync_directory(crate_dir)
}

impl NugetProjectSnapshot {
    fn capture(crate_dir: &Path) -> Result<Self> {
        let capability = DirectoryCapability::open(crate_dir)?;
        let mut roots = Vec::new();
        for root in NUGET_TRANSACTION_ROOTS {
            let relative = PathBuf::from(root);
            let path = crate_dir.join(&relative);
            let snapshot = match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => SnapshotRoot::Missing,
                Err(error) => return Err(error.into()),
                Ok(metadata)
                    if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) =>
                {
                    bail!("NuGet project state contains a link: {}", path.display())
                }
                Ok(metadata) if metadata.is_file() => {
                    let (_, file) = capability.open_regular(&relative)?;
                    let permissions = file.metadata()?.permissions();
                    let bytes = capability.snapshot_regular(&relative)?.1;
                    SnapshotRoot::File(ProvisionedFile {
                        relative: relative.clone(),
                        bytes,
                        permissions,
                    })
                }
                Ok(metadata) if metadata.is_dir() => {
                    let tree = capability.subdirectory(&relative)?;
                    let mut directories = Vec::new();
                    let mut files = Vec::new();
                    tree.walk_regular_tree(&[], &mut |child, node| {
                        match node {
                            TreeWalkNode::DirectoryEnter(_) => {
                                directories.push(child.to_path_buf())
                            }
                            TreeWalkNode::File(file) => files.push(ProvisionedFile {
                                relative: relative.join(child),
                                bytes: rust_dotnet_sdk_core::safe_fs::read_opened_regular(
                                    file,
                                    &tree.root().join(child),
                                )?,
                                permissions: file.metadata()?.permissions(),
                            }),
                            TreeWalkNode::DirectoryLeave(_) => {}
                        }
                        Ok(())
                    })?;
                    SnapshotRoot::Directory { directories, files }
                }
                Ok(_) => bail!(
                    "NuGet project state has an unsupported type: {}",
                    path.display()
                ),
            };
            roots.push((relative, snapshot));
        }
        capability.ensure_path_still_bound()?;
        Ok(Self { roots })
    }

    fn materialize(&self, destination: &Path) -> Result<()> {
        fs::create_dir_all(destination)?;
        let capability = DirectoryCapability::open(destination)?;
        for (root, snapshot) in &self.roots {
            match snapshot {
                SnapshotRoot::Missing => {}
                SnapshotRoot::File(file) => restore_project_file(&capability, file)?,
                SnapshotRoot::Directory { directories, files } => {
                    fs::create_dir_all(destination.join(root))?;
                    for directory in directories {
                        fs::create_dir_all(destination.join(root).join(directory))?;
                    }
                    for file in files {
                        restore_project_file(&capability, file)?;
                    }
                }
            }
        }
        capability.ensure_path_still_bound()?;
        crate::install_transaction::sync_tree(destination)
    }

    fn restore(self, crate_dir: &Path) -> Result<()> {
        for (relative, _) in &self.roots {
            let path = crate_dir.join(relative);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                    crate::path_safety::remove_dir_all_within(crate_dir, &path)?;
                }
                Ok(_) => fs::remove_file(&path)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let capability = DirectoryCapability::open(crate_dir)?;
        for (root, snapshot) in self.roots {
            match snapshot {
                SnapshotRoot::Missing => {}
                SnapshotRoot::File(file) => restore_project_file(&capability, &file)?,
                SnapshotRoot::Directory { directories, files } => {
                    fs::create_dir_all(crate_dir.join(&root))?;
                    for directory in directories {
                        fs::create_dir_all(crate_dir.join(&root).join(directory))?;
                    }
                    for file in files {
                        restore_project_file(&capability, &file)?;
                    }
                }
            }
        }
        capability.ensure_path_still_bound()
    }
}

fn restore_project_file(capability: &DirectoryCapability, file: &ProvisionedFile) -> Result<()> {
    capability.publish_bytes(&file.relative, &file.bytes)?;
    let (_, restored) = capability.open_regular(&file.relative)?;
    restored.set_permissions(file.permissions.clone())?;
    restored.sync_all()?;
    Ok(())
}

/// Upsert `{id: version}` into `<crate_dir>/.cargo-dotnet-nuget-deps.json`, creating it if this is
/// the crate's first `add-nuget` call. Last-write-wins per id, mirroring how re-running `add-nuget
/// <id> <newer-version>` already overwrites that id's cached dll.
#[cfg(test)]
fn record_dependency(
    crate_dir: &Path,
    id: &str,
    version: &str,
    rid: Option<&str>,
    tfm: &str,
    sources: &[String],
) -> Result<()> {
    with_nuget_transaction(crate_dir, || {
        record_dependency_locked(crate_dir, id, version, rid, tfm, sources)
    })
}

fn record_dependency_locked(
    crate_dir: &Path,
    id: &str,
    version: &str,
    rid: Option<&str>,
    tfm: &str,
    sources: &[String],
) -> Result<()> {
    crate::path_safety::validate_nuget_id(id)?;
    crate::path_safety::validate_nuget_version(version)?;
    let path = crate_dir.join(DEPS_MANIFEST_FILE);
    let mut manifest = load_deps_manifest(crate_dir)?;
    let sources = dependency_sources(crate_dir, sources)?;
    let source_identity_sha256 = dependency_source_identity(&sources)?;
    manifest.schema = DEPS_MANIFEST_SCHEMA;
    manifest.dependencies.insert(
        id.to_string(),
        DependencyRecord {
            version: version.into(),
            rid: rid.map(str::to_string),
            tfm: tfm.into(),
            sources,
            source_identity_sha256,
        },
    );
    let mut temporary = tempfile::Builder::new()
        .prefix(".cargo-dotnet-nuget-deps-")
        .tempfile_in(crate_dir)?;
    temporary.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing {}", path.display()))?;
    Ok(())
}

fn load_deps_manifest(crate_dir: &Path) -> Result<DepsManifest> {
    let path = crate_dir.join(DEPS_MANIFEST_FILE);
    if !path.exists() {
        return Ok(DepsManifest {
            schema: DEPS_MANIFEST_SCHEMA,
            ..DepsManifest::default()
        });
    }
    let bytes = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    let manifest = if value.get("schema").is_some() {
        let manifest: DepsManifest = serde_json::from_value(value)?;
        if manifest.schema != DEPS_MANIFEST_SCHEMA {
            bail!(
                "unsupported NuGet dependency manifest schema {}",
                manifest.schema
            );
        }
        manifest
    } else {
        // Read compatibility for the pre-0.0.2 `{ id: version }` shape. The next explicit
        // add-nuget upgrades it to the versioned reproducible contract.
        let legacy: BTreeMap<String, String> = serde_json::from_value(value)?;
        let sources = vec![DependencySource::Default];
        let source_identity_sha256 = dependency_source_identity(&sources)?;
        DepsManifest {
            schema: DEPS_MANIFEST_SCHEMA,
            dependencies: legacy
                .into_iter()
                .map(|(id, version)| {
                    (
                        id,
                        DependencyRecord {
                            version,
                            rid: None,
                            tfm: "net10.0".into(),
                            sources: sources.clone(),
                            source_identity_sha256: source_identity_sha256.clone(),
                        },
                    )
                })
                .collect(),
        }
    };
    validate_dependency_manifest(crate_dir, &manifest)?;
    Ok(manifest)
}

fn dependency_sources(crate_dir: &Path, sources: &[String]) -> Result<Vec<DependencySource>> {
    if sources.is_empty() {
        return Ok(vec![DependencySource::Default]);
    }
    let crate_dir = fs::canonicalize(crate_dir)
        .with_context(|| format!("resolving consumer crate {}", crate_dir.display()))?;
    let mut result = Vec::new();
    for source in sources {
        if source.contains("://") {
            if source.contains(['?', '#'])
                || source.split_once("://").is_some_and(|(_, rest)| {
                    rest.split('/')
                        .next()
                        .is_some_and(|host| host.contains('@'))
                })
            {
                bail!(
                    "NuGet source URLs recorded in the project must not contain credentials, query strings, or fragments; use a credential provider for {source:?}"
                );
            }
            result.push(DependencySource::Url {
                value: source.clone(),
            });
        } else {
            let candidate = PathBuf::from(source);
            let absolute = if candidate.is_absolute() {
                candidate
            } else {
                std::env::current_dir()?.join(candidate)
            };
            let absolute = fs::canonicalize(&absolute)
                .with_context(|| format!("resolving local NuGet source {}", absolute.display()))?;
            let relative = absolute.strip_prefix(&crate_dir).with_context(|| {
                format!(
                    "local NuGet source {} must be inside the consumer crate so a fresh clone can reproduce it",
                    absolute.display()
                )
            })?;
            crate::path_safety::validate_relative_path(relative)?;
            result.push(DependencySource::CratePath {
                path: relative.to_string_lossy().replace('\\', "/"),
            });
        }
    }
    result.sort_by_key(|source| serde_json::to_string(source).unwrap_or_default());
    result.dedup();
    Ok(result)
}

fn dependency_source_identity(sources: &[DependencySource]) -> Result<String> {
    Ok(crate::content_cache::digest_parts([
        b"cargo-dotnet-nuget-source-v1".as_slice(),
        serde_json::to_vec(sources)?.as_slice(),
    ]))
}

fn validate_dependency_manifest(crate_dir: &Path, manifest: &DepsManifest) -> Result<()> {
    if manifest.schema != DEPS_MANIFEST_SCHEMA {
        bail!(
            "unsupported NuGet dependency manifest schema {}",
            manifest.schema
        );
    }
    for (id, record) in &manifest.dependencies {
        crate::path_safety::validate_nuget_id(id)?;
        crate::path_safety::validate_nuget_version(&record.version)?;
        if let Some(rid) = &record.rid {
            crate::path_safety::validate_path_component("NuGet dependency RID", rid)?;
        }
        if record.tfm.is_empty()
            || record.sources.is_empty()
            || dependency_source_identity(&record.sources)? != record.source_identity_sha256
        {
            bail!("NuGet dependency {id} has an invalid TFM or source identity");
        }
        for source in &record.sources {
            if let DependencySource::CratePath { path } = source {
                crate::path_safety::canonical_file_or_directory_within(crate_dir, Path::new(path))?;
            }
        }
    }
    Ok(())
}

fn resolved_dependency_sources(
    crate_dir: &Path,
    sources: &[DependencySource],
) -> Result<Vec<String>> {
    sources
        .iter()
        .filter_map(|source| match source {
            DependencySource::Default => None,
            DependencySource::Url { value } => Some(Ok(value.clone())),
            DependencySource::CratePath { path } => Some(
                crate::path_safety::canonical_file_or_directory_within(crate_dir, Path::new(path))
                    .map(|path| path.to_string_lossy().into_owned()),
            ),
        })
        .collect()
}

/// Read back every `{id: version}` an `add-nuget` crate has recorded — `Vec::new()` (not an
/// error) if the crate never ran `add-nuget`. Used by `pack` to populate real `.nuspec`
/// `<dependency>` entries instead of bundling raw dlls (see that module's doc for why).
fn recorded_dependencies_locked(crate_dir: &Path) -> Result<Vec<(String, String)>> {
    Ok(load_deps_manifest(crate_dir)?
        .dependencies
        .into_iter()
        .map(|(id, record)| (id, record.version))
        .collect())
}

/// Return the complete non-compile closure staged by `add-nuget`, preserving its package
/// layout.  In particular, `runtimes/<rid>/native/` and culture-resource subdirectories are
/// deliberately not converted to output-directory filenames here.
pub(crate) fn staged_package_assets(crate_dir: &Path) -> Result<Vec<StagedPackageAsset>> {
    acquire_project_lease(crate_dir)?.staged_package_assets()
}

fn staged_package_assets_locked(crate_dir: &Path) -> Result<Vec<StagedPackageAsset>> {
    let mut assets = nuget_assets::package_assets(crate_dir)?;
    assets.extend(local_native_assets(crate_dir)?);
    let mut paths = BTreeMap::<String, PathBuf>::new();
    for asset in &assets {
        if let Some(previous) = paths.insert(asset.logical_path.clone(), asset.source.clone())
            && previous != asset.source
        {
            bail!(
                "native asset collision at {}: {} and {}",
                asset.logical_path,
                previous.display(),
                asset.source.display()
            );
        }
    }
    Ok(assets)
}

fn local_native_assets(crate_dir: &Path) -> Result<Vec<StagedPackageAsset>> {
    let manifest_path = crate_dir.join(LOCAL_NATIVE_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Ok(Vec::new());
    }
    let manifest: LocalNativeManifest = serde_json::from_slice(
        &rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parsing {}", manifest_path.display()))?;
    if manifest.schema != 1 {
        bail!(
            "unsupported local native manifest schema {} in {}",
            manifest.schema,
            manifest_path.display()
        );
    }
    let mut assets = Vec::new();
    for (library, rid_paths) in manifest.libraries {
        for (rid, relative) in rid_paths {
            crate::path_safety::validate_path_component("native asset RID", &rid)?;
            let relative_path = Path::new(&relative);
            crate::path_safety::validate_relative_path(relative_path)?;
            let (source, contents) =
                rust_dotnet_sdk_core::safe_fs::snapshot_regular_within(crate_dir, relative_path)
                    .with_context(|| {
                        format!(
                            "invalid vendored native file {relative:?} recorded in {}",
                            manifest_path.display()
                        )
                    })?;
            let filename = source
                .file_name()
                .and_then(|name| name.to_str())
                .context("vendored native filename is not UTF-8")?;
            assets.push(StagedPackageAsset {
                owner: format!("local:{library}"),
                logical_path: format!("runtimes/{rid}/native/{filename}"),
                source,
                contents: contents.into(),
                kind: StagedPackageAssetKind::Native,
                rid: Some(rid),
            });
        }
    }
    Ok(assets)
}

/// `{id: version}` for every `add-nuget` dependency whose staged runtime closure under
/// `.cargo-dotnet-nuget-assets/` is missing or incomplete relative to what
/// `.cargo-dotnet-nuget-deps.json` recorded — including a staged graph whose version no longer
/// matches the recorded one (see `nuget_assets::missing_recorded_roots`'s doc for the version-
/// drift case this catches). `Vec::new()` for a crate that never ran `add-nuget` — cheap and
/// silent, since it never touches `nuget_assets` beyond the deps manifest read. This is the
/// fresh-clone detector: the deps manifest is checked in, the assets dir is gitignored, so
/// cloning the repo and building leaves every recorded id "missing" here until `ensure_staged`
/// re-restores it.
fn missing_assets(crate_dir: &Path) -> Result<Vec<(String, DependencyRecord)>> {
    let _lease = acquire_project_lease(crate_dir)?;
    missing_assets_locked(crate_dir)
}

fn missing_assets_locked(crate_dir: &Path) -> Result<Vec<(String, DependencyRecord)>> {
    let manifest = load_deps_manifest(crate_dir)?;
    if manifest.dependencies.is_empty() {
        return Ok(Vec::new());
    }
    let recorded = manifest
        .dependencies
        .iter()
        .map(|(id, record)| (id.clone(), record.version.clone()))
        .collect::<Vec<_>>();
    let missing_ids = nuget_assets::missing_recorded_roots(crate_dir, &recorded)?
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    Ok(manifest
        .dependencies
        .into_iter()
        .filter(|(id, _)| missing_ids.contains(id))
        .collect())
}

/// Re-restore and re-stage every `add-nuget` dependency whose runtime closure is missing or
/// incomplete, called from the `build`/`run`/`test` pipeline (before `copy_assets`) and from
/// `cargo dotnet restore` — the sanctioned offline-prepare step. A no-op (and silent) for crates
/// that never ran `add-nuget`, and for crates whose staged assets are already complete.
///
/// The versioned dependency manifest records the exact RID/TFM/source selection. Local feeds must
/// live below the consumer crate and are stored project-relative, so a fresh clone can reproduce
/// the restore without retaining machine-specific absolute paths or credentials.
///
/// Offline (`--offline`/`--frozen`) builds must never silently hit the network: if assets are
/// missing while offline, this fails with a clear, actionable error instead of restoring.
pub fn ensure_staged(ctx: &Context) -> Result<()> {
    let missing = missing_assets(&ctx.crate_dir)?;
    if missing.is_empty() {
        return Ok(());
    }
    if ctx.is_offline() {
        let names = missing
            .iter()
            .map(|(id, record)| format!("{id} {}", record.version))
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "offline build is missing staged NuGet assets for {names} (recorded in {}); run \
             `cargo dotnet restore` while network access is available, or re-run `cargo dotnet \
             add-nuget` online",
            ctx.crate_dir.join(DEPS_MANIFEST_FILE).display()
        );
    }
    with_nuget_transaction(&ctx.crate_dir, || {
        let missing = missing_assets_locked(&ctx.crate_dir)?;
        if missing.is_empty() {
            return Ok(());
        }
        eprintln!(
            "==> cargo dotnet: auto-restoring staged NuGet assets for {} (missing or incomplete)",
            missing
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        for (id, record) in &missing {
            let sources = resolved_dependency_sources(&ctx.crate_dir, &record.sources)?;
            let resolved = crate::nuget_cache::restore(
                id,
                &record.version,
                record.rid.as_deref(),
                &record.tfm,
                &sources,
                false,
            )?;
            nuget_assets::stage_assets(&ctx.crate_dir, id, &resolved.assets)?;
        }
        // Re-check rather than trusting the restore loop unconditionally: the recorded version is
        // whatever the user typed to `add-nuget`, while the asset manifest carries NuGet's
        // normalized version.
        let still_missing = missing_assets_locked(&ctx.crate_dir)?;
        if !still_missing.is_empty() {
            let names = still_missing
                .iter()
                .map(|(id, record)| format!("{id} {}", record.version))
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "restored NuGet assets for {names} but they still don't satisfy the recorded \
                 version in {} — the recorded version may not be NuGet's normalized form; re-run \
                 `cargo dotnet add-nuget` with the exact version NuGet reports for this package",
                ctx.crate_dir.join(DEPS_MANIFEST_FILE).display()
            );
        }
        Ok(())
    })
}

/// spinacz's reflection core, embedded at COMPILE TIME of `cargo-dotnet` itself. Written out
/// verbatim into the ephemeral bindgen crate at RUN time (see the module doc's "why not a
/// normal dependency" note).
const REFLECT_RS: &str = include_str!("../../../cargo_tests/spinacz/src/reflect.rs");

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
    let dotnet: crate::context::DotnetVersion = args.dotnet.parse().map_err(anyhow::Error::msg)?;
    let crate_dir = args.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let crate_dir = fs::canonicalize(&crate_dir)
        .with_context(|| format!("add-nuget: no such directory: {}", crate_dir.display()))?;
    if !crate_dir.join("Cargo.toml").is_file() {
        bail!(
            "add-nuget: not a crate dir (no Cargo.toml): {}",
            crate_dir.display()
        );
    }

    crate::path_safety::validate_nuget_id(&args.id)?;
    crate::path_safety::validate_nuget_version(&args.version)?;
    // Restore snapshots are immutable, integrity-checked, and atomically activated. `--force`
    // rebuilds the same exact key under its per-key lock for mutable local/private feeds.
    let resolved = crate::nuget_cache::restore(
        &args.id,
        &args.version,
        args.rid.as_deref(),
        dotnet.tfm(),
        &args.source,
        args.force,
    )?;
    let restore_key = resolved.cache_key.clone();
    let (dll, compile_dlls, runtime_dlls, resolved_assets) = (
        resolved.primary_dll.with_context(|| {
            format!(
                "add-nuget: restored package has no managed compile/runtime DLL for {}",
                dotnet.tfm()
            )
        })?,
        resolved.compile_dlls,
        resolved.runtime_dlls,
        resolved.assets,
    );
    if !dll.is_file() {
        bail!("add-nuget: resolved dll does not exist: {}", dll.display());
    }

    // SDK restore owns TFM selection, version negotiation, and the full transitive graph.
    // Runtime assets are used for CLR probing; compile assets remain available as a fallback
    // for reference-only packages.
    let mut extra_dlls = runtime_dlls;
    for compile in compile_dlls {
        if !extra_dlls
            .iter()
            .any(|p| p.file_name() == compile.file_name())
        {
            extra_dlls.push(compile);
        }
    }
    extra_dlls.retain(|path| path != &dll);

    let asm_name = dll
        .file_stem()
        .and_then(|s| s.to_str())
        .context("add-nuget: dll has no file stem")?
        .to_string();

    eprintln!(
        "== cargo dotnet add-nuget: {} {} -> {} (assembly '{asm_name}') ==",
        args.id,
        args.version,
        dll.display()
    );

    let out_rs = cached_bindings(
        &restore_key,
        &dll,
        &extra_dlls,
        dotnet,
        args.verbose,
        args.force,
    )?;

    // ---- wire into the consumer crate ----
    let mod_name = to_snake_ident(&args.id);
    let nuget_dir = crate_dir.join("src").join("nuget");
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
         #![allow(non_camel_case_types, unused_imports)]\n\
         #[allow(unused_imports)]\nuse mycorrhiza::bindings::System;\n\n",
    );
    bindings_src.push_str(&fs::read_to_string(&out_rs.path)?);
    let mod_rs = nuget_dir.join("mod.rs");
    let decl = format!("pub mod {mod_name};\n");

    // The runtime-asset marker dir: `pipeline.rs` copies every file here alongside the final
    // build output on every subsequent `build`/`run` of THIS crate (see its doc comment).
    // Record {id: version} so `pack` (which does NOT bundle .cargo-dotnet-nuget-assets/, see its
    // own doc) can instead emit a real `<dependency>` in the produced .nuspec — the idiomatic
    // NuGet path, which gets RID-specific native assets (e.g. SQLitePCLRaw's native SQLite driver)
    // and transitive version negotiation right in a way bundling raw dlls never could. Stored as a
    // SIBLING of assets_dir, not inside it, so `copy_assets`'s "copy every file" loop below doesn't
    // also ship this bookkeeping file next to the compiled build output.
    with_nuget_transaction(&crate_dir, || {
        nuget_assets::stage_assets(&crate_dir, &args.id, &resolved_assets)?;
        let capability = DirectoryCapability::open(&crate_dir)?;
        capability.publish_bytes(
            Path::new("src/nuget")
                .join(format!("{mod_name}.rs"))
                .as_path(),
            bindings_src.as_bytes(),
        )?;
        let mut existing = match fs::symlink_metadata(&mod_rs) {
            Ok(_) => String::from_utf8(
                capability
                    .snapshot_regular(Path::new("src/nuget/mod.rs"))?
                    .1,
            )
            .context("src/nuget/mod.rs is not UTF-8")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        if !existing.contains(&decl) {
            existing.push_str(&decl);
            capability.publish_bytes(Path::new("src/nuget/mod.rs"), existing.as_bytes())?;
        }
        record_dependency_locked(
            &crate_dir,
            &args.id,
            &args.version,
            args.rid.as_deref(),
            dotnet.tfm(),
            &args.source,
        )?;
        capability.ensure_path_still_bound()
    })?;

    eprintln!(
        "== cargo dotnet add-nuget: wrote {} ==",
        dest_file.display()
    );
    eprintln!(
        "== cargo dotnet add-nuget: add `mod nuget;` to your crate root if this is the first \
         package added; the generated module is `nuget::{mod_name}` =="
    );
    eprintln!(
        "== cargo dotnet add-nuget: the SDK-selected runtime/native/resource graph is staged \
         under .cargo-dotnet-nuget-assets/ and will be copied next to your build output automatically =="
    );
    Ok(0)
}

/// Restore and stage a native-only package.  This deliberately does not invoke reflection
/// binding: native packages commonly contain no managed compile assembly.
pub fn run_native(args: &AddNativeArgs) -> Result<i32> {
    let dotnet: crate::context::DotnetVersion = args.dotnet.parse().map_err(anyhow::Error::msg)?;
    let crate_dir = fs::canonicalize(args.path.clone().unwrap_or_else(|| PathBuf::from(".")))?;
    if !crate_dir.join("Cargo.toml").is_file() {
        bail!("add-native: not a crate dir: {}", crate_dir.display());
    }
    crate::path_safety::validate_nuget_id(&args.id)?;
    crate::path_safety::validate_nuget_version(&args.version)?;
    let host = crate::host::HostFacts::detect();
    let rid = args.rid.as_deref().unwrap_or(host.host_rid);
    crate::path_safety::validate_path_component("native asset RID", rid)?;
    let resolved =
        crate::nuget_cache::restore(&args.id, &args.version, Some(rid), dotnet.tfm(), &[], false)?;
    let native_assets = resolved
        .assets
        .iter()
        .filter(|asset| asset.kind == rust_dotnet_assets::AssetKind::Native)
        .collect::<Vec<_>>();
    if native_assets.is_empty() {
        bail!(
            "add-native: package {} {} has no native assets",
            args.id,
            args.version
        );
    }
    if !native_assets
        .iter()
        .any(|asset| native_library_matches(&args.library, &asset.source))
    {
        let available = native_assets
            .iter()
            .filter_map(|asset| asset.source.file_name()?.to_str())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "add-native: --library {:?} does not match a selected native file for {rid}; \
             available: {available}",
            args.library
        );
    }
    with_nuget_transaction(&crate_dir, || {
        nuget_assets::stage_assets(&crate_dir, &args.id, &resolved.assets)?;
        // Use the same durable package manifest as managed dependencies so fresh clones
        // auto-restore this native graph and pack retains the real NuGet dependency.
        record_dependency_locked(
            &crate_dir,
            &args.id,
            &args.version,
            Some(rid),
            dotnet.tfm(),
            &[],
        )
    })?;
    eprintln!(
        "== cargo dotnet add-native: staged {} {} for {rid}; declare #[link(name = {:?})] ==",
        args.id, args.version, args.library
    );
    Ok(0)
}

/// Vendor a local native library under a RID-qualified project path and record it for every
/// subsequent build, run, test, and pack. Copying rather than retaining an absolute source path
/// keeps the project reproducible for collaborators and CI.
pub fn run_native_file(args: &AddNativeFileArgs) -> Result<i32> {
    run_native_file_with_hook(args, &mut |_| Ok(()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativePublishPoint {
    File,
    Manifest,
}

fn run_native_file_with_hook(
    args: &AddNativeFileArgs,
    hook: &mut dyn FnMut(NativePublishPoint) -> Result<()>,
) -> Result<i32> {
    let crate_dir = fs::canonicalize(args.path.clone().unwrap_or_else(|| PathBuf::from(".")))?;
    if !crate_dir.join("Cargo.toml").is_file() {
        bail!("add-native-file: not a crate dir: {}", crate_dir.display());
    }
    let source = fs::canonicalize(&args.file)
        .with_context(|| format!("resolving native library {}", args.file.display()))?;
    if !source.is_file() {
        bail!("add-native-file: not a file: {}", source.display());
    }
    if !native_library_matches(&args.library, &source) {
        bail!(
            "add-native-file: --library {:?} does not match native filename {}",
            args.library,
            source.display()
        );
    }
    let rid = args
        .rid
        .as_deref()
        .unwrap_or_else(|| crate::host::HostFacts::detect().host_rid);
    crate::path_safety::validate_path_component("native asset RID", rid)?;
    let filename = source
        .file_name()
        .context("native library has no filename")?;
    let relative = PathBuf::from("native").join(rid).join(filename);
    let contents = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&source)
        .with_context(|| format!("reading native library {}", source.display()))?;
    let destination = publish_local_native_transaction(
        &crate_dir,
        &relative,
        &contents,
        &args.library,
        rid,
        hook,
    )?;
    eprintln!(
        "== cargo dotnet add-native-file: vendored {} for {rid}; declare #[link(name = {:?})] ==",
        destination.display(),
        args.library
    );
    Ok(0)
}

#[derive(Debug)]
enum LeafSnapshot {
    Missing,
    Regular {
        bytes: Vec<u8>,
        permissions: fs::Permissions,
    },
    #[cfg(unix)]
    Symlink(PathBuf),
}

fn capture_leaf(capability: &DirectoryCapability, relative: &Path) -> Result<LeafSnapshot> {
    let path = capability.root().join(relative);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LeafSnapshot::Missing),
        Err(error) => Err(error.into()),
        #[cfg(unix)]
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Ok(LeafSnapshot::Symlink(fs::read_link(&path)?))
        }
        Ok(metadata) if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) => {
            bail!(
                "native transaction leaf is a reparse point: {}",
                path.display()
            )
        }
        Ok(metadata) if metadata.is_file() => Ok(LeafSnapshot::Regular {
            bytes: capability.snapshot_regular(relative)?.1,
            permissions: metadata.permissions(),
        }),
        Ok(_) => bail!(
            "native transaction leaf is not a regular file: {}",
            path.display()
        ),
    }
}

fn restore_leaf(
    capability: &DirectoryCapability,
    relative: &Path,
    snapshot: LeafSnapshot,
) -> Result<()> {
    let path = capability.root().join(relative);
    match snapshot {
        LeafSnapshot::Missing => match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                bail!("refusing to remove a directory during native rollback")
            }
            Ok(_) => fs::remove_file(path).map_err(Into::into),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        },
        LeafSnapshot::Regular { bytes, permissions } => {
            capability.publish_bytes(relative, &bytes)?;
            let (_, restored) = capability.open_regular(relative)?;
            restored.set_permissions(permissions)?;
            restored.sync_all()?;
            Ok(())
        }
        #[cfg(unix)]
        LeafSnapshot::Symlink(target) => {
            match fs::symlink_metadata(&path) {
                Ok(_) => fs::remove_file(&path)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            std::os::unix::fs::symlink(target, path)?;
            Ok(())
        }
    }
}

fn publish_local_native_transaction(
    crate_dir: &Path,
    relative: &Path,
    contents: &[u8],
    library: &str,
    rid: &str,
    hook: &mut dyn FnMut(NativePublishPoint) -> Result<()>,
) -> Result<PathBuf> {
    let _lock = nuget_transaction_lock(crate_dir)?;
    recover_nuget_transaction_locked(crate_dir)?;
    let capability = DirectoryCapability::open(crate_dir)?;
    let manifest_relative = Path::new(LOCAL_NATIVE_MANIFEST_FILE);
    let previous_file = capture_leaf(&capability, relative)?;
    let previous_manifest = capture_leaf(&capability, manifest_relative)?;
    let mut manifest = match &previous_manifest {
        LeafSnapshot::Missing => LocalNativeManifest {
            schema: 1,
            ..Default::default()
        },
        LeafSnapshot::Regular { bytes, .. } => serde_json::from_slice(bytes)
            .with_context(|| format!("parsing {}", crate_dir.join(manifest_relative).display()))?,
        #[cfg(unix)]
        LeafSnapshot::Symlink(_) => bail!("local native manifest must not be a symlink"),
    };
    manifest.schema = 1;
    manifest
        .libraries
        .entry(library.to_string())
        .or_default()
        .insert(
            rid.to_string(),
            relative.to_string_lossy().replace('\\', "/"),
        );
    let publish = (|| -> Result<()> {
        capability.publish_bytes(relative, contents)?;
        hook(NativePublishPoint::File)?;
        capability.publish_bytes(manifest_relative, &serde_json::to_vec_pretty(&manifest)?)?;
        hook(NativePublishPoint::Manifest)?;
        capability.ensure_path_still_bound()
    })();
    if let Err(error) = publish {
        let rollback = (|| -> Result<()> {
            restore_leaf(&capability, relative, previous_file)?;
            restore_leaf(&capability, manifest_relative, previous_manifest)?;
            capability.ensure_path_still_bound()
        })();
        return match rollback {
            Ok(()) => Err(error).context("local native project transaction rolled back"),
            Err(rollback) => bail!(
                "local native project transaction failed ({error:#}); rollback also failed: {rollback:#}"
            ),
        };
    }
    Ok(crate_dir.join(relative))
}

pub(crate) fn native_library_matches(logical: &str, path: &Path) -> bool {
    fn normalized(name: &str) -> &str {
        let name = name.strip_prefix("lib").unwrap_or(name);
        name.strip_suffix(".dylib")
            .or_else(|| name.strip_suffix(".dll"))
            .or_else(|| name.split_once(".so").map(|(stem, _)| stem))
            .unwrap_or(name)
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|file| normalized(file).eq_ignore_ascii_case(normalized(logical)))
}

fn cached_bindings(
    restore_key: &str,
    dll: &Path,
    extra_dlls: &[PathBuf],
    dotnet: crate::context::DotnetVersion,
    verbose: bool,
    force: bool,
) -> Result<CachedBindings> {
    let work = tempfile::Builder::new()
        .prefix("cargo-dotnet-nuget-bindgen-")
        .tempdir()?;
    write_bindgen_crate(dll, work.path())?;
    let ctx = bindgen_context(work.path(), verbose, dotnet)?;
    let private_sysroot = crate::private_sysroot::prepare(&ctx)?;
    let key = bindgen_key(
        restore_key,
        dll,
        extra_dlls,
        dotnet,
        &private_sysroot.key,
        &private_sysroot.payload_sha256,
    )?;
    let store = crate::content_cache::ContentStore::new(
        crate::context::cargo_dotnet_cache_home()?.join("nuget-bindgen/v3"),
        BINDGEN_CACHE_LIMIT,
    )?;
    let validate = |snapshot: &Path| validate_bindgen_snapshot(snapshot, &key);
    let build = |snapshot: &Path| {
        let _build_lock = crate::build_lock::BuildLock::acquire_crate(&ctx)?;
        generate_bindings_prepared(dll, work.path(), extra_dlls, &ctx, &private_sysroot)?;
        let produced = work.path().join("out.rs");
        let output = snapshot.join("out.rs");
        let output_bytes = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&produced)
            .with_context(|| {
                format!(
                    "add-nuget: bindgen ran but produced no out.rs at {}",
                    produced.display()
                )
            })?;
        let mut output_file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output)?;
        output_file.write_all(&output_bytes)?;
        output_file.sync_all()?;
        let output_sha256 = format!("{:x}", Sha256::digest(&output_bytes));
        fs::write(
            snapshot.join("receipt.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": BINDGEN_CACHE_SCHEMA,
                "key": key,
                "output": "out.rs",
                "output_sha256": output_sha256,
            }))?,
        )?;
        Ok(())
    };
    let (snapshot, status) = if force {
        store.materialize_forced(&key, validate, build)?
    } else {
        store.materialize(&key, validate, build)?
    };
    if status == crate::content_cache::CacheStatus::Hit {
        eprintln!(
            "== cargo dotnet add-nuget: using content-addressed bindings (pass --force to regenerate) =="
        );
    }
    Ok(CachedBindings {
        path: snapshot.path().join("out.rs"),
        _lease: snapshot,
    })
}

fn validate_bindgen_snapshot(snapshot: &Path, key: &str) -> Result<bool> {
    let output = snapshot.join("out.rs");
    let receipt = snapshot.join("receipt.json");
    for path in [&output, &receipt] {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
            _ => return Ok(false),
        };
        if metadata.len() == 0 {
            return Ok(false);
        }
    }
    let Ok(receipt_bytes) = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&receipt) else {
        return Ok(false);
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&receipt_bytes) else {
        return Ok(false);
    };
    let actual = format!(
        "{:x}",
        Sha256::digest(rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(
            &output
        )?)
    );
    Ok(value.get("schema").and_then(serde_json::Value::as_u64)
        == Some(u64::from(BINDGEN_CACHE_SCHEMA))
        && value.get("key").and_then(serde_json::Value::as_str) == Some(key)
        && value
            .get("output_sha256")
            .and_then(serde_json::Value::as_str)
            == Some(actual.as_str()))
}

fn bindgen_key(
    restore_key: &str,
    dll: &Path,
    extra_dlls: &[PathBuf],
    dotnet: crate::context::DotnetVersion,
    private_sysroot_key: &str,
    private_sysroot_payload_sha256: &str,
) -> Result<String> {
    let mut hash = Sha256::new();
    hash_bindgen_identity_parts(
        &mut hash,
        restore_key.as_bytes(),
        dotnet.as_env().as_bytes(),
        crate::mode::DEFAULT_TOOLCHAIN.as_bytes(),
        env!("CARGO_PKG_VERSION").as_bytes(),
        private_sysroot_key.as_bytes(),
        private_sysroot_payload_sha256.as_bytes(),
        include_bytes!("nuget.rs"),
        REFLECT_RS.as_bytes(),
    );
    hash_binding_assembly(&mut hash, dll)?;
    for extra in extra_dlls {
        hash_binding_assembly(&mut hash, extra)?;
    }
    hash_bindgen_sdk_inputs(&mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

#[allow(clippy::too_many_arguments)]
fn hash_bindgen_identity_parts(
    hash: &mut Sha256,
    restore_key: &[u8],
    dotnet: &[u8],
    toolchain: &[u8],
    tool_version: &[u8],
    private_sysroot_key: &[u8],
    private_sysroot_payload_sha256: &[u8],
    tooling_source: &[u8],
    reflect_source: &[u8],
) {
    bindgen_hash_part(hash, b"cargo-dotnet-nuget-bindgen-v3");
    for part in [
        restore_key,
        dotnet,
        toolchain,
        tool_version,
        private_sysroot_key,
        private_sysroot_payload_sha256,
        tooling_source,
        reflect_source,
    ] {
        bindgen_hash_part(hash, part);
    }
}

fn hash_binding_assembly(hash: &mut Sha256, path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .context("NuGet binding assembly has no filename")?
        .as_encoded_bytes();
    bindgen_hash_part(hash, name);
    bindgen_hash_part(
        hash,
        &rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(path)
            .with_context(|| format!("reading binding input {}", path.display()))?,
    );
    Ok(())
}

fn hash_bindgen_sdk_inputs(hash: &mut Sha256) -> Result<()> {
    let facts = crate::host::HostFacts::detect();
    bindgen_hash_part(hash, facts.os.as_bytes());
    bindgen_hash_part(hash, facts.arch.as_bytes());
    bindgen_hash_part(hash, facts.host_rid.as_bytes());
    match crate::mode::detect()? {
        crate::mode::Mode::Installed { home } => {
            let layout = crate::bundle::installed_layout(&home)?;
            for relative in [
                layout.version,
                layout.backend,
                layout.linker,
                layout.target_spec,
                layout.crates_root,
            ] {
                hash_optional_path(hash, &home.join(relative))?;
            }
        }
        crate::mode::Mode::Dev { repo_root } => {
            for path in [
                repo_root.join("x86_64-unknown-dotnet.json"),
                repo_root
                    .join("target/release")
                    .join(facts.backend_dylib_name()),
                repo_root.join(format!("target/release/linker{}", facts.exe_ext)),
                repo_root.join("mycorrhiza"),
                repo_root.join("dotnet_macros"),
            ] {
                hash_optional_path(hash, &path)?;
            }
        }
    }
    Ok(())
}

fn hash_optional_path(hash: &mut Sha256, path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bindgen_hash_part(hash, b"missing");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        bail!("NuGet bindgen SDK input is a symlink: {}", path.display());
    }
    if metadata.is_file() {
        bindgen_hash_part(hash, b"file");
        bindgen_hash_part(
            hash,
            &rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(path)?,
        );
    } else if metadata.is_dir() {
        bindgen_hash_part(hash, b"directory");
        crate::content_cache::hash_tree(path, hash)?;
    } else {
        bail!(
            "NuGet bindgen SDK input has unsupported type: {}",
            path.display()
        );
    }
    Ok(())
}

fn bindgen_hash_part(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

/// Materialize the owned SDK-selected NuGet runtime closure into `out_dir` (the directory holding
/// the just-built artifact). Legacy flat marker directories remain readable for older consumers.
/// Called from `pipeline.rs` after `artifact::locate`, before `run`/`report` — a no-op (and silent)
/// for crates that never ran `add-nuget`.
fn copy_assets_locked(crate_dir: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
    let assets_dir = crate_dir.join(".cargo-dotnet-nuget-assets");
    let mut copied = match nuget_assets::copy_staged_assets(crate_dir, out_dir)? {
        Some(paths) => paths,
        None if assets_dir.is_dir() => {
            let source = DirectoryCapability::open(&assets_dir)?;
            fs::create_dir_all(out_dir)?;
            let output = DirectoryCapability::open(out_dir)?;
            let mut paths = Vec::new();
            source.walk_regular_tree(&[], &mut |relative, node| {
                if relative.components().count() != 1 {
                    return Ok(());
                }
                let TreeWalkNode::File(file) = node else {
                    return Ok(());
                };
                let bytes = rust_dotnet_sdk_core::safe_fs::read_opened_regular(
                    file,
                    &assets_dir.join(relative),
                )?;
                let destination = output.publish_bytes(relative, &bytes).with_context(|| {
                    format!(
                        "copying legacy staged NuGet asset {} -> {}",
                        assets_dir.join(relative).display(),
                        out_dir.join(relative).display()
                    )
                })?;
                paths.push(destination);
                Ok(())
            })?;
            paths
        }
        None => Vec::new(),
    };
    fs::create_dir_all(out_dir)?;
    let output = DirectoryCapability::open(out_dir)?;
    let host_rid = crate::host::HostFacts::detect().host_rid;
    for asset in local_native_assets(crate_dir)? {
        if asset.rid.as_deref() != Some(host_rid) {
            continue;
        }
        let filename = asset
            .source
            .file_name()
            .context("vendored native asset has no filename")?;
        let destination = output
            .publish_bytes(Path::new(filename), &asset.contents)
            .with_context(|| {
                format!(
                    "copying snapshotted vendored native asset {} -> {}",
                    asset.source.display(),
                    out_dir.join(filename).display()
                )
            })?;
        copied.push(destination);
    }
    copied.sort();
    copied.dedup();
    Ok(copied)
}

/// Snapshot the exact generated reflection crate before selecting its private sysroot identity.
fn write_bindgen_crate(dll: &Path, bindgen_dir: &Path) -> Result<()> {
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
    let dll_path_esc = dll
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
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
#![allow(unused_imports, unused_must_use)]
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
    Ok(())
}

fn bindgen_context(
    bindgen_dir: &Path,
    verbose: bool,
    dotnet: crate::context::DotnetVersion,
) -> Result<Context> {
    let build_args = BuildArgs {
        path: Some(bindgen_dir.to_path_buf()),
        release: true,
        debug: false,
        clean: false,
        verbose,
        backend: None,
        dotnet: dotnet.as_env().to_string(),
        source_link_url: None,
        features: clap_cargo::Features::default(),
        manifest: clap_cargo::Manifest::default(),
        workspace: clap_cargo::Workspace::default(),
        extra: Vec::new(),
        prog_args: Vec::new(),
    };
    Context::resolve(&build_args, true)
}

/// Build and run a prepared bindgen crate with the exact private-sysroot identity already folded
/// into the content key.
fn generate_bindings_prepared(
    dll: &Path,
    bindgen_dir: &Path,
    extra_dlls: &[PathBuf],
    ctx: &Context,
    private_sysroot: &crate::private_sysroot::PrivateSysroot,
) -> Result<()> {
    overlays::apply(ctx)?;
    let json = buildstd::build_with_sysroot(ctx, private_sysroot)?;
    let art = artifact::locate(&json, ctx)?;
    crate::receipt::write(ctx, &art, private_sysroot)?;
    let Artifact::Executable(exe) = art else {
        bail!("add-nuget: bindgen crate did not produce a runnable apphost (got {art:?})");
    };

    // SDK-resolved dependency DLLs must sit next to the apphost itself — that's where the CLR's
    // default probing looks, NOT `bindgen_dir` (the
    // `cmd.current_dir` below is the ephemeral crate's SOURCE dir, unrelated to assembly
    // probing). Without this, `Assembly.LoadFrom(dll)` + `Module.GetTypes()` throws
    // `ReflectionTypeLoadException`/`FileNotFoundException` for any type that references an
    // unresolved dependency assembly, even one reflection never otherwise touches.
    if let Some(exe_dir) = exe.parent() {
        for extra in extra_dlls {
            let dest = exe_dir.join(
                extra
                    .file_name()
                    .context("add-nuget: dependency dll has no filename")?,
            );
            fs::copy(extra, &dest).with_context(|| {
                format!(
                    "add-nuget: copying dependency dll {} -> {}",
                    extra.display(),
                    dest.display()
                )
            })?;
        }
    }

    eprintln!(
        "== cargo dotnet add-nuget: running bindgen (reflecting {}) ==",
        dll.display()
    );
    let mut cmd = Command::new(&exe);
    cmd.current_dir(bindgen_dir);
    if let Some((path_add, dotnet_root)) = &ctx.dotnet_heal {
        let mut paths = vec![path_add.clone()];
        if let Some(current) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&current));
        }
        cmd.env(
            "PATH",
            std::env::join_paths(paths).context("constructing PATH for NuGet bindgen")?,
        );
        cmd.env("DOTNET_ROOT", dotnet_root);
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to run bindgen apphost {}", exe.display()))?;
    if !status.success() {
        bail!(
            "add-nuget: bindgen apphost exited with {status} — the target dll may use a \
               shape spinacz's reflect_assembly can't handle (generics-heavy, non-public API \
               surface only, etc.)"
        );
    }
    if !bindgen_dir.join("out.rs").is_file() {
        bail!(
            "add-nuget: bindgen ran but wrote no out.rs in {}",
            bindgen_dir.display()
        );
    }
    Ok(())
}

/// Resolve `mycorrhiza` from the active SDK inventory. Installed cargo-dotnet binaries must remain
/// relocatable after their build checkout disappears; only development mode uses the repo root.
fn mycorrhiza_path() -> Result<String> {
    mycorrhiza_path_for_mode(crate::mode::detect()?)
}

fn mycorrhiza_path_for_mode(mode: crate::mode::Mode) -> Result<String> {
    let mycorrhiza = match mode {
        crate::mode::Mode::Dev { repo_root } => repo_root.join("mycorrhiza"),
        crate::mode::Mode::Installed { home } => {
            let layout = crate::bundle::installed_layout(&home)?;
            home.join(layout.crates_root).join("mycorrhiza")
        }
    };
    let mycorrhiza = fs::canonicalize(&mycorrhiza).with_context(|| {
        format!(
            "add-nuget: active SDK inventory has no mycorrhiza crate at {}",
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
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
fn test_nuget_transaction_crash_point(point: &str) {
    if std::env::var_os("CARGO_DOTNET_NUGET_CRASH_POINT").as_deref()
        != Some(std::ffi::OsStr::new(point))
    {
        return;
    }
    #[cfg(unix)]
    unsafe {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        if kill(std::process::id() as i32, 9) != 0 {
            std::process::abort();
        }
        loop {
            std::hint::spin_loop();
        }
    }
    #[cfg(not(unix))]
    std::process::abort();
}

#[cfg(not(test))]
fn test_nuget_transaction_crash_point(_point: &str) {}

#[cfg(test)]
mod tests {
    use super::{
        DependencySource, NativePublishPoint, hash_bindgen_identity_parts, hash_binding_assembly,
        hash_optional_path, load_deps_manifest, mycorrhiza_path_for_mode, native_library_matches,
        record_dependency, resolved_dependency_sources, run_native_file, run_native_file_with_hook,
        staged_package_assets, with_nuget_transaction,
    };
    use sha2::Digest;

    #[test]
    fn binding_fingerprint_changes_with_assembly_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let dll = temp.path().join("Example.dll");
        std::fs::write(&dll, b"first").unwrap();
        let mut first = sha2::Sha256::new();
        hash_binding_assembly(&mut first, &dll).unwrap();
        std::fs::write(&dll, b"second").unwrap();
        let mut second = sha2::Sha256::new();
        hash_binding_assembly(&mut second, &dll).unwrap();
        assert_ne!(first.finalize(), second.finalize());
    }

    #[test]
    fn installed_bindgen_resolves_mycorrhiza_from_the_runtime_sdk_inventory() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("relocated-sdk");
        let facts = crate::host::HostFacts::detect();
        let manifest = rust_dotnet_sdk_core::sdk::SdkManifest::new(
            &facts,
            crate::mode::DEFAULT_TOOLCHAIN.into(),
            env!("CARGO_PKG_VERSION").into(),
            Vec::new(),
        );
        let layout = manifest.layout.clone().unwrap();
        let expected = home.join(&layout.crates_root).join("mycorrhiza");
        std::fs::create_dir_all(&expected).unwrap();
        std::fs::write(
            home.join(rust_dotnet_sdk_core::sdk::SDK_MANIFEST_FILE),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        assert_eq!(
            std::path::PathBuf::from(
                mycorrhiza_path_for_mode(crate::mode::Mode::Installed { home }).unwrap()
            ),
            std::fs::canonicalize(expected).unwrap()
        );
    }

    #[test]
    fn concurrent_dependency_records_preserve_every_root() {
        use std::sync::{Arc, Barrier};

        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let participants = 8;
        let barrier = Arc::new(Barrier::new(participants));
        let mut threads = Vec::new();
        for index in 0..participants {
            let crate_dir = crate_dir.clone();
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                record_dependency(
                    &crate_dir,
                    &format!("Example.Package.{index}"),
                    "1.0.0",
                    Some("linux-x64"),
                    "net10.0",
                    &[],
                )
            }));
        }
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
        assert_eq!(
            load_deps_manifest(&crate_dir).unwrap().dependencies.len(),
            participants
        );
        assert!(!crate_dir.join(".cargo-dotnet-nuget-deps.lock").exists());
    }

    fn publish_transaction_version(crate_dir: &std::path::Path, version: &str) {
        let capability =
            rust_dotnet_sdk_core::safe_fs::DirectoryCapability::open(crate_dir).unwrap();
        capability
            .publish_bytes(
                std::path::Path::new(".cargo-dotnet-nuget-assets/test-version"),
                version.as_bytes(),
            )
            .unwrap();
        capability
            .publish_bytes(
                std::path::Path::new("src/nuget/test.rs"),
                version.as_bytes(),
            )
            .unwrap();
        capability
            .publish_bytes(
                std::path::Path::new(super::DEPS_MANIFEST_FILE),
                version.as_bytes(),
            )
            .unwrap();
    }

    fn observe_transaction_version(crate_dir: &std::path::Path) -> [Vec<u8>; 3] {
        let _lease = super::acquire_project_lease(crate_dir).unwrap();
        [
            std::fs::read(crate_dir.join(".cargo-dotnet-nuget-assets/test-version")).unwrap(),
            std::fs::read(crate_dir.join("src/nuget/test.rs")).unwrap(),
            std::fs::read(crate_dir.join(super::DEPS_MANIFEST_FILE)).unwrap(),
        ]
    }

    #[test]
    fn concurrent_same_and_different_versions_publish_one_coherent_project_revision() {
        use std::sync::{Arc, Barrier};

        for versions in [["1.0.0", "1.0.0"], ["1.0.0", "2.0.0"]] {
            let temp = tempfile::tempdir().unwrap();
            let crate_dir = temp.path().join("consumer");
            std::fs::create_dir(&crate_dir).unwrap();
            let barrier = Arc::new(Barrier::new(2));
            let mut threads = Vec::new();
            for version in versions {
                let crate_dir = crate_dir.clone();
                let barrier = Arc::clone(&barrier);
                threads.push(std::thread::spawn(move || {
                    barrier.wait();
                    with_nuget_transaction(&crate_dir, || {
                        publish_transaction_version(&crate_dir, version);
                        Ok(())
                    })
                }));
            }
            for thread in threads {
                thread.join().unwrap().unwrap();
            }
            let observed = observe_transaction_version(&crate_dir);
            assert_eq!(observed[0], observed[1]);
            assert_eq!(observed[1], observed[2]);
            assert!(
                versions
                    .iter()
                    .any(|version| observed[0] == version.as_bytes())
            );
        }
    }

    #[test]
    fn offline_observer_cannot_see_assets_without_matching_bindings_and_dependency() {
        use std::sync::mpsc;
        use std::time::Duration;

        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        with_nuget_transaction(&crate_dir, || {
            publish_transaction_version(&crate_dir, "before");
            Ok(())
        })
        .unwrap();
        let (partial_tx, partial_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let writer_dir = crate_dir.clone();
        let writer = std::thread::spawn(move || {
            with_nuget_transaction(&writer_dir, || {
                let capability =
                    rust_dotnet_sdk_core::safe_fs::DirectoryCapability::open(&writer_dir)?;
                capability.publish_bytes(
                    std::path::Path::new(".cargo-dotnet-nuget-assets/test-version"),
                    b"after",
                )?;
                partial_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                publish_transaction_version(&writer_dir, "after");
                Ok(())
            })
        });
        partial_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (observed_tx, observed_rx) = mpsc::channel();
        let observer_dir = crate_dir.clone();
        let observer = std::thread::spawn(move || {
            observed_tx
                .send(observe_transaction_version(&observer_dir))
                .unwrap();
        });
        assert!(
            observed_rx
                .recv_timeout(Duration::from_millis(150))
                .is_err(),
            "offline observer crossed an in-progress NuGet transaction"
        );
        release_tx.send(()).unwrap();
        writer.join().unwrap().unwrap();
        let observed = observed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        observer.join().unwrap();
        assert_eq!(
            observed,
            [b"after".to_vec(), b"after".to_vec(), b"after".to_vec()]
        );
    }

    #[test]
    fn failed_nuget_transaction_restores_all_project_surfaces() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        with_nuget_transaction(&crate_dir, || {
            publish_transaction_version(&crate_dir, "before");
            Ok(())
        })
        .unwrap();
        let error = with_nuget_transaction(&crate_dir, || -> anyhow::Result<()> {
            publish_transaction_version(&crate_dir, "partial");
            anyhow::bail!("injected binding publication failure")
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("rolled back"));
        assert_eq!(
            observe_transaction_version(&crate_dir),
            [b"before".to_vec(), b"before".to_vec(), b"before".to_vec()]
        );
    }

    fn publish_crash_matrix_version(
        crate_dir: &std::path::Path,
        version: &str,
        crash_at: Option<&str>,
    ) -> anyhow::Result<()> {
        let capability =
            rust_dotnet_sdk_core::safe_fs::DirectoryCapability::open(crate_dir).unwrap();
        for (point, path) in [
            (
                "after-assets",
                std::path::Path::new(".cargo-dotnet-nuget-assets/test-version"),
            ),
            ("after-binding", std::path::Path::new("src/nuget/test.rs")),
            ("after-mod", std::path::Path::new("src/nuget/mod.rs")),
            (
                "after-dependencies",
                std::path::Path::new(super::DEPS_MANIFEST_FILE),
            ),
        ] {
            capability.publish_bytes(path, version.as_bytes())?;
            if crash_at == Some(point) {
                crash_process_now();
            }
        }
        Ok(())
    }

    fn observe_crash_matrix_version(crate_dir: &std::path::Path) -> [Vec<u8>; 4] {
        let _lease = super::acquire_project_lease(crate_dir).unwrap();
        [
            std::fs::read(crate_dir.join(".cargo-dotnet-nuget-assets/test-version")).unwrap(),
            std::fs::read(crate_dir.join("src/nuget/test.rs")).unwrap(),
            std::fs::read(crate_dir.join("src/nuget/mod.rs")).unwrap(),
            std::fs::read(crate_dir.join(super::DEPS_MANIFEST_FILE)).unwrap(),
        ]
    }

    fn crash_process_now() -> ! {
        #[cfg(unix)]
        unsafe {
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            if kill(std::process::id() as i32, 9) != 0 {
                std::process::abort();
            }
            loop {
                std::hint::spin_loop();
            }
        }
        #[cfg(not(unix))]
        std::process::abort()
    }

    #[test]
    fn nuget_transaction_crash_helper() {
        let Some(point) = std::env::var_os("CARGO_DOTNET_NUGET_CRASH_POINT") else {
            return;
        };
        let crate_dir =
            std::path::PathBuf::from(std::env::var_os("CARGO_DOTNET_NUGET_CRASH_CRATE").unwrap());
        if std::env::var_os("CARGO_DOTNET_NUGET_RECOVER_ONLY").is_some() {
            let _lease = super::acquire_project_lease(&crate_dir).unwrap();
            return;
        }
        with_nuget_transaction(&crate_dir, || {
            publish_crash_matrix_version(&crate_dir, "partial", point.to_str())
        })
        .unwrap();
    }

    fn run_crash_child(
        crate_dir: &std::path::Path,
        point: &str,
        recover_only: bool,
    ) -> std::process::ExitStatus {
        use std::process::{Command, Stdio};

        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "nuget::tests::nuget_transaction_crash_helper",
                "--nocapture",
            ])
            .env("CARGO_DOTNET_NUGET_CRASH_POINT", point)
            .env("CARGO_DOTNET_NUGET_CRASH_CRATE", crate_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if recover_only {
            command.env("CARGO_DOTNET_NUGET_RECOVER_ONLY", "1");
        }
        command.status().unwrap()
    }

    #[test]
    fn abnormal_exit_recovery_covers_preparation_action_and_retirement() {
        for (point, expected) in [
            ("during-residue-receipt-publication", "before"),
            ("after-residue-directory-before-binding", "before"),
            ("after-residue-receipt", "before"),
            ("after-journal-stage", "before"),
            ("after-assets", "before"),
            ("after-binding", "before"),
            ("after-mod", "before"),
            ("after-dependencies", "before"),
            ("after-journal-retire", "partial"),
            ("mid-residue-cleanup", "partial"),
            ("after-directory-quarantine", "partial"),
            ("after-directory-removal", "partial"),
            ("after-marker-quarantine", "partial"),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let crate_dir = temp.path().join("consumer");
            std::fs::create_dir(&crate_dir).unwrap();
            with_nuget_transaction(&crate_dir, || {
                publish_crash_matrix_version(&crate_dir, "before", None)
            })
            .unwrap();

            let status = run_crash_child(&crate_dir, point, false);
            assert!(!status.success(), "crash helper survived {point}");
            assert!(
                std::fs::read_dir(&crate_dir).unwrap().any(|entry| {
                    let name = entry.unwrap().file_name();
                    name.to_str().is_some_and(|name| {
                        name == super::NUGET_JOURNAL_DIRECTORY
                            || name.starts_with(super::NUGET_JOURNAL_STAGE_PREFIX)
                            || name.starts_with(super::NUGET_JOURNAL_CLEANUP_PREFIX)
                            || name.starts_with(super::NUGET_QUARANTINE_AUTHORITY_PREFIX)
                            || name.starts_with(super::NUGET_BOUND_DIRECTORY_PREFIX)
                            || name.starts_with(super::NUGET_BOUND_MARKER_PREFIX)
                            || name.starts_with(super::NUGET_CREATION_BINDING_PREFIX)
                    })
                }),
                "crash at {point} left no recoverable journal authority"
            );

            let _lease = super::acquire_project_lease(&crate_dir).unwrap();
            drop(_lease);
            assert_eq!(
                observe_crash_matrix_version(&crate_dir),
                [
                    expected.as_bytes().to_vec(),
                    expected.as_bytes().to_vec(),
                    expected.as_bytes().to_vec(),
                    expected.as_bytes().to_vec(),
                ],
                "recovery after {point} produced a mixed project revision"
            );
            assert!(!crate_dir.join(super::NUGET_JOURNAL_DIRECTORY).exists());
            for entry in std::fs::read_dir(&crate_dir).unwrap() {
                let name = entry.unwrap().file_name();
                assert!(
                    !name.to_str().is_some_and(|name| {
                        name.starts_with(".cargo-dotnet-nuget-journal-")
                            || name.starts_with(super::NUGET_QUARANTINE_AUTHORITY_PREFIX)
                            || name.starts_with(super::NUGET_BOUND_DIRECTORY_PREFIX)
                            || name.starts_with(super::NUGET_BOUND_MARKER_PREFIX)
                            || name.starts_with(super::NUGET_CREATION_BINDING_PREFIX)
                    }),
                    "recovery left NuGet journal residue after {point}"
                );
            }
        }
    }

    #[test]
    fn crash_before_receipt_publication_leaves_no_persistent_residue() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();

        assert!(
            !run_crash_child(&crate_dir, "before-residue-receipt-publication", false).success()
        );
        assert!(!std::fs::read_dir(&crate_dir).unwrap().any(|entry| {
            entry.unwrap().file_name().to_str().is_some_and(|name| {
                name.starts_with(super::NUGET_JOURNAL_STAGE_PREFIX)
                    || name.starts_with(super::NUGET_JOURNAL_CLEANUP_PREFIX)
                    || name.starts_with(super::NUGET_QUARANTINE_AUTHORITY_PREFIX)
                    || name.starts_with(super::NUGET_BOUND_DIRECTORY_PREFIX)
                    || name.starts_with(super::NUGET_BOUND_MARKER_PREFIX)
                    || name.starts_with(super::NUGET_CREATION_BINDING_PREFIX)
            })
        }));
        let _lease = super::acquire_project_lease(&crate_dir).unwrap();
    }

    #[test]
    fn successful_transaction_retires_every_residue_authority() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        super::with_nuget_transaction(&crate_dir, || Ok(())).unwrap();

        for entry in std::fs::read_dir(&crate_dir).unwrap() {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            assert!(
                name != super::NUGET_JOURNAL_DIRECTORY
                    && !name.starts_with(super::NUGET_JOURNAL_STAGE_PREFIX)
                    && !name.starts_with(super::NUGET_JOURNAL_CLEANUP_PREFIX)
                    && !name.starts_with(super::NUGET_QUARANTINE_AUTHORITY_PREFIX)
                    && !name.starts_with(super::NUGET_BOUND_DIRECTORY_PREFIX)
                    && !name.starts_with(super::NUGET_BOUND_MARKER_PREFIX)
                    && !name.starts_with(super::NUGET_CREATION_BINDING_PREFIX),
                "successful transaction leaked residue {name}"
            );
        }
    }

    #[test]
    fn retirement_move_is_no_replace_for_empty_and_nonempty_destinations() {
        for nonempty in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let crate_dir = temp.path().join("consumer");
            std::fs::create_dir(&crate_dir).unwrap();
            let snapshot = super::NugetProjectSnapshot::capture(&crate_dir).unwrap();
            let journal = super::NugetJournal::prepare(&crate_dir, &snapshot).unwrap();
            let mut destination = None;
            let error = super::retire_nuget_journal_with_hook(
                &crate_dir,
                &journal.path,
                &mut |_, cleanup| {
                    std::fs::create_dir(cleanup)?;
                    if nonempty {
                        std::fs::write(cleanup.join("foreign"), b"keep")?;
                    }
                    destination = Some(cleanup.to_path_buf());
                    Ok(())
                },
            )
            .unwrap_err();
            assert!(
                format!("{error:#}").contains("exist")
                    || format!("{error:#}").contains("destination"),
                "{error:#}"
            );
            let destination = destination.unwrap();
            assert!(destination.is_dir());
            if nonempty {
                assert_eq!(std::fs::read(destination.join("foreign")).unwrap(), b"keep");
            }
            assert!(journal.path.is_dir());
        }
    }

    #[test]
    fn retirement_move_rejects_a_source_rebind_before_move() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let snapshot = super::NugetProjectSnapshot::capture(&crate_dir).unwrap();
        let journal = super::NugetJournal::prepare(&crate_dir, &snapshot).unwrap();
        let original = crate_dir.join("journal-original");
        let error =
            super::retire_nuget_journal_with_hook(&crate_dir, &journal.path, &mut |source, _| {
                std::fs::rename(source, &original)?;
                std::fs::create_dir(source)?;
                std::fs::write(source.join("foreign"), b"keep")?;
                Ok(())
            })
            .unwrap_err();
        assert!(format!("{error:#}").contains("identity"), "{error:#}");
        assert_eq!(
            std::fs::read(journal.path.join("foreign")).unwrap(),
            b"keep"
        );
        assert!(original.join(super::NUGET_JOURNAL_RECEIPT).is_file());
    }

    #[test]
    fn retirement_keeps_validated_identity_across_copied_receipt_rebind() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let snapshot = super::NugetProjectSnapshot::capture(&crate_dir).unwrap();
        let journal = super::NugetJournal::prepare(&crate_dir, &snapshot).unwrap();
        let original = crate_dir.join("validated-journal-original");
        let _result = super::retire_nuget_journal_with_hooks(
            &crate_dir,
            &journal.path,
            &mut |source| {
                std::fs::rename(source, &original)?;
                std::fs::create_dir(source)?;
                std::fs::copy(
                    original.join(super::NUGET_JOURNAL_RECEIPT),
                    source.join(super::NUGET_JOURNAL_RECEIPT),
                )?;
                std::fs::write(source.join("foreign"), b"keep")?;
                Ok(())
            },
            &mut |_, _| Ok(()),
        );

        assert_eq!(
            std::fs::read(journal.path.join("foreign")).unwrap(),
            b"keep"
        );
        assert!(journal.path.join(super::NUGET_JOURNAL_RECEIPT).is_file());
        #[cfg(unix)]
        assert!(original.join(super::NUGET_JOURNAL_RECEIPT).is_file());
    }

    #[test]
    fn residue_and_marker_grammars_reject_near_misses() {
        let stage = format!("{}{}", super::NUGET_JOURNAL_STAGE_PREFIX, "a".repeat(32));
        assert!(super::is_nuget_residue_name(&stage));
        assert!(!super::is_nuget_residue_name(&format!("{stage}extra")));
        assert!(!super::is_nuget_residue_name(&format!(
            "{}{}",
            super::NUGET_JOURNAL_STAGE_PREFIX,
            "A".repeat(32)
        )));
        assert!(!super::is_nuget_residue_name(&format!(
            "{}{}",
            super::NUGET_JOURNAL_STAGE_PREFIX,
            "a".repeat(31)
        )));

        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let residue = crate_dir.join(stage);
        let marker = super::nuget_residue_receipt_path(&crate_dir, &residue).unwrap();
        assert_eq!(
            super::residue_name_from_receipt(&marker),
            residue.file_name().and_then(|name| name.to_str())
        );
        assert!(
            super::residue_name_from_receipt(std::path::Path::new(&format!(
                "{}extra",
                marker.file_name().unwrap().to_string_lossy()
            )))
            .is_none()
        );
    }

    #[test]
    fn repeated_crash_during_residue_cleanup_still_converges() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        with_nuget_transaction(&crate_dir, || {
            publish_crash_matrix_version(&crate_dir, "before", None)
        })
        .unwrap();

        assert!(!run_crash_child(&crate_dir, "after-journal-retire", false).success());
        assert!(!run_crash_child(&crate_dir, "mid-residue-cleanup", true).success());

        let _lease = super::acquire_project_lease(&crate_dir).unwrap();
        drop(_lease);
        assert_eq!(
            observe_crash_matrix_version(&crate_dir),
            [
                b"partial".to_vec(),
                b"partial".to_vec(),
                b"partial".to_vec(),
                b"partial".to_vec(),
            ]
        );
        assert!(!crate_dir.join(super::NUGET_JOURNAL_DIRECTORY).exists());
        assert!(!std::fs::read_dir(&crate_dir).unwrap().any(|entry| {
            entry.unwrap().file_name().to_str().is_some_and(|name| {
                name.starts_with(super::NUGET_JOURNAL_STAGE_PREFIX)
                    || name.starts_with(super::NUGET_JOURNAL_CLEANUP_PREFIX)
                    || name.starts_with(super::NUGET_QUARANTINE_AUTHORITY_PREFIX)
                    || name.starts_with(super::NUGET_BOUND_DIRECTORY_PREFIX)
                    || name.starts_with(super::NUGET_BOUND_MARKER_PREFIX)
                    || name.starts_with(super::NUGET_CREATION_BINDING_PREFIX)
            })
        }));
    }

    #[test]
    fn recovery_finishes_an_owned_directory_already_moved_to_quarantine() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let prepared =
            super::prepare_nuget_residue(&crate_dir, super::NUGET_JOURNAL_CLEANUP_PREFIX).unwrap();
        std::fs::write(prepared.path.join("payload"), b"owned").unwrap();
        let (residue, receipt) = prepared.persist();
        let root = rust_dotnet_sdk_core::safe_fs::DirectoryCapability::open(&crate_dir).unwrap();
        let (_, marker) =
            super::validate_nuget_residue_receipt(&crate_dir, &residue, &receipt).unwrap();
        let authority = super::prepare_quarantine_authority(&crate_dir, &residue, &marker).unwrap();
        let quarantined_name = super::quarantine_directory_name(&authority.authority);
        let quarantined = root
            .quarantine_subdirectory_bound(
                std::path::Path::new(residue.file_name().unwrap()),
                std::path::Path::new(&quarantined_name),
                authority.authority.directory_identity,
            )
            .unwrap();
        let quarantined_path = quarantined.path().to_path_buf();
        drop(quarantined);

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            1
        );
        assert!(!quarantined_path.exists());
        assert!(!receipt.exists());
    }

    #[test]
    fn recovery_finishes_a_valid_marker_already_moved_to_file_quarantine() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let prepared =
            super::prepare_nuget_residue(&crate_dir, super::NUGET_JOURNAL_CLEANUP_PREFIX).unwrap();
        let (residue, receipt) = prepared.persist();
        let root = rust_dotnet_sdk_core::safe_fs::DirectoryCapability::open(&crate_dir).unwrap();
        let (_, marker) =
            super::validate_nuget_residue_receipt(&crate_dir, &residue, &receipt).unwrap();
        let authority = super::prepare_quarantine_authority(&crate_dir, &residue, &marker).unwrap();
        let directory_name = super::quarantine_directory_name(&authority.authority);
        root.quarantine_subdirectory_bound(
            std::path::Path::new(residue.file_name().unwrap()),
            std::path::Path::new(&directory_name),
            authority.authority.directory_identity,
        )
        .unwrap()
        .remove()
        .unwrap();
        let marker_name = super::quarantine_marker_name(&authority.authority);
        let quarantined = root
            .quarantine_regular_bound(
                std::path::Path::new(receipt.file_name().unwrap()),
                &marker,
                std::path::Path::new(&marker_name),
                authority.authority.marker_identity,
            )
            .unwrap();
        let quarantine = crate_dir.join(marker_name);
        drop(quarantined);

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            1
        );
        assert!(!quarantine.exists());
    }

    #[test]
    fn identity_bound_recovery_preserves_rebound_source_and_quarantine() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let prepared =
            super::prepare_nuget_residue(&crate_dir, super::NUGET_JOURNAL_CLEANUP_PREFIX).unwrap();
        std::fs::write(prepared.path.join("payload"), b"owned").unwrap();
        let (residue, receipt) = prepared.persist();
        let (_, marker) =
            super::validate_nuget_residue_receipt(&crate_dir, &residue, &receipt).unwrap();
        let authority = super::prepare_quarantine_authority(&crate_dir, &residue, &marker).unwrap();
        let original = crate_dir.join("owned-original");
        std::fs::rename(&residue, &original).unwrap();
        std::fs::create_dir(&residue).unwrap();
        std::fs::write(residue.join("payload"), b"foreign-source").unwrap();

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            0
        );
        assert_eq!(
            std::fs::read(residue.join("payload")).unwrap(),
            b"foreign-source"
        );
        assert_eq!(std::fs::read(original.join("payload")).unwrap(), b"owned");
        assert!(authority.path.exists());

        std::fs::remove_dir_all(&residue).unwrap();
        std::fs::rename(&original, &residue).unwrap();
        let directory_name = super::quarantine_directory_name(&authority.authority);
        let root = rust_dotnet_sdk_core::safe_fs::DirectoryCapability::open(&crate_dir).unwrap();
        let moved = root
            .quarantine_subdirectory_bound(
                std::path::Path::new(residue.file_name().unwrap()),
                std::path::Path::new(&directory_name),
                authority.authority.directory_identity,
            )
            .unwrap();
        let moved_original = crate_dir.join("moved-original");
        std::fs::rename(moved.path(), &moved_original).unwrap();
        std::fs::create_dir(moved.path()).unwrap();
        std::fs::write(moved.path().join("payload"), b"foreign-quarantine").unwrap();
        drop(moved);

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            0
        );
        assert_eq!(
            std::fs::read(crate_dir.join(directory_name).join("payload")).unwrap(),
            b"foreign-quarantine"
        );
        assert_eq!(
            std::fs::read(moved_original.join("payload")).unwrap(),
            b"owned"
        );
        assert!(receipt.exists());
        assert!(authority.path.exists());
    }

    #[test]
    fn creation_binding_preserves_a_nonempty_swap_before_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let prepared =
            super::prepare_nuget_residue(&crate_dir, super::NUGET_JOURNAL_CLEANUP_PREFIX).unwrap();
        std::fs::write(prepared.path.join("payload"), b"owned").unwrap();
        let (residue, receipt) = prepared.persist();
        let original = crate_dir.join("creation-bound-original");
        std::fs::rename(&residue, &original).unwrap();
        std::fs::create_dir(&residue).unwrap();
        std::fs::write(residue.join("payload"), b"foreign").unwrap();

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            0
        );
        assert_eq!(std::fs::read(residue.join("payload")).unwrap(), b"foreign");
        assert_eq!(std::fs::read(original.join("payload")).unwrap(), b"owned");
        assert!(receipt.exists());
        assert!(std::fs::read_dir(&crate_dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(super::NUGET_CREATION_BINDING_PREFIX))
        }));
    }

    #[test]
    fn residue_cleanup_preserves_nonempty_directory_without_typed_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let foreign = crate_dir.join(format!(
            "{}{}",
            super::NUGET_JOURNAL_STAGE_PREFIX,
            "0".repeat(32)
        ));
        std::fs::create_dir(&foreign).unwrap();
        std::fs::write(foreign.join("user-data"), b"keep").unwrap();
        std::fs::write(
            super::nuget_residue_receipt_path(&crate_dir, &foreign).unwrap(),
            b"not an ownership receipt",
        )
        .unwrap();

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            0
        );
        assert_eq!(std::fs::read(foreign.join("user-data")).unwrap(), b"keep");
        assert_eq!(
            std::fs::read(super::nuget_residue_receipt_path(&crate_dir, &foreign).unwrap())
                .unwrap(),
            b"not an ownership receipt"
        );
    }

    #[test]
    fn copied_valid_internal_receipt_cannot_authorize_foreign_residue_deletion() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let foreign = crate_dir.join(format!(
            "{}{}",
            super::NUGET_JOURNAL_CLEANUP_PREFIX,
            "1".repeat(32)
        ));
        std::fs::create_dir(&foreign).unwrap();
        std::fs::write(foreign.join("user-data"), b"keep").unwrap();
        std::fs::write(
            foreign.join(super::NUGET_JOURNAL_RECEIPT),
            serde_json::to_vec_pretty(&super::expected_nuget_journal_receipt(&crate_dir).unwrap())
                .unwrap(),
        )
        .unwrap();

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            0
        );
        assert_eq!(std::fs::read(foreign.join("user-data")).unwrap(), b"keep");
        assert!(foreign.join(super::NUGET_JOURNAL_RECEIPT).is_file());
        assert!(
            !super::nuget_residue_receipt_path(&crate_dir, &foreign)
                .unwrap()
                .exists()
        );
    }

    #[test]
    fn residue_cleanup_leaves_unauthenticated_empty_directory_nonblocking() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let foreign = crate_dir.join(format!("{}empty", super::NUGET_JOURNAL_STAGE_PREFIX));
        std::fs::create_dir(&foreign).unwrap();

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            0
        );
        assert!(foreign.is_dir());
        assert!(super::acquire_project_lease(&crate_dir).is_ok());
    }

    #[test]
    fn residue_cleanup_leaves_invalid_orphaned_sidecar_nonblocking() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let residue = crate_dir.join(format!(
            "{}{}",
            super::NUGET_JOURNAL_CLEANUP_PREFIX,
            "2".repeat(32)
        ));
        let sidecar = super::nuget_residue_receipt_path(&crate_dir, &residue).unwrap();
        std::fs::write(&sidecar, b"foreign sidecar").unwrap();

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            0
        );
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"foreign sidecar");
        assert!(super::acquire_project_lease(&crate_dir).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn residue_cleanup_leaves_unauthenticated_matching_symlink_nonblocking() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&crate_dir).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), b"keep").unwrap();
        symlink(
            &outside,
            crate_dir.join(format!("{}link", super::NUGET_JOURNAL_CLEANUP_PREFIX)),
        )
        .unwrap();

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            0
        );
        assert_eq!(std::fs::read(outside.join("sentinel")).unwrap(), b"keep");
        assert!(super::acquire_project_lease(&crate_dir).is_ok());
    }

    #[test]
    fn residue_cleanup_leaves_unauthenticated_matching_file_nonblocking() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let foreign = crate_dir.join(format!("{}file", super::NUGET_JOURNAL_STAGE_PREFIX));
        std::fs::write(&foreign, b"keep").unwrap();

        assert_eq!(
            super::cleanup_nuget_journal_residue_locked(&crate_dir).unwrap(),
            0
        );
        assert_eq!(std::fs::read(&foreign).unwrap(), b"keep");
        assert!(super::acquire_project_lease(&crate_dir).is_ok());
    }

    #[test]
    fn build_reader_waits_for_complete_nuget_transaction() {
        use std::sync::mpsc;
        use std::time::Duration;

        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        let (writer_started_tx, writer_started_rx) = mpsc::channel();
        let (release_writer_tx, release_writer_rx) = mpsc::channel();
        let writer_root = crate_dir.clone();
        let writer = std::thread::spawn(move || {
            with_nuget_transaction(&writer_root, || {
                writer_started_tx.send(()).unwrap();
                release_writer_rx.recv().unwrap();
                Ok(())
            })
        });
        writer_started_rx.recv().unwrap();

        let (reader_ready_tx, reader_ready_rx) = mpsc::channel();
        let (reader_acquired_tx, reader_acquired_rx) = mpsc::channel();
        let reader_root = crate_dir.clone();
        let reader = std::thread::spawn(move || {
            reader_ready_tx.send(()).unwrap();
            let lease = super::acquire_project_lease(&reader_root).unwrap();
            reader_acquired_tx.send(()).unwrap();
            lease.staged_package_assets().unwrap()
        });
        reader_ready_rx.recv().unwrap();
        assert!(
            reader_acquired_rx
                .recv_timeout(Duration::from_millis(150))
                .is_err(),
            "representative build reader crossed an in-progress NuGet transaction"
        );
        release_writer_tx.send(()).unwrap();
        reader_acquired_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        writer.join().unwrap().unwrap();
        assert!(reader.join().unwrap().is_empty());
    }

    #[test]
    fn bindgen_identity_changes_with_reflector_rustc_and_pal_inputs() {
        fn identity(reflector: &[u8], private_sysroot: &[u8]) -> Vec<u8> {
            let mut hash = sha2::Sha256::new();
            hash_bindgen_identity_parts(
                &mut hash,
                b"restore",
                b"dotnet",
                b"nightly",
                b"0.0.2",
                private_sysroot,
                b"payload",
                b"nuget-tooling",
                reflector,
            );
            hash.finalize().to_vec()
        }
        assert_ne!(
            identity(b"reflect-a", b"rustc-a"),
            identity(b"reflect-b", b"rustc-a")
        );
        assert_ne!(
            identity(b"reflect-a", b"rustc-a"),
            identity(b"reflect-a", b"rustc-b")
        );

        let temp = tempfile::tempdir().unwrap();
        let pal = temp.path().join("pal");
        std::fs::create_dir(&pal).unwrap();
        std::fs::write(pal.join("mod.rs"), b"first").unwrap();
        let mut first = sha2::Sha256::new();
        hash_optional_path(&mut first, &pal).unwrap();
        std::fs::write(pal.join("mod.rs"), b"second").unwrap();
        let mut second = sha2::Sha256::new();
        hash_optional_path(&mut second, &pal).unwrap();
        assert_ne!(first.finalize(), second.finalize());
    }

    #[test]
    fn logical_pinvoke_name_matches_platform_library_filenames() {
        assert!(native_library_matches(
            "e_sqlite3",
            std::path::Path::new("libe_sqlite3.dylib")
        ));
        assert!(native_library_matches(
            "e_sqlite3",
            std::path::Path::new("libe_sqlite3.so.0")
        ));
        assert!(native_library_matches(
            "e_sqlite3",
            std::path::Path::new("e_sqlite3.dll")
        ));
        assert!(!native_library_matches(
            "sqlite3",
            std::path::Path::new("e_sqlite3.dll")
        ));
    }

    #[test]
    fn local_native_file_is_vendored_and_projected_to_its_rid() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir_all(&crate_dir).unwrap();
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        let source = temp.path().join("libsample.so");
        std::fs::write(&source, b"native").unwrap();
        run_native_file(&crate::cli::AddNativeFileArgs {
            file: source,
            library: "sample".into(),
            path: Some(crate_dir.clone()),
            rid: Some("linux-x64".into()),
        })
        .unwrap();
        let assets = staged_package_assets(&crate_dir).unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(
            assets[0].logical_path,
            "runtimes/linux-x64/native/libsample.so"
        );
        assert_eq!(std::fs::read(&assets[0].source).unwrap(), b"native");
    }

    #[test]
    fn concurrent_local_native_updates_preserve_every_library_and_rid() {
        use std::sync::{Arc, Barrier};

        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        std::fs::create_dir(&crate_dir).unwrap();
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        let participants = 8;
        let barrier = Arc::new(Barrier::new(participants));
        let mut threads = Vec::new();
        for index in 0..participants {
            let library = format!("sample{index}");
            let source = temp.path().join(format!("lib{library}.so"));
            std::fs::write(&source, format!("native-{index}")).unwrap();
            let crate_dir = crate_dir.clone();
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                run_native_file(&crate::cli::AddNativeFileArgs {
                    file: source,
                    library,
                    path: Some(crate_dir),
                    rid: Some(if index % 2 == 0 {
                        "linux-x64".into()
                    } else {
                        "linux-arm64".into()
                    }),
                })
            }));
        }
        for thread in threads {
            thread.join().unwrap().unwrap();
        }

        let bytes = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(
            &crate_dir.join(super::LOCAL_NATIVE_MANIFEST_FILE),
        )
        .unwrap();
        let manifest: super::LocalNativeManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(manifest.libraries.len(), participants);
        for (library, by_rid) in manifest.libraries {
            assert_eq!(by_rid.len(), 1);
            let relative = by_rid.into_values().next().unwrap();
            assert!(crate_dir.join(relative).is_file(), "missing {library}");
        }
    }

    #[test]
    fn local_native_transaction_rolls_back_after_file_or_manifest_publication() {
        for failure in [NativePublishPoint::File, NativePublishPoint::Manifest] {
            let temp = tempfile::tempdir().unwrap();
            let crate_dir = temp.path().join("consumer");
            std::fs::create_dir(&crate_dir).unwrap();
            std::fs::write(
                crate_dir.join("Cargo.toml"),
                "[package]\nname='x'\nversion='0.1.0'\n",
            )
            .unwrap();
            let first = temp.path().join("libsample.so");
            std::fs::write(&first, b"before").unwrap();
            let args = crate::cli::AddNativeFileArgs {
                file: first,
                library: "sample".into(),
                path: Some(crate_dir.clone()),
                rid: Some("linux-x64".into()),
            };
            run_native_file(&args).unwrap();
            let manifest_before =
                std::fs::read(crate_dir.join(super::LOCAL_NATIVE_MANIFEST_FILE)).unwrap();
            let second = temp.path().join("replacement/libsample.so");
            std::fs::create_dir(second.parent().unwrap()).unwrap();
            std::fs::write(&second, b"after").unwrap();
            let args = crate::cli::AddNativeFileArgs {
                file: second,
                ..args
            };
            let error = run_native_file_with_hook(&args, &mut |point| {
                if point == failure {
                    anyhow::bail!("injected native publication failure")
                }
                Ok(())
            })
            .unwrap_err();
            assert!(format!("{error:#}").contains("rolled back"), "{error:#}");
            assert_eq!(
                std::fs::read(crate_dir.join("native/linux-x64/libsample.so")).unwrap(),
                b"before"
            );
            assert_eq!(
                std::fs::read(crate_dir.join(super::LOCAL_NATIVE_MANIFEST_FILE)).unwrap(),
                manifest_before
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn local_native_publication_never_follows_destination_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&crate_dir).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        let source = temp.path().join("libsample.so");
        std::fs::write(&source, b"native").unwrap();
        let destination_dir = crate_dir.join("native/linux-x64");
        std::fs::create_dir_all(&destination_dir).unwrap();
        let sentinel = outside.join("sentinel");
        std::fs::write(&sentinel, b"keep").unwrap();
        symlink(&sentinel, destination_dir.join("libsample.so")).unwrap();

        run_native_file(&crate::cli::AddNativeFileArgs {
            file: source,
            library: "sample".into(),
            path: Some(crate_dir.clone()),
            rid: Some("linux-x64".into()),
        })
        .unwrap();
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
        assert_eq!(
            std::fs::read(destination_dir.join("libsample.so")).unwrap(),
            b"native"
        );

        let linked_crate = temp.path().join("linked-consumer");
        std::fs::create_dir(&linked_crate).unwrap();
        std::fs::write(
            linked_crate.join("Cargo.toml"),
            "[package]\nname='y'\nversion='0.1.0'\n",
        )
        .unwrap();
        symlink(&outside, linked_crate.join("native")).unwrap();
        let other = temp.path().join("libother.so");
        std::fs::write(&other, b"other").unwrap();
        assert!(
            run_native_file(&crate::cli::AddNativeFileArgs {
                file: other,
                library: "other".into(),
                path: Some(linked_crate),
                rid: Some("linux-x64".into()),
            })
            .is_err()
        );
        assert!(!outside.join("linux-x64/libother.so").exists());
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn runtime_asset_copy_replaces_output_symlinks_without_touching_targets() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("consumer");
        let assets_dir = crate_dir.join(".cargo-dotnet-nuget-assets");
        let output = temp.path().join("output");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::create_dir(&output).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(assets_dir.join("Legacy.dll"), b"managed").unwrap();
        let managed_sentinel = outside.join("managed-sentinel");
        std::fs::write(&managed_sentinel, b"keep-managed").unwrap();
        symlink(&managed_sentinel, output.join("Legacy.dll")).unwrap();

        let native_name = if cfg!(target_os = "windows") {
            "sample.dll"
        } else if cfg!(target_os = "macos") {
            "libsample.dylib"
        } else {
            "libsample.so"
        };
        let native_source = temp.path().join(native_name);
        std::fs::write(&native_source, b"native").unwrap();
        run_native_file(&crate::cli::AddNativeFileArgs {
            file: native_source,
            library: "sample".into(),
            path: Some(crate_dir.clone()),
            rid: Some(crate::host::HostFacts::detect().host_rid.into()),
        })
        .unwrap();
        let native_sentinel = outside.join("native-sentinel");
        std::fs::write(&native_sentinel, b"keep-native").unwrap();
        symlink(&native_sentinel, output.join(native_name)).unwrap();

        super::acquire_project_lease(&crate_dir)
            .unwrap()
            .copy_assets(&output)
            .unwrap();

        assert_eq!(std::fs::read(&managed_sentinel).unwrap(), b"keep-managed");
        assert_eq!(std::fs::read(&native_sentinel).unwrap(), b"keep-native");
        assert_eq!(
            std::fs::read(output.join("Legacy.dll")).unwrap(),
            b"managed"
        );
        assert_eq!(std::fs::read(output.join(native_name)).unwrap(), b"native");
        assert!(
            !std::fs::symlink_metadata(output.join("Legacy.dll"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            !std::fs::symlink_metadata(output.join(native_name))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn versioned_dependency_manifest_rebinds_project_local_feed_in_fresh_clone() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original");
        let clone = temp.path().join("clone");
        for crate_dir in [&original, &clone] {
            std::fs::create_dir_all(crate_dir.join("feeds/private")).unwrap();
            std::fs::write(
                crate_dir.join("Cargo.toml"),
                "[package]\nname='consumer'\nversion='0.1.0'\n",
            )
            .unwrap();
        }
        record_dependency(
            &original,
            "Example.Private",
            "1.2.3",
            Some("linux-x64"),
            "net10.0",
            &[original
                .join("feeds/private")
                .to_string_lossy()
                .into_owned()],
        )
        .unwrap();
        std::fs::copy(
            original.join(super::DEPS_MANIFEST_FILE),
            clone.join(super::DEPS_MANIFEST_FILE),
        )
        .unwrap();

        let manifest = load_deps_manifest(&clone).unwrap();
        let record = manifest.dependencies.get("Example.Private").unwrap();
        assert_eq!(record.rid.as_deref(), Some("linux-x64"));
        assert_eq!(record.tfm, "net10.0");
        assert_eq!(
            record.sources,
            vec![DependencySource::CratePath {
                path: "feeds/private".into()
            }]
        );
        assert_eq!(
            resolved_dependency_sources(&clone, &record.sources).unwrap(),
            vec![
                std::fs::canonicalize(clone.join("feeds/private"))
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            ]
        );
    }

    #[test]
    fn dependency_manifest_reads_legacy_shape_and_rejects_secret_source_urls() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join(super::DEPS_MANIFEST_FILE),
            r#"{"Example":"1.0.0"}"#,
        )
        .unwrap();
        let manifest = load_deps_manifest(temp.path()).unwrap();
        let legacy = manifest.dependencies.get("Example").unwrap();
        assert_eq!(legacy.version, "1.0.0");
        assert_eq!(legacy.sources, vec![DependencySource::Default]);

        assert!(
            record_dependency(
                temp.path(),
                "Example",
                "1.0.0",
                None,
                "net10.0",
                &["https://user:secret@private.invalid/v3/index.json".into()],
            )
            .is_err()
        );
        assert!(
            record_dependency(
                temp.path(),
                "Example",
                "1.0.0",
                None,
                "net10.0",
                &["https://private.invalid/v3/index.json?token=secret".into()],
            )
            .is_err()
        );
    }
}
