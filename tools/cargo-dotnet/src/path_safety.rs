//! Filesystem boundary checks for values that originate in CLI arguments or project manifests.

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, bail};

pub(crate) fn validate_nuget_id(id: &str) -> Result<()> {
    validate_component("NuGet package id", id, 100, |byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
    })
}

pub(crate) fn validate_nuget_version(version: &str) -> Result<()> {
    // NuGet accepts legacy four-component versions in addition to SemVer. Keep that compatibility
    // while rejecting every byte that can affect path interpretation or shell/tool arguments.
    validate_component("NuGet package version", version, 128, |byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_')
    })
}

pub(crate) fn validate_path_component(label: &str, value: &str) -> Result<()> {
    validate_component(label, value, 128, |byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
    })
}

fn validate_component(
    label: &str,
    value: &str,
    max_len: usize,
    allowed: impl Fn(u8) -> bool,
) -> Result<()> {
    if value.is_empty() || value.len() > max_len {
        bail!("{label} must contain between 1 and {max_len} ASCII characters");
    }
    if value == "." || value == ".." || !value.is_ascii() || !value.bytes().all(allowed) {
        bail!("invalid {label} {value:?}");
    }
    if !value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        || !value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        bail!("{label} must start and end with an ASCII letter or digit");
    }
    Ok(())
}

pub(crate) fn package_cache_dir(base: &Path, id: &str, version: &str) -> Result<PathBuf> {
    validate_nuget_id(id)?;
    validate_nuget_version(version)?;
    Ok(base.join(id.to_ascii_lowercase()).join(version))
}

pub(crate) fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!(
            "path must be a non-empty normalized relative path: {}",
            path.display()
        );
    }
    Ok(())
}

pub(crate) fn canonical_file_within(root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_path(relative)?;
    let canonical_root = fs::canonicalize(root)
        .with_context(|| format!("resolving boundary root {}", root.display()))?;
    let candidate = root.join(relative);
    let canonical = fs::canonicalize(&candidate)
        .with_context(|| format!("resolving contained file {}", candidate.display()))?;
    if !canonical.starts_with(&canonical_root) || !canonical.is_file() {
        bail!(
            "path escapes its project root: {} (root {})",
            candidate.display(),
            canonical_root.display()
        );
    }
    Ok(canonical)
}

pub(crate) fn remove_dir_all_within(root: &Path, target: &Path) -> Result<()> {
    if !target.exists() {
        return Ok(());
    }
    let canonical_root = fs::canonicalize(root)
        .with_context(|| format!("resolving deletion boundary {}", root.display()))?;
    let canonical_target = fs::canonicalize(target)
        .with_context(|| format!("resolving deletion target {}", target.display()))?;
    if canonical_target == canonical_root || !canonical_target.starts_with(&canonical_root) {
        bail!(
            "refusing to recursively delete {} outside boundary {}",
            canonical_target.display(),
            canonical_root.display()
        );
    }
    fs::remove_dir_all(target).with_context(|| format!("removing {}", target.display()))
}

pub(crate) fn planned_absolute(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        bail!("path must be normalized: {}", path.display());
    }
    if path.exists() {
        return fs::canonicalize(path)
            .with_context(|| format!("resolving install destination {}", path.display()));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .context("install destination must have a final path component")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("creating install parent {}", parent.display()))?;
    Ok(fs::canonicalize(parent)?.join(name))
}

pub(crate) fn reject_ancestor_of(
    install_home: &Path,
    protected: impl IntoIterator<Item = (&'static str, PathBuf)>,
) -> Result<()> {
    let install_home = planned_absolute(install_home)?;
    for (label, path) in protected {
        let protected = if path.exists() {
            fs::canonicalize(&path)
                .with_context(|| format!("resolving protected {label} {}", path.display()))?
        } else {
            planned_absolute(&path)?
        };
        if protected == install_home || protected.starts_with(&install_home) {
            bail!(
                "SDK install home {} would contain protected {label} {}",
                install_home.display(),
                protected.display()
            );
        }
    }
    Ok(())
}

/// An existing SDK home is replaceable only when it is empty or carries the VERSION ownership
/// marker written by setup/release bundles. This prevents a typo in `--home --force` from turning
/// an arbitrary directory into a recursively-deleted transaction backup.
pub(crate) fn require_owned_or_empty_sdk_home(home: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(home) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| format!("inspecting {}", home.display())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "SDK install home is not a regular directory: {}",
            home.display()
        );
    }
    if fs::read_dir(home)?.next().transpose()?.is_none() {
        return Ok(());
    }
    let marker = home.join("VERSION");
    let marker_metadata = fs::symlink_metadata(&marker).with_context(|| {
        format!(
            "non-empty SDK home has no ownership marker: {}",
            marker.display()
        )
    })?;
    if marker_metadata.file_type().is_symlink() || !marker_metadata.is_file() {
        bail!(
            "SDK ownership marker is not a regular file: {}",
            marker.display()
        );
    }
    let text = fs::read_to_string(&marker)
        .with_context(|| format!("reading SDK ownership marker {}", marker.display()))?;
    let has = |key: &str| {
        text.lines().any(|line| {
            line.split_once('=').is_some_and(|(candidate, value)| {
                candidate.trim() == key && !value.trim().is_empty()
            })
        })
    };
    if !text.lines().any(|line| {
        line.split_once('=')
            .is_some_and(|(key, value)| key.trim() == "schema" && value.trim() == "1")
    }) || !has("release_tag")
        || !has("host_rid")
        || !has("toolchain")
    {
        bail!("SDK ownership marker is invalid: {}", marker.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_components_reject_path_syntax() {
        for value in ["../escape", "a/b", "a\\b", ".", "..", " leading"] {
            assert!(validate_nuget_id(value).is_err(), "{value:?}");
            assert!(validate_nuget_version(value).is_err(), "{value:?}");
        }
        assert!(validate_nuget_id("Microsoft.Extensions.Logging").is_ok());
        assert!(validate_nuget_version("10.0.0-preview.1+build_2").is_ok());
        assert!(validate_nuget_version("1.0.0.0").is_ok());
    }

    #[test]
    fn recursive_delete_cannot_remove_boundary() {
        let root = tempfile::tempdir().unwrap();
        assert!(remove_dir_all_within(root.path(), root.path()).is_err());
        let child = root.path().join("child");
        fs::create_dir(&child).unwrap();
        remove_dir_all_within(root.path(), &child).unwrap();
        assert!(!child.exists());
        assert!(root.path().exists());
    }

    #[test]
    fn unrelated_nonempty_directory_is_not_an_sdk_home() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("unrelated"), b"keep").unwrap();
        assert!(require_owned_or_empty_sdk_home(root.path()).is_err());
        assert_eq!(fs::read(root.path().join("unrelated")).unwrap(), b"keep");

        fs::remove_file(root.path().join("unrelated")).unwrap();
        assert!(require_owned_or_empty_sdk_home(root.path()).is_ok());
        fs::write(
            root.path().join("VERSION"),
            "schema = 1\nrelease_tag = untagged\nhost_rid = linux-x64\ntoolchain = nightly\n",
        )
        .unwrap();
        assert!(require_owned_or_empty_sdk_home(root.path()).is_ok());
    }

    #[test]
    fn install_home_cannot_be_an_ancestor_of_protected_state() {
        let root = tempfile::tempdir().unwrap();
        let protected = root.path().join("repo/target/debug");
        fs::create_dir_all(&protected).unwrap();
        assert!(reject_ancestor_of(root.path(), [("repository", protected.clone())]).is_err());
        assert!(reject_ancestor_of(&root.path().join("sdk"), [("repository", protected)]).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn canonical_containment_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("secret");
        fs::write(&file, b"secret").unwrap();
        symlink(&file, project.path().join("linked")).unwrap();
        assert!(canonical_file_within(project.path(), Path::new("linked")).is_err());
    }
}
