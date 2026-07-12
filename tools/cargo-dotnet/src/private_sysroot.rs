//! Content-addressed private sysroot provisioning for native builds.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use sha2::{Digest, Sha256};

use crate::context::Context;

pub struct PrivateSysroot {
    pub root: PathBuf,
    pub library: PathBuf,
}

pub fn prepare(ctx: &Context) -> Result<PrivateSysroot> {
    // Validation may remove an incomplete snapshot and first publication renames into a shared
    // content-addressed store. Keep that narrow mutation serialized; the expensive consumer
    // compilations proceed independently after this function returns.
    let _provision_lock = crate::build_lock::BuildLock::acquire_scope("sysroot-provision")?;
    let ambient = ctx.rustc_sysroot()?;
    let ambient_library = ambient.join("lib/rustlib/src/rust/library");
    if !ambient_library.is_dir() {
        bail!("rust-src not found at {}", ambient_library.display());
    }
    let store = store_root()?;
    fs::create_dir_all(&store).with_context(|| format!("create {}", store.display()))?;
    let key = snapshot_key(ctx, &ambient)?;
    let root = store.join(&key);
    let ready = root.join("READY");
    if ready.is_file() {
        let receipt = fs::read_to_string(root.join("receipt.json")).unwrap_or_default();
        let valid = serde_json::from_str::<serde_json::Value>(&receipt)
            .ok()
            .and_then(|value| {
                let saved_key = value.get("key").and_then(|key| key.as_str())?;
                let saved_tree = value
                    .get("injected_library_sha256")
                    .and_then(|tree| tree.as_str())?;
                let actual_tree = tree_digest(&root.join("lib/rustlib/src/rust/library")).ok()?;
                Some(saved_key == key && saved_tree == actual_tree)
            })
            .unwrap_or(false);
        if !valid {
            fs::remove_dir_all(&root)
                .with_context(|| format!("remove invalid private sysroot {}", root.display()))?;
        }
    }
    if !ready.is_file() {
        let tmp = store.join(format!(".tmp-{}-{}", std::process::id(), unique_suffix()));
        fs::remove_dir_all(&tmp).ok();
        clone_tree(&ambient, &tmp)?;
        // Apply every PAL/rust-src transformation before publishing READY. Once renamed into the
        // content-addressed store, a private sysroot is immutable and concurrent builds may read it
        // without observing a half-injected tree.
        let tmp_library = tmp.join("lib/rustlib/src/rust/library");
        crate::palinject::inject_all_at(ctx, &tmp_library)?;
        let injected_library_sha256 = tree_digest(&tmp_library)?;
        fs::write(
            tmp.join("receipt.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": 1,
                "key": key,
                "ambient_sysroot": ambient,
                "library": "lib/rustlib/src/rust/library",
                "injected_library_sha256": injected_library_sha256
            }))?,
        )?;
        fs::write(
            tmp.join("READY"),
            b"cargo-dotnet-private-sysroot-v2-injected\n",
        )?;
        match fs::rename(&tmp, &root) {
            Ok(()) => {}
            Err(_) if ready.is_file() => {
                fs::remove_dir_all(&tmp).ok();
            }
            Err(error) => {
                return Err(error).with_context(|| format!("publish {}", root.display()));
            }
        }
    }
    let library = root.join("lib/rustlib/src/rust/library");
    if !library.is_dir() {
        bail!("private sysroot is incomplete: {}", library.display());
    }
    Ok(PrivateSysroot { root, library })
}

fn store_root() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("CARGO_DOTNET_HOME").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(home).join("sysroots"));
    }
    let home = crate::host::home_dir().context("neither HOME nor USERPROFILE is set")?;
    Ok(home.join(".cargo-dotnet/sysroots"))
}

fn snapshot_key(ctx: &Context, ambient: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(b"cargo-dotnet-private-sysroot-v2-injected\0");
    hash.update(ambient.as_os_str().to_string_lossy().as_bytes());
    let mut rustc = std::process::Command::new("rustc");
    if let Some(toolchain) = &ctx.toolchain {
        rustc.env("RUSTUP_TOOLCHAIN", toolchain);
    }
    let rustc_vv = rustc
        .arg("-Vv")
        .output()
        .context("query rustc -Vv for private sysroot identity")?;
    if !rustc_vv.status.success() {
        bail!("rustc -Vv failed while identifying private sysroot");
    }
    hash.update(&rustc_vv.stdout);
    if let Some(toolchain) = &ctx.toolchain {
        hash.update(toolchain.as_bytes());
    }
    hash.update(fs::read(&ctx.paths.target_spec)?);
    // rustc's version alone does not identify a locally modified rust-src component. Include the
    // complete source tree so an ambient edit can never reuse a stale private snapshot.
    hash_tree(&ambient.join("lib/rustlib/src/rust/library"), &mut hash)?;
    // Any change to the injection engine must produce a new immutable snapshot even when the PAL
    // source tree itself is unchanged.
    hash.update(include_bytes!("palinject.rs"));
    hash_tree(&ctx.paths.pal_root, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn hash_tree(path: &Path, hash: &mut Sha256) -> Result<()> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        hash.update(entry.file_name().as_encoded_bytes());
        if path.is_dir() {
            hash_tree(&path, hash)?;
        } else if path.is_file() {
            hash.update(fs::read(path)?);
        }
    }
    Ok(())
}

fn tree_digest(path: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    hash_tree(path, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn clone_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let src = entry.path();
        let dst = destination.join(entry.file_name());
        if src.is_dir() {
            clone_tree(&src, &dst)?;
        } else if src.is_file() {
            fs::copy(&src, &dst).with_context(|| format!("copy {}", src.display()))?;
        }
    }
    Ok(())
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutable_library_files_are_copied_not_linked() {
        let base = std::env::temp_dir().join(format!("cd-sysroot-test-{}", unique_suffix()));
        let src = base.join("ambient");
        let library = src.join("lib/rustlib/src/rust/library");
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("marker.rs"), "ambient").unwrap();
        let dst = base.join("private");
        clone_tree(&src, &dst).unwrap();
        fs::write(
            dst.join("lib/rustlib/src/rust/library/marker.rs"),
            "private",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(library.join("marker.rs")).unwrap(),
            "ambient"
        );
        fs::remove_dir_all(base).ok();
    }
}
