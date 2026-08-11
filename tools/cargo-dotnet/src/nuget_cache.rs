//! Versioned, integrity-checked NuGet restore snapshots.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use rust_dotnet_assets::{ResolvedAsset, ResolvedAssets};

const NUGET_CACHE_SCHEMA: u32 = 4;
const NUGET_CACHE_LIMIT: usize = 24;
const RESOLUTION_FILE: &str = "resolution.json";
const RECEIPT_FILE: &str = "receipt.json";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct NugetCacheIdentity {
    schema: u32,
    id: String,
    version: String,
    host_os: String,
    host_arch: String,
    rid: Option<String>,
    tfm: String,
    source_config_sha256: String,
    dotnet_identity_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NugetCacheReceipt {
    schema: u32,
    key: String,
    identity: NugetCacheIdentity,
    payload_sha256: String,
    resolution_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedResolution {
    primary_dll: Option<PathBuf>,
    compile_dlls: Vec<PathBuf>,
    runtime_dlls: Vec<PathBuf>,
    assets: Vec<ResolvedAsset>,
}

pub(crate) struct CachedRestore {
    pub(crate) cache_key: String,
    pub(crate) primary_dll: Option<PathBuf>,
    pub(crate) compile_dlls: Vec<PathBuf>,
    pub(crate) runtime_dlls: Vec<PathBuf>,
    pub(crate) assets: Vec<ResolvedAsset>,
    _lease: Option<crate::content_cache::CacheObject>,
}

pub(crate) fn restore(
    id: &str,
    version: &str,
    rid: Option<&str>,
    tfm: &str,
    sources: &[String],
    force: bool,
) -> Result<CachedRestore> {
    crate::path_safety::validate_nuget_id(id)?;
    crate::path_safety::validate_nuget_version(version)?;
    if let Some(rid) = rid {
        crate::path_safety::validate_path_component("NuGet restore RID", rid)?;
    }
    let dotnet = rust_dotnet_assets::dotnet_version(tfm)?;
    let source_config = rust_dotnet_assets::isolated_source_config(sources);
    let identity = identity(id, version, rid, tfm, &source_config, &dotnet);
    let key = cache_key(&identity)?;
    let store = crate::content_cache::ContentStore::new(
        crate::context::cargo_dotnet_cache_home()?.join("nuget/v4"),
        NUGET_CACHE_LIMIT,
    )?;
    let validate = |snapshot: &Path| validate_snapshot(snapshot, &key, &identity);
    let build = |snapshot: &Path| build_snapshot(snapshot, &key, &identity, &source_config);
    let (snapshot, _) = if force {
        store.materialize_forced(&key, validate, build)?
    } else {
        store.materialize(&key, validate, build)?
    };
    let mut restored = load_resolution(snapshot.path())?;
    restored.cache_key = key;
    restored._lease = Some(snapshot);
    Ok(restored)
}

fn identity(
    id: &str,
    version: &str,
    rid: Option<&str>,
    tfm: &str,
    source_config: &[u8],
    dotnet_identity: &str,
) -> NugetCacheIdentity {
    NugetCacheIdentity {
        schema: NUGET_CACHE_SCHEMA,
        id: id.to_ascii_lowercase(),
        version: version.to_string(),
        host_os: std::env::consts::OS.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        rid: rid.map(str::to_string),
        tfm: tfm.to_string(),
        source_config_sha256: format!("{:x}", Sha256::digest(source_config)),
        dotnet_identity_sha256: format!("{:x}", Sha256::digest(dotnet_identity.as_bytes())),
    }
}

fn cache_key(identity: &NugetCacheIdentity) -> Result<String> {
    let bytes = serde_json::to_vec(identity)?;
    Ok(crate::content_cache::digest_parts([
        b"cargo-dotnet-nuget-cache-v4".as_slice(),
        bytes.as_slice(),
    ]))
}

fn build_snapshot(
    snapshot: &Path,
    key: &str,
    identity: &NugetCacheIdentity,
    source_config: &[u8],
) -> Result<()> {
    let restore_work = snapshot.join("restore-work");
    let config = snapshot.join("restore.NuGet.Config");
    write_new(&config, source_config)?;
    let resolved = rust_dotnet_assets::restore_with_config(
        &identity.id,
        &identity.version,
        &restore_work,
        identity.rid.as_deref(),
        &identity.tfm,
        &config,
    )?;
    // Publish only the selected compile/runtime/native/resource closure. NuGet's packages tree
    // contains `.nupkg.metadata` source URLs (and may contain inline credentials), so the restore
    // work tree must never become part of the immutable cache object.
    let cached = CachedResolution::from_resolved(snapshot, &restore_work, resolved)?;
    fs::remove_file(&config)?;
    crate::path_safety::remove_dir_all_within(snapshot, &restore_work)?;
    write_new(
        &snapshot.join(RESOLUTION_FILE),
        &serde_json::to_vec_pretty(&cached)?,
    )?;
    write_receipt(snapshot, key, identity)?;
    Ok(())
}

impl CachedResolution {
    fn from_resolved(
        snapshot: &Path,
        restore_work: &Path,
        resolved: ResolvedAssets,
    ) -> Result<Self> {
        let payload = snapshot.join("payload");
        fs::create_dir(&payload)?;
        let mut published = BTreeMap::new();
        let mut assets = resolved.assets;
        for asset in &mut assets {
            asset.source = publish_selected(
                snapshot,
                restore_work,
                &payload,
                &asset.source,
                &mut published,
            )?;
        }
        Ok(Self {
            primary_dll: resolved
                .primary_dll
                .as_deref()
                .map(|path| {
                    publish_selected(snapshot, restore_work, &payload, path, &mut published)
                })
                .transpose()?,
            compile_dlls: publish_selected_files(
                snapshot,
                restore_work,
                &payload,
                &resolved.compile_dlls,
                &mut published,
            )?,
            runtime_dlls: publish_selected_files(
                snapshot,
                restore_work,
                &payload,
                &resolved.runtime_dlls,
                &mut published,
            )?,
            assets,
        })
    }
}

fn publish_selected_files(
    snapshot: &Path,
    restore_work: &Path,
    payload: &Path,
    paths: &[PathBuf],
    published: &mut BTreeMap<PathBuf, PathBuf>,
) -> Result<Vec<PathBuf>> {
    paths
        .iter()
        .map(|path| publish_selected(snapshot, restore_work, payload, path, published))
        .collect()
}

fn publish_selected(
    snapshot: &Path,
    restore_work: &Path,
    payload: &Path,
    path: &Path,
    published: &mut BTreeMap<PathBuf, PathBuf>,
) -> Result<PathBuf> {
    let relative = path
        .strip_prefix(restore_work)
        .with_context(|| {
            format!(
                "NuGet restore output escaped its owned work tree: {}",
                path.display()
            )
        })?
        .to_path_buf();
    crate::path_safety::validate_relative_path(&relative)?;
    let source = crate::path_safety::canonical_file_or_directory_within(restore_work, &relative)?;
    if !source.is_file() {
        bail!(
            "NuGet selected asset is not a regular file: {}",
            path.display()
        );
    }
    if let Some(existing) = published.get(&source) {
        return Ok(existing.clone());
    }
    let bytes = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&source)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let filename = source
        .file_name()
        .context("NuGet selected asset has no filename")?;
    let destination_dir = payload.join(&digest);
    if !destination_dir.exists() {
        fs::create_dir(&destination_dir)?;
    }
    let destination = destination_dir.join(filename);
    match OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&destination)
    {
        Ok(mut output) => {
            output.write_all(&bytes)?;
            output.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&destination)? != bytes {
                bail!(
                    "NuGet selected asset digest collision at {}",
                    destination.display()
                );
            }
        }
        Err(error) => return Err(error.into()),
    }
    let relative = destination.strip_prefix(snapshot)?.to_path_buf();
    crate::path_safety::validate_relative_path(&relative)?;
    published.insert(source, relative.clone());
    Ok(relative)
}

fn load_resolution(snapshot: &Path) -> Result<CachedRestore> {
    let mut cached: CachedResolution = serde_json::from_slice(
        &rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&snapshot.join(RESOLUTION_FILE))?,
    )?;
    let primary_dll = cached
        .primary_dll
        .as_deref()
        .map(|path| crate::path_safety::canonical_file_within(snapshot, path))
        .transpose()?;
    let compile_dlls = absolute_files(snapshot, &cached.compile_dlls)?;
    let runtime_dlls = absolute_files(snapshot, &cached.runtime_dlls)?;
    for asset in &mut cached.assets {
        asset.source = crate::path_safety::canonical_file_within(snapshot, &asset.source)?;
    }
    Ok(CachedRestore {
        cache_key: String::new(),
        primary_dll,
        compile_dlls,
        runtime_dlls,
        assets: cached.assets,
        _lease: None,
    })
}

fn absolute_files(snapshot: &Path, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    paths
        .iter()
        .map(|path| crate::path_safety::canonical_file_within(snapshot, path))
        .collect()
}

fn write_receipt(snapshot: &Path, key: &str, identity: &NugetCacheIdentity) -> Result<()> {
    let resolution = fs::read(snapshot.join(RESOLUTION_FILE))?;
    let receipt = NugetCacheReceipt {
        schema: NUGET_CACHE_SCHEMA,
        key: key.to_string(),
        identity: identity.clone(),
        payload_sha256: crate::content_cache::tree_digest(&snapshot.join("payload"))?,
        resolution_sha256: format!("{:x}", Sha256::digest(&resolution)),
    };
    write_new(
        &snapshot.join(RECEIPT_FILE),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(())
}

fn validate_snapshot(snapshot: &Path, key: &str, identity: &NugetCacheIdentity) -> Result<bool> {
    if !regular_file(&snapshot.join(RECEIPT_FILE))
        || !regular_file(&snapshot.join(RESOLUTION_FILE))
        || !regular_directory(&snapshot.join("payload"))
    {
        return Ok(false);
    }
    let Ok(receipt_bytes) =
        rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&snapshot.join(RECEIPT_FILE))
    else {
        return Ok(false);
    };
    let Ok(receipt) = serde_json::from_slice::<NugetCacheReceipt>(&receipt_bytes) else {
        return Ok(false);
    };
    if receipt.schema != NUGET_CACHE_SCHEMA || receipt.key != key || &receipt.identity != identity {
        return Ok(false);
    }
    let resolution =
        match rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&snapshot.join(RESOLUTION_FILE))
        {
            Ok(bytes) => bytes,
            Err(_) => return Ok(false),
        };
    let Ok(payload_sha256) = crate::content_cache::tree_digest(&snapshot.join("payload")) else {
        return Ok(false);
    };
    if receipt.resolution_sha256 != format!("{:x}", Sha256::digest(&resolution))
        || receipt.payload_sha256 != payload_sha256
    {
        return Ok(false);
    }
    Ok(load_resolution(snapshot).is_ok())
}

fn regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn regular_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_dotnet_assets::{
        AssetKind, DEFAULT_NUGET_SOURCE, ResolvedAsset, ResolvedAssets, isolated_source_config,
    };

    #[test]
    fn cache_key_covers_resolution_inputs_without_storing_source_urls() {
        let private_config =
            isolated_source_config(&["https://feed.invalid/private?token=secret".into()]);
        let base = identity(
            "Example.Package",
            "1.2.3",
            Some("linux-x64"),
            "net10.0",
            &private_config,
            "10.0.100",
        );
        let serialized = serde_json::to_string(&base).unwrap();
        assert!(!serialized.contains("token=secret"));
        let key = cache_key(&base).unwrap();
        for changed in [
            identity(
                "Example.Package",
                "1.2.4",
                Some("linux-x64"),
                "net10.0",
                &isolated_source_config(&[]),
                "10.0.100",
            ),
            identity(
                "Example.Package",
                "1.2.3",
                Some("osx-arm64"),
                "net10.0",
                &isolated_source_config(&[]),
                "10.0.100",
            ),
            identity(
                "Example.Package",
                "1.2.3",
                Some("linux-x64"),
                "net8.0",
                &isolated_source_config(&[]),
                "10.0.100",
            ),
            identity(
                "Example.Package",
                "1.2.3",
                Some("linux-x64"),
                "net10.0",
                &isolated_source_config(&[]),
                "10.0.200",
            ),
        ] {
            assert_ne!(key, cache_key(&changed).unwrap());
        }
    }

    #[test]
    fn integrity_receipt_detects_payload_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let payload = temp.path().join("payload/packages/example/1.0/lib/net10.0");
        fs::create_dir_all(&payload).unwrap();
        let dll = payload.join("Example.dll");
        fs::write(&dll, b"assembly").unwrap();
        let relative = dll.strip_prefix(temp.path()).unwrap().to_path_buf();
        let resolution = CachedResolution {
            primary_dll: Some(relative.clone()),
            compile_dlls: vec![relative.clone()],
            runtime_dlls: vec![relative.clone()],
            assets: vec![ResolvedAsset {
                owner: "Example/1.0".into(),
                kind: rust_dotnet_assets::AssetKind::Runtime,
                logical_path: "lib/net10.0/Example.dll".into(),
                source: relative,
                rid: None,
                fallback: false,
            }],
        };
        fs::write(
            temp.path().join(RESOLUTION_FILE),
            serde_json::to_vec_pretty(&resolution).unwrap(),
        )
        .unwrap();
        let identity = identity(
            "Example",
            "1.0",
            None,
            "net10.0",
            &isolated_source_config(&[]),
            "10.0.100",
        );
        let key = cache_key(&identity).unwrap();
        write_receipt(temp.path(), &key, &identity).unwrap();
        assert!(validate_snapshot(temp.path(), &key, &identity).unwrap());
        fs::write(dll, b"tampered").unwrap();
        assert!(!validate_snapshot(temp.path(), &key, &identity).unwrap());
    }

    #[test]
    fn default_source_config_is_explicit_and_ignores_ambient_configuration() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("NuGet.Config"),
            b"<configuration><packageSources><add key=\"ambient\" value=\"https://ambient.invalid\" /></packageSources></configuration>",
        )
        .unwrap();

        let config = String::from_utf8(isolated_source_config(&[])).unwrap();
        assert!(config.contains("<clear />"));
        assert!(config.contains(DEFAULT_NUGET_SOURCE));
        assert!(!config.contains("ambient.invalid"));
        let default_identity = identity(
            "Example",
            "1.0",
            None,
            "net10.0",
            config.as_bytes(),
            "10.0.100",
        );
        let explicit_identity = identity(
            "Example",
            "1.0",
            None,
            "net10.0",
            &isolated_source_config(&["https://ambient.invalid".into()]),
            "10.0.100",
        );
        assert_ne!(
            cache_key(&default_identity).unwrap(),
            cache_key(&explicit_identity).unwrap()
        );
    }

    #[test]
    fn published_snapshot_contains_only_selected_closure_and_no_feed_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let snapshot = temp.path().join("snapshot");
        let restore = snapshot.join("restore-work");
        let package = restore.join("packages/example/1.0/lib/net10.0");
        fs::create_dir_all(&package).unwrap();
        let dll = package.join("Example.dll");
        fs::write(&dll, b"selected-assembly").unwrap();
        fs::write(
            restore.join("packages/example/1.0/.nupkg.metadata"),
            br#"{"source":"https://user:secret@private.invalid/index.json"}"#,
        )
        .unwrap();
        let resolved = ResolvedAssets {
            primary_dll: Some(dll.clone()),
            compile_dlls: vec![dll.clone()],
            runtime_dlls: vec![dll.clone()],
            assets: vec![ResolvedAsset {
                owner: "Example/1.0".into(),
                kind: AssetKind::Runtime,
                logical_path: "lib/net10.0/Example.dll".into(),
                source: dll,
                rid: None,
                fallback: false,
            }],
            collisions: Vec::new(),
            requested_rid: None,
        };
        let cached = CachedResolution::from_resolved(&snapshot, &restore, resolved).unwrap();
        crate::path_safety::remove_dir_all_within(&snapshot, &restore).unwrap();
        write_new(
            &snapshot.join(RESOLUTION_FILE),
            &serde_json::to_vec_pretty(&cached).unwrap(),
        )
        .unwrap();

        let mut bytes = serde_json::to_vec(&cached).unwrap();
        bytes.extend(
            fs::read(
                snapshot.join(
                    cached
                        .primary_dll
                        .as_ref()
                        .expect("selected primary assembly"),
                ),
            )
            .unwrap(),
        );
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("selected-assembly"));
        assert!(!text.contains("private.invalid"));
        assert!(!text.contains("secret"));
        assert!(!snapshot.join("restore-work").exists());
    }

    #[cfg(unix)]
    #[test]
    fn selected_closure_rejects_symlinked_restore_asset() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let snapshot = temp.path().join("snapshot");
        let restore = snapshot.join("restore-work");
        fs::create_dir_all(&restore).unwrap();
        let outside = temp.path().join("outside.dll");
        fs::write(&outside, b"outside").unwrap();
        let linked = restore.join("linked.dll");
        symlink(&outside, &linked).unwrap();
        let resolved = ResolvedAssets {
            primary_dll: Some(linked),
            compile_dlls: Vec::new(),
            runtime_dlls: Vec::new(),
            assets: Vec::new(),
            collisions: Vec::new(),
            requested_rid: None,
        };
        assert!(CachedResolution::from_resolved(&snapshot, &restore, resolved).is_err());
        assert_eq!(fs::read(outside).unwrap(), b"outside");
    }
}
