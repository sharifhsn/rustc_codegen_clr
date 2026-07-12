//! Machine-readable identity receipt for every successful cargo-dotnet artifact.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::rustflags::normalize_producer_binary;

use crate::artifact::Artifact;
use crate::context::Context;
use crate::context::ManagedIdentity;
use crate::private_sysroot::PrivateSysroot;

#[derive(Serialize)]
struct FileIdentity {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Serialize)]
struct SourceIdentity {
    repository: Option<String>,
    revision: Option<String>,
    dirty: Option<bool>,
    cargo_lock_sha256: Option<String>,
}

#[derive(Serialize)]
struct BuildReceipt {
    schema: u32,
    host_os: &'static str,
    source: SourceIdentity,
    profile: &'static str,
    target: String,
    dotnet: &'static str,
    toolchain: Option<String>,
    private_sysroot_receipt: FileIdentity,
    cargo_home: String,
    cargo_arguments: Vec<String>,
    backend: FileIdentity,
    linker: FileIdentity,
    target_spec: FileIdentity,
    pal_tree_sha256: String,
    overlays_tree_sha256: String,
    artifact: FileIdentity,
    pdb: Option<FileIdentity>,
    xml_docs: Option<FileIdentity>,
    managed_identity: Option<ManagedIdentityReceipt>,
    local_input_closure: Option<InputClosureIdentity>,
}

#[derive(Serialize)]
struct InputClosureIdentity {
    manifest: FileIdentity,
    files: usize,
    sha256: String,
}

#[derive(Serialize)]
struct ManagedIdentityReceipt {
    schema: u16,
    package_id: String,
    assembly_name: String,
    root_namespace: String,
    module_type: String,
    legacy_main_module: bool,
}

pub fn write(
    ctx: &Context,
    artifact: &Artifact,
    sysroot: &PrivateSysroot,
) -> Result<Option<PathBuf>> {
    let artifact_path = match artifact {
        Artifact::Executable(path) => path,
        Artifact::Library { dll, .. } => dll,
        Artifact::None => return Ok(None),
    };
    let receipt = BuildReceipt {
        schema: 1,
        host_os: ctx.host.os,
        source: source_identity(&ctx.crate_dir)?,
        profile: ctx.profile.dir(),
        target: ctx.paths.target_spec.to_string_lossy().into_owned(),
        dotnet: ctx.dotnet.as_env(),
        toolchain: ctx.toolchain.clone(),
        private_sysroot_receipt: file_identity(&sysroot.root.join("receipt.json"))?,
        cargo_home: ctx.paths.cargo_home.to_string_lossy().into_owned(),
        cargo_arguments: ctx.flags.extra_cargo.clone(),
        backend: producer_identity(&ctx.paths.backend_dylib)?,
        linker: producer_identity(&ctx.paths.linker)?,
        target_spec: file_identity(&ctx.paths.target_spec)?,
        pal_tree_sha256: tree_hash(&ctx.paths.pal_root)?,
        overlays_tree_sha256: tree_hash(&ctx.paths.overlays_root)?,
        artifact: file_identity(artifact_path)?,
        pdb: sidecar_identity(artifact_path, "pdb")?,
        xml_docs: sidecar_identity(artifact_path, "xml")?,
        managed_identity: ctx.managed_identity.as_ref().map(identity_receipt),
        local_input_closure: input_closure_identity(ctx)?,
    };
    let path = PathBuf::from(format!(
        "{}.rustdotnet.receipt.json",
        artifact_path.display()
    ));
    let temp = path.with_extension("receipt.json.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(&receipt)?)
        .with_context(|| format!("write artifact receipt temp {}", temp.display()))?;
    fs::rename(&temp, &path)
        .with_context(|| format!("publish artifact receipt {}", path.display()))?;
    Ok(Some(path))
}

fn input_closure_identity(ctx: &Context) -> Result<Option<InputClosureIdentity>> {
    let target = ctx
        .paths
        .target_spec
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("x86_64-unknown-dotnet");
    let manifest = ctx
        .crate_dir
        .join("target")
        .join(target)
        .join(ctx.profile.dir())
        .join(".rustdotnet-cargo-inputs");
    if !manifest.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&manifest)
        .with_context(|| format!("read Cargo input closure {}", manifest.display()))?;
    let mut digest = Sha256::new();
    let mut files = 0;
    for line in text.lines().filter(|line| !line.is_empty()) {
        let path = Path::new(line);
        digest.update(line.as_bytes());
        digest.update([0]);
        digest.update(hash_file(path)?.as_bytes());
        digest.update([0]);
        files += 1;
    }
    Ok(Some(InputClosureIdentity {
        manifest: file_identity(&manifest)?,
        files,
        sha256: format!("{:x}", digest.finalize()),
    }))
}

fn identity_receipt(identity: &ManagedIdentity) -> ManagedIdentityReceipt {
    ManagedIdentityReceipt {
        schema: identity.schema,
        package_id: identity.package_id.clone(),
        assembly_name: identity.assembly_name.clone(),
        root_namespace: identity.root_namespace.clone(),
        module_type: identity.module_type.clone(),
        legacy_main_module: identity.legacy_main_module,
    }
}

fn source_identity(crate_dir: &Path) -> Result<SourceIdentity> {
    let revision = git_output(crate_dir, &["rev-parse", "HEAD"]);
    // A checkout path is machine-specific and would make otherwise identical packages differ.
    // Record only the stable source remote; repositories without one remain explicitly unknown.
    let repository = git_output(crate_dir, &["config", "--get", "remote.origin.url"]);
    let dirty = if revision.is_some() {
        Some(
            Command::new("git")
                .args(["status", "--porcelain", "--untracked-files=all"])
                .current_dir(crate_dir)
                .output()
                .map(|output| !output.stdout.is_empty())
                .unwrap_or(true),
        )
    } else {
        None
    };
    let lock = crate_dir.join("Cargo.lock");
    Ok(SourceIdentity {
        repository,
        revision,
        dirty,
        cargo_lock_sha256: lock.is_file().then(|| hash_file(&lock)).transpose()?,
    })
}

fn git_output(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn file_identity(path: &Path) -> Result<FileIdentity> {
    Ok(FileIdentity {
        path: path.to_string_lossy().into_owned(),
        sha256: hash_file(path)?,
        bytes: fs::metadata(path)
            .with_context(|| format!("stat receipt input {}", path.display()))?
            .len(),
    })
}

fn producer_identity(path: &Path) -> Result<FileIdentity> {
    let mut bytes = fs::read(path).with_context(|| format!("read producer {}", path.display()))?;
    let size = bytes.len() as u64;
    normalize_producer_binary(path, &mut bytes);
    Ok(FileIdentity {
        path: path.to_string_lossy().into_owned(),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        bytes: size,
    })
}

fn sidecar_identity(artifact: &Path, extension: &str) -> Result<Option<FileIdentity>> {
    let path = artifact.with_extension(extension);
    path.is_file().then(|| file_identity(&path)).transpose()
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("hash {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn tree_hash(root: &Path) -> Result<String> {
    fn collect(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in
            fs::read_dir(current).with_context(|| format!("read tree {}", current.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                collect(root, &path, files)?;
            } else if path.is_file() {
                files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    files.sort();
    let mut digest = Sha256::new();
    for relative in files {
        digest.update(relative.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(hash_file(&root.join(&relative))?.as_bytes());
        digest.update([0]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_hash_is_order_independent_and_content_sensitive() {
        let root =
            std::env::temp_dir().join(format!("cargo-dotnet-receipt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("z"), b"one").unwrap();
        fs::write(root.join("nested/a"), b"two").unwrap();
        let first = tree_hash(&root).unwrap();
        assert_eq!(first, tree_hash(&root).unwrap());
        fs::write(root.join("nested/a"), b"changed").unwrap();
        assert_ne!(first, tree_hash(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
