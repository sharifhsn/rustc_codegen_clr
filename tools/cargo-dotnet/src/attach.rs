//! Conservative existing-project wiring for `cargo dotnet attach`.
//!
//! The command owns one marked XML block and never rewrites the rest of the project. Projects with
//! hand-authored RustDotnet wiring are rejected instead of guessed at; rerunning against an
//! identical generated block is a no-op.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::cli::AttachArgs;

const BEGIN: &str = "<!-- cargo-dotnet attach: begin -->";
const END: &str = "<!-- cargo-dotnet attach: end -->";

struct RustProject {
    compatibility_profile: String,
}

pub fn run(args: &AttachArgs) -> Result<i32> {
    let project = canonical_file(&args.project, "C# project")?;
    if project.extension().and_then(|value| value.to_str()) != Some("csproj") {
        bail!("attach expects a .csproj file, found {}", project.display());
    }
    let crate_dir = args.rust_crate.canonicalize().with_context(|| {
        format!(
            "could not resolve Rust crate: {}",
            args.rust_crate.display()
        )
    })?;
    let cargo_toml = crate_dir.join("Cargo.toml");
    if !cargo_toml.is_file() {
        bail!(
            "Rust crate has no Cargo.toml: {}",
            args.rust_crate.display()
        );
    }
    let rust_project = read_rust_project(&cargo_toml)?;
    attach(
        &project,
        &crate_dir,
        &rust_project,
        args.containers,
        args.dry_run,
    )?;
    Ok(0)
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .with_context(|| format!("could not resolve {label}: {}", path.display()))?;
    if !path.is_file() {
        bail!("{label} is not a file: {}", path.display());
    }
    Ok(path)
}

fn read_rust_project(cargo_toml: &Path) -> Result<RustProject> {
    let text = fs::read_to_string(cargo_toml)
        .with_context(|| format!("could not read {}", cargo_toml.display()))?;
    let cargo: toml::Value = toml::from_str(&text)
        .with_context(|| format!("invalid Cargo manifest: {}", cargo_toml.display()))?;
    let metadata = cargo
        .get("package")
        .and_then(|value| value.get("metadata"))
        .and_then(|value| value.get("dotnet"))
        .and_then(toml::Value::as_table)
        .context(
            "Rust crate must declare [package.metadata.dotnet] schema 1 before attachment; see docs/MANAGED_IDENTITY_AND_ABI.md",
        )?;
    if metadata
        .get("identity-schema")
        .and_then(toml::Value::as_integer)
        != Some(1)
    {
        bail!("package.metadata.dotnet.identity-schema must be integer 1");
    }
    for required in [
        "package-id",
        "assembly-name",
        "root-namespace",
        "module-type",
        "public-namespaces",
        "compatibility-profile",
    ] {
        if !metadata.contains_key(required) {
            bail!("package.metadata.dotnet.{required} is required for attachment");
        }
    }
    let compatibility_profile = metadata
        .get("compatibility-profile")
        .and_then(toml::Value::as_str)
        .context("package.metadata.dotnet.compatibility-profile must be a string")?
        .to_owned();
    if !crate::profiles::is_known(&compatibility_profile) {
        bail!(
            "unknown compatibility profile {compatibility_profile:?}; run `cargo dotnet profiles`"
        );
    }
    Ok(RustProject {
        compatibility_profile,
    })
}

fn attach(
    project: &Path,
    crate_dir: &Path,
    rust_project: &RustProject,
    containers: bool,
    dry_run: bool,
) -> Result<()> {
    let source = fs::read_to_string(project)
        .with_context(|| format!("could not read {}", project.display()))?;
    validate_project_shape(&source, &rust_project.compatibility_profile)?;

    let project_dir = project
        .parent()
        .context("C# project has no parent directory")?;
    let relative = pathdiff::diff_paths(crate_dir, project_dir).context(
        "Rust crate and C# project are on unrelated filesystem roots; move them to one solution tree",
    )?;
    let relative = relative.to_string_lossy().replace('\\', "/");
    let block = render_block(
        &xml_attribute(&relative),
        &rust_project.compatibility_profile,
        containers,
    );

    if let Some(existing) = marked_block(&source)? {
        if existing.trim() == block.trim() {
            println!(
                "cargo dotnet attach: already configured {} -> {}",
                project.display(),
                relative
            );
            return Ok(());
        }
        bail!(
            "{} already has a cargo-dotnet attachment with different settings; edit or remove the marked block explicitly",
            project.display()
        );
    }
    if source.contains("<RustCrate ") || source.contains("RustDotnet.targets") {
        bail!(
            "{} already contains hand-authored RustDotnet wiring; validate it manually or remove it before using attach",
            project.display()
        );
    }

    if dry_run {
        println!("{block}");
        return Ok(());
    }

    let close_count = source.matches("</Project>").count();
    if close_count != 1 {
        bail!(
            "expected exactly one </Project> element in {}, found {close_count}",
            project.display()
        );
    }
    let replacement = format!("{block}\n</Project>");
    let updated = source.replacen("</Project>", &replacement, 1);
    let parent = project
        .parent()
        .context("C# project has no parent directory")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "could not create a temporary file beside {}",
            project.display()
        )
    })?;
    std::io::Write::write_all(&mut temp, updated.as_bytes())
        .with_context(|| format!("could not write updated project {}", project.display()))?;
    temp.persist(project)
        .map_err(|error| error.error)
        .with_context(|| format!("could not atomically replace {}", project.display()))?;

    println!(
        "cargo dotnet attach: configured {} -> {} ({})",
        project.display(),
        relative,
        rust_project.compatibility_profile
    );
    Ok(())
}

fn validate_project_shape(source: &str, profile: &str) -> Result<()> {
    if !source.contains("<Project") || !source.contains("Sdk=") {
        bail!("attach currently supports SDK-style C# projects only");
    }
    let target_framework = property_value(source, "TargetFramework")
        .or_else(|| property_value(source, "TargetFrameworks"))
        .context(
            "attach requires an explicit TargetFramework or TargetFrameworks property so host compatibility can be validated",
        )?;
    let target_frameworks: Vec<_> = target_framework
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect();
    if target_frameworks.is_empty()
        || target_frameworks
            .iter()
            .any(|framework| !framework.starts_with("net10.0"))
    {
        bail!(
            "the public rust-dotnet SDK requires .NET 10; found target framework(s) {target_framework:?}"
        );
    }
    if let Some(existing) = property_value(source, "RustDotnetCompatibilityProfile")
        && existing != profile
    {
        bail!(
            "C# project declares RustDotnetCompatibilityProfile {existing:?}, but the Rust crate declares {profile:?}"
        );
    }
    let windows_profile = matches!(
        profile,
        "excel-dna-net10-windows" | "maui-windows-net10" | "winui3-net10-windows"
    );
    if windows_profile
        && target_frameworks
            .iter()
            .any(|framework| !framework.contains("-windows"))
    {
        bail!(
            "compatibility profile {profile:?} requires a Windows TargetFramework, such as net10.0-windows10.0.19041.0"
        );
    }
    if profile == "net10-coreclr"
        && target_frameworks.iter().any(|framework| {
            ["-android", "-ios", "-maccatalyst"]
                .iter()
                .any(|suffix| framework.contains(suffix))
        })
    {
        bail!(
            "mobile target framework(s) {target_framework:?} require a proven mobile compatibility profile; net10-coreclr is not a mobile claim"
        );
    }
    if source.contains("<UseMaui>true</UseMaui>") && profile != "maui-windows-net10" {
        bail!("a MAUI project requires the maui-windows-net10 Rust compatibility profile");
    }
    if source.contains("<UseWinUI>true</UseWinUI>") && profile != "winui3-net10-windows" {
        bail!("a WinUI project requires the winui3-net10-windows Rust compatibility profile");
    }
    if matches!(
        profile,
        "unity-netstandard2.1"
            | "maui-android-net10"
            | "maui-apple-net10"
            | "vsto-net10-in-process"
    ) {
        bail!(
            "compatibility profile {profile:?} is not attachable yet; `cargo dotnet profiles` explains the missing runtime evidence"
        );
    }
    Ok(())
}

fn property_value<'a>(source: &'a str, property: &str) -> Option<&'a str> {
    let open = format!("<{property}>");
    let close = format!("</{property}>");
    let start = source.find(&open)? + open.len();
    let end = source[start..].find(&close)? + start;
    Some(source[start..end].trim())
}

fn marked_block(source: &str) -> Result<Option<&str>> {
    let begin = source.find(BEGIN);
    let end = source.find(END);
    match (begin, end) {
        (None, None) => Ok(None),
        (Some(begin), Some(end)) if begin < end => Ok(Some(&source[begin..end + END.len()])),
        _ => bail!("project contains an incomplete or reversed cargo-dotnet attach marker block"),
    }
}

fn render_block(relative_crate: &str, profile: &str, containers: bool) -> String {
    let container_property = if containers {
        "\n    <UseRustDotnetContainers>true</UseRustDotnetContainers>"
    } else {
        ""
    };
    format!(
        "  {BEGIN}\n\
         \x20 <PropertyGroup>\n\
         \x20   <RustDotnetVersion Condition=\"'$(RustDotnetVersion)'==''\">10</RustDotnetVersion>\n\
         \x20   <RustDotnetCompatibilityProfile>{profile}</RustDotnetCompatibilityProfile>{container_property}\n\
         \x20 </PropertyGroup>\n\
         \x20 <ItemGroup>\n\
         \x20   <RustCrate Include=\"{relative_crate}\" />\n\
         \x20 </ItemGroup>\n\
         \x20 <Import Project=\"$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets\"\n\
         \x20         Condition=\"'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')\" />\n\
         \x20 <Import Project=\"$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets\"\n\
         \x20         Condition=\"'$(CARGO_DOTNET_HOME)'=='' and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')\" />\n\
         \x20 {END}"
    )
}

fn xml_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust_project(profile: &str) -> RustProject {
        RustProject {
            compatibility_profile: profile.to_owned(),
        }
    }

    #[test]
    fn attachment_preserves_existing_project_and_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("rustlib");
        fs::create_dir(&crate_dir).unwrap();
        let project = temp.path().join("host.csproj");
        fs::write(
            &project,
            "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    <TargetFramework>net10.0</TargetFramework>\n    <Unrelated>keep me</Unrelated>\n  </PropertyGroup>\n</Project>\n",
        )
        .unwrap();

        attach(
            &project,
            &crate_dir,
            &rust_project("net10-coreclr"),
            true,
            false,
        )
        .unwrap();
        let once = fs::read_to_string(&project).unwrap();
        assert!(once.contains("<Unrelated>keep me</Unrelated>"));
        assert!(once.contains("<RustCrate Include=\"rustlib\" />"));
        assert!(once.contains("<UseRustDotnetContainers>true"));
        assert_eq!(once.matches(BEGIN).count(), 1);

        attach(
            &project,
            &crate_dir,
            &rust_project("net10-coreclr"),
            true,
            false,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&project).unwrap(), once);
    }

    #[test]
    fn attachment_rejects_profile_mismatch_without_modifying_project() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("rustlib");
        fs::create_dir(&crate_dir).unwrap();
        let project = temp.path().join("host.csproj");
        let original = "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>";
        fs::write(&project, original).unwrap();

        let error = attach(
            &project,
            &crate_dir,
            &rust_project("maui-windows-net10"),
            false,
            false,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires a Windows TargetFramework")
        );
        assert_eq!(fs::read_to_string(&project).unwrap(), original);
    }

    #[test]
    fn attachment_refuses_to_merge_unknown_hand_authored_wiring() {
        let temp = tempfile::tempdir().unwrap();
        let crate_dir = temp.path().join("rustlib");
        fs::create_dir(&crate_dir).unwrap();
        let project = temp.path().join("host.csproj");
        fs::write(
            &project,
            "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup><RustCrate Include=\"old\" /></ItemGroup></Project>",
        )
        .unwrap();
        let error = attach(
            &project,
            &crate_dir,
            &rust_project("net10-coreclr"),
            false,
            false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("hand-authored"));
    }

    #[test]
    fn attachment_rejects_non_net10_and_mobile_coreclr_targets() {
        for (framework, expected) in [
            (" net9.0 ", "requires .NET 10"),
            ("net10.0-android", "not a mobile claim"),
        ] {
            let source = format!(
                "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>{framework}</TargetFramework></PropertyGroup></Project>"
            );
            let error = validate_project_shape(&source, "net10-coreclr").unwrap_err();
            assert!(error.to_string().contains(expected));
        }
    }
}
