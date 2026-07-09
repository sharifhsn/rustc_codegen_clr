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
//! Only embeds the crate's own cdylib — unlike `build`/`run`, this does NOT read
//! `.cargo-dotnet-nuget-assets/` (see [`nuget`]), so a dependency wired in via
//! `add-nuget` is silently absent from the produced package.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context as _, Result};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::artifact::{self, Artifact};
use crate::cli::{BuildArgs, PackArgs};
use crate::context::Context;
use crate::{buildstd, overlays, palinject};

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
    let name = args.id.clone().unwrap_or_else(|| pkg.name.clone());
    let ver = args.version.clone().unwrap_or_else(|| pkg.version.to_string());
    if name.is_empty() {
        bail!("pack: could not determine crate name (pass --id)");
    }
    if ver.is_empty() {
        bail!("pack: could not determine crate version (pass --version)");
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
    eprintln!("== cargo dotnet pack: {name} {ver} ({}) ==", ctx.profile.dir());

    // ---- build the cdylib via the SAME native pipeline ----
    palinject::inject_all(&ctx)?;
    overlays::apply(&ctx)?;
    let json = buildstd::build(&ctx)?;
    let art = artifact::locate(&json, &ctx)?;
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

    // ---- assemble the OPC .nupkg ----
    let out_dir = args
        .out
        .clone()
        .unwrap_or_else(|| ctx.crate_dir.join("target/nupkg"));
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("mkdir -p {}", out_dir.display()))?;
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
    write_nupkg(&nupkg, &name, &ver, &dll_bytes, ctx.dotnet.tfm(), &meta, readme_bytes.as_deref())?;

    eprintln!();
    eprintln!("== packed: {} ==", nupkg.display());
    eprintln!(
        "   id={name}  version={ver}  {tfm}  lib/{tfm}/{name}.dll",
        tfm = ctx.dotnet.tfm()
    );
    eprintln!();
    eprintln!(" Consume it from a C# project via a local feed:");
    eprintln!(
        "   <PropertyGroup><RestoreSources>$(RestoreSources);{}</RestoreSources></PropertyGroup>",
        out_dir.display()
    );
    eprintln!("   <ItemGroup><PackageReference Include=\"{name}\" Version=\"{ver}\" /></ItemGroup>");
    eprintln!();
    eprintln!(" NOTE (cache footgun): NuGet pins {name} {ver} in ~/.nuget/packages. After");
    eprintln!(" changing the Rust and re-packing the SAME version, clear the cache or bump");
    eprintln!(" --version: dotnet nuget locals global-packages --clear");
    Ok(0)
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
    ver: &str,
    dll: &[u8],
    tfm: &str,
    meta: &NuspecMeta<'_>,
    readme: Option<&[u8]>,
) -> Result<()> {
    let guid = random_hex_guid();
    let file = File::create(nupkg)
        .with_context(|| format!("create {}", nupkg.display()))?;
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
</Types>
";
    add_entry(&mut zip, "[Content_Types].xml", content_types.as_bytes(), stored)?;

    // _rels/.rels
    let rels = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">
  <Relationship Type=\"http://schemas.microsoft.com/packaging/2010/07/manifest\" Target=\"/{name}.nuspec\" Id=\"Rcd1\" />
  <Relationship Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"/package/services/metadata/core-properties/{guid}.psmdcp\" Id=\"Rcd2\" />
</Relationships>
"
    );
    add_entry(&mut zip, "_rels/.rels", rels.as_bytes(), stored)?;

    // package/services/metadata/core-properties/<guid>.psmdcp
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
        &format!("package/services/metadata/core-properties/{guid}.psmdcp"),
        core_props.as_bytes(),
        stored,
    )?;

    // <id>.nuspec — real metadata from the crate's own Cargo.toml (see `NuspecMeta`),
    // not hardcoded placeholders. `<license>`/`<repository>`/`<readme>` are each
    // omitted when the source field is absent, per NuGet's nuspec schema (an empty
    // element is a validation error, not a no-op).
    let license_elem = meta
        .license
        .map(|l| format!("\n    <license type=\"expression\">{}</license>", xml_escape(l)))
        .unwrap_or_default();
    let repo_elem = meta
        .repository
        .map(|r| format!("\n    <repository type=\"git\" url=\"{}\" />", xml_escape(r)))
        .unwrap_or_default();
    let readme_elem = if meta.has_readme {
        "\n    <readme>README.md</readme>"
    } else {
        ""
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
      <group targetFramework=\"{tfm}\" />
    </dependencies>
  </metadata>
</package>
",
        authors = xml_escape(meta.authors),
        description = xml_escape(meta.description),
    );
    add_entry(&mut zip, &format!("{name}.nuspec"), nuspec.as_bytes(), stored)?;

    // README.md, if the crate declares one — surfaced on nuget.org / VS package manager UIs.
    if let Some(bytes) = readme {
        add_entry(&mut zip, "README.md", bytes, deflated)?;
    }

    // build/<id>.targets — explicit <Reference> + copy-local for build/-convention consumers.
    let targets = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<!-- Auto-generated by `cargo dotnet pack` for the Rust .NET assembly '{name}'. -->
<Project>
  <ItemGroup>
    <Reference Include=\"{name}\">
      <HintPath>$(MSBuildThisFileDirectory)../lib/{tfm}/{name}.dll</HintPath>
      <Private>true</Private>
    </Reference>
  </ItemGroup>
</Project>
"
    );
    add_entry(&mut zip, &format!("build/{name}.targets"), targets.as_bytes(), stored)?;

    // lib/<tfm>/<id>.dll — the assembly (deflated).
    add_entry(&mut zip, &format!("lib/{tfm}/{name}.dll"), dll, deflated)?;

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

/// A 32-char lowercase hex GUID (no `uuidgen` shell-out). Not a real UUID — OPC only
/// needs a unique psmdcp part name, and a time+pid-seeded hex string suffices.
fn random_hex_guid() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    // splitmix64-style scramble of two words for 32 hex chars.
    let mut a = nanos ^ (pid << 64);
    let mut b = nanos.rotate_left(40) ^ pid.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    a ^= a >> 33;
    a = a.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    b ^= b >> 29;
    b = b.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    format!("{:016x}{:016x}", (a as u64), (b as u64))
}
