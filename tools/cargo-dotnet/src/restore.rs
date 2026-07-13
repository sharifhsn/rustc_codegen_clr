//! Explicit dependency restore and the receipt consumed by `build --offline`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cli::BuildArgs;
use crate::context::Context;
use crate::mode::Backend;
use crate::private_sysroot::PrivateSysroot;
use crate::{buildstd, interop_helpers, overlays, private_sysroot};

const SCHEMA: u32 = 1;
const RECEIPT_NAME: &str = "restore-receipt.json";

#[derive(Debug, Serialize, Deserialize)]
struct RestoreReceipt {
    schema: u32,
    toolchain: Option<String>,
    dotnet: String,
    cargo_home: String,
    private_sysroot: String,
    inputs: Vec<FileRecord>,
    cache: Vec<FileRecord>,
    inputs_sha256: String,
    cache_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileRecord {
    path: String,
    bytes: u64,
    sha256: String,
}

pub fn run(args: &BuildArgs) -> Result<i32> {
    let mode = crate::mode::detect()?;
    if Backend::resolve(args.backend.as_deref(), &mode)? == Backend::Docker {
        bail!("`cargo dotnet restore` supports the native installed SDK only");
    }
    let ctx = Context::resolve(args, false)?;
    let _crate_lock = crate::build_lock::BuildLock::acquire_crate(&ctx)?;
    let sysroot = private_sysroot::prepare(&ctx)?;
    overlays::apply(&ctx)?;
    buildstd::fetch_dependencies(&ctx, &sysroot)?;
    interop_helpers::restore_if_needed(&ctx)?;
    let receipt = create_receipt(&ctx, &sysroot)?;
    let path = receipt_path(&ctx);
    write_atomic(&path, &receipt)?;
    eprintln!("==> cargo dotnet: restore complete");
    eprintln!("== restore receipt: {} ==", path.display());
    Ok(0)
}

pub fn verify(ctx: &Context, sysroot: &PrivateSysroot) -> Result<()> {
    let path = receipt_path(ctx);
    if !path.is_file() {
        bail!(
            "offline build has no verified restore receipt; run `cargo dotnet restore {}` while network access is available",
            ctx.crate_dir.display()
        );
    }
    let receipt: RestoreReceipt = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("read restore receipt {}", path.display()))?,
    )
    .with_context(|| format!("parse restore receipt {}", path.display()))?;
    if receipt.schema != SCHEMA {
        bail!("unsupported restore receipt schema {}", receipt.schema);
    }
    if receipt.toolchain != ctx.toolchain
        || receipt.dotnet != ctx.dotnet.as_env()
        || Path::new(&receipt.cargo_home) != ctx.paths.cargo_home
        || Path::new(&receipt.private_sysroot) != sysroot.root
    {
        stale(ctx, "toolchain, runtime, cache, or private sysroot changed")?;
    }
    verify_records(&receipt.inputs, &receipt.inputs_sha256)
        .or_else(|error| stale(ctx, &format!("dependency inputs changed: {error:#}")))?;
    verify_records(&receipt.cache, &receipt.cache_sha256)
        .or_else(|error| stale(ctx, &format!("private Cargo cache changed: {error:#}")))?;
    eprintln!("==> verified offline restore receipt: {}", path.display());
    Ok(())
}

fn stale<T>(ctx: &Context, reason: &str) -> Result<T> {
    bail!(
        "offline restore receipt is stale ({reason}); run `cargo dotnet restore {}` while network access is available",
        ctx.crate_dir.display()
    )
}

fn create_receipt(ctx: &Context, sysroot: &PrivateSysroot) -> Result<RestoreReceipt> {
    let inputs = input_records(ctx, sysroot)?;
    let cache = cache_records(&ctx.paths.cargo_home)?;
    if cache.is_empty() {
        bail!(
            "cargo fetch completed without populating the private cache at {}",
            ctx.paths.cargo_home.display()
        );
    }
    Ok(RestoreReceipt {
        schema: SCHEMA,
        toolchain: ctx.toolchain.clone(),
        dotnet: ctx.dotnet.as_env().to_string(),
        cargo_home: ctx.paths.cargo_home.to_string_lossy().into_owned(),
        private_sysroot: sysroot.root.to_string_lossy().into_owned(),
        inputs_sha256: records_digest(&inputs),
        cache_sha256: records_digest(&cache),
        inputs,
        cache,
    })
}

fn input_records(ctx: &Context, sysroot: &PrivateSysroot) -> Result<Vec<FileRecord>> {
    let mut paths = BTreeSet::new();
    for manifest in buildstd::local_manifest_paths(ctx, sysroot)? {
        paths.insert(manifest.clone());
        if let Some(root) = manifest.parent() {
            for ancestor in root.ancestors() {
                for relative in ["Cargo.lock", ".cargo/config.toml", ".cargo/config"] {
                    let candidate = ancestor.join(relative);
                    if candidate.is_file() {
                        paths.insert(candidate);
                    }
                }
                if ancestor == ctx.crate_dir {
                    break;
                }
            }
        }
    }
    for path in [
        overlays::generated_config_path(ctx),
        ctx.paths.target_spec.clone(),
        ctx.paths.overlays_root.join("REGISTRY.toml"),
        sysroot.root.join("receipt.json"),
        ctx.paths
            .interop_helpers_root
            .join("Mycorrhiza.Interop.Helpers.csproj"),
        ctx.paths
            .interop_helpers_root
            .join("obj/project.assets.json"),
    ] {
        if path.is_file() {
            paths.insert(path);
        }
    }
    paths.into_iter().map(|path| record(&path)).collect()
}

fn cache_records(cargo_home: &Path) -> Result<Vec<FileRecord>> {
    let mut paths = Vec::new();
    for root in [cargo_home.join("registry"), cargo_home.join("git")] {
        collect_files(&root, &mut paths)?;
    }
    paths.sort();
    paths.into_iter().map(|path| record(&path)).collect()
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn record(path: &Path) -> Result<FileRecord> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(FileRecord {
        path: path.to_string_lossy().into_owned(),
        bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    })
}

fn records_digest(records: &[FileRecord]) -> String {
    let mut digest = Sha256::new();
    for record in records {
        digest.update(record.path.as_bytes());
        digest.update([0]);
        digest.update(record.bytes.to_le_bytes());
        digest.update(record.sha256.as_bytes());
        digest.update([0]);
    }
    format!("{:x}", digest.finalize())
}

fn verify_records(records: &[FileRecord], expected: &str) -> Result<()> {
    if records.is_empty() {
        bail!("receipt contains no files");
    }
    let actual = records
        .iter()
        .map(|saved| {
            let current = record(Path::new(&saved.path))?;
            if current.bytes != saved.bytes || current.sha256 != saved.sha256 {
                bail!("{} no longer matches its recorded hash", saved.path);
            }
            Ok(current)
        })
        .collect::<Result<Vec<_>>>()?;
    if records_digest(&actual) != expected {
        bail!("receipt aggregate hash does not match");
    }
    Ok(())
}

fn receipt_path(ctx: &Context) -> PathBuf {
    ctx.paths.cargo_home.join("restore").join(RECEIPT_NAME)
}

fn write_atomic(path: &Path, receipt: &RestoreReceipt) -> Result<()> {
    let parent = path.parent().context("restore receipt has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".{RECEIPT_NAME}.{}.tmp", std::process::id()));
    fs::write(&temp, serde_json::to_vec_pretty(receipt)?)?;
    fs::rename(&temp, path).with_context(|| format!("publish {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_verification_detects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crate.cache");
        fs::write(&path, b"original").unwrap();
        let records = vec![record(&path).unwrap()];
        let digest = records_digest(&records);
        verify_records(&records, &digest).unwrap();
        fs::write(&path, b"tampered").unwrap();
        assert!(verify_records(&records, &digest).is_err());
    }

    #[test]
    fn cache_scan_excludes_credentials() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("credentials.toml"), "secret").unwrap();
        fs::create_dir_all(dir.path().join("registry/cache")).unwrap();
        fs::write(dir.path().join("registry/cache/pkg.crate"), "crate").unwrap();
        let records = cache_records(dir.path()).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].path.ends_with("pkg.crate"));
    }
}
