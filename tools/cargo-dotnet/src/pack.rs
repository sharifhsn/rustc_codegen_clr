//! `pack` — produce a NuGet `.nupkg` of the crate's .NET assembly, NATIVE (no bash).
//!
//! Rewrite of the bash `cd_pack` (feasibility/cargo-dotnet :408-551): build the crate's
//! cdylib via the SAME native build pipeline, read name/version from cargo_metadata
//! (typed, replacing the grep -o), then assemble a valid OPC (Open Packaging
//! Conventions) `.nupkg` with the `zip` crate (replacing the hand-rolled `zip -X` +
//! heredocs + uuidgen). `dotnet pack` only packs a .csproj (the Rust lib has none), so a
//! dependency-free in-memory OPC build is the deterministic route.
//!
//! Output: `<crate>/target/nupkg/<id>.<version>.nupkg` (override with `--out`).
//!
//! **Runtime completeness for a consumer who never runs `cargo dotnet`** (a plain
//! `dotnet add package`/`<PackageReference>`, exactly as if this were any other C# library):
//!   * `add-nuget`-consumed .NET dependencies are declared as REAL `.nuspec` `<dependency>`
//!     entries (`nuget::recorded_dependencies`), not bundled as raw dlls. This is the idiomatic
//!     NuGet path and, critically, the only one that gets RID-specific native assets right (e.g.
//!     `Microsoft.EntityFrameworkCore.Sqlite`'s SQLitePCLRaw native driver) — bundling flat dlls
//!     ourselves has no way to replicate NuGet's own `runtimes/<rid>/native/...` targeting, and
//!     would risk shipping a second, conflicting copy of an assembly the consumer's own project
//!     already references transitively (e.g. `Microsoft.Extensions.*`).
//!   * `mycorrhiza::linq`'s `&`/`|` combinators and `mycorrhiza::dynamic` need
//!     `Mycorrhiza.Interop.Helpers.dll` at runtime — NOT a published NuGet package anywhere, so it
//!     can't be a `<dependency>`; it's bundled directly into `lib/<tfm>/` alongside the crate's own
//!     assembly instead (`interop_helpers::dll_bytes_if_needed`, gated on the crate depending on
//!     `mycorrhiza` at all, same criterion `build`/`run` already use).

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::artifact::{self, Artifact};
use crate::cli::{BuildArgs, PackArgs};
use crate::context::Context;
use crate::{buildstd, interop_helpers, nuget, overlays, provenance};

pub fn run(args: &PackArgs) -> Result<i32> {
    // Build a BuildArgs view so we reuse the same Context resolution + stage pipeline.
    let build_args = BuildArgs {
        path: args.path.clone(),
        release: args.release,
        debug: args.debug,
        clean: false,
        verbose: false,
        backend: None, // pack is native-only (it builds a cdylib through the Rust stages).
        dotnet: args.dotnet.clone(),
        features: clap_cargo::Features::default(),
        manifest: clap_cargo::Manifest::default(),
        workspace: clap_cargo::Workspace::default(),
        extra: Vec::new(),
        prog_args: Vec::new(),
    };
    let ctx = Context::resolve(&build_args, false)?;

    // ---- crate name + version from cargo_metadata (typed) ----
    let meta = cargo_metadata::MetadataCommand::new()
        .manifest_path(ctx.crate_dir.join("Cargo.toml"))
        .no_deps()
        .exec()
        .context("pack: `cargo metadata` failed")?;
    let pkg = meta
        .packages
        .first()
        .context("pack: no package in cargo metadata")?;
    let assembly_name = ctx
        .managed_identity
        .as_ref()
        .map(|identity| identity.assembly_name.clone())
        .unwrap_or_else(|| pkg.name.clone());
    let name = args
        .id
        .clone()
        .or_else(|| {
            ctx.managed_identity
                .as_ref()
                .map(|identity| identity.package_id.clone())
        })
        .unwrap_or_else(|| pkg.name.clone());
    let ver = args
        .version
        .clone()
        .unwrap_or_else(|| pkg.version.to_string());
    if name.is_empty() {
        bail!("pack: could not determine crate name (pass --id)");
    }
    if ver.is_empty() {
        bail!("pack: could not determine crate version (pass --version)");
    }
    if args.sign_certificate.is_some() {
        require_signed_release_inputs(args, &ctx.crate_dir, &ver)?;
    }
    // ---- real metadata sourced from the crate's own Cargo.toml, not placeholders ----
    let authors = if pkg.authors.is_empty() {
        "cargo dotnet".to_string()
    } else {
        pkg.authors.join(", ")
    };
    let desc = pkg.description.clone().unwrap_or_else(|| {
        format!("Rust crate '{name}' compiled to a .NET assembly by rustc_codegen_clr (cargo dotnet pack).")
    });
    let license = pkg.license.clone();
    let repo = pkg.repository.clone();
    let readme_path = pkg.readme().map(|p| p.into_std_path_buf());
    eprintln!(
        "== cargo dotnet pack: {name} {ver} ({}) ==",
        ctx.profile.dir()
    );

    // ---- build the cdylib via the SAME native pipeline ----
    let _build_lock = crate::build_lock::BuildLock::acquire_crate(&ctx)?;
    let private_sysroot = crate::private_sysroot::prepare(&ctx)?;
    overlays::apply(&ctx)?;
    let json = buildstd::build_with_sysroot(&ctx, &private_sysroot)?;
    let art = artifact::locate(&json, &ctx)?;
    let artifact_receipt = crate::receipt::write(&ctx, &art, &private_sysroot)?
        .context("pack: artifact build did not produce a receipt")?;
    let dll = match art {
        Artifact::Library { dll, .. } => dll,
        _ => bail!(
            "pack: produced no library assembly (is [lib] crate-type = [\"cdylib\"]? \
             does --id match the [package] name?)"
        ),
    };
    if !dll.is_file() {
        bail!("pack: produced assembly not found: {}", dll.display());
    }
    let xml_path = dll.with_extension("xml");
    let pdb_path = dll.with_extension("pdb");
    let xml_docs = xml_path
        .is_file()
        .then(|| fs::read(&xml_path))
        .transpose()?;
    let pdb = pdb_path
        .is_file()
        .then(|| fs::read(&pdb_path))
        .transpose()?;
    if args.validate && xml_docs.is_none() {
        bail!(
            "pack validation: required XML documentation is missing: {}",
            xml_path.display()
        );
    }
    let components = provenance::cargo_inventory(&ctx.crate_dir.join("Cargo.toml"))?;
    let sbom = provenance::sbom_json(&components)?;
    let licenses = provenance::licenses_json(&components)?;
    let artifact_provenance = provenance::artifact_provenance(&artifact_receipt)?;

    // ---- assemble the OPC .nupkg ----
    let out_dir = args
        .out
        .clone()
        .unwrap_or_else(|| ctx.crate_dir.join("target/nupkg"));
    fs::create_dir_all(&out_dir).with_context(|| format!("mkdir -p {}", out_dir.display()))?;
    let nupkg = out_dir.join(format!("{name}.{ver}.nupkg"));
    let _ = fs::remove_file(&nupkg);

    let dll_bytes = fs::read(&dll).with_context(|| format!("read {}", dll.display()))?;
    let readme_bytes = readme_path.as_ref().and_then(|p| fs::read(p).ok());
    let meta = NuspecMeta {
        authors: &authors,
        description: &desc,
        license: license.as_deref(),
        repository: repo.as_deref(),
        has_readme: readme_bytes.is_some(),
    };

    // Real transitive NuGet dependencies for anything this crate pulled in via `add-nuget` — see
    // the module doc for why this is a `<dependency>` and not a bundled dll.
    let dependencies = nuget::recorded_dependencies(&ctx.crate_dir)?;
    for (id, ver) in &dependencies {
        eprintln!("== cargo dotnet pack: declaring NuGet dependency {id} {ver} ==");
    }

    // The interop-helpers companion assembly, IFF this crate depends on `mycorrhiza` — see the
    // module doc. `Ok(None)` (not an error) for a crate that doesn't need it.
    let helper_dll = interop_helpers::dll_bytes_if_needed(&ctx)?;
    if helper_dll.is_some() {
        eprintln!(
            "== cargo dotnet pack: bundling {} (mycorrhiza dependency detected) ==",
            interop_helpers::HELPER_DLL_NAME
        );
    }

    // `add-nuget` owns a complete SDK-selected graph under the crate.  Package it under its
    // original NuGet paths: `copy_assets` flattens for CLR probing beside an executable, but a
    // `.nupkg` must retain RID/native/culture directories for the consumer SDK to select.
    let staged_assets = nuget::staged_package_assets(&ctx.crate_dir)?;
    if !staged_assets.is_empty() {
        eprintln!(
            "== cargo dotnet pack: preserving {} staged runtime/native/resource NuGet assets ==",
            staged_assets.len()
        );
    }

    write_nupkg(
        &nupkg,
        &name,
        &assembly_name,
        &ver,
        &dll_bytes,
        ctx.dotnet.tfm(),
        &meta,
        readme_bytes.as_deref(),
        &dependencies,
        helper_dll
            .as_deref()
            .map(|b| (interop_helpers::HELPER_DLL_NAME, b)),
        &staged_assets,
        xml_docs.as_deref(),
        pdb.as_deref(),
        &artifact_provenance,
        &sbom,
        &licenses,
    )?;
    if args.validate {
        validate_nupkg(&nupkg, &name, &assembly_name, ctx.dotnet.tfm())?;
    }
    if let Some(certificate) = &args.sign_certificate {
        sign_and_verify(
            &nupkg,
            certificate,
            args.sign_password_env.as_deref(),
            args.timestamper.as_deref(),
            args.signer_fingerprint.as_deref().unwrap(),
        )?;
    }
    let package_bytes = fs::read(&nupkg)?;
    let package_sha256 = format!("{:x}", Sha256::digest(&package_bytes));
    let checksum_path = PathBuf::from(format!("{}.sha256", nupkg.display()));
    fs::write(
        &checksum_path,
        format!(
            "{}  {}\n",
            package_sha256,
            nupkg
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("package.nupkg")
        ),
    )?;
    let entry_hashes = package_entry_hashes(&nupkg)?;
    let package_receipt_path =
        PathBuf::from(format!("{}.rustdotnet.receipt.json", nupkg.display()));
    fs::write(
        &package_receipt_path,
        provenance::package_receipt(&nupkg, &entry_hashes)?,
    )?;

    eprintln!();
    eprintln!("== packed: {} ==", nupkg.display());
    eprintln!("== checksum: {} ==", checksum_path.display());
    eprintln!("== receipt: {} ==", package_receipt_path.display());
    eprintln!(
        "   id={name}  version={ver}  {tfm}  lib/{tfm}/{assembly_name}.dll",
        tfm = ctx.dotnet.tfm()
    );
    eprintln!();
    eprintln!(" Consume it from a C# project via a local feed:");
    eprintln!(
        "   <PropertyGroup><RestoreSources>$(RestoreSources);{}</RestoreSources></PropertyGroup>",
        out_dir.display()
    );
    eprintln!(
        "   <ItemGroup><PackageReference Include=\"{name}\" Version=\"{ver}\" /></ItemGroup>"
    );
    eprintln!();
    eprintln!(" NOTE (cache footgun): NuGet pins {name} {ver} in ~/.nuget/packages. After");
    eprintln!(" changing the Rust and re-packing the SAME version, clear the cache or bump");
    eprintln!(" --version: dotnet nuget locals global-packages --clear");
    Ok(0)
}

fn require_signed_release_inputs(
    args: &PackArgs,
    crate_dir: &std::path::Path,
    version: &str,
) -> Result<()> {
    if !args.release || args.debug || !args.validate {
        bail!("signed pack requires both --release and --validate");
    }
    if args.sign_password_env.is_none() || args.signer_fingerprint.is_none() {
        bail!("signed pack requires --sign-password-env and --signer-fingerprint");
    }
    crate::push::require_release_version(version)?;
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(crate_dir)
        .output()
        .context("signed pack: inspect Git revision")?;
    if !revision.status.success() || revision.stdout.is_empty() {
        bail!("signed pack requires a Git revision");
    }
    let status = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(crate_dir)
        .output()
        .context("signed pack: inspect Git status")?;
    if !status.status.success() || !status.stdout.is_empty() {
        bail!("signed pack requires a clean Git revision");
    }
    Ok(())
}

fn sign_and_verify(
    package: &std::path::Path,
    certificate: &std::path::Path,
    password_env: Option<&str>,
    timestamper: Option<&str>,
    fingerprint: &str,
) -> Result<()> {
    let env_name =
        password_env.context("signing password environment variable name is required")?;
    let password = std::env::var_os(env_name)
        .with_context(|| format!("signing password environment variable {env_name} is not set"))?;
    let fingerprint = crate::push::normalize_fingerprint(fingerprint)?;
    let mut command = Command::new("dotnet");
    command
        .args(["nuget", "sign"])
        .arg(package)
        .arg("--certificate-path")
        .arg(certificate)
        .arg("--certificate-password")
        .arg(password);
    if let Some(url) = timestamper {
        command.arg("--timestamper").arg(url);
    }
    let status = command.status().context("run `dotnet nuget sign`")?;
    if !status.success() {
        bail!("dotnet nuget sign failed");
    }
    crate::push::verify_signed_package(package, &fingerprint)
}

/// Real NuGet metadata sourced from the crate's own `Cargo.toml` (see `pack::run`), as
/// opposed to the hardcoded placeholders this used to ship. `authors`/`description`
/// always have a value (falling back to a generic default); `license`/`repository`
/// are omitted from the `.nuspec` entirely when absent, since NuGet clients treat a
/// present-but-empty `<license>`/`<repository>` element as a hard validation error.
struct NuspecMeta<'a> {
    authors: &'a str,
    description: &'a str,
    license: Option<&'a str>,
    repository: Option<&'a str>,
    has_readme: bool,
}

/// Write the OPC `.nupkg` with the zip crate. `[Content_Types].xml` is written FIRST
/// (strict OPC readers expect it as the first entry), then the rest. Stored (no
/// compression) for the small XML parts; the dll is deflated.
fn write_nupkg(
    nupkg: &PathBuf,
    name: &str,
    assembly_name: &str,
    ver: &str,
    dll: &[u8],
    tfm: &str,
    meta: &NuspecMeta<'_>,
    readme: Option<&[u8]>,
    dependencies: &[(String, String)],
    extra_assembly: Option<(&str, &[u8])>,
    staged_assets: &[nuget::StagedPackageAsset],
    xml_docs: Option<&[u8]>,
    pdb: Option<&[u8]>,
    artifact_provenance: &[u8],
    sbom: &[u8],
    licenses: &[u8],
) -> Result<()> {
    provenance::require_nonempty("artifact provenance", artifact_provenance)?;
    provenance::require_nonempty("SBOM", sbom)?;
    provenance::require_nonempty("license inventory", licenses)?;
    validate_package_entries(
        name,
        assembly_name,
        tfm,
        readme.is_some(),
        extra_assembly.map(|(filename, _)| filename),
        staged_assets,
        xml_docs.is_some(),
        pdb.is_some(),
    )?;
    // OPC does not require a random core-properties part name. A fixed name removes time/PID
    // entropy so identical inputs can produce byte-for-byte identical packages.
    let core_properties_part = "rustdotnet.psmdcp";
    let file = File::create(nupkg).with_context(|| format!("create {}", nupkg.display()))?;
    let mut zip = ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // [Content_Types].xml — FIRST.
    let content_types = "\
<?xml version=\"1.0\" encoding=\"utf-8\"?>
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">
  <Default Extension=\"dll\" ContentType=\"application/octet\" />
  <Default Extension=\"targets\" ContentType=\"application/xml\" />
  <Default Extension=\"nuspec\" ContentType=\"application/octet\" />
  <Default Extension=\"psmdcp\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\" />
  <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\" />
  <Default Extension=\"md\" ContentType=\"text/markdown\" />
  <Default Extension=\"xml\" ContentType=\"application/xml\" />
  <Default Extension=\"pdb\" ContentType=\"application/octet\" />
  <Default Extension=\"json\" ContentType=\"application/json\" />
</Types>
";
    add_entry(
        &mut zip,
        "[Content_Types].xml",
        content_types.as_bytes(),
        stored,
    )?;

    // _rels/.rels
    let rels = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">
  <Relationship Type=\"http://schemas.microsoft.com/packaging/2010/07/manifest\" Target=\"/{name}.nuspec\" Id=\"Rcd1\" />
  <Relationship Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"/package/services/metadata/core-properties/{core_properties_part}\" Id=\"Rcd2\" />
</Relationships>
"
    );
    add_entry(&mut zip, "_rels/.rels", rels.as_bytes(), stored)?;

    // package/services/metadata/core-properties/rustdotnet.psmdcp
    let core_props = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<coreProperties xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\">
  <dc:creator>{authors}</dc:creator>
  <dc:description>{description}</dc:description>
  <dc:identifier>{name}</dc:identifier>
  <version>{ver}</version>
</coreProperties>
",
        authors = xml_escape(meta.authors),
        description = xml_escape(meta.description),
    );
    add_entry(
        &mut zip,
        &format!("package/services/metadata/core-properties/{core_properties_part}"),
        core_props.as_bytes(),
        stored,
    )?;

    // <id>.nuspec — real metadata from the crate's own Cargo.toml (see `NuspecMeta`),
    // not hardcoded placeholders. `<license>`/`<repository>`/`<readme>` are each
    // omitted when the source field is absent, per NuGet's nuspec schema (an empty
    // element is a validation error, not a no-op).
    let license_elem = meta
        .license
        .map(|l| {
            format!(
                "\n    <license type=\"expression\">{}</license>",
                xml_escape(l)
            )
        })
        .unwrap_or_default();
    let repo_elem = meta
        .repository
        .map(|r| {
            format!(
                "\n    <repository type=\"git\" url=\"{}\" />",
                xml_escape(r)
            )
        })
        .unwrap_or_default();
    let readme_elem = if meta.has_readme {
        "\n    <readme>README.md</readme>"
    } else {
        ""
    };
    // Real `<dependency>` entries for anything this crate pulled in via `add-nuget` — see the
    // module doc for why this beats bundling raw dlls. Self-closing `<group .../>` when empty
    // (the common case, a crate that made no `add-nuget` calls) matches the original shape rather
    // than an open/close tag around whitespace-only content.
    let deps_group = if dependencies.is_empty() {
        format!("<group targetFramework=\"{tfm}\" />")
    } else {
        let entries: String = dependencies
            .iter()
            .map(|(id, ver)| {
                format!(
                    "\n        <dependency id=\"{}\" version=\"{}\" />",
                    xml_escape(id),
                    xml_escape(ver)
                )
            })
            .collect();
        format!("<group targetFramework=\"{tfm}\">{entries}\n      </group>")
    };
    let nuspec = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<package xmlns=\"http://schemas.microsoft.com/packaging/2013/05/nuspec.xsd\">
  <metadata>
    <id>{name}</id>
    <version>{ver}</version>
    <authors>{authors}</authors>
    <description>{description}</description>{license_elem}{repo_elem}{readme_elem}
    <dependencies>
      {deps_group}
    </dependencies>
  </metadata>
</package>
",
        authors = xml_escape(meta.authors),
        description = xml_escape(meta.description),
    );
    add_entry(
        &mut zip,
        &format!("{name}.nuspec"),
        nuspec.as_bytes(),
        stored,
    )?;

    // README.md, if the crate declares one — surfaced on nuget.org / VS package manager UIs.
    if let Some(bytes) = readme {
        add_entry(&mut zip, "README.md", bytes, deflated)?;
    }

    // build/<id>.targets — explicit <Reference> + copy-local for build/-convention consumers.
    // Also references `extra_assembly` (the mycorrhiza interop-helpers dll), when present, so a
    // legacy non-SDK-style consumer gets it too — an SDK-style project already auto-references
    // every dll placed under `lib/<tfm>/` in the package without needing this at all.
    let extra_name = extra_assembly.map(|(n, _)| n.trim_end_matches(".dll"));
    let extra_reference = extra_name
        .map(|n| {
            format!(
                "\n    <Reference Include=\"{n}\">
      <HintPath>$(MSBuildThisFileDirectory)../lib/{tfm}/{n}.dll</HintPath>
      <Private>true</Private>
    </Reference>"
            )
        })
        .unwrap_or_default();
    let targets = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<!-- Auto-generated by `cargo dotnet pack` for the Rust .NET assembly '{assembly_name}'. -->
<Project>
  <ItemGroup>
    <Reference Include=\"{assembly_name}\">
      <HintPath>$(MSBuildThisFileDirectory)../lib/{tfm}/{assembly_name}.dll</HintPath>
      <Private>true</Private>
    </Reference>{extra_reference}
  </ItemGroup>
</Project>
"
    );
    add_entry(
        &mut zip,
        &format!("build/{name}.targets"),
        targets.as_bytes(),
        stored,
    )?;

    // Keep the managed filename aligned with the CLR assembly identity. The NuGet package ID may
    // be independently overridden with --id.
    add_entry(
        &mut zip,
        &format!("lib/{tfm}/{assembly_name}.dll"),
        dll,
        deflated,
    )?;
    if let Some(bytes) = xml_docs {
        add_entry(
            &mut zip,
            &format!("lib/{tfm}/{assembly_name}.xml"),
            bytes,
            deflated,
        )?;
    }
    if let Some(bytes) = pdb {
        add_entry(
            &mut zip,
            &format!("lib/{tfm}/{assembly_name}.pdb"),
            bytes,
            deflated,
        )?;
    }
    add_entry(
        &mut zip,
        "build/rustdotnet/artifact-provenance.json",
        artifact_provenance,
        deflated,
    )?;
    add_entry(&mut zip, "build/rustdotnet/sbom.cdx.json", sbom, deflated)?;
    add_entry(
        &mut zip,
        "build/rustdotnet/licenses.json",
        licenses,
        deflated,
    )?;

    // lib/<tfm>/<extra>.dll — the mycorrhiza interop-helpers companion assembly, bundled directly
    // (not a `<dependency>`: it isn't published anywhere, see the module doc) whenever this crate
    // depends on `mycorrhiza` at all. An SDK-style consumer auto-references every dll under
    // lib/<tfm>/ in a referenced package, so this alone is enough for the common case; the
    // build/<id>.targets entry above additionally covers legacy non-SDK-style projects.
    if let Some((extra_filename, extra_bytes)) = extra_assembly {
        add_entry(
            &mut zip,
            &format!("lib/{tfm}/{extra_filename}"),
            extra_bytes,
            deflated,
        )?;
    }

    // The path comes from the SDK's project.assets.json graph and is validated above.  Do not
    // use file_name()/basename here: that would silently turn distinct RID or culture assets
    // into one platform-dependent file.
    for asset in staged_assets {
        let bytes = fs::read(&asset.source)
            .with_context(|| format!("read staged NuGet asset {}", asset.source.display()))?;
        add_entry(&mut zip, &asset.logical_path, &bytes, deflated)?;
    }

    zip.finish().context("finalize .nupkg zip")?;
    Ok(())
}

/// Minimal XML text-content escaping for values sourced from free-text `Cargo.toml`
/// fields (description/authors/license/repository) that land inside XML elements.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn add_entry(
    zip: &mut ZipWriter<File>,
    path: &str,
    bytes: &[u8],
    opts: SimpleFileOptions,
) -> Result<()> {
    zip.start_file(path, opts)
        .with_context(|| format!("zip start {path}"))?;
    zip.write_all(bytes)
        .with_context(|| format!("zip write {path}"))?;
    Ok(())
}

fn validate_nupkg(path: &PathBuf, name: &str, assembly_name: &str, tfm: &str) -> Result<()> {
    let file =
        File::open(path).with_context(|| format!("open {} for validation", path.display()))?;
    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("parse {} as OPC zip", path.display()))?;
    if zip.is_empty() || zip.by_index(0)?.name() != "[Content_Types].xml" {
        bail!("pack validation: [Content_Types].xml must be the first OPC part");
    }
    let mut names = std::collections::BTreeSet::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        let entry_name = entry.name();
        if entry_name.starts_with('/') || entry_name.split('/').any(|part| part == "..") {
            bail!("pack validation: unsafe package path {entry_name:?}");
        }
        if !names.insert(entry_name.to_string()) {
            bail!("pack validation: duplicate package path {entry_name:?}");
        }
    }
    for required in [
        format!("{name}.nuspec"),
        format!("lib/{tfm}/{assembly_name}.dll"),
        "package/services/metadata/core-properties/rustdotnet.psmdcp".to_string(),
        "_rels/.rels".to_string(),
        format!("lib/{tfm}/{assembly_name}.xml"),
        "build/rustdotnet/artifact-provenance.json".to_string(),
        "build/rustdotnet/sbom.cdx.json".to_string(),
        "build/rustdotnet/licenses.json".to_string(),
    ] {
        if !names.contains(&required) {
            bail!("pack validation: required package part is missing: {required}");
        }
    }
    for entry_name in &names {
        if entry_name.starts_with("runtimes/") {
            validate_runtime_package_path(entry_name)?;
        }
    }
    Ok(())
}

fn package_entry_hashes(path: &PathBuf) -> Result<std::collections::BTreeMap<String, String>> {
    use std::io::Read as _;
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut hashes = std::collections::BTreeMap::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        hashes.insert(name, provenance::hash_bytes(&bytes));
    }
    Ok(hashes)
}

/// Reject invalid package paths and every collision before the ZIP writer starts.  `ZipWriter`
/// permits duplicate entry names; NuGet's behavior for them is not a safe release contract.
fn validate_package_entries(
    name: &str,
    assembly_name: &str,
    tfm: &str,
    has_readme: bool,
    extra_assembly: Option<&str>,
    staged_assets: &[nuget::StagedPackageAsset],
    has_xml_docs: bool,
    has_pdb: bool,
) -> Result<()> {
    let mut entries = std::collections::BTreeSet::new();
    for entry in [
        "[Content_Types].xml".to_string(),
        "_rels/.rels".to_string(),
        "package/services/metadata/core-properties/rustdotnet.psmdcp".to_string(),
        format!("{name}.nuspec"),
        format!("build/{name}.targets"),
        format!("lib/{tfm}/{assembly_name}.dll"),
        "build/rustdotnet/artifact-provenance.json".to_string(),
        "build/rustdotnet/sbom.cdx.json".to_string(),
        "build/rustdotnet/licenses.json".to_string(),
    ]
    .into_iter()
    .chain(has_readme.then(|| "README.md".to_string()))
    .chain(extra_assembly.map(|filename| format!("lib/{tfm}/{filename}")))
    .chain(has_xml_docs.then(|| format!("lib/{tfm}/{assembly_name}.xml")))
    .chain(has_pdb.then(|| format!("lib/{tfm}/{assembly_name}.pdb")))
    {
        if !entries.insert(entry.clone()) {
            bail!("pack: duplicate built-in package entry {entry}");
        }
    }
    for asset in staged_assets {
        validate_staged_package_asset(asset)?;
        if !entries.insert(asset.logical_path.clone()) {
            bail!(
                "pack: staged NuGet asset collides with another package entry: {}",
                asset.logical_path
            );
        }
    }
    Ok(())
}

fn validate_staged_package_asset(asset: &nuget::StagedPackageAsset) -> Result<()> {
    if !is_safe_package_path(&asset.logical_path) {
        bail!(
            "pack: unsafe staged NuGet package path: {}",
            asset.logical_path
        );
    }
    match asset.kind {
        nuget::StagedPackageAssetKind::Runtime => {
            if asset.logical_path.starts_with("runtimes/") {
                validate_runtime_package_path(&asset.logical_path)?;
                if !asset.logical_path.contains("/lib/") {
                    bail!(
                        "pack: RID runtime asset is not under runtimes/<rid>/lib: {}",
                        asset.logical_path
                    );
                }
            } else if !asset.logical_path.starts_with("lib/") {
                bail!(
                    "pack: portable runtime asset is not under lib/<tfm>: {}",
                    asset.logical_path
                );
            }
        }
        nuget::StagedPackageAssetKind::Native => {
            validate_runtime_package_path(&asset.logical_path)?;
            if !asset.logical_path.contains("/native/") {
                bail!(
                    "pack: native asset is not under runtimes/<rid>/native: {}",
                    asset.logical_path
                );
            }
        }
        nuget::StagedPackageAssetKind::Resource => {
            if asset.logical_path.starts_with("runtimes/") {
                validate_runtime_package_path(&asset.logical_path)?;
                if !asset.logical_path.contains("/lib/") {
                    bail!(
                        "pack: RID resource is not under runtimes/<rid>/lib: {}",
                        asset.logical_path
                    );
                }
            } else if !asset.logical_path.starts_with("lib/") {
                bail!(
                    "pack: resource is not under lib/<tfm>: {}",
                    asset.logical_path
                );
            }
        }
    }
    if let Some(rid) = &asset.rid {
        let expected_prefix = format!("runtimes/{rid}/");
        if !asset.logical_path.starts_with(&expected_prefix) {
            bail!(
                "pack: staged asset RID {rid} disagrees with package path {}",
                asset.logical_path
            );
        }
    }
    Ok(())
}

fn validate_runtime_package_path(path: &str) -> Result<()> {
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() < 4 || parts[0] != "runtimes" || parts[1].is_empty() {
        bail!("pack validation: invalid runtime asset path {path}");
    }
    if parts[2] == "native" {
        if parts.len() < 4 || parts[3..].iter().any(|part| part.is_empty()) {
            bail!("pack validation: invalid native asset path {path}");
        }
    } else if parts[2] == "lib" {
        if parts.len() < 5 || parts[3].is_empty() || parts[4..].iter().any(|part| part.is_empty()) {
            bail!("pack validation: invalid RID managed/resource asset path {path}");
        }
    } else {
        bail!("pack validation: runtime asset must use lib or native layout: {path}");
    }
    Ok(())
}

fn is_safe_package_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_asset(
        path: &str,
        source: PathBuf,
        kind: nuget::StagedPackageAssetKind,
        rid: Option<&str>,
    ) -> nuget::StagedPackageAsset {
        nuget::StagedPackageAsset {
            logical_path: path.to_owned(),
            source,
            kind,
            rid: rid.map(str::to_owned),
        }
    }

    fn test_meta() -> NuspecMeta<'static> {
        NuspecMeta {
            authors: "RustDotnet",
            description: "RID package fixture",
            license: Some("MIT"),
            repository: None,
            has_readme: false,
        }
    }

    #[test]
    fn identical_inputs_produce_identical_nupkg_bytes() {
        let root = std::env::temp_dir().join(format!(
            "cargo-dotnet-pack-determinism-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create deterministic pack test directory");
        let first = root.join("first.nupkg");
        let second = root.join("second.nupkg");
        let meta = NuspecMeta {
            authors: "RustDotnet",
            description: "determinism fixture",
            license: Some("MIT"),
            repository: Some("https://example.invalid/repo"),
            has_readme: true,
        };
        let readme = b"# deterministic\n";
        let dependencies = vec![("Example.Dependency".to_string(), "1.2.3".to_string())];

        for path in [&first, &second] {
            write_nupkg(
                path,
                "deterministic_fixture",
                "fixture_assembly",
                "1.0.0",
                b"managed-assembly-bytes",
                "net8.0",
                &meta,
                Some(readme),
                &dependencies,
                Some(("Helper.dll", b"helper-bytes")),
                &[],
                Some(b"<doc/>"),
                None,
                b"{}",
                b"{}",
                b"{}",
            )
            .expect("write deterministic nupkg");
        }

        assert_eq!(
            fs::read(&first).expect("read first package"),
            fs::read(&second).expect("read second package")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_preserves_rid_native_and_resource_paths() {
        let root = std::env::temp_dir().join(format!(
            "cargo-dotnet-pack-rid-layout-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let runtime = root.join("runtime.dll");
        let native = root.join("libfixture.dylib");
        let resource = root.join("fixture.resources.dll");
        fs::write(&runtime, b"rid-managed").unwrap();
        fs::write(&native, b"rid-native").unwrap();
        fs::write(&resource, b"rid-resource").unwrap();
        let package = root.join("fixture.nupkg");
        let assets = vec![
            package_asset(
                "runtimes/osx-arm64/lib/net8.0/Fixture.Rid.dll",
                runtime,
                nuget::StagedPackageAssetKind::Runtime,
                Some("osx-arm64"),
            ),
            package_asset(
                "runtimes/osx-arm64/native/libfixture.dylib",
                native,
                nuget::StagedPackageAssetKind::Native,
                Some("osx-arm64"),
            ),
            package_asset(
                "runtimes/osx-arm64/lib/net8.0/ja/Fixture.resources.dll",
                resource,
                nuget::StagedPackageAssetKind::Resource,
                Some("osx-arm64"),
            ),
        ];
        write_nupkg(
            &package,
            "Fixture.Package",
            "Fixture.Assembly",
            "1.0.0",
            b"main-assembly",
            "net8.0",
            &test_meta(),
            None,
            &[],
            None,
            &assets,
            Some(b"<doc/>"),
            None,
            b"{}",
            b"{}",
            b"{}",
        )
        .unwrap();
        validate_nupkg(&package, "Fixture.Package", "Fixture.Assembly", "net8.0").unwrap();
        let file = File::open(&package).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        for expected in [
            "runtimes/osx-arm64/lib/net8.0/Fixture.Rid.dll",
            "runtimes/osx-arm64/native/libfixture.dylib",
            "runtimes/osx-arm64/lib/net8.0/ja/Fixture.resources.dll",
        ] {
            assert!(zip.by_name(expected).is_ok(), "missing {expected}");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_rejects_asset_path_collisions_and_invalid_rid_layouts() {
        let root = std::env::temp_dir().join(format!(
            "cargo-dotnet-pack-rid-invalid-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("asset.bin");
        fs::write(&source, b"asset").unwrap();
        let collision = package_asset(
            "lib/net8.0/Fixture.Assembly.dll",
            source.clone(),
            nuget::StagedPackageAssetKind::Runtime,
            None,
        );
        let collision_error = validate_package_entries(
            "Fixture.Package",
            "Fixture.Assembly",
            "net8.0",
            false,
            None,
            &[collision],
            true,
            false,
        )
        .unwrap_err();
        assert!(collision_error.to_string().contains("collides"));

        let invalid = package_asset(
            "runtimes/osx-arm64/not-native/libfixture.dylib",
            source,
            nuget::StagedPackageAssetKind::Native,
            Some("osx-arm64"),
        );
        let invalid_error = validate_package_entries(
            "Fixture.Package",
            "Fixture.Assembly",
            "net8.0",
            false,
            None,
            &[invalid],
            true,
            false,
        )
        .unwrap_err();
        assert!(
            invalid_error
                .to_string()
                .contains("runtime asset must use lib or native")
        );
        let _ = fs::remove_dir_all(root);
    }
}
