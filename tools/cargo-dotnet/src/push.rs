//! Immutable, signed NuGet publication.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::cli::PushArgs;

#[derive(Serialize)]
struct PublishReceipt<'a> {
    schema: u32,
    package: String,
    package_sha256: String,
    package_id: &'a str,
    version: &'a str,
    source: &'a str,
    signer_sha256: &'a str,
}

pub fn run(args: &PushArgs) -> Result<i32> {
    if args.source.trim().is_empty() {
        bail!("push requires an explicit non-empty --source");
    }
    let api_key = std::env::var_os(&args.api_key_env).with_context(|| {
        format!(
            "API key environment variable {} is not set",
            args.api_key_env
        )
    })?;
    let fingerprint = normalize_fingerprint(&args.signer_fingerprint)?;
    let (id, version) = package_identity(&args.package)?;
    require_release_version(&version)?;
    verify_signed_package(&args.package, &fingerprint)?;

    // Deliberately omit --skip-duplicate: an existing exact version is a release failure.
    let status = Command::new("dotnet")
        .args(["nuget", "push"])
        .arg(&args.package)
        .arg("--source")
        .arg(&args.source)
        .arg("--api-key")
        .arg(api_key)
        .status()
        .context("run `dotnet nuget push`")?;
    if !status.success() {
        return Ok(status.code().unwrap_or(1));
    }

    let bytes = fs::read(&args.package)?;
    let receipt = PublishReceipt {
        schema: 1,
        package: args.package.to_string_lossy().into_owned(),
        package_sha256: format!("{:x}", Sha256::digest(bytes)),
        package_id: &id,
        version: &version,
        source: &args.source,
        signer_sha256: &fingerprint,
    };
    let path = args.package.with_extension("nupkg.rustdotnet.publish.json");
    fs::write(&path, serde_json::to_vec_pretty(&receipt)?)?;
    eprintln!("== publish receipt: {} ==", path.display());
    Ok(0)
}

pub(crate) fn verify_signed_package(package: &Path, fingerprint: &str) -> Result<()> {
    let status = Command::new("dotnet")
        .args(["nuget", "verify"])
        .arg(package)
        .arg("--all")
        .arg("--certificate-fingerprint")
        .arg(fingerprint)
        .status()
        .context("run `dotnet nuget verify`")?;
    if !status.success() {
        bail!("signed package verification failed");
    }
    Ok(())
}

pub(crate) fn normalize_fingerprint(value: &str) -> Result<String> {
    let normalized: String = value
        .chars()
        .filter(|c| !matches!(c, ':' | ' ' | '-'))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if normalized.len() != 64 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("signer fingerprint must be exactly 32 SHA-256 bytes (64 hex digits)");
    }
    Ok(normalized)
}

pub(crate) fn require_release_version(version: &str) -> Result<()> {
    let parsed = semver::Version::parse(version).context("release version must be exact SemVer")?;
    if !parsed.build.is_empty()
        || ["0.0.0", "VERSION", "PLACEHOLDER", "TODO"]
            .iter()
            .any(|placeholder| version.eq_ignore_ascii_case(placeholder))
    {
        bail!("release version must be exact, immutable, and non-placeholder SemVer");
    }
    Ok(())
}

fn package_identity(path: &Path) -> Result<(String, String)> {
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("open nupkg")?;
    let index = (0..archive.len())
        .find(|&index| {
            archive
                .by_index(index)
                .ok()
                .is_some_and(|file| file.name().ends_with(".nuspec"))
        })
        .context("package has no .nuspec")?;
    let mut nuspec = String::new();
    use std::io::Read as _;
    archive.by_index(index)?.read_to_string(&mut nuspec)?;
    Ok((xml_value(&nuspec, "id")?, xml_value(&nuspec, "version")?))
}

fn xml_value(xml: &str, tag: &str) -> Result<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml
        .find(&open)
        .map(|position| position + open.len())
        .context("missing package identity")?;
    let end = xml[start..]
        .find(&close)
        .map(|position| start + position)
        .context("missing package identity")?;
    Ok(xml[start..end].trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_version_policy() {
        assert!(require_release_version("2.3.4").is_ok());
        assert!(require_release_version("2.3.4-rc.1").is_ok());
        assert!(require_release_version("1.0.0").is_ok());
        assert!(require_release_version("2.3").is_err());
        assert!(require_release_version("2.3.4+rebuilt").is_err());
    }

    #[test]
    fn fingerprint_policy() {
        let input = "aa:".repeat(31) + "aa";
        assert_eq!(normalize_fingerprint(&input).unwrap(), "AA".repeat(32));
        assert!(normalize_fingerprint("abc").is_err());
    }
}
