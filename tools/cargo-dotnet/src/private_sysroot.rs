//! Content-addressed private sysroot provisioning for native builds.
//!
//! Warm builds compare a per-file identity/stat index against the ambient toolchain, hashing only
//! changed file contents. The expensive whole-tree byte proof is created once and remains
//! available explicitly through `cargo dotnet doctor --full-integrity`.
//! The root `share/` tree is deliberately outside that payload: rustup uses it for HTML/manual
//! documentation and shell completions, while rustc/build-std resolve executable inputs from
//! `bin/`, `lib/`, and `libexec/`. Copying almost a gigabyte of docs into every content-addressed
//! PAL snapshot adds no compiler authority and turns each backend revision into minutes of I/O.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use rust_dotnet_sdk_core::safe_fs::{DirectoryCapability, TreeWalkNode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::context::Context;

const SYSROOT_CACHE_SCHEMA: u32 = 6;
const INPUT_INDEX_SCHEMA: u32 = 3;
const SYSROOT_CACHE_LIMIT: usize = 4;
const INPUT_INDEX_LIMIT: usize = 8;
const READY_BYTES: &[u8] = b"cargo-dotnet-private-sysroot-v6\n";
const LIBRARY_PATH: &str = "lib/rustlib/src/rust/library";
const AMBIENT_EXCLUDED_ROOT_ENTRIES: &[&str] = &["share"];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct SysrootReceipt {
    schema: u32,
    key: String,
    ambient_index_key: String,
    ambient_payload_sha256: String,
    pal_payload_sha256: String,
    library: String,
    published_payload_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct InputIndexReceipt {
    schema: u32,
    key: String,
    root: String,
    payload_sha256: String,
    index_sha256: String,
    entry_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TreeEntryKind {
    Directory,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TreeEntry {
    path: String,
    kind: TreeEntryKind,
    bytes: u64,
    modified_ns: u128,
    file_id: String,
    executable: bool,
    sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TreeIndex {
    schema: u32,
    root: String,
    entries: Vec<TreeEntry>,
    payload_sha256: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ScanStats {
    files_hashed: usize,
    files_reused: usize,
}

struct AmbientIndex {
    receipt: InputIndexReceipt,
    object: crate::content_cache::CacheObject,
}

impl AmbientIndex {
    fn load(&self) -> Result<TreeIndex> {
        let (receipt, index) = load_input_index_object(self.object.path(), &self.receipt.key)?;
        if receipt != self.receipt {
            bail!("private-sysroot input index does not match its typed receipt");
        }
        Ok(index)
    }
}

pub struct PrivateSysroot {
    pub root: PathBuf,
    pub library: PathBuf,
    pub(crate) key: String,
    pub(crate) payload_sha256: String,
    _lease: crate::content_cache::CacheObject,
}

pub fn prepare(ctx: &Context) -> Result<PrivateSysroot> {
    let ambient = canonical_regular_directory(&ctx.rustc_sysroot()?, "ambient rustc sysroot")?;
    let ambient_library = ambient.join(LIBRARY_PATH);
    if !regular_directory(&ambient_library) {
        bail!("rust-src not found at {}", ambient_library.display());
    }

    let ambient_index = materialize_ambient_index(ctx, &ambient)?;
    // PAL is tiny compared with a toolchain sysroot, so indexing it each invocation keeps local
    // source edits naturally invalidating. Use the same relocation-stable payload schema as the
    // owned clone below; comparing unrelated tree-digest schemas would reject every cold miss.
    let (pal_index, _) = build_tree_index(&ctx.paths.pal_root, None, &[])?;
    let pal_payload_sha256 = pal_index.payload_sha256;
    let key = snapshot_key(ctx, &ambient_index.receipt, &pal_payload_sha256)?;
    let store = crate::content_cache::ContentStore::new(store_root()?, SYSROOT_CACHE_LIMIT)?;
    let (root, _) = store.materialize(
        &key,
        |root| validate_snapshot_fast(root, &key),
        |tmp| {
            let expected_ambient = ambient_index.load()?;
            let cloned_ambient = clone_tree_indexed_excluding(
                &ambient,
                tmp,
                AMBIENT_EXCLUDED_ROOT_ENTRIES,
            )?;
            if cloned_ambient.payload_sha256 != expected_ambient.payload_sha256 {
                bail!(
                    "ambient rustc sysroot changed while it was cloned; retry so cargo-dotnet can build a fresh content key"
                );
            }

            let pal_area = tempfile::Builder::new()
                .prefix("cargo-dotnet-pal-snapshot-")
                .tempdir()?;
            let pal_snapshot = pal_area.path().join("pal");
            let cloned_pal = clone_tree_indexed(&ctx.paths.pal_root, &pal_snapshot)?;
            if cloned_pal.payload_sha256 != pal_payload_sha256 {
                bail!(
                    "PAL sources changed while they were snapshotted; retry so cargo-dotnet can build a fresh content key"
                );
            }

            let tmp_library = tmp.join(LIBRARY_PATH);
            crate::palinject::inject_all_from(&pal_snapshot, ctx.flags.verbose, &tmp_library)?;
            // Reuse hashes for all clone-identical files; only PAL-mutated leaves are reread.
            let (published, _) = build_tree_index(tmp, Some(&cloned_ambient), &[])?;
            let receipt = SysrootReceipt {
                schema: SYSROOT_CACHE_SCHEMA,
                key: key.clone(),
                ambient_index_key: ambient_index.receipt.key.clone(),
                ambient_payload_sha256: ambient_index.receipt.payload_sha256.clone(),
                pal_payload_sha256: pal_payload_sha256.clone(),
                library: LIBRARY_PATH.into(),
                published_payload_sha256: published.payload_sha256,
            };
            write_new_json(&tmp.join("receipt.json"), &receipt)?;
            write_new(&tmp.join("READY"), READY_BYTES)?;
            Ok(())
        },
    )?;
    let receipt = read_sysroot_receipt(root.path())?;
    let root_path = root.path().to_path_buf();
    let library = root_path.join(LIBRARY_PATH);
    if !regular_directory(&library) {
        bail!("private sysroot is incomplete: {}", library.display());
    }
    Ok(PrivateSysroot {
        root: root_path,
        library,
        key: receipt.key,
        payload_sha256: receipt.published_payload_sha256,
        _lease: root,
    })
}

fn store_root() -> Result<PathBuf> {
    Ok(crate::context::cargo_dotnet_cache_home()?.join("sysroots/v6"))
}

fn input_index_store_root() -> Result<PathBuf> {
    Ok(crate::context::cargo_dotnet_cache_home()?.join("sysroots/input-index/v3"))
}

fn materialize_ambient_index(ctx: &Context, ambient: &Path) -> Result<AmbientIndex> {
    let key = ambient_identity_key(ctx, ambient)?;
    materialize_ambient_index_at(input_index_store_root()?, key, ambient)
}

fn materialize_ambient_index_at(
    store_root: PathBuf,
    key: String,
    ambient: &Path,
) -> Result<AmbientIndex> {
    let store = crate::content_cache::ContentStore::new(store_root, INPUT_INDEX_LIMIT)?;
    let refreshed = RefCell::<Option<TreeIndex>>::new(None);
    let (object, _) = store.materialize(
        &key,
        |root| validate_input_index_current(root, &key, ambient, &refreshed),
        |tmp| {
            let index = refreshed.borrow_mut().take().map(Ok).unwrap_or_else(|| {
                build_tree_index(ambient, None, AMBIENT_EXCLUDED_ROOT_ENTRIES).map(|value| value.0)
            })?;
            let index_bytes = serde_json::to_vec_pretty(&index)?;
            let receipt = InputIndexReceipt {
                schema: INPUT_INDEX_SCHEMA,
                key: key.clone(),
                root: ambient.to_string_lossy().into_owned(),
                payload_sha256: index.payload_sha256.clone(),
                index_sha256: format!("{:x}", Sha256::digest(&index_bytes)),
                entry_count: index.entries.len() as u64,
            };
            write_new(&tmp.join("index.json"), &index_bytes)?;
            write_new_json(&tmp.join("receipt.json"), &receipt)
        },
    )?;
    let receipt = read_input_receipt(object.path())?;
    Ok(AmbientIndex { receipt, object })
}

fn validate_input_index_current(
    root: &Path,
    key: &str,
    ambient: &Path,
    refreshed: &RefCell<Option<TreeIndex>>,
) -> Result<bool> {
    if !regular_file(&root.join("index.json")) || !regular_file(&root.join("receipt.json")) {
        return Ok(false);
    }
    let Ok((receipt, previous)) = load_input_index_object(root, key) else {
        return Ok(false);
    };
    if Path::new(&receipt.root) != ambient {
        return Ok(false);
    }
    let current = {
        let refreshed = refreshed.borrow();
        let prior = refreshed.as_ref().unwrap_or(&previous);
        build_tree_index(ambient, Some(prior), AMBIENT_EXCLUDED_ROOT_ENTRIES)?.0
    };
    if current.payload_sha256 != receipt.payload_sha256 {
        *refreshed.borrow_mut() = Some(current);
        return Ok(false);
    }
    Ok(true)
}

fn validate_snapshot_fast(root: &Path, key: &str) -> Result<bool> {
    let library = root.join(LIBRARY_PATH);
    if !regular_file(&root.join("READY"))
        || !regular_file(&root.join("receipt.json"))
        || !regular_directory(&library)
    {
        return Ok(false);
    }
    if rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&root.join("READY"))? != READY_BYTES {
        return Ok(false);
    }
    let Ok(receipt) = read_sysroot_receipt(root) else {
        return Ok(false);
    };
    Ok(receipt.schema == SYSROOT_CACHE_SCHEMA
        && receipt.key == key
        && receipt.library == LIBRARY_PATH
        && valid_digest(&receipt.ambient_index_key)
        && valid_digest(&receipt.ambient_payload_sha256)
        && valid_digest(&receipt.pal_payload_sha256)
        && valid_digest(&receipt.published_payload_sha256))
}

fn validate_snapshot_full(root: &Path, key: &str) -> Result<bool> {
    if !validate_snapshot_fast(root, key)? {
        return Ok(false);
    }
    let receipt = read_sysroot_receipt(root)?;
    let (actual, _) = build_tree_index(root, None, &["READY", "receipt.json"])?;
    Ok(actual.payload_sha256 == receipt.published_payload_sha256)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FullIntegrityReport {
    pub sysroots: usize,
    pub ambient_inputs: usize,
    pub stale_ambient_inputs: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AmbientInputAudit {
    Active,
    Stale,
}

/// Explicit expensive integrity path used by doctor. Normal builds deliberately avoid this scan.
pub(crate) fn verify_all_cached_full() -> Result<FullIntegrityReport> {
    let mut report = FullIntegrityReport::default();
    let sysroots = crate::content_cache::ContentStore::new(store_root()?, SYSROOT_CACHE_LIMIT)?;
    for key in sysroots.object_keys()? {
        let object = sysroots
            .open_existing(&key, |root| validate_snapshot_full(root, &key))?
            .with_context(|| {
                format!("private sysroot cache object failed full integrity: {key}")
            })?;
        // Keep the lease alive through the accounting point so a concurrent GC cannot make a
        // successful check refer to an object that disappeared mid-read.
        report.sysroots += usize::from(object.path().is_dir());
    }

    let trusted_roots = trusted_rustc_sysroots()?;
    let inputs =
        crate::content_cache::ContentStore::new(input_index_store_root()?, INPUT_INDEX_LIMIT)?;
    for key in inputs.object_keys()? {
        let object = inputs
            .open_existing(&key, |root| Ok(load_input_index_object(root, &key).is_ok()))?
            .with_context(|| {
                format!("ambient-sysroot input index failed typed receipt validation: {key}")
            })?;
        match audit_ambient_input_full(object.path(), &key, &trusted_roots)? {
            AmbientInputAudit::Active => report.ambient_inputs += 1,
            AmbientInputAudit::Stale => report.stale_ambient_inputs += 1,
        }
    }
    Ok(report)
}

fn audit_ambient_input_full(
    object: &Path,
    key: &str,
    trusted_roots: &BTreeSet<PathBuf>,
) -> Result<AmbientInputAudit> {
    let (receipt, _sealed) = load_input_index_object(object, key)?;
    let root = PathBuf::from(&receipt.root);
    let Ok(canonical_root) = canonical_regular_directory(&root, "ambient sysroot receipt root")
    else {
        return Ok(AmbientInputAudit::Stale);
    };
    if canonical_root != root || !trusted_roots.contains(&canonical_root) {
        // Never follow a mutable cache receipt to an arbitrary absolute path. Old toolchains
        // legitimately leave sealed indexes behind after uninstall/move; report those as stale
        // without treating their pathname as current authority.
        return Ok(AmbientInputAudit::Stale);
    }
    // Full integrity means full bytes: do not reuse hashes from the very index being audited.
    let (actual, _) = build_tree_index(&canonical_root, None, &[])?;
    if actual.payload_sha256 != receipt.payload_sha256 {
        bail!(
            "ambient rustc sysroot changed since its content index was sealed: {}",
            canonical_root.display()
        );
    }
    Ok(AmbientInputAudit::Active)
}

fn trusted_rustc_sysroots() -> Result<BTreeSet<PathBuf>> {
    fn query(toolchain: Option<&str>) -> Option<PathBuf> {
        let mut command = std::process::Command::new("rustc");
        if let Some(toolchain) = toolchain {
            command.env("RUSTUP_TOOLCHAIN", toolchain);
        }
        let output = command.args(["--print", "sysroot"]).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
        canonical_regular_directory(&path, "installed rustc sysroot").ok()
    }

    let mut roots = BTreeSet::new();
    if let Some(root) = query(None) {
        roots.insert(root);
    }
    if let Ok(output) = std::process::Command::new("rustup")
        .args(["toolchain", "list"])
        .output()
        && output.status.success()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some(toolchain) = line.split_whitespace().next()
                && let Some(root) = query(Some(toolchain))
            {
                roots.insert(root);
            }
        }
    }
    if roots.is_empty() {
        bail!("could not resolve any trusted rustc sysroot for full integrity checking");
    }
    Ok(roots)
}

fn ambient_identity_key(ctx: &Context, ambient: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    hash_part(&mut hash, b"cargo-dotnet-ambient-sysroot-index-v2");
    hash_part(&mut hash, ambient.as_os_str().as_encoded_bytes());
    hash_part(&mut hash, &rustc_identity(ctx)?);
    hash_part(&mut hash, ctx.toolchain.as_deref().unwrap_or("").as_bytes());
    Ok(format!("{:x}", hash.finalize()))
}

fn snapshot_key(
    ctx: &Context,
    ambient: &InputIndexReceipt,
    pal_payload_sha256: &str,
) -> Result<String> {
    let mut hash = Sha256::new();
    hash_part(&mut hash, b"cargo-dotnet-private-sysroot-v6");
    for value in [
        ctx.host.os,
        ctx.host.arch,
        ctx.host.host_rid,
        ctx.toolchain.as_deref().unwrap_or(""),
        &ambient.key,
        &ambient.payload_sha256,
        pal_payload_sha256,
        env!("CARGO_PKG_VERSION"),
    ] {
        hash_part(&mut hash, value.as_bytes());
    }
    hash_part(
        &mut hash,
        &rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&ctx.paths.target_spec)
            .context("reading target specification")?,
    );
    hash_part(&mut hash, include_bytes!("palinject.rs"));
    Ok(format!("{:x}", hash.finalize()))
}

fn rustc_identity(ctx: &Context) -> Result<Vec<u8>> {
    let mut rustc = std::process::Command::new("rustc");
    if let Some(toolchain) = &ctx.toolchain {
        rustc.env("RUSTUP_TOOLCHAIN", toolchain);
    }
    let output = rustc
        .arg("-Vv")
        .output()
        .context("query rustc -Vv for private sysroot identity")?;
    if !output.status.success() {
        bail!("rustc -Vv failed while identifying private sysroot");
    }
    Ok(output.stdout)
}

fn hash_part(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn build_tree_index(
    root: &Path,
    previous: Option<&TreeIndex>,
    excluded_root_entries: &[&str],
) -> Result<(TreeIndex, ScanStats)> {
    let root = canonical_regular_directory(root, "tree index root")?;
    let capability = DirectoryCapability::open(&root)?;
    let previous = previous
        .into_iter()
        .flat_map(|index| index.entries.iter())
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut entries = Vec::new();
    let mut stats = ScanStats::default();
    capability.walk_regular_tree(excluded_root_entries, &mut |relative_path, node| {
        let relative = portable_relative_path(relative_path)?;
        let display = root.join(relative_path);
        match node {
            TreeWalkNode::DirectoryEnter(directory) => {
                let metadata = directory.metadata()?;
                entries.push(TreeEntry {
                    path: relative,
                    kind: TreeEntryKind::Directory,
                    bytes: 0,
                    modified_ns: modified_ns(&metadata),
                    file_id: rust_dotnet_sdk_core::safe_fs::opened_file_identity(directory)?,
                    executable: false,
                    sha256: String::new(),
                });
            }
            TreeWalkNode::File(input) => {
                let opened_metadata = input.metadata()?;
                let executable_bit = executable(&opened_metadata);
                let modified_timestamp = modified_ns(&opened_metadata);
                let stable_file_id = rust_dotnet_sdk_core::safe_fs::opened_file_identity(input)?;
                let reused = previous.get(relative.as_str()).filter(|prior| {
                    stat_reuse_supported(&stable_file_id)
                        && prior.kind == TreeEntryKind::File
                        && prior.bytes == opened_metadata.len()
                        && prior.modified_ns == modified_timestamp
                        && prior.file_id == stable_file_id
                        && prior.executable == executable_bit
                        && valid_digest(&prior.sha256)
                });
                let mut bytes_read = None;
                let sha256 = if let Some(prior) = reused {
                    stats.files_reused += 1;
                    prior.sha256.clone()
                } else {
                    stats.files_hashed += 1;
                    let mut hash = Sha256::new();
                    let mut buffer = vec![0_u8; 1024 * 1024];
                    let mut total = 0_u64;
                    loop {
                        let count = input.read(&mut buffer)?;
                        if count == 0 {
                            break;
                        }
                        hash.update(&buffer[..count]);
                        total += count as u64;
                    }
                    bytes_read = Some(total);
                    format!("{:x}", hash.finalize())
                };
                let after = input.metadata()?;
                if after.len() != opened_metadata.len()
                    || modified_ns(&after) != modified_timestamp
                    || rust_dotnet_sdk_core::safe_fs::opened_file_identity(input)? != stable_file_id
                    || executable(&after) != executable_bit
                    || bytes_read.is_some_and(|bytes| bytes != after.len())
                {
                    bail!(
                        "indexed file changed while it was read: {}",
                        display.display()
                    );
                }
                entries.push(TreeEntry {
                    path: relative,
                    kind: TreeEntryKind::File,
                    bytes: opened_metadata.len(),
                    modified_ns: modified_timestamp,
                    file_id: stable_file_id,
                    executable: executable_bit,
                    sha256,
                });
            }
            TreeWalkNode::DirectoryLeave(_) => {}
        }
        Ok(())
    })?;
    let payload_sha256 = index_payload_digest(&entries);
    Ok((
        TreeIndex {
            schema: INPUT_INDEX_SCHEMA,
            root: root.to_string_lossy().into_owned(),
            entries,
            payload_sha256,
        },
        stats,
    ))
}

fn clone_tree_indexed(source: &Path, destination: &Path) -> Result<TreeIndex> {
    clone_tree_indexed_excluding(source, destination, &[])
}

fn clone_tree_indexed_excluding(
    source: &Path,
    destination: &Path,
    excluded_root_entries: &[&str],
) -> Result<TreeIndex> {
    let source = canonical_regular_directory(source, "snapshot source")?;
    let capability = DirectoryCapability::open(&source)?;
    match fs::symlink_metadata(destination) {
        Ok(metadata)
            if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata)
                || !metadata.is_dir() =>
        {
            bail!(
                "snapshot destination is not a regular directory: {}",
                destination.display()
            )
        }
        Ok(_) if fs::read_dir(destination)?.next().is_some() => {
            bail!(
                "snapshot destination is not empty: {}",
                destination.display()
            )
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(destination)?,
        Err(error) => return Err(error.into()),
    }
    let destination = fs::canonicalize(destination)?;
    let mut entries = Vec::new();
    capability.walk_regular_tree(excluded_root_entries, &mut |relative_path, node| {
        let relative = portable_relative_path(relative_path)?;
        let dst = destination.join(relative_path);
        match node {
            TreeWalkNode::DirectoryEnter(directory) => {
                fs::create_dir(&dst)?;
                let metadata = directory.metadata()?;
                entries.push(TreeEntry {
                    path: relative,
                    kind: TreeEntryKind::Directory,
                    bytes: 0,
                    modified_ns: modified_ns(&metadata),
                    file_id: rust_dotnet_sdk_core::safe_fs::opened_file_identity(directory)?,
                    executable: false,
                    sha256: String::new(),
                });
            }
            TreeWalkNode::File(input) => {
                let permissions = input.metadata()?.permissions();
                let mut output = OpenOptions::new().create_new(true).write(true).open(&dst)?;
                let mut hash = Sha256::new();
                let mut bytes = 0_u64;
                let mut buffer = vec![0_u8; 1024 * 1024];
                loop {
                    let count = input.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    output.write_all(&buffer[..count])?;
                    hash.update(&buffer[..count]);
                    bytes += count as u64;
                }
                output.set_permissions(permissions)?;
                output.sync_all()?;
                let metadata = output.metadata()?;
                entries.push(TreeEntry {
                    path: relative,
                    kind: TreeEntryKind::File,
                    bytes,
                    modified_ns: modified_ns(&metadata),
                    file_id: file_id(&metadata).unwrap_or_default(),
                    executable: executable(&metadata),
                    sha256: format!("{:x}", hash.finalize()),
                });
            }
            TreeWalkNode::DirectoryLeave(_) => {
                if fs::canonicalize(&dst)? != dst {
                    bail!("snapshot destination directory changed during copy");
                }
            }
        }
        Ok(())
    })?;
    if fs::canonicalize(&destination)? != destination {
        bail!("snapshot destination root changed during copy");
    }
    Ok(TreeIndex {
        schema: INPUT_INDEX_SCHEMA,
        root: destination.to_string_lossy().into_owned(),
        payload_sha256: index_payload_digest(&entries),
        entries,
    })
}

fn index_payload_digest(entries: &[TreeEntry]) -> String {
    let mut hash = Sha256::new();
    hash_part(&mut hash, b"cargo-dotnet-tree-index-payload-v1");
    for entry in entries {
        hash_part(&mut hash, entry.path.as_bytes());
        hash_part(
            &mut hash,
            match entry.kind {
                TreeEntryKind::Directory => b"directory",
                TreeEntryKind::File => b"file",
            },
        );
        if entry.kind == TreeEntryKind::File {
            hash_part(&mut hash, &entry.bytes.to_le_bytes());
            hash_part(&mut hash, &[u8::from(entry.executable)]);
            hash_part(&mut hash, entry.sha256.as_bytes());
        }
    }
    format!("{:x}", hash.finalize())
}

fn portable_relative_path(relative: &Path) -> Result<String> {
    crate::path_safety::validate_relative_path(relative)?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            bail!("tree index path is not normalized: {}", relative.display());
        };
        parts.push(
            value
                .to_str()
                .with_context(|| format!("tree index path is not UTF-8: {}", relative.display()))?,
        );
    }
    Ok(parts.join("/"))
}

fn read_sysroot_receipt(root: &Path) -> Result<SysrootReceipt> {
    serde_json::from_slice(&rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(
        &root.join("receipt.json"),
    )?)
    .context("parsing private-sysroot receipt")
}

fn read_input_receipt(root: &Path) -> Result<InputIndexReceipt> {
    serde_json::from_slice(&rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(
        &root.join("receipt.json"),
    )?)
    .context("parsing ambient-sysroot index receipt")
}

fn load_input_index_object(root: &Path, key: &str) -> Result<(InputIndexReceipt, TreeIndex)> {
    let receipt = read_input_receipt(root)?;
    let index_bytes =
        rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&root.join("index.json"))?;
    let index: TreeIndex =
        serde_json::from_slice(&index_bytes).context("parsing ambient-sysroot input index")?;
    if receipt.schema != INPUT_INDEX_SCHEMA
        || receipt.key != key
        || !valid_digest(&receipt.payload_sha256)
        || !valid_digest(&receipt.index_sha256)
        || receipt.index_sha256 != format!("{:x}", Sha256::digest(&index_bytes))
        || receipt.entry_count == 0
        || index.schema != INPUT_INDEX_SCHEMA
        || index.root != receipt.root
        || index.payload_sha256 != receipt.payload_sha256
        || index.entries.len() as u64 != receipt.entry_count
        || index_payload_digest(&index.entries) != index.payload_sha256
    {
        bail!("ambient-sysroot input index does not match its typed receipt");
    }
    Ok((receipt, index))
}

fn write_new_json(path: &Path, value: &impl Serialize) -> Result<()> {
    write_new(path, &serde_json::to_vec_pretty(value)?)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && !rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata)
    })
}

fn regular_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_dir() && !rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata)
    })
}

fn canonical_regular_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspecting {label} {}", path.display()))?;
    if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        bail!("{label} is not a regular directory: {}", path.display());
    }
    fs::canonicalize(path).with_context(|| format!("resolving {label} {}", path.display()))
}

fn modified_ns(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos())
}

#[cfg(unix)]
fn file_id(metadata: &fs::Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt as _;
    Some(format!(
        "{}:{}:{}:{}",
        metadata.dev(),
        metadata.ino(),
        metadata.ctime(),
        metadata.ctime_nsec()
    ))
}

#[cfg(windows)]
fn file_id(_metadata: &fs::Metadata) -> Option<String> {
    None
}

#[cfg(not(any(unix, windows)))]
fn file_id(_metadata: &fs::Metadata) -> Option<String> {
    None
}

#[cfg(unix)]
fn stat_reuse_supported(file_id: &str) -> bool {
    !file_id.is_empty()
}

// Windows exposes stable volume/file indices, which are essential for replacement detection,
// but its ordinary metadata API does not expose NTFS ChangeTime. A writer can therefore change
// bytes and restore length/LastWriteTime on the same file identity. Rehash rather than trusting
// a poisonable stat tuple; a future USN/ChangeTime-backed index can safely restore warm reuse.
#[cfg(not(unix))]
fn stat_reuse_supported(_file_id: &str) -> bool {
    false
}

#[cfg(unix)]
fn executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
fn clone_tree(source: &Path, destination: &Path) -> Result<()> {
    clone_tree_indexed(source, destination).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_snapshot_receipt(root: &Path, key: &str) {
        let (index, _) = build_tree_index(root, None, &[]).unwrap();
        let receipt = SysrootReceipt {
            schema: SYSROOT_CACHE_SCHEMA,
            key: key.into(),
            ambient_index_key: "1".repeat(64),
            ambient_payload_sha256: "2".repeat(64),
            pal_payload_sha256: "3".repeat(64),
            library: LIBRARY_PATH.into(),
            published_payload_sha256: index.payload_sha256,
        };
        write_new_json(&root.join("receipt.json"), &receipt).unwrap();
        write_new(&root.join("READY"), READY_BYTES).unwrap();
    }

    fn write_input_index_object(object: &Path, ambient: &Path, key: &str) {
        fs::create_dir_all(object).unwrap();
        let (index, _) = build_tree_index(ambient, None, &[]).unwrap();
        let bytes = serde_json::to_vec_pretty(&index).unwrap();
        let receipt = InputIndexReceipt {
            schema: INPUT_INDEX_SCHEMA,
            key: key.into(),
            root: fs::canonicalize(ambient)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            payload_sha256: index.payload_sha256.clone(),
            index_sha256: format!("{:x}", Sha256::digest(&bytes)),
            entry_count: index.entries.len() as u64,
        };
        write_new(&object.join("index.json"), &bytes).unwrap();
        write_new_json(&object.join("receipt.json"), &receipt).unwrap();
    }

    #[test]
    fn mutable_library_files_are_copied_not_linked() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("ambient");
        let library = source.join(LIBRARY_PATH);
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("marker.rs"), "ambient").unwrap();
        let destination = temp.path().join("private");
        clone_tree(&source, &destination).unwrap();
        fs::write(destination.join(LIBRARY_PATH).join("marker.rs"), "private").unwrap();
        assert_eq!(
            fs::read_to_string(library.join("marker.rs")).unwrap(),
            "ambient"
        );
    }

    #[test]
    fn indexed_payload_digest_survives_snapshot_relocation() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("pal");
        let snapshot = temp.path().join("snapshot");
        fs::create_dir_all(source.join("sys/nested")).unwrap();
        fs::write(source.join("sys/nested/pal.rs"), b"pub fn pal() {}\n").unwrap();

        let (source_index, _) = build_tree_index(&source, None, &[]).unwrap();
        let snapshot_index = clone_tree_indexed(&source, &snapshot).unwrap();

        assert_eq!(source_index.payload_sha256, snapshot_index.payload_sha256);
        assert_eq!(source_index.entries.len(), snapshot_index.entries.len());
    }

    #[test]
    fn private_sysroot_omits_root_share_documentation() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("ambient");
        let destination = temp.path().join("private");
        fs::create_dir_all(source.join(LIBRARY_PATH)).unwrap();
        fs::create_dir_all(source.join("share/doc/rust/html")).unwrap();
        fs::write(source.join(LIBRARY_PATH).join("lib.rs"), b"compiler input").unwrap();
        fs::write(
            source.join("share/doc/rust/html/index.html"),
            b"documentation",
        )
        .unwrap();

        let (source_index, _) =
            build_tree_index(&source, None, AMBIENT_EXCLUDED_ROOT_ENTRIES).unwrap();
        let snapshot_index =
            clone_tree_indexed_excluding(&source, &destination, AMBIENT_EXCLUDED_ROOT_ENTRIES)
                .unwrap();

        assert_eq!(source_index.payload_sha256, snapshot_index.payload_sha256);
        assert!(destination.join(LIBRARY_PATH).join("lib.rs").is_file());
        assert!(!destination.join("share").exists());
        assert!(
            snapshot_index
                .entries
                .iter()
                .all(|entry| entry.path != "share" && !entry.path.starts_with("share/"))
        );
    }

    #[test]
    fn warm_validation_is_bounded_while_explicit_full_check_detects_nonlibrary_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("snapshot");
        fs::create_dir_all(root.join(LIBRARY_PATH)).unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join(LIBRARY_PATH).join("lib.rs"), b"library").unwrap();
        fs::write(root.join("bin/rustc"), b"toolchain payload").unwrap();
        write_snapshot_receipt(&root, &"a".repeat(64));

        assert!(validate_snapshot_fast(&root, &"a".repeat(64)).unwrap());
        assert!(validate_snapshot_full(&root, &"a".repeat(64)).unwrap());
        fs::write(root.join("bin/rustc"), b"corrupt").unwrap();
        // No 2 GiB payload walk is hidden in the normal hit path.
        assert!(validate_snapshot_fast(&root, &"a".repeat(64)).unwrap());
        assert!(!validate_snapshot_full(&root, &"a".repeat(64)).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn stat_index_reuses_unchanged_hashes_and_invalidates_one_changed_file() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("a"), b"a").unwrap();
        fs::write(temp.path().join("b"), b"b").unwrap();
        let (first, first_stats) = build_tree_index(temp.path(), None, &[]).unwrap();
        assert_eq!(first_stats.files_hashed, 2);
        let (second, second_stats) = build_tree_index(temp.path(), Some(&first), &[]).unwrap();
        assert_eq!(second_stats.files_hashed, 0);
        assert_eq!(second_stats.files_reused, 2);
        assert_eq!(first.payload_sha256, second.payload_sha256);

        fs::write(temp.path().join("b"), b"changed bytes").unwrap();
        let (third, third_stats) = build_tree_index(temp.path(), Some(&second), &[]).unwrap();
        assert_eq!(third_stats.files_hashed, 1);
        assert_eq!(third_stats.files_reused, 1);
        assert_ne!(second.payload_sha256, third.payload_sha256);
    }

    #[cfg(windows)]
    #[test]
    fn windows_stat_index_rehashes_even_when_identity_size_and_time_can_be_replayed() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("payload"), b"first").unwrap();
        let (first, _) = build_tree_index(temp.path(), None, &[]).unwrap();
        // Windows MetadataExt has a volume/file index but no NTFS ChangeTime. The cache therefore
        // refuses stat-only reuse even before considering an attacker who restores LastWriteTime.
        let (_, unchanged) = build_tree_index(temp.path(), Some(&first), &[]).unwrap();
        assert_eq!(unchanged.files_hashed, 1);
        assert_eq!(unchanged.files_reused, 0);
        fs::write(temp.path().join("payload"), b"other").unwrap();
        let (changed, stats) = build_tree_index(temp.path(), Some(&first), &[]).unwrap();
        assert_eq!(stats.files_hashed, 1);
        assert_ne!(changed.payload_sha256, first.payload_sha256);
    }

    #[test]
    fn warm_ambient_validation_observes_current_tree_and_rehashes_only_changes() {
        let temp = tempfile::tempdir().unwrap();
        let ambient = temp.path().join("ambient");
        let object = temp.path().join("object");
        fs::create_dir(&ambient).unwrap();
        let ambient = fs::canonicalize(ambient).unwrap();
        fs::write(ambient.join("a"), b"a").unwrap();
        fs::write(ambient.join("b"), b"b").unwrap();
        let key = "a".repeat(64);
        write_input_index_object(&object, &ambient, &key);
        let refreshed = RefCell::new(None);
        assert!(validate_input_index_current(&object, &key, &ambient, &refreshed).unwrap());
        assert!(refreshed.borrow().is_none());

        fs::write(ambient.join("b"), b"c").unwrap();
        assert!(!validate_input_index_current(&object, &key, &ambient, &refreshed).unwrap());
        let refreshed = refreshed.borrow();
        let current = refreshed.as_ref().unwrap();
        let (_, sealed) = load_input_index_object(&object, &key).unwrap();
        let changed = current
            .entries
            .iter()
            .find(|entry| entry.path == "b")
            .unwrap();
        let old = sealed
            .entries
            .iter()
            .find(|entry| entry.path == "b")
            .unwrap();
        assert_ne!(changed.sha256, old.sha256);
    }

    #[test]
    fn warm_materialization_replaces_same_key_after_ambient_content_changes() {
        let temp = tempfile::tempdir().unwrap();
        let ambient = temp.path().join("ambient");
        let store = temp.path().join("store");
        fs::create_dir(&ambient).unwrap();
        let ambient = fs::canonicalize(ambient).unwrap();
        fs::write(ambient.join("payload"), b"first").unwrap();
        let key = "a".repeat(64);
        let first = materialize_ambient_index_at(store.clone(), key.clone(), &ambient).unwrap();
        let first_payload = first.receipt.payload_sha256.clone();
        drop(first);

        // Preserve the key inputs (root/rustc/toolchain) and file length. Production warm hits
        // must still inspect the ambient tree and replace the immutable index object.
        fs::write(ambient.join("payload"), b"other").unwrap();
        let second = materialize_ambient_index_at(store, key, &ambient).unwrap();
        assert_ne!(first_payload, second.receipt.payload_sha256);
        assert_eq!(second.load().unwrap().entries.len(), 1);
    }

    #[test]
    fn input_index_receipt_rejects_tampered_index_root_and_digest() {
        let temp = tempfile::tempdir().unwrap();
        let ambient = temp.path().join("ambient");
        let object = temp.path().join("object");
        fs::create_dir(&ambient).unwrap();
        let ambient = fs::canonicalize(ambient).unwrap();
        fs::write(ambient.join("a"), b"a").unwrap();
        let key = "a".repeat(64);
        write_input_index_object(&object, &ambient, &key);
        assert!(load_input_index_object(&object, &key).is_ok());

        let mut index: TreeIndex =
            serde_json::from_slice(&fs::read(object.join("index.json")).unwrap()).unwrap();
        index.root = temp.path().join("outside").to_string_lossy().into_owned();
        fs::write(
            object.join("index.json"),
            serde_json::to_vec_pretty(&index).unwrap(),
        )
        .unwrap();
        assert!(load_input_index_object(&object, &key).is_err());
    }

    #[test]
    fn full_input_audit_uses_typed_receipt_and_never_follows_forged_or_moved_roots() {
        let temp = tempfile::tempdir().unwrap();
        let ambient = temp.path().join("ambient");
        let object = temp.path().join("object");
        fs::create_dir(&ambient).unwrap();
        let ambient = fs::canonicalize(ambient).unwrap();
        fs::write(ambient.join("payload"), b"trusted").unwrap();
        let key = "b".repeat(64);
        write_input_index_object(&object, &ambient, &key);
        let trusted = BTreeSet::from([ambient.clone()]);
        assert_eq!(
            audit_ambient_input_full(&object, &key, &trusted).unwrap(),
            AmbientInputAudit::Active
        );

        let receipt_path = object.join("receipt.json");
        let original_receipt = fs::read(&receipt_path).unwrap();
        let mut wrong_schema: InputIndexReceipt =
            serde_json::from_slice(&original_receipt).unwrap();
        wrong_schema.schema += 1;
        fs::write(&receipt_path, serde_json::to_vec(&wrong_schema).unwrap()).unwrap();
        assert!(audit_ambient_input_full(&object, &key, &trusted).is_err());
        fs::write(&receipt_path, &original_receipt).unwrap();

        fs::write(object.join("index.json"), b"{}\n").unwrap();
        assert!(audit_ambient_input_full(&object, &key, &trusted).is_err());
        fs::remove_dir_all(&object).unwrap();

        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"do-not-follow").unwrap();
        let outside = fs::canonicalize(outside).unwrap();
        write_input_index_object(&object, &outside, &key);
        assert_eq!(
            audit_ambient_input_full(&object, &key, &trusted).unwrap(),
            AmbientInputAudit::Stale
        );
        assert_eq!(
            fs::read(outside.join("sentinel")).unwrap(),
            b"do-not-follow"
        );

        fs::remove_dir_all(&object).unwrap();
        let moved = temp.path().join("moved");
        fs::create_dir(&moved).unwrap();
        let moved = fs::canonicalize(moved).unwrap();
        fs::write(moved.join("payload"), b"old").unwrap();
        write_input_index_object(&object, &moved, &key);
        fs::remove_dir_all(&moved).unwrap();
        assert_eq!(
            audit_ambient_input_full(&object, &key, &trusted).unwrap(),
            AmbientInputAudit::Stale
        );
    }

    #[cfg(unix)]
    #[test]
    fn full_validation_rejects_outside_symlink_injection() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("snapshot");
        let outside = temp.path().join("outside");
        fs::create_dir_all(root.join(LIBRARY_PATH)).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(root.join(LIBRARY_PATH).join("lib.rs"), b"library").unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        write_snapshot_receipt(&root, &"a".repeat(64));
        symlink(&outside, root.join("injected")).unwrap();

        assert!(validate_snapshot_full(&root, &"a".repeat(64)).is_err());
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
    }
}
