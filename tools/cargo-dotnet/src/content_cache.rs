//! Concurrency-safe immutable content-addressed cache snapshots.

use std::fs::{self, File};
use std::io::Write as _;
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use fs2::FileExt;
use rust_dotnet_sdk_core::safe_fs::{DirectoryCapability, TreeWalkNode};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CacheStatus {
    Hit,
    Built,
}

/// A shared lease that prevents garbage collection while a caller consumes a snapshot.
#[derive(Debug)]
pub(crate) struct CacheObject {
    path: PathBuf,
    _lease: CacheLease,
}

impl CacheObject {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Deref for CacheObject {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

pub(crate) struct ContentStore {
    root: PathBuf,
    max_entries: usize,
}

impl ContentStore {
    pub(crate) fn new(root: PathBuf, max_entries: usize) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let metadata = fs::symlink_metadata(&root)
            .with_context(|| format!("inspecting content-cache root {}", root.display()))?;
        if link_or_reparse(&metadata) || !metadata.is_dir() {
            bail!(
                "content-cache root is not a regular directory: {}",
                root.display()
            );
        }
        let root = fs::canonicalize(&root)
            .with_context(|| format!("resolving content-cache root {}", root.display()))?;
        for name in ["objects", "locks", "tmp", "access"] {
            ensure_fixed_directory(&root, name)?;
        }
        let store = Self {
            root,
            max_entries: max_entries.max(1),
        };
        store.cleanup_stale_temps(8)?;
        Ok(store)
    }

    pub(crate) fn materialize<V, B>(
        &self,
        key: &str,
        validate: V,
        build: B,
    ) -> Result<(CacheObject, CacheStatus)>
    where
        V: Fn(&Path) -> Result<bool>,
        B: FnOnce(&Path) -> Result<()>,
    {
        self.materialize_inner(key, false, validate, build)
    }

    pub(crate) fn materialize_forced<V, B>(
        &self,
        key: &str,
        validate: V,
        build: B,
    ) -> Result<(CacheObject, CacheStatus)>
    where
        V: Fn(&Path) -> Result<bool>,
        B: FnOnce(&Path) -> Result<()>,
    {
        self.materialize_inner(key, true, validate, build)
    }

    /// Enumerate typed object keys without following foreign entries. Callers must acquire each
    /// object through [`Self::open_existing`] before reading it; enumeration alone is not a lease.
    pub(crate) fn object_keys(&self) -> Result<Vec<String>> {
        let objects = self.fixed_directory("objects")?;
        let mut keys = Vec::new();
        for entry in fs::read_dir(objects)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.is_dir() && !link_or_reparse(&metadata) && validate_key(&name).is_ok() {
                keys.push(name);
            }
        }
        keys.sort();
        Ok(keys)
    }

    /// Open one immutable object under a shared consumer lease and validate it while the lease
    /// prevents concurrent replacement or garbage collection.
    pub(crate) fn open_existing<V>(&self, key: &str, validate: V) -> Result<Option<CacheObject>>
    where
        V: Fn(&Path) -> Result<bool>,
    {
        validate_key(key)?;
        let objects = self.fixed_directory("objects")?;
        let locks = self.fixed_directory("locks")?;
        let target = objects.join(key);
        let lease = CacheLease::acquire(
            &locks.join(format!("{key}.lease.lock")),
            &locks.join(format!("{key}.consumer.lock")),
        )?;
        if !valid_target(&target, &validate)? {
            return Ok(None);
        }
        Ok(Some(CacheObject {
            path: target,
            _lease: lease,
        }))
    }

    fn materialize_inner<V, B>(
        &self,
        key: &str,
        force: bool,
        validate: V,
        build: B,
    ) -> Result<(CacheObject, CacheStatus)>
    where
        V: Fn(&Path) -> Result<bool>,
        B: FnOnce(&Path) -> Result<()>,
    {
        validate_key(key)?;
        let locks = self.fixed_directory("locks")?;
        let objects = self.fixed_directory("objects")?;
        let tmp = self.fixed_directory("tmp")?;
        let build_lock_path = locks.join(format!("{key}.build.lock"));
        let lease_lock_path = locks.join(format!("{key}.lease.lock"));
        let consumer_lock_path = locks.join(format!("{key}.consumer.lock"));
        let target = objects.join(key);
        if !force {
            let lease = CacheLease::acquire(&lease_lock_path, &consumer_lock_path)?;
            if valid_target(&target, &validate)? {
                self.touch(key)?;
                let _ = self.gc(Some(key));
                return Ok((
                    CacheObject {
                        path: target,
                        _lease: lease,
                    },
                    CacheStatus::Hit,
                ));
            }
            drop(lease);
        }

        let build_lock = KeyLock::acquire(&build_lock_path)?;
        // Another process may have completed this key while we waited for its builder. Recheck
        // under a shared consumer lease before doing expensive work.
        if !force {
            let lease = CacheLease::acquire(&lease_lock_path, &consumer_lock_path)?;
            if valid_target(&target, &validate)? {
                self.touch(key)?;
                drop(build_lock);
                let _ = self.gc(Some(key));
                return Ok((
                    CacheObject {
                        path: target,
                        _lease: lease,
                    },
                    CacheStatus::Hit,
                ));
            }
            drop(lease);
        }

        let temporary = tempfile::Builder::new()
            .prefix(&format!(".{key}."))
            .tempdir_in(&tmp)?;
        build(temporary.path())?;
        if !validate(temporary.path())? {
            bail!("cache builder produced an invalid snapshot for {key}");
        }
        let staged = temporary.keep();
        // Readers may continue using the old immutable object while a forced replacement builds.
        // Take the exclusive lease only for the short rollback-capable promotion window.
        // The guard is acquired before the primary lease lock and retained across the exclusive
        // -> shared conversion below. Garbage collection follows the same order. Consequently
        // there is no instant in which a promoted object is both returned to its consumer and
        // unprotected from deletion.
        let consumer_guard = SharedLock::acquire(&consumer_lock_path)?;
        let lease_mutation = KeyLock::acquire(&lease_lock_path)?;
        let backup_area = tempfile::Builder::new()
            .prefix(&format!(".{key}.backup."))
            .tempdir_in(&tmp)?;
        let backup = backup_area.path().join("previous");
        let had_previous = fs::symlink_metadata(&target).is_ok();
        if had_previous {
            fs::rename(&target, &backup)
                .with_context(|| format!("backing up cache object {key}"))?;
        }
        let promotion = fs::rename(&staged, &target)
            .with_context(|| format!("publishing cache object {key}"))
            .and_then(|()| {
                if validate(&target)? {
                    Ok(())
                } else {
                    bail!("published cache object failed revalidation for {key}")
                }
            });
        if let Err(error) = promotion {
            let _ = if fs::symlink_metadata(&target).is_ok() {
                remove_cache_entry(&objects, &target)
            } else {
                Ok(())
            };
            if had_previous {
                fs::rename(&backup, &target).with_context(|| {
                    format!("restoring previous cache object {key} after: {error:#}")
                })?;
            }
            if fs::symlink_metadata(&staged).is_ok() {
                crate::path_safety::remove_dir_all_within(&tmp, &staged)?;
            }
            return Err(error);
        }
        self.touch(key)?;
        let lease = lease_mutation.into_shared(consumer_guard)?;
        drop(build_lock);
        let _ = self.gc(Some(key));
        Ok((
            CacheObject {
                path: target,
                _lease: lease,
            },
            CacheStatus::Built,
        ))
    }

    fn touch(&self, key: &str) -> Result<()> {
        let access = self.fixed_directory("access")?;
        let marker = access.join(key);
        let value = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let mut temporary = tempfile::Builder::new()
            .prefix(&format!(".{key}.access."))
            .tempfile_in(&access)?;
        temporary.write_all(value.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&marker)
            .map_err(|error| error.error)
            .with_context(|| format!("publishing cache access marker for {key}"))?;
        let metadata = fs::symlink_metadata(&marker)?;
        if link_or_reparse(&metadata) || !metadata.is_file() {
            bail!("cache access marker is not a regular file");
        }
        Ok(())
    }

    /// Retain the most recently-used bounded set. Only immediate, validated digest directories
    /// below this store's `objects` root are candidates; symlinks and foreign names are ignored.
    pub(crate) fn gc(&self, keep: Option<&str>) -> Result<usize> {
        let objects = self.fixed_directory("objects")?;
        let access = self.fixed_directory("access")?;
        let locks = self.fixed_directory("locks")?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(&objects)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.is_dir() || link_or_reparse(&metadata) || validate_key(&name).is_err() {
                continue;
            }
            let recency = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&access.join(&name))
                .ok()
                .and_then(|value| String::from_utf8(value).ok())
                .and_then(|value| value.parse::<u128>().ok())
                .unwrap_or_default();
            entries.push((recency, name, entry.path()));
        }
        entries.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
        let mut removed = 0usize;
        let mut retained =
            usize::from(keep.is_some_and(|keep| entries.iter().any(|(_, name, _)| name == keep)));
        for (_, name, path) in entries {
            if keep == Some(name.as_str()) {
                continue;
            }
            if retained < self.max_entries {
                retained += 1;
                continue;
            }
            let Ok(build_lock) = KeyLock::try_acquire(&locks.join(format!("{name}.build.lock")))
            else {
                continue;
            };
            let Ok(consumer_lock) =
                KeyLock::try_acquire(&locks.join(format!("{name}.consumer.lock")))
            else {
                continue;
            };
            let Ok(lease_lock) = KeyLock::try_acquire(&locks.join(format!("{name}.lease.lock")))
            else {
                continue;
            };
            remove_cache_entry(&objects, &path)?;
            let _ = fs::remove_file(access.join(name));
            drop(lease_lock);
            drop(consumer_lock);
            drop(build_lock);
            removed += 1;
        }
        Ok(removed)
    }

    /// Reclaim every object not protected by an active builder/consumer lease. This is used only
    /// for superseded schema roots; a live older cargo-dotnet process keeps its object intact and
    /// a later invocation retries best-effort cleanup.
    fn prune_all(&self) -> Result<usize> {
        let objects = self.fixed_directory("objects")?;
        let access = self.fixed_directory("access")?;
        let locks = self.fixed_directory("locks")?;
        let mut removed = 0;
        for entry in fs::read_dir(&objects)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.is_dir() || link_or_reparse(&metadata) || validate_key(&name).is_err() {
                continue;
            }
            let Ok(build_lock) = KeyLock::try_acquire(&locks.join(format!("{name}.build.lock")))
            else {
                continue;
            };
            let Ok(consumer_lock) =
                KeyLock::try_acquire(&locks.join(format!("{name}.consumer.lock")))
            else {
                continue;
            };
            let Ok(lease_lock) = KeyLock::try_acquire(&locks.join(format!("{name}.lease.lock")))
            else {
                continue;
            };
            if remove_cache_entry(&objects, &entry.path()).is_ok() {
                let _ = fs::remove_file(access.join(&name));
                removed += 1;
            }
            drop(lease_lock);
            drop(consumer_lock);
            drop(build_lock);
        }
        Ok(removed)
    }

    fn cleanup_stale_temps(&self, max_removals: usize) -> Result<usize> {
        let tmp = self.fixed_directory("tmp")?;
        let locks = self.fixed_directory("locks")?;
        let mut removed = 0usize;
        for entry in fs::read_dir(&tmp)? {
            if removed >= max_removals {
                break;
            }
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.is_dir() || link_or_reparse(&metadata) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(key) = name.strip_prefix('.').and_then(|name| name.get(..64)) else {
                continue;
            };
            if validate_key(key).is_err() {
                continue;
            }
            let lock_path = locks.join(format!("{key}.build.lock"));
            let lock = rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(&lock_path)?;
            if FileExt::try_lock_exclusive(&lock).is_err() {
                continue;
            }
            crate::path_safety::remove_dir_all_within(&tmp, &entry.path())?;
            let _ = FileExt::unlock(&lock);
            removed += 1;
        }
        Ok(removed)
    }

    fn fixed_directory(&self, name: &'static str) -> Result<PathBuf> {
        verify_fixed_directory(&self.root, name)
    }
}

/// Best-effort bounded migration cleanup for cache schemas no longer read by 0.0.2. The roots
/// are fixed relative names below the canonical cache home; symlinked roots fail closed and are
/// never traversed.
pub(crate) fn prune_legacy_caches(cache_home: &Path) -> Result<usize> {
    let cache_home = fs::canonicalize(cache_home)
        .with_context(|| format!("resolving cargo-dotnet cache home {}", cache_home.display()))?;
    let mut removed = 0;
    for relative in [
        "sysroots/v3",
        "sysroots/v4",
        "sysroots/input-index/v1",
        "helpers/v1",
        "nuget/v2",
        "nuget/v3",
        "nuget-bindgen/v1",
        "nuget-bindgen/v2",
    ] {
        let relative = Path::new(relative);
        crate::path_safety::validate_relative_path(relative)?;
        let root = cache_home.join(relative);
        let metadata = match fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if link_or_reparse(&metadata) || !metadata.is_dir() {
            bail!(
                "legacy cache root is not a regular directory: {}",
                root.display()
            );
        }
        let canonical = fs::canonicalize(&root)?;
        if canonical != root || !canonical.starts_with(&cache_home) {
            bail!(
                "legacy cache root escapes cache home: {}",
                canonical.display()
            );
        }
        removed += ContentStore::new(canonical, 1)?.prune_all()?;
    }
    Ok(removed)
}

fn ensure_fixed_directory(root: &Path, name: &'static str) -> Result<PathBuf> {
    let path = root.join(name);
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error).with_context(|| format!("creating content-cache {name} directory"));
        }
    }
    verify_fixed_directory(root, name)
}

fn verify_fixed_directory(root: &Path, name: &'static str) -> Result<PathBuf> {
    let path = root.join(name);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("inspecting content-cache {name} directory"))?;
    if link_or_reparse(&metadata) || !metadata.is_dir() {
        bail!(
            "content-cache {name} path is not a regular directory: {}",
            path.display()
        );
    }
    let canonical = fs::canonicalize(&path)
        .with_context(|| format!("resolving content-cache {name} directory"))?;
    if canonical.parent() != Some(root) || canonical != path {
        bail!(
            "content-cache {name} directory escapes its canonical root: {}",
            canonical.display()
        );
    }
    Ok(canonical)
}

fn valid_target<V>(target: &Path, validate: &V) -> Result<bool>
where
    V: Fn(&Path) -> Result<bool>,
{
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(metadata.is_dir() && !link_or_reparse(&metadata) && validate(target)?)
}

struct KeyLock(Option<File>);

impl KeyLock {
    fn acquire(path: &Path) -> Result<Self> {
        let file = rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(path)?;
        FileExt::lock_exclusive(&file)?;
        Ok(Self(Some(file)))
    }

    fn try_acquire(path: &Path) -> Result<Self> {
        let file = rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(path)?;
        FileExt::try_lock_exclusive(&file)?;
        Ok(Self(Some(file)))
    }

    fn into_shared(self, guard: SharedLock) -> Result<CacheLease> {
        self.into_shared_with_hook(guard, || {})
    }

    fn into_shared_with_hook(
        mut self,
        guard: SharedLock,
        after_exclusive_unlock: impl FnOnce(),
    ) -> Result<CacheLease> {
        let file = self.0.take().expect("key lock has a file");
        FileExt::unlock(&file)?;
        after_exclusive_unlock();
        FileExt::lock_shared(&file)?;
        Ok(CacheLease {
            primary: file,
            _guard: guard,
        })
    }
}

impl Drop for KeyLock {
    fn drop(&mut self) {
        if let Some(file) = &self.0 {
            let _ = FileExt::unlock(file);
        }
    }
}

#[derive(Debug)]
struct SharedLock(File);

impl SharedLock {
    fn acquire(path: &Path) -> Result<Self> {
        let file = rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(path)?;
        FileExt::lock_shared(&file)?;
        Ok(Self(file))
    }
}

impl Drop for SharedLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

#[derive(Debug)]
struct CacheLease {
    primary: File,
    _guard: SharedLock,
}

impl CacheLease {
    fn acquire(primary_path: &Path, guard_path: &Path) -> Result<Self> {
        let guard = SharedLock::acquire(guard_path)?;
        let file = rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(primary_path)?;
        FileExt::lock_shared(&file)?;
        Ok(Self {
            primary: file,
            _guard: guard,
        })
    }
}

impl Drop for CacheLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.primary);
    }
}

fn validate_key(key: &str) -> Result<()> {
    if key.len() != 64
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("content-cache key must be a lowercase SHA-256 digest");
    }
    Ok(())
}

fn remove_cache_entry(objects: &Path, target: &Path) -> Result<()> {
    if target.parent() != Some(objects) {
        bail!(
            "cache deletion target is not an immediate object: {}",
            target.display()
        );
    }
    let canonical_objects = fs::canonicalize(objects)
        .with_context(|| format!("resolving cache objects root {}", objects.display()))?;
    if &canonical_objects != objects {
        bail!("cache objects root changed during deletion");
    }
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if link_or_reparse(&metadata) || !metadata.is_dir() {
        bail!("cache deletion target is not a regular object directory");
    }
    let canonical_target = fs::canonicalize(target)
        .with_context(|| format!("resolving cache object {}", target.display()))?;
    if canonical_target.parent() != Some(&canonical_objects) || canonical_target != target {
        bail!("cache deletion target escapes the canonical objects root");
    }
    let store = objects
        .parent()
        .context("cache objects root has no parent")?;
    verify_fixed_directory(store, "tmp")?;
    let key = target
        .file_name()
        .and_then(|name| name.to_str())
        .context("cache object has no digest filename")?;
    validate_key(key)?;
    let trash = tempfile::Builder::new()
        .prefix(&format!(".{key}.cache-trash-"))
        .tempdir_in(store.join("tmp"))?;
    let moved = trash.path().join("object");
    match fs::rename(target, &moved) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("moving {} to cache trash", target.display()));
        }
    }
    let metadata = fs::symlink_metadata(&moved)?;
    if link_or_reparse(&metadata) || !metadata.is_dir() {
        fs::remove_file(&moved).with_context(|| format!("removing {}", moved.display()))
    } else {
        crate::path_safety::remove_dir_all_within(trash.path(), &moved)
    }
}

pub(crate) fn digest_parts<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

pub(crate) fn hash_tree(path: &Path, hash: &mut Sha256) -> Result<()> {
    hash_tree_with_hook(path, hash, &[], &mut |_| {})
}

fn hash_tree_with_hook(
    root: &Path,
    hash: &mut Sha256,
    excluded_root_entries: &[&str],
    before_file_open: &mut dyn FnMut(&Path),
) -> Result<()> {
    validate_tree_root(root)?;
    let capability = DirectoryCapability::open(root)?;
    hash.update(b"rust-dotnet-tree-digest-v2\0");
    let mut relative_hook = |relative: &Path| before_file_open(&root.join(relative));
    capability.walk_regular_tree_with_hook(
        excluded_root_entries,
        &mut relative_hook,
        &mut |relative, node| {
            match node {
                TreeWalkNode::DirectoryEnter(_) => {
                    hash.update(b"directory-enter\0");
                    hash_relative_path(relative, hash)?;
                }
                TreeWalkNode::File(file) => {
                    hash.update(b"file\0");
                    hash_relative_path(relative, hash)?;
                    let display = root.join(relative);
                    let bytes = rust_dotnet_sdk_core::safe_fs::read_opened_regular(file, &display)?;
                    hash.update((bytes.len() as u64).to_le_bytes());
                    hash.update(bytes);
                }
                TreeWalkNode::DirectoryLeave(_) => {
                    hash.update(b"directory-leave\0");
                    hash_relative_path(relative, hash)?;
                }
            }
            Ok(())
        },
    )?;
    hash.update(b"tree-end\0");
    Ok(())
}

fn hash_relative_path(relative: &Path, hash: &mut Sha256) -> Result<()> {
    crate::path_safety::validate_relative_path(relative)?;
    let components = relative.components().collect::<Vec<_>>();
    hash.update((components.len() as u64).to_le_bytes());
    for component in components {
        let Component::Normal(component) = component else {
            bail!("tree hash path is not normalized: {}", relative.display());
        };
        let encoded = component.as_encoded_bytes();
        hash.update((encoded.len() as u64).to_le_bytes());
        hash.update(encoded);
    }
    Ok(())
}

pub(crate) fn tree_digest(path: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    hash_tree(path, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn tree_digest_excluding_root_entries(
    path: &Path,
    excluded_root_entries: &[&str],
) -> Result<String> {
    let mut hash = Sha256::new();
    hash_tree_with_hook(path, &mut hash, excluded_root_entries, &mut |_| {})?;
    Ok(format!("{:x}", hash.finalize()))
}

fn validate_tree_root(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspecting tree hash root {}", path.display()))?;
    if link_or_reparse(&metadata) || !metadata.is_dir() {
        bail!(
            "tree hash root is not a regular directory: {}",
            path.display()
        );
    }
    Ok(())
}

fn link_or_reparse(metadata: &fs::Metadata) -> bool {
    rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(metadata)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::*;

    fn key(value: &str) -> String {
        digest_parts([value.as_bytes()])
    }

    fn valid(path: &Path) -> Result<bool> {
        Ok(fs::read(path.join("artifact")).is_ok_and(|bytes| bytes == b"good"))
    }

    #[test]
    fn hit_miss_and_integrity_invalidation_use_operation_counts() {
        let temp = tempfile::tempdir().unwrap();
        let store = ContentStore::new(temp.path().join("cache"), 8).unwrap();
        let operations = AtomicUsize::new(0);
        let build = |path: &Path| {
            operations.fetch_add(1, Ordering::SeqCst);
            fs::write(path.join("artifact"), b"good")?;
            Ok(())
        };
        let (object, status) = store.materialize(&key("a"), valid, build).unwrap();
        assert_eq!(status, CacheStatus::Built);
        let path = object.path().to_path_buf();
        drop(object);
        let (object, status) = store.materialize(&key("a"), valid, build).unwrap();
        assert_eq!(status, CacheStatus::Hit);
        assert_eq!(operations.load(Ordering::SeqCst), 1);
        drop(object);

        fs::write(path.join("artifact"), b"corrupt").unwrap();
        let (object, status) = store.materialize(&key("a"), valid, build).unwrap();
        assert_eq!(status, CacheStatus::Built);
        assert_eq!(operations.load(Ordering::SeqCst), 2);
        drop(object);
        store.materialize(&key("b"), valid, build).unwrap();
        assert_eq!(operations.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn concurrent_miss_builds_once() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cache");
        let operations = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(6));
        let mut workers = Vec::new();
        for _ in 0..6 {
            let root = root.clone();
            let operations = Arc::clone(&operations);
            let barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                let store = ContentStore::new(root, 8).unwrap();
                barrier.wait();
                store
                    .materialize(&key("shared"), valid, |path| {
                        operations.fetch_add(1, Ordering::SeqCst);
                        thread::sleep(std::time::Duration::from_millis(25));
                        fs::write(path.join("artifact"), b"good")?;
                        Ok(())
                    })
                    .unwrap();
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(operations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn garbage_collection_is_bounded_and_root_confined() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cache");
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        let store = ContentStore::new(root.clone(), 2).unwrap();
        for value in ["a", "b", "c"] {
            store
                .materialize(&key(value), valid, |path| {
                    fs::write(path.join("artifact"), b"good")?;
                    Ok(())
                })
                .unwrap();
        }
        let count = fs::read_dir(root.join("objects")).unwrap().count();
        assert_eq!(count, 2);
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
    }

    #[test]
    fn garbage_collection_respects_active_snapshot_leases() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cache");
        let store = ContentStore::new(root.clone(), 1).unwrap();
        let first_key = key("leased");
        let second_key = key("current");
        let (leased, _) = store
            .materialize(&first_key, valid, |path| {
                fs::write(path.join("artifact"), b"good")?;
                Ok(())
            })
            .unwrap();
        let (current, _) = store
            .materialize(&second_key, valid, |path| {
                fs::write(path.join("artifact"), b"good")?;
                Ok(())
            })
            .unwrap();
        assert!(leased.path().is_dir());
        assert!(current.path().is_dir());
        drop(leased);
        assert_eq!(store.gc(Some(&second_key)).unwrap(), 1);
        assert!(!root.join("objects").join(first_key).exists());
    }

    #[test]
    fn promotion_downgrade_has_no_garbage_collection_deletion_gap() {
        let temp = tempfile::tempdir().unwrap();
        let store = ContentStore::new(temp.path().join("cache"), 1).unwrap();
        let digest = key("downgrade-gap");
        let objects = store.fixed_directory("objects").unwrap();
        let locks = store.fixed_directory("locks").unwrap();
        let target = objects.join(&digest);
        fs::create_dir(&target).unwrap();
        fs::write(target.join("artifact"), b"good").unwrap();
        let primary_path = locks.join(format!("{digest}.lease.lock"));
        let guard_path = locks.join(format!("{digest}.consumer.lock"));
        let guard = SharedLock::acquire(&guard_path).unwrap();
        let exclusive = KeyLock::acquire(&primary_path).unwrap();

        let lease = exclusive
            .into_shared_with_hook(guard, || {
                assert!(
                    KeyLock::try_acquire(&guard_path).is_err(),
                    "GC acquired its deletion guard during the primary-lock downgrade"
                );
                assert!(
                    target.is_dir(),
                    "GC-visible object disappeared during downgrade"
                );
            })
            .unwrap();
        assert!(target.is_dir());
        drop(lease);
        assert!(KeyLock::try_acquire(&guard_path).is_ok());
    }

    #[test]
    fn forced_rebuild_rolls_back_when_promoted_object_fails_revalidation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cache");
        let store = ContentStore::new(root.clone(), 2).unwrap();
        let digest = key("rollback");
        let (old, _) = store
            .materialize(&digest, valid, |path| {
                fs::write(path.join("artifact"), b"good")?;
                Ok(())
            })
            .unwrap();
        assert_eq!(fs::read(old.join("artifact")).unwrap(), b"good");
        drop(old);

        let objects = fs::canonicalize(root.join("objects")).unwrap();
        let error = store
            .materialize_forced(
                &digest,
                |path| {
                    let bytes = fs::read(path.join("artifact")).unwrap_or_default();
                    Ok(!(path.starts_with(&objects) && bytes == b"replacement"))
                },
                |path| {
                    fs::write(path.join("artifact"), b"replacement")?;
                    Ok(())
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("revalidation"));
        assert_eq!(
            fs::read(root.join("objects").join(digest).join("artifact")).unwrap(),
            b"good"
        );
    }

    #[test]
    fn tree_digest_distinguishes_files_relocated_across_directory_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let shallow = temp.path().join("shallow");
        let nested = temp.path().join("nested");
        fs::create_dir_all(shallow.join("a/b")).unwrap();
        fs::write(shallow.join("c"), b"same bytes").unwrap();
        fs::create_dir_all(nested.join("a/b")).unwrap();
        fs::write(nested.join("a/b/c"), b"same bytes").unwrap();

        assert_ne!(
            tree_digest(&shallow).unwrap(),
            tree_digest(&nested).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn tree_hash_rejects_a_leaf_swapped_after_directory_enumeration() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let outside = temp.path().join("outside-secret");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("victim"), b"inside").unwrap();
        fs::write(&outside, b"outside").unwrap();
        let mut swapped = false;
        let mut hash = Sha256::new();
        let error = hash_tree_with_hook(&root, &mut hash, &[], &mut |path| {
            if !swapped && path.file_name().is_some_and(|name| name == "victim") {
                fs::remove_file(path).unwrap();
                symlink(&outside, path).unwrap();
                swapped = true;
            }
        })
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("following links")
                || format!("{error:#}").contains("regular file"),
            "{error:#}"
        );
        assert_eq!(fs::read(outside).unwrap(), b"outside");
    }

    #[cfg(unix)]
    #[test]
    fn tree_hash_keeps_one_root_capability_after_enumeration() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let expected = temp.path().join("expected");
        let replacement = temp.path().join("replacement");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&expected).unwrap();
        fs::create_dir(&replacement).unwrap();
        fs::write(root.join("victim"), b"inside").unwrap();
        fs::write(expected.join("victim"), b"inside").unwrap();
        fs::write(replacement.join("victim"), b"outside-secret").unwrap();
        let expected_digest = tree_digest(&expected).unwrap();

        let mut swapped = false;
        let mut hash = Sha256::new();
        hash_tree_with_hook(&root, &mut hash, &[], &mut |_| {
            if !swapped {
                fs::rename(&root, temp.path().join("root.original")).unwrap();
                fs::rename(&replacement, &root).unwrap();
                swapped = true;
            }
        })
        .unwrap();

        assert!(swapped);
        assert_eq!(format!("{:x}", hash.finalize()), expected_digest);
        assert_eq!(fs::read(root.join("victim")).unwrap(), b"outside-secret");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_fixed_directory_is_rejected_without_touching_outside_state() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cache");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        symlink(&outside, root.join("objects")).unwrap();

        let error = match ContentStore::new(root, 1) {
            Ok(_) => panic!("symlinked objects directory was accepted"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("objects"));
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn access_marker_publish_replaces_symlink_without_clobbering_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cache");
        let store = ContentStore::new(root.clone(), 2).unwrap();
        let digest = key("access-symlink");
        let (object, _) = store
            .materialize(&digest, valid, |path| {
                fs::write(path.join("artifact"), b"good")?;
                Ok(())
            })
            .unwrap();
        drop(object);
        let marker = root.join("access").join(&digest);
        fs::remove_file(&marker).unwrap();
        let sentinel = temp.path().join("outside-sentinel");
        fs::write(&sentinel, b"keep").unwrap();
        symlink(&sentinel, &marker).unwrap();

        let (object, status) = store
            .materialize(&digest, valid, |_| unreachable!())
            .unwrap();
        assert_eq!(status, CacheStatus::Hit);
        assert!(object.is_dir());
        assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
        assert!(
            !fs::symlink_metadata(marker)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_key_lock_is_rejected_without_touching_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cache");
        let store = ContentStore::new(root.clone(), 2).unwrap();
        let digest = key("lock-symlink");
        let sentinel = temp.path().join("outside-sentinel");
        fs::write(&sentinel, b"keep").unwrap();
        symlink(
            &sentinel,
            root.join("locks").join(format!("{digest}.lease.lock")),
        )
        .unwrap();

        let error = store
            .materialize(&digest, valid, |_| unreachable!())
            .unwrap_err();
        assert!(error.to_string().contains("non-symlink"), "{error:#}");
        assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
    }

    #[test]
    fn killed_keyed_trash_is_reclaimed_on_next_open() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("cache");
        let digest = key("killed-trash");
        ContentStore::new(root.clone(), 2).unwrap();
        let trash = root
            .join("tmp")
            .join(format!(".{digest}.cache-trash-interrupted"));
        fs::create_dir(&trash).unwrap();
        fs::write(trash.join("large-object"), vec![7_u8; 128 * 1024]).unwrap();

        ContentStore::new(root, 2).unwrap();
        assert!(!trash.exists());
    }

    #[test]
    fn legacy_prune_reclaims_unleased_objects_and_retries_leased_objects() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("nuget/v3");
        let store = ContentStore::new(root.clone(), 8).unwrap();
        let build = |path: &Path| {
            fs::write(path.join("artifact"), b"good")?;
            Ok(())
        };
        let (leased, _) = store.materialize(&key("leased"), valid, build).unwrap();
        let (unleased, _) = store.materialize(&key("unleased"), valid, build).unwrap();
        let unleased_path = unleased.path().to_path_buf();
        drop(unleased);

        assert_eq!(prune_legacy_caches(temp.path()).unwrap(), 1);
        assert!(leased.path().exists());
        assert!(!unleased_path.exists());
        let leased_path = leased.path().to_path_buf();
        drop(leased);
        assert_eq!(prune_legacy_caches(temp.path()).unwrap(), 1);
        assert!(!leased_path.exists());
    }
}
