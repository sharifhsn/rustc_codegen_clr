//! Scoped cross-process locks for the few mutable cargo-dotnet resources.

use std::fs::{self, File};
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use fs2::FileExt;

use crate::context::{Context, crate_cache_key};

pub struct BuildLock {
    file: File,
}

impl BuildLock {
    /// Serialize writes to one consumer's target/config/docs/receipt while allowing unrelated
    /// crates to compile concurrently.
    pub fn acquire_crate(ctx: &Context) -> Result<Self> {
        Self::acquire_scope(&format!("crate-{}", crate_cache_key(&ctx.crate_dir)?))
    }

    /// Protect a named SDK-owned shared resource for only the duration of its mutation.
    pub fn acquire_scope(scope: &str) -> Result<Self> {
        Self::acquire_scope_with(scope, false)
    }

    /// Observe one mutable SDK-owned resource without crossing an in-progress transaction.
    pub fn acquire_scope_shared(scope: &str) -> Result<Self> {
        Self::acquire_scope_with(scope, true)
    }

    fn acquire_scope_with(scope: &str, shared: bool) -> Result<Self> {
        debug_assert!(
            scope
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        );
        let path = lock_path(scope)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create cargo-dotnet lock dir {}", parent.display()))?;
        }
        let file = rust_dotnet_sdk_core::safe_fs::create_or_open_regular_nofollow(&path)
            .with_context(|| format!("open cargo-dotnet build lock {}", path.display()))?;
        eprintln!("==> cargo dotnet: waiting for {scope} lock");
        if shared {
            FileExt::lock_shared(&file)
                .with_context(|| format!("share cargo-dotnet build lock {}", path.display()))?;
        } else {
            FileExt::lock_exclusive(&file)
                .with_context(|| format!("lock cargo-dotnet build lock {}", path.display()))?;
        }
        Ok(Self { file })
    }

    /// Convert an exclusive recovery lock into a shared reader lease. The unlock/relock window is
    /// intentional; callers must re-check their recovery marker after this returns before reading.
    pub(crate) fn downgrade_to_shared(self) -> Result<Self> {
        FileExt::unlock(&self.file).context("unlock cargo-dotnet build lock for downgrade")?;
        FileExt::lock_shared(&self.file).context("share cargo-dotnet build lock after recovery")?;
        Ok(self)
    }
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn lock_path(scope: &str) -> Result<PathBuf> {
    let root = crate::context::cargo_dotnet_cache_home()?;
    fs::create_dir_all(&root)?;
    let metadata = fs::symlink_metadata(&root)?;
    if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        anyhow::bail!(
            "cargo-dotnet cache root is not a regular directory: {}",
            root.display()
        );
    }
    let root = fs::canonicalize(root)?;
    let locks = root.join("locks");
    match fs::create_dir(&locks) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(&locks)?;
    if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        anyhow::bail!(
            "cargo-dotnet lock root is not a regular directory: {}",
            locks.display()
        );
    }
    let locks = fs::canonicalize(locks)?;
    if locks.parent() != Some(root.as_path()) {
        anyhow::bail!("cargo-dotnet lock root escaped its cache root");
    }
    Ok(locks.join(format!("{scope}.lock")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lock_is_outside_the_consumer_crate() {
        let path = lock_path("crate-abc").unwrap();
        assert!(path.ends_with(".cargo-dotnet-cache/locks/crate-abc.lock"));
    }
}
