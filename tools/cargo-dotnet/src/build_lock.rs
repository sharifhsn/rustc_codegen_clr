//! Scoped cross-process locks for the few mutable cargo-dotnet resources.

use std::fs::{self, File, OpenOptions};
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
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("open cargo-dotnet build lock {}", path.display()))?;
        eprintln!("==> cargo dotnet: waiting for {scope} lock");
        file.lock_exclusive()
            .with_context(|| format!("lock cargo-dotnet build lock {}", path.display()))?;
        Ok(Self { file })
    }
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn lock_path(scope: &str) -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("CARGO_DOTNET_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home).join(format!("locks/{scope}.lock")));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("neither HOME nor USERPROFILE is set for cargo-dotnet build lock")?;
    Ok(PathBuf::from(home).join(format!(".cargo-dotnet/locks/{scope}.lock")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lock_is_outside_the_consumer_crate() {
        let path = lock_path("crate-abc").unwrap();
        assert!(path.ends_with(".cargo-dotnet/locks/crate-abc.lock"));
    }
}
