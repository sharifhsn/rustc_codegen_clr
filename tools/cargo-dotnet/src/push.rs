//! Immutable, signed NuGet publication.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, ExitStatus};

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
    let api_key = std::env::var(&args.api_key_env).with_context(|| {
        format!(
            "API key environment variable {} is not set or is not valid Unicode",
            args.api_key_env
        )
    })?;
    let fingerprint = normalize_fingerprint(&args.signer_fingerprint)?;
    let (id, version) = package_identity(&args.package)?;
    require_release_version(&version)?;
    verify_signed_package(&args.package, &fingerprint)?;

    // Deliberately omit --skip-duplicate: an existing exact version is a release failure.
    let status = push_with_ephemeral_response(
        Path::new("dotnet"),
        &args.package,
        &args.source,
        &args.api_key_env,
        &api_key,
    )?;
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

/// Invoke NuGet without placing the API key in the child process's argument vector or environment.
///
/// `dotnet nuget push` has no stdin/API-key-file option, but the top-level `dotnet` host supports
/// response files: an argv entry of `@/private/path` expands to one argument per line. Keep the
/// plaintext key in a mode-0600 response file inside a private temporary directory for only the
/// lifetime of the child. `TempDir` removes it on every ordinary success/error return. This is also
/// portable to non-Windows hosts, where NuGet.Config API-key entries cannot be used because NuGet
/// insists on Windows-only credential encryption.
fn push_with_ephemeral_response(
    dotnet: &Path,
    package: &Path,
    source: &str,
    api_key_env: &str,
    api_key: &str,
) -> Result<ExitStatus> {
    if api_key.is_empty() {
        bail!("NuGet API key must not be empty");
    }
    let package = fs::canonicalize(package)
        .with_context(|| format!("resolve package path {}", package.display()))?;
    let package = package
        .to_str()
        .context("NuGet package path is not valid Unicode")?
        .to_owned();
    let arguments = vec![
        "nuget".to_owned(),
        "push".to_owned(),
        package,
        "--source".to_owned(),
        source.to_owned(),
        "--api-key".to_owned(),
        api_key.to_owned(),
    ];
    run_dotnet_with_ephemeral_response(dotnet, &arguments, &[api_key_env])
}

/// Run the .NET host with one argument per response-file line, keeping secret-bearing arguments out
/// of the process list. Callers must list every environment variable that supplied a secret so the
/// child cannot inherit a second copy.
pub(crate) fn run_dotnet_with_ephemeral_response(
    dotnet: &Path,
    arguments: &[String],
    secret_envs: &[&str],
) -> Result<ExitStatus> {
    if arguments.is_empty() {
        bail!("dotnet response requires at least one argument");
    }
    for argument in arguments {
        if argument.contains('\r') || argument.contains('\n') {
            bail!("dotnet response arguments cannot contain a newline");
        }
    }
    let private_dir = tempfile::Builder::new()
        .prefix("cargo-dotnet-nuget-push-")
        .tempdir()
        .context("create private NuGet credential directory")?;
    let response_path = private_dir.path().join("push.rsp");
    // The dotnet host treats each response-file line as one complete argument, including spaces.
    let mut response = arguments.join("\n");
    response.push('\n');
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&response_path)
        .with_context(|| format!("create {}", response_path.display()))?;
    file.write_all(response.as_bytes())
        .with_context(|| format!("write {}", response_path.display()))?;
    file.sync_all()
        .with_context(|| format!("sync {}", response_path.display()))?;
    drop(file);

    // Deliberately omit --skip-duplicate: an existing exact version is a release failure.
    // The only real argv value is the non-secret response-file path.
    let mut command = Command::new(dotnet);
    command.arg(format!("@{}", response_path.display()));
    for env_name in secret_envs {
        command.env_remove(env_name);
    }
    command.status().context("run dotnet response file")
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

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;

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

    #[cfg(unix)]
    fn fake_dotnet(exit_code: i32) -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("dotnet");
        let script = format!(
            r#"#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
log="$root/invocation.log"
{{
  printf 'cwd=%s\n' "$PWD"
  rsp=${{1#@}}
  if test -f "$rsp"; then printf 'response=present\n'; else printf 'response=missing\n'; fi
  if grep -Eq -- '^--(api-key|certificate-password)$' "$rsp"; then printf 'secret_option=present\n'; else printf 'secret_option=missing\n'; fi
  if test "${{CARGO_DOTNET_TEST_API_KEY+x}}" = x; then printf 'api_env=present\n'; else printf 'api_env=absent\n'; fi
  if test "${{HOME+x}}" = x; then printf 'ambient_env=present\n'; else printf 'ambient_env=absent\n'; fi
  for arg in "$@"; do printf 'arg=%s\n' "$arg"; done
}} > "$log"
exit {exit_code}
"#
        );
        fs::write(&executable, script).unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&executable, permissions).unwrap();
        (root, executable)
    }

    #[cfg(unix)]
    fn assert_ephemeral_push(exit_code: i32) {
        let (root, dotnet) = fake_dotnet(exit_code);
        let package = root.path().join("probe.1.0.0.nupkg");
        fs::write(&package, b"not inspected by fake dotnet").unwrap();
        let secret = "sentinel API key <&\"";
        let status = push_with_ephemeral_response(
            &dotnet,
            &package,
            "https://packages.example.test/v3/index.json?a=1&b=2",
            "CARGO_DOTNET_TEST_API_KEY",
            secret,
        )
        .unwrap();
        assert_eq!(status.code(), Some(exit_code));

        let log = fs::read_to_string(root.path().join("invocation.log")).unwrap();
        assert!(log.contains("response=present"), "{log}");
        assert!(log.contains("secret_option=present"), "{log}");
        assert!(log.contains("api_env=absent"), "{log}");
        assert!(log.contains("arg=@"), "{log}");
        assert!(!log.contains("--api-key"), "{log}");
        assert!(!log.contains(secret), "{log}");
        let response_path = log
            .lines()
            .find_map(|line| line.strip_prefix("arg=@"))
            .unwrap();
        assert!(
            !Path::new(response_path).exists(),
            "temporary credential response survived: {response_path}"
        );
        assert!(!Path::new(response_path).parent().unwrap().exists());
    }

    #[cfg(unix)]
    #[test]
    fn push_hides_api_key_and_cleans_credentials_after_success() {
        assert_ephemeral_push(0);
    }

    #[cfg(unix)]
    #[test]
    fn push_hides_api_key_and_cleans_credentials_after_failure() {
        assert_ephemeral_push(23);
    }

    #[cfg(unix)]
    #[test]
    fn response_hides_signing_password_and_cleans_credentials() {
        let (root, dotnet) = fake_dotnet(0);
        let secret = "sentinel signing password";
        let arguments = vec![
            "nuget".to_owned(),
            "sign".to_owned(),
            "/tmp/package with spaces.nupkg".to_owned(),
            "--certificate-password".to_owned(),
            secret.to_owned(),
        ];
        let status = run_dotnet_with_ephemeral_response(&dotnet, &arguments, &["HOME"]).unwrap();
        assert!(status.success());

        let log = fs::read_to_string(root.path().join("invocation.log")).unwrap();
        assert!(log.contains("response=present"), "{log}");
        assert!(log.contains("secret_option=present"), "{log}");
        assert!(log.contains("ambient_env=absent"), "{log}");
        assert!(!log.contains(secret), "{log}");
        let response_path = log
            .lines()
            .find_map(|line| line.strip_prefix("arg=@"))
            .unwrap();
        assert!(!Path::new(response_path).exists());
        assert!(!Path::new(response_path).parent().unwrap().exists());
    }
}
