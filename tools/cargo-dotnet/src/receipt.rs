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
use crate::context::ManagedProjectConfig;
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
    source_link_url: Option<String>,
    private_sysroot_receipt: FileIdentity,
    cargo_home: String,
    cargo_arguments: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    test_target: Option<TestTargetReceipt>,
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

#[derive(Debug, Serialize)]
struct TestTargetReceipt {
    selector: String,
    package_id: String,
    artifact_name: String,
    artifact_kind: Vec<String>,
    target_dir: Option<String>,
    locked: bool,
    cargo_lock_sha256: Option<String>,
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
    public_namespaces: Vec<String>,
    compatibility_profile: String,
}

pub fn write(
    ctx: &Context,
    artifact: &Artifact,
    sysroot: &PrivateSysroot,
) -> Result<Option<PathBuf>> {
    write_receipt(ctx, artifact, sysroot, None)
}

pub fn write_with_test_target(
    ctx: &Context,
    artifact: &Artifact,
    sysroot: &PrivateSysroot,
    actual_target: crate::artifact::TestTargetIdentity,
) -> Result<Option<PathBuf>> {
    write_receipt(ctx, artifact, sysroot, Some(actual_target))
}

/// Remove any earlier receipt before a new invocation materializes runtime sidecars.
/// A failed sidecar copy must never leave a successful-looking receipt from an older run.
pub fn invalidate_for_artifact(artifact_path: &Path) -> Result<()> {
    let path = PathBuf::from(format!(
        "{}.rustdotnet.receipt.json",
        artifact_path.display()
    ));
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("invalidate prior artifact receipt {}", path.display())),
    }
}

fn write_receipt(
    ctx: &Context,
    artifact: &Artifact,
    sysroot: &PrivateSysroot,
    actual_test_target: Option<crate::artifact::TestTargetIdentity>,
) -> Result<Option<PathBuf>> {
    let artifact_path = match artifact {
        Artifact::Executable(path) => path,
        Artifact::Library { dll, .. } => dll,
        Artifact::None => return Ok(None),
    };
    let cargo_lock_sha256 = workspace_cargo_lock_sha256(ctx)?;
    let test_target = receipt_test_target(
        &ctx.flags.extra_cargo,
        actual_test_target,
        cargo_lock_sha256.clone(),
    )?;
    let receipt = BuildReceipt {
        // Schema 2 adds `test_target`; keep ordinary build receipts on schema 1 so
        // existing consumers do not need to understand a test-only field.
        schema: receipt_schema(test_target.is_some()),
        host_os: ctx.host.os,
        source: source_identity(&ctx.crate_dir, cargo_lock_sha256)?,
        profile: ctx.profile.dir(),
        target: ctx.paths.target_spec.to_string_lossy().into_owned(),
        dotnet: ctx.dotnet.as_env(),
        toolchain: ctx.toolchain.clone(),
        source_link_url: ctx.source_link_url.clone(),
        private_sysroot_receipt: file_identity(&sysroot.root.join("receipt.json"))?,
        cargo_home: ctx.paths.cargo_home.to_string_lossy().into_owned(),
        cargo_arguments: ctx.flags.extra_cargo.clone(),
        test_target,
        backend: producer_identity(&ctx.paths.backend_dylib)?,
        linker: producer_identity(&ctx.paths.linker)?,
        target_spec: file_identity(&ctx.paths.target_spec)?,
        pal_tree_sha256: tree_hash(&ctx.paths.pal_root)?,
        overlays_tree_sha256: tree_hash(&ctx.paths.overlays_root)?,
        artifact: file_identity(artifact_path)?,
        pdb: sidecar_identity(artifact_path, "pdb")?,
        xml_docs: sidecar_identity(artifact_path, "xml")?,
        managed_identity: ctx.managed_project.as_ref().map(identity_receipt),
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

const fn receipt_schema(has_test_target: bool) -> u32 {
    if has_test_target { 2 } else { 1 }
}

fn test_target_receipt(
    flags: &[String],
    actual_target: crate::artifact::TestTargetIdentity,
    cargo_lock_sha256: Option<String>,
) -> Result<TestTargetReceipt> {
    let selector = if flags.iter().any(|flag| flag == "--lib") {
        Some("--lib".to_owned())
    } else if let Some(name) = last_cargo_option_value(flags, "--test")? {
        Some(format!("--test={name}"))
    } else {
        Some("selectorless".to_owned())
    };
    let selector = selector.context("test receipt selector is missing")?;
    let target_dir = last_cargo_option_value(flags, "--target-dir")?;
    let locked = flags
        .iter()
        .any(|flag| flag == "--locked" || flag == "--frozen");
    if locked && cargo_lock_sha256.is_none() {
        anyhow::bail!(
            "locked test receipt requires Cargo's workspace-root Cargo.lock to be present and hashed"
        );
    }
    Ok(TestTargetReceipt {
        selector,
        package_id: actual_target.package_id,
        artifact_name: actual_target.name,
        artifact_kind: actual_target.kind,
        target_dir,
        locked,
        cargo_lock_sha256,
    })
}

/// Return Cargo's effective value for a repeated long option. Cargo accepts both
/// `--flag value` and `--flag=value`; later occurrences override earlier ones.
/// Reject a malformed option here as well, rather than recording a value borrowed
/// from an unrelated following flag.
pub(crate) fn last_cargo_option_value(flags: &[String], option: &str) -> Result<Option<String>> {
    let equals = format!("{option}=");
    let mut selected = None;
    let mut index = 0;
    while let Some(flag) = flags.get(index) {
        if flag == option {
            let value = flags
                .get(index + 1)
                .with_context(|| format!("{option} requires an argument"))?;
            if value.is_empty() {
                anyhow::bail!("{option} requires a non-empty argument");
            }
            selected = Some(value.clone());
            index += 2;
            continue;
        } else if let Some(value) = flag.strip_prefix(&equals) {
            if value.is_empty() {
                anyhow::bail!("{option} requires a non-empty argument");
            }
            selected = Some(value.to_owned());
        }
        index += 1;
    }
    Ok(selected)
}

fn receipt_test_target(
    flags: &[String],
    actual_target: Option<crate::artifact::TestTargetIdentity>,
    cargo_lock_sha256: Option<String>,
) -> Result<Option<TestTargetReceipt>> {
    actual_target
        .map(|target| test_target_receipt(flags, target, cargo_lock_sha256))
        .transpose()
}

fn workspace_cargo_lock_sha256(ctx: &Context) -> Result<Option<String>> {
    workspace_cargo_lock_sha256_at(&ctx.workspace_root)
}

/// The context constructor already resolved Cargo metadata's workspace root.
/// Re-running `cargo locate-project` here could select a different topology from
/// the build, so the receipt binds the lock directly to that resolved root.
fn workspace_cargo_lock_sha256_at(workspace_root: &Path) -> Result<Option<String>> {
    let lock = workspace_root.join("Cargo.lock");
    lock.is_file().then(|| hash_file(&lock)).transpose()
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

fn identity_receipt(project: &ManagedProjectConfig) -> ManagedIdentityReceipt {
    let identity = &project.identity;
    ManagedIdentityReceipt {
        schema: identity.schema,
        package_id: identity.package_id.clone(),
        assembly_name: identity.assembly_name.clone(),
        root_namespace: identity.root_namespace.clone(),
        module_type: identity.module_type.clone(),
        public_namespaces: project.public_namespaces.clone(),
        compatibility_profile: project.compatibility_profile.clone(),
    }
}

fn source_identity(crate_dir: &Path, cargo_lock_sha256: Option<String>) -> Result<SourceIdentity> {
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
    Ok(SourceIdentity {
        repository,
        revision,
        dirty,
        cargo_lock_sha256,
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

    #[test]
    fn invalidating_a_prior_artifact_receipt_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let artifact = root.path().join("suite");
        fs::write(&artifact, b"apphost").unwrap();
        let receipt = PathBuf::from(format!("{}.rustdotnet.receipt.json", artifact.display()));
        fs::write(&receipt, b"stale").unwrap();
        invalidate_for_artifact(&artifact).unwrap();
        assert!(!receipt.exists());
        invalidate_for_artifact(&artifact).unwrap();
    }

    #[test]
    fn ordinary_build_selectors_do_not_create_test_target_receipts() {
        assert!(
            receipt_test_target(&["--lib".into()], None, None)
                .unwrap()
                .is_none()
        );
        assert!(
            receipt_test_target(&["--test".into(), "integration".into()], None, None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn selectorless_unique_test_receipt_records_cargo_identity() {
        let receipt = receipt_test_target(
            &[],
            Some(crate::artifact::TestTargetIdentity {
                package_id: "path+file:///tmp/crate#0.0.0".into(),
                name: "crate_name".into(),
                kind: vec!["lib".into()],
            }),
            Some("lock-hash".into()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(receipt.selector, "selectorless");
        assert_eq!(receipt.package_id, "path+file:///tmp/crate#0.0.0");
        assert_eq!(receipt.artifact_name, "crate_name");
        assert_eq!(receipt.artifact_kind, ["lib"]);
        assert_eq!(receipt.cargo_lock_sha256.as_deref(), Some("lock-hash"));
    }

    #[test]
    fn locked_test_receipt_requires_a_workspace_lock_hash() {
        let error = receipt_test_target(
            &["--lib".into(), "--locked".into()],
            Some(crate::artifact::TestTargetIdentity {
                package_id: "path+file:///tmp/crate#0.0.0".into(),
                name: "crate_name".into(),
                kind: vec!["lib".into()],
            }),
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("workspace-root Cargo.lock"));
    }

    #[test]
    fn repeated_target_dir_uses_cargos_final_value_across_both_forms() {
        assert_eq!(
            last_cargo_option_value(
                &[
                    "--target-dir".into(),
                    "first".into(),
                    "--target-dir=second".into(),
                    "--target-dir".into(),
                    "final".into(),
                ],
                "--target-dir",
            )
            .unwrap(),
            Some("final".into())
        );
    }

    #[test]
    fn repeated_test_selector_uses_cargos_final_value_across_both_forms() {
        assert_eq!(
            last_cargo_option_value(
                &["--test=first".into(), "--test".into(), "final".into(),],
                "--test",
            )
            .unwrap(),
            Some("final".into())
        );
    }

    #[test]
    fn ordinary_and_test_receipts_use_the_compatible_schemas() {
        assert_eq!(receipt_schema(false), 1);
        assert_eq!(receipt_schema(true), 2);
    }

    #[test]
    fn workspace_member_receipt_hashes_the_context_workspace_root_lock() {
        let root = tempfile::tempdir().unwrap();
        let member = root.path().join("member");
        fs::create_dir_all(member.join("src")).unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"member\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(member.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
        fs::write(root.path().join("Cargo.lock"), "workspace lock\n").unwrap();
        fs::write(member.join("Cargo.lock"), "member decoy\n").unwrap();

        assert_eq!(
            workspace_cargo_lock_sha256_at(root.path()).unwrap(),
            Some(hash_file(&root.path().join("Cargo.lock")).unwrap())
        );
        assert_ne!(
            workspace_cargo_lock_sha256_at(root.path()).unwrap(),
            Some(hash_file(&member.join("Cargo.lock")).unwrap())
        );
    }
}
