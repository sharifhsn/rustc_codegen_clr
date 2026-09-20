//! Bundled `Mycorrhiza.Interop.Helpers` companion assembly — builds and copies it into any
//! consumer of `mycorrhiza` automatically, no per-crate setup step required.
//!
//! Background: `mycorrhiza::linq`'s `TypedPredicate` `&`/`|` combinators (see
//! `mycorrhiza/src/linq.rs`'s `rebind_param`/`PARAMETER_REBINDER_ASSEMBLY` doc comment) call into a
//! small C# `ExpressionVisitor` helper, `Mycorrhiza.Linq.ParameterRebinder`, that the Rust interop
//! bridge resolves by simple assembly name (`Mycorrhiza.Interop.Helpers`) via normal
//! `AssemblyLoadContext` probing next to the consumer's own build output — the same resolution model
//! as any other runtime dll sitting alongside the app. That C# source now lives in this repo at
//! `mycorrhiza_interop_helpers/` (a small standalone `net8.0` class-library project); this module is
//! the delivery mechanism.
//!
//! This deliberately does NOT reuse `nuget::copy_assets`'s `.cargo-dotnet-nuget-assets/` marker-dir
//! pattern: that mechanism is per-crate opt-in (a consumer only gets a dll there after explicitly
//! running `cargo dotnet add-nuget`, or hand-copying one next to a crate like `cd_linq_groupby`'s
//! `LinqGroupHelper.dll`). `Mycorrhiza.Interop.Helpers` is not crate-specific — it's a runtime
//! dependency of `mycorrhiza` itself (specifically its `linq` module), so ANY crate that depends on
//! `mycorrhiza` needs it with zero extra steps. Instead we ask Cargo for the selected package's exact
//! resolved closure (including a workspace-root lockfile) and copy when that closure contains
//! `mycorrhiza` — same end result, with no false positive from an unrelated workspace member.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::context::{Context, DotnetVersion};

/// Must match `mycorrhiza::linq::PARAMETER_REBINDER_ASSEMBLY` and the helper project's
/// `<AssemblyName>` exactly.
pub(crate) const HELPER_DLL_NAME: &str = "Mycorrhiza.Interop.Helpers.dll";
const HELPER_CACHE_SCHEMA: u32 = 2;
const HELPER_CACHE_LIMIT: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct HelperReceipt {
    schema: u32,
    key: String,
    source_sha256: String,
    tfm: String,
    host_rid: String,
    dotnet_identity_sha256: String,
    dll: String,
    dll_sha256: String,
}

struct CachedDll {
    path: PathBuf,
    _lease: crate::content_cache::CacheObject,
}

/// Build (if needed) the bundled interop-helpers project and copy its output dll into `out_dir`,
/// IFF this crate's locked dependency graph includes `mycorrhiza`. A silent no-op otherwise, and
/// also a silent no-op if the helper project isn't present at `ctx.paths.interop_helpers_root`
/// (e.g. an older Installed-mode home predating this feature) — mirrors `nuget::copy_assets`'s
/// "never fatal for a crate that doesn't need it" shape.
pub fn ensure_and_copy(ctx: &Context, out_dir: &Path) -> Result<Option<PathBuf>> {
    let Some(dll) = ensure_built(ctx)? else {
        return Ok(None);
    };
    let dest = out_dir.join(HELPER_DLL_NAME);
    fs::create_dir_all(out_dir)?;
    let bytes = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&dll.path)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".cargo-dotnet-helper-")
        .tempfile_in(out_dir)?;
    use std::io::Write as _;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(&dest)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing helper DLL {}", dest.display()))?;
    if ctx.flags.verbose {
        eprintln!("==> copied {} -> {}", HELPER_DLL_NAME, dest.display());
    }
    Ok(Some(dest))
}

/// Same gating as [`ensure_and_copy`], but returns the built dll's raw bytes instead of copying
/// it to a directory — for `pack`, which assembles its `.nupkg` in-memory rather than through a
/// staging directory. `Ok(None)` for the same "doesn't need it" cases `ensure_and_copy` no-ops on.
pub fn dll_bytes_if_needed(ctx: &Context) -> Result<Option<Vec<u8>>> {
    let Some(dll) = ensure_built(ctx)? else {
        return Ok(None);
    };
    let bytes = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&dll.path)
        .with_context(|| format!("reading {}", dll.path.display()))?;
    Ok(Some(bytes))
}

/// Shared gate + build step behind both [`ensure_and_copy`] and [`dll_bytes_if_needed`]: `Ok(None)`
/// if this crate doesn't depend on `mycorrhiza` or the helper project isn't present in this
/// install, else `Ok(Some(built_dll_path))`.
fn ensure_built(ctx: &Context) -> Result<Option<CachedDll>> {
    let root = &ctx.paths.interop_helpers_root;
    if !root.is_dir() {
        return Ok(None);
    }
    if !depends_on_mycorrhiza(ctx)? {
        return Ok(None);
    }
    Ok(Some(cached_dll(root, ctx)?))
}

/// Materialize the helper project's NuGet assets during the explicit online restore phase.
pub(crate) fn restore_if_needed(ctx: &Context) -> Result<()> {
    let root = &ctx.paths.interop_helpers_root;
    if !root.is_dir() || !depends_on_mycorrhiza(ctx)? {
        return Ok(());
    }
    // Restore is the product's explicit online preparation boundary. Prebuild the immutable helper
    // here so the later build/pack phase is a content-cache hit and performs no helper compilation.
    cached_dll(root, ctx)?;
    Ok(())
}

/// Ask Cargo for the exact package selected by this manifest, then traverse only that package's
/// resolved dependency closure. A workspace-wide lockfile is authority for versions, but package
/// names elsewhere in that lockfile are not evidence that this member needs the helper.
fn depends_on_mycorrhiza(ctx: &Context) -> Result<bool> {
    let mut selection = CargoGraphSelection::parse(&ctx.crate_dir, &ctx.flags.extra_cargo)?;
    selection.canonicalize_package_specs(ctx)?;
    let base_options = metadata_base_options(ctx, &selection);
    let mut discovery = metadata_command(ctx, &selection.manifest);
    discovery.no_deps().other_options(base_options.clone());
    let discovery = discovery
        .exec()
        .context("discover selected Cargo packages for interop-helper delivery")?;
    let selected = selected_package_ids(&discovery, &selection)?;
    let mut options = base_options;
    options.extend(resolution_feature_options(
        &discovery, &selected, &selection,
    )?);
    let mut command = metadata_command(ctx, &selection.manifest);
    command.other_options(options);
    let metadata = command
        .exec()
        .context("resolve selected Cargo graph for interop-helper delivery")?;
    selected_graph_contains_mycorrhiza(&metadata, &selection)
}

fn metadata_command(ctx: &Context, manifest: &Path) -> cargo_metadata::MetadataCommand {
    let mut command = cargo_metadata::MetadataCommand::new();
    command
        .manifest_path(manifest)
        .current_dir(&ctx.crate_dir)
        .cargo_path(&ctx.cargo)
        .env("CARGO_HOME", &ctx.paths.cargo_home);
    if let Some(toolchain) = &ctx.toolchain {
        command.env("RUSTUP_TOOLCHAIN", toolchain);
    }
    command
}

fn metadata_base_options(ctx: &Context, selection: &CargoGraphSelection) -> Vec<String> {
    let mut options = cargo_common_options(ctx, selection);
    options.extend(metadata_platform_options(&ctx.paths.target_spec));
    options
}

fn metadata_platform_options(target: &Path) -> [String; 2] {
    [
        "--filter-platform".into(),
        target.to_string_lossy().into_owned(),
    ]
}

fn cargo_common_options(ctx: &Context, selection: &CargoGraphSelection) -> Vec<String> {
    let mut options = Vec::new();
    if let Some(config) = crate::overlays::ambient_cargo_config(ctx) {
        options.push("--config".into());
        options.push(config.to_string_lossy().into_owned());
    }
    let generated = crate::overlays::generated_config_path(ctx);
    if generated.is_file() {
        options.push("--config".into());
        options.push(generated.to_string_lossy().into_owned());
        options.push("-Zjson-target-spec".into());
    }
    options.extend(selection.metadata_options.iter().cloned());
    options
}

#[derive(Debug)]
struct CargoGraphSelection {
    manifest: PathBuf,
    packages: Vec<String>,
    workspace: bool,
    excludes: Vec<String>,
    features: Vec<String>,
    all_features: bool,
    no_default_features: bool,
    metadata_options: Vec<String>,
}

/// Resolve the single package that the forwarded Cargo selectors name. The resulting directory
/// is the authority for every subsequent cargo-dotnet concern (locks, NuGet state, receipts,
/// helper detection, and artifact lookup), not merely for the helper metadata query.
pub(crate) fn resolve_single_selected_crate(
    initial_crate_dir: &Path,
    flags: &[String],
    cargo: &str,
    cargo_home: Option<&Path>,
    toolchain: Option<&str>,
) -> Result<(PathBuf, Vec<String>)> {
    let normalized_flags = normalize_manifest_flags(initial_crate_dir, flags)?;
    let mut selection = CargoGraphSelection::parse(initial_crate_dir, &normalized_flags)?;
    if selection.workspace || !selection.excludes.is_empty() {
        bail!(
            "cargo dotnet requires exactly one selected package; --workspace/--all and --exclude are not supported"
        );
    }
    if selection.packages.len() > 1 {
        bail!(
            "cargo dotnet requires exactly one selected package; received {} -p/--package selectors",
            selection.packages.len()
        );
    }

    let common_options = selection.metadata_options.clone();
    for spec in &mut selection.packages {
        // Names, name@version selectors, and Cargo's documented globs can be matched from the
        // workspace package table without requiring an already-created Cargo.lock. Source-
        // qualified/package-ID selectors need Cargo's own canonical parser.
        if !package_spec_contains_glob(spec) && spec.contains("://") {
            *spec = cargo_pkgid(
                cargo,
                initial_crate_dir,
                &selection.manifest,
                cargo_home,
                toolchain,
                &common_options,
                spec,
            )?;
        }
    }
    let mut command = cargo_metadata::MetadataCommand::new();
    command
        .manifest_path(&selection.manifest)
        .current_dir(initial_crate_dir)
        .cargo_path(cargo)
        .no_deps()
        .other_options(common_options);
    if let Some(cargo_home) = cargo_home {
        command.env("CARGO_HOME", cargo_home);
    }
    if let Some(toolchain) = toolchain {
        command.env("RUSTUP_TOOLCHAIN", toolchain);
    }
    let metadata = command
        .exec()
        .context("resolve the single Cargo package selected for cargo-dotnet")?;
    let selected = selected_package_ids(&metadata, &selection)?;
    if selected.len() != 1 {
        bail!(
            "cargo dotnet requires exactly one selected package; Cargo selected {} packages",
            selected.len()
        );
    }
    let package = metadata
        .packages
        .iter()
        .find(|package| package.id == selected[0])
        .context("selected Cargo package was omitted from metadata")?;
    let manifest = package
        .manifest_path
        .as_std_path()
        .canonicalize()
        .with_context(|| {
            format!(
                "canonicalize selected Cargo manifest {}",
                package.manifest_path
            )
        })?;
    let crate_dir = manifest
        .parent()
        .context("selected Cargo manifest has no parent directory")?
        .to_path_buf();
    let selected_flags = bind_flags_to_selected_manifest(&normalized_flags, &manifest)?;
    Ok((crate_dir, selected_flags))
}

fn bind_flags_to_selected_manifest(flags: &[String], manifest: &Path) -> Result<Vec<String>> {
    let mut bound = Vec::with_capacity(flags.len() + 2);
    let mut index = 0_usize;
    while index < flags.len() {
        match flags[index].as_str() {
            "--manifest-path" | "-p" | "--package" => {
                flags
                    .get(index + 1)
                    .with_context(|| format!("{} requires a value", flags[index]))?;
                index += 2;
                continue;
            }
            _ if flags[index].starts_with("--manifest-path=")
                || flags[index].starts_with("--package=")
                || (flags[index].starts_with("-p") && flags[index].len() > 2) =>
            {
                index += 1;
                continue;
            }
            _ => bound.push(flags[index].clone()),
        }
        index += 1;
    }
    bound.push("--manifest-path".into());
    bound.push(manifest.display().to_string());
    Ok(bound)
}

fn normalize_manifest_flags(crate_dir: &Path, flags: &[String]) -> Result<Vec<String>> {
    let mut normalized = flags.to_vec();
    let mut manifests = 0_usize;
    let mut index = 0_usize;
    while index < normalized.len() {
        if normalized[index] == "--manifest-path" {
            manifests += 1;
            let value = normalized
                .get(index + 1)
                .cloned()
                .context("--manifest-path requires a value")?;
            let path = PathBuf::from(value);
            normalized[index + 1] = absolutize_from(crate_dir, path).display().to_string();
            index += 2;
            continue;
        }
        if let Some(value) = normalized[index].strip_prefix("--manifest-path=") {
            manifests += 1;
            let path = absolutize_from(crate_dir, PathBuf::from(value));
            normalized[index] = format!("--manifest-path={}", path.display());
        }
        index += 1;
    }
    if manifests > 1 {
        bail!("cargo dotnet accepts at most one --manifest-path selector");
    }
    Ok(normalized)
}

fn absolutize_from(base: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

impl CargoGraphSelection {
    fn parse(crate_dir: &Path, flags: &[String]) -> Result<Self> {
        let mut manifest = crate_dir.join("Cargo.toml");
        let mut packages = Vec::new();
        let mut workspace = false;
        let mut excludes = Vec::new();
        let mut features = Vec::new();
        let mut all_features = false;
        let mut no_default_features = false;
        let mut metadata_options = Vec::new();
        let mut index = 0;
        while index < flags.len() {
            let flag = &flags[index];
            let mut next_value = |name: &str| -> Result<String> {
                index += 1;
                flags
                    .get(index)
                    .cloned()
                    .with_context(|| format!("{name} requires a value"))
            };
            match flag.as_str() {
                "--manifest-path" => {
                    manifest = PathBuf::from(next_value("--manifest-path")?);
                }
                "-p" | "--package" => packages.push(next_value(flag)?),
                "--exclude" => excludes.push(next_value("--exclude")?),
                "--workspace" | "--all" => workspace = true,
                "--features" | "-F" => {
                    let value = next_value(flag)?;
                    features.push(value);
                }
                "--config" => {
                    let value = next_value(flag)?;
                    metadata_options.push(flag.clone());
                    metadata_options.push(value);
                }
                "--all-features" => all_features = true,
                "--no-default-features" => no_default_features = true,
                "--locked" | "--offline" | "--frozen" => metadata_options.push(flag.clone()),
                _ => {
                    if let Some(value) = flag.strip_prefix("--manifest-path=") {
                        manifest = PathBuf::from(value);
                    } else if let Some(value) = flag.strip_prefix("--package=") {
                        packages.push(value.to_string());
                    } else if let Some(value) = flag.strip_prefix("--exclude=") {
                        excludes.push(value.to_string());
                    } else if let Some(value) = flag.strip_prefix("--features=") {
                        features.push(value.to_string());
                    } else if let Some(value) = flag.strip_prefix("--config=") {
                        metadata_options.push(format!("--config={value}"));
                    } else if let Some(value) = flag.strip_prefix("-F")
                        && !value.is_empty()
                    {
                        features.push(value.to_string());
                    } else if let Some(value) = flag.strip_prefix("-p")
                        && !value.is_empty()
                    {
                        packages.push(value.to_string());
                    }
                }
            }
            index += 1;
        }
        if !manifest.is_absolute() {
            manifest = crate_dir.join(manifest);
        }
        Ok(Self {
            manifest,
            packages,
            workspace,
            excludes,
            features,
            all_features,
            no_default_features,
            metadata_options,
        })
    }

    fn canonicalize_package_specs(&mut self, ctx: &Context) -> Result<()> {
        let common_options = cargo_common_options(ctx, self);
        for spec in self.packages.iter_mut().chain(self.excludes.iter_mut()) {
            if package_spec_contains_glob(spec) {
                continue;
            }
            *spec = cargo_pkgid(
                &ctx.cargo,
                &ctx.crate_dir,
                &self.manifest,
                Some(&ctx.paths.cargo_home),
                ctx.toolchain.as_deref(),
                &common_options,
                spec,
            )?;
        }
        Ok(())
    }
}

fn cargo_pkgid(
    cargo: &str,
    current_dir: &Path,
    manifest: &Path,
    cargo_home: Option<&Path>,
    toolchain: Option<&str>,
    common_options: &[String],
    spec: &str,
) -> Result<String> {
    let mut command = Command::new(cargo);
    command
        .current_dir(current_dir)
        .arg("pkgid")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--package")
        .arg(spec)
        .args(common_options);
    if let Some(cargo_home) = cargo_home {
        command.env("CARGO_HOME", cargo_home);
    }
    if let Some(toolchain) = toolchain {
        command.env("RUSTUP_TOOLCHAIN", toolchain);
    }
    let output = command
        .output()
        .with_context(|| format!("launch Cargo package-id resolution for {spec:?}"))?;
    if !output.status.success() {
        bail!(
            "Cargo package selection {spec:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let id = String::from_utf8(output.stdout)?;
    let id = id.trim();
    if id.is_empty() {
        bail!("Cargo returned an empty package ID for {spec:?}");
    }
    Ok(id.to_string())
}

fn package_spec_contains_glob(spec: &str) -> bool {
    spec.bytes().any(|byte| matches!(byte, b'*' | b'?' | b'['))
}

fn resolution_feature_options(
    metadata: &cargo_metadata::Metadata,
    selected_ids: &[cargo_metadata::PackageId],
    selection: &CargoGraphSelection,
) -> Result<Vec<String>> {
    let selected = metadata
        .packages
        .iter()
        .filter(|package| selected_ids.contains(&package.id))
        .collect::<Vec<_>>();
    let mut enabled = BTreeSet::new();
    if !selection.no_default_features {
        for package in &selected {
            if package.features.contains_key("default") {
                enabled.insert(format!("{}/default", package.name));
            }
        }
    }
    if selection.all_features {
        for package in &selected {
            for feature in package.features.keys() {
                enabled.insert(format!("{}/{feature}", package.name));
            }
        }
    }
    for requested in selection
        .features
        .iter()
        .flat_map(|value| value.split([',', ' ']))
        .filter(|feature| !feature.is_empty())
    {
        if requested.contains('/') {
            enabled.insert(requested.to_string());
            continue;
        }
        let mut matched = false;
        for package in &selected {
            if package.features.contains_key(requested) {
                enabled.insert(format!("{}/{requested}", package.name));
                matched = true;
            }
        }
        if !matched {
            bail!("Cargo feature {requested:?} does not exist on any selected package");
        }
    }
    let mut options = vec!["--no-default-features".to_string()];
    if !enabled.is_empty() {
        options.push("--features".into());
        options.push(enabled.into_iter().collect::<Vec<_>>().join(","));
    }
    Ok(options)
}

fn selected_package_ids(
    metadata: &cargo_metadata::Metadata,
    selection: &CargoGraphSelection,
) -> Result<Vec<cargo_metadata::PackageId>> {
    let selected_manifest = selection.manifest.canonicalize().with_context(|| {
        format!(
            "canonicalize selected Cargo manifest {}",
            selection.manifest.display()
        )
    })?;
    let workspace_members = metadata.workspace_members.iter().collect::<HashSet<_>>();
    let mut selected = Vec::new();
    if !selection.packages.is_empty() {
        for spec in &selection.packages {
            let matches = metadata
                .packages
                .iter()
                .filter(|package| {
                    workspace_members.contains(&package.id) && package_matches_spec(package, spec)
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                bail!(
                    "Cargo package selection {spec:?} matched {} workspace packages",
                    matches.len()
                );
            }
            selected.push(matches[0].id.clone());
        }
    } else if selection.workspace {
        selected.extend(metadata.workspace_members.iter().cloned());
    } else if let Some(package) = metadata.packages.iter().find(|package| {
        package
            .manifest_path
            .as_std_path()
            .canonicalize()
            .is_ok_and(|path| path == selected_manifest)
    }) {
        selected.push(package.id.clone());
    } else if selected_manifest
        == metadata
            .workspace_root
            .as_std_path()
            .join("Cargo.toml")
            .canonicalize()?
    {
        selected.extend(metadata.workspace_default_members.iter().cloned());
    } else {
        bail!(
            "Cargo metadata omitted selected package {}",
            selected_manifest.display()
        );
    }
    if selection.workspace && !selection.excludes.is_empty() {
        selected.retain(|id| {
            metadata
                .packages
                .iter()
                .find(|package| &package.id == id)
                .is_some_and(|package| {
                    !selection
                        .excludes
                        .iter()
                        .any(|spec| package_matches_spec(package, spec))
                })
        });
    }
    Ok(selected)
}

fn package_matches_spec(package: &cargo_metadata::Package, spec: &str) -> bool {
    if spec == package.id.repr {
        return true;
    }
    // Every non-glob spec is canonicalized by `cargo pkgid` on the product path. This abbreviated
    // matcher is retained for Cargo's documented package-name glob selection and for pure metadata
    // tests that do not need to spawn a second Cargo process.
    if spec.contains("://") {
        return false;
    }
    let (name, version) = spec
        .rsplit_once('@')
        .or_else(|| spec.rsplit_once(':'))
        .filter(|(_, version)| version.as_bytes().first().is_some_and(u8::is_ascii_digit))
        .map_or((spec, None), |(name, version)| (name, Some(version)));
    package_name_pattern_matches(name, package.name.as_str())
        && version
            .is_none_or(|version| partial_version_matches(version, &package.version.to_string()))
}

fn partial_version_matches(requested: &str, actual: &str) -> bool {
    actual == requested
        || actual
            .strip_prefix(requested)
            .is_some_and(|tail| tail.starts_with('.'))
}

fn package_name_pattern_matches(pattern: &str, name: &str) -> bool {
    fn matches(pattern: &[u8], name: &[u8]) -> bool {
        match pattern {
            [] => name.is_empty(),
            [b'*', rest @ ..] => {
                matches(rest, name) || (!name.is_empty() && matches(pattern, &name[1..]))
            }
            [b'?', rest @ ..] => !name.is_empty() && matches(rest, &name[1..]),
            [b'[', rest @ ..] => {
                let Some(close) = rest.iter().position(|byte| *byte == b']') else {
                    return !name.is_empty() && name[0] == b'[' && matches(rest, &name[1..]);
                };
                if name.is_empty() {
                    return false;
                }
                let class = &rest[..close];
                let (negated, class) = class
                    .strip_prefix(b"!")
                    .or_else(|| class.strip_prefix(b"^"))
                    .map_or((false, class), |class| (true, class));
                let mut hit = false;
                let mut index = 0;
                while index < class.len() {
                    if index + 2 < class.len() && class[index + 1] == b'-' {
                        hit |= (class[index]..=class[index + 2]).contains(&name[0]);
                        index += 3;
                    } else {
                        hit |= class[index] == name[0];
                        index += 1;
                    }
                }
                (hit != negated) && matches(&rest[close + 1..], &name[1..])
            }
            [literal, rest @ ..] => {
                !name.is_empty() && *literal == name[0] && matches(rest, &name[1..])
            }
        }
    }
    matches(pattern.as_bytes(), name.as_bytes())
}

fn selected_graph_contains_mycorrhiza(
    metadata: &cargo_metadata::Metadata,
    selection: &CargoGraphSelection,
) -> Result<bool> {
    let selected = selected_package_ids(metadata, selection)?;
    let resolve = metadata
        .resolve
        .as_ref()
        .context("Cargo metadata omitted the resolved dependency graph")?;
    let mut reachable = selected.into_iter().collect::<HashSet<_>>();
    loop {
        let before = reachable.len();
        for node in &resolve.nodes {
            if reachable.contains(&node.id) {
                reachable.extend(node.dependencies.iter().cloned());
            }
        }
        if reachable.len() == before {
            break;
        }
    }
    Ok(metadata
        .packages
        .iter()
        .any(|package| package.name == "mycorrhiza" && reachable.contains(&package.id)))
}

/// Return a verified immutable helper artifact, compiling it exactly once for an exact
/// source/toolchain/host/TFM fingerprint.
fn cached_dll(root: &Path, ctx: &Context) -> Result<CachedDll> {
    let dotnet = dotnet_tool(ctx);
    let dotnet_identity = dotnet.identity()?;
    let source_sha256 = helper_source_digest(root)?;
    let key = helper_key(ctx, &dotnet_identity, &source_sha256);
    let store = crate::content_cache::ContentStore::new(
        crate::context::cargo_dotnet_cache_home()?.join("helpers/v2"),
        HELPER_CACHE_LIMIT,
    )?;
    let (snapshot, _) = store.materialize(
        &key,
        |snapshot| validate_snapshot(snapshot, &key),
        |snapshot| {
            if ctx.is_offline() {
                bail!(
                    "interop-helper cache miss for {}; run `cargo dotnet restore` once without --offline/--frozen",
                    helper_tfm(ctx.dotnet)
                );
            }
            build_snapshot(
                root,
                ctx,
                snapshot,
                &key,
                &dotnet_identity,
                &source_sha256,
                &dotnet,
            )
        },
    )?;
    let path = snapshot.path().join("artifact").join(HELPER_DLL_NAME);
    Ok(CachedDll {
        path,
        _lease: snapshot,
    })
}

fn build_snapshot(
    root: &Path,
    ctx: &Context,
    snapshot: &Path,
    key: &str,
    dotnet_identity: &str,
    source_sha256: &str,
    dotnet: &rust_dotnet_sdk_core::dotnet::DotnetTool,
) -> Result<()> {
    let source = snapshot.join("source");
    copy_helper_sources(root, &source)?;
    if helper_source_digest(&source)? != source_sha256 {
        bail!(
            "interop-helper sources changed while being snapshotted; retry to build a fresh content key"
        );
    }
    let build_root = snapshot.join("build");
    fs::create_dir_all(&build_root)?;
    let mut cmd = dotnet.command();
    configure_dotnet_command(&mut cmd, ctx)?;
    cmd.arg("build")
        .arg(&source)
        .arg("-c")
        .arg("Release")
        .arg("-f")
        .arg(helper_tfm(ctx.dotnet))
        .arg("--nologo")
        .arg("-p:Deterministic=true")
        .arg("-p:ContinuousIntegrationBuild=true")
        .arg("-p:DebugType=None")
        .arg("-p:DebugSymbols=false")
        .arg(format!(
            "-p:BaseOutputPath={}/",
            build_root.join("bin").display()
        ))
        .arg(format!(
            "-p:BaseIntermediateOutputPath={}/",
            build_root.join("obj").display()
        ))
        .arg(format!(
            "-p:PathMap={}={}",
            source.display(),
            "/_/mycorrhiza-interop-helpers"
        ));
    if !ctx.flags.verbose {
        cmd.arg("-v").arg("quiet");
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn `dotnet build` for {}", source.display()))?;
    if !status.success() {
        bail!("`dotnet build -c Release` failed for {}", source.display());
    }
    if dotnet.identity()? != dotnet_identity {
        bail!("selected .NET host changed while building the interop helper");
    }
    let dll = build_root
        .join("bin/Release")
        .join(helper_tfm(ctx.dotnet))
        .join(HELPER_DLL_NAME);
    if !dll.is_file() {
        bail!(
            "expected {} to exist after building {} — check the project's <AssemblyName>/TFM",
            dll.display(),
            source.display()
        );
    }
    let artifact_dir = snapshot.join("artifact");
    fs::create_dir(&artifact_dir)?;
    let artifact = artifact_dir.join(HELPER_DLL_NAME);
    let bytes = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&dll)?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&artifact)?;
    use std::io::Write as _;
    output.write_all(&bytes)?;
    output.sync_all()?;
    let receipt = HelperReceipt {
        schema: HELPER_CACHE_SCHEMA,
        key: key.into(),
        source_sha256: source_sha256.into(),
        tfm: helper_tfm(ctx.dotnet).into(),
        host_rid: ctx.host.host_rid.into(),
        dotnet_identity_sha256: format!("{:x}", Sha256::digest(dotnet_identity.as_bytes())),
        dll: format!("artifact/{HELPER_DLL_NAME}"),
        dll_sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    let mut receipt_file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(snapshot.join("receipt.json"))?;
    receipt_file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    receipt_file.sync_all()?;
    crate::path_safety::remove_dir_all_within(snapshot, &build_root)?;
    crate::path_safety::remove_dir_all_within(snapshot, &source)?;
    Ok(())
}

fn validate_snapshot(snapshot: &Path, key: &str) -> Result<bool> {
    let dll = snapshot.join("artifact").join(HELPER_DLL_NAME);
    let receipt = snapshot.join("receipt.json");
    let dll_metadata = match fs::symlink_metadata(&dll) {
        Ok(metadata)
            if metadata.is_file()
                && !rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) =>
        {
            metadata
        }
        _ => return Ok(false),
    };
    if dll_metadata.len() == 0 {
        return Ok(false);
    }
    let receipt_metadata = match fs::symlink_metadata(&receipt) {
        Ok(metadata)
            if metadata.is_file()
                && !rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) =>
        {
            metadata
        }
        _ => return Ok(false),
    };
    if receipt_metadata.len() == 0 {
        return Ok(false);
    }
    let Ok(receipt_bytes) = rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&receipt) else {
        return Ok(false);
    };
    let Ok(receipt) = serde_json::from_slice::<HelperReceipt>(&receipt_bytes) else {
        return Ok(false);
    };
    let actual = format!(
        "{:x}",
        Sha256::digest(rust_dotnet_sdk_core::safe_fs::read_regular_nofollow(&dll)?)
    );
    Ok(receipt.schema == HELPER_CACHE_SCHEMA
        && receipt.key == key
        && receipt.dll == format!("artifact/{HELPER_DLL_NAME}")
        && receipt.dll_sha256 == actual)
}

fn helper_key(ctx: &Context, dotnet_identity: &str, source_sha256: &str) -> String {
    helper_key_for_digest(
        helper_tfm(ctx.dotnet),
        ctx.host.os,
        ctx.host.arch,
        ctx.host.host_rid,
        ctx.toolchain.as_deref().unwrap_or(""),
        dotnet_identity,
        source_sha256,
    )
}

#[cfg(test)]
fn helper_key_for(
    root: &Path,
    tfm: &str,
    host_os: &str,
    host_arch: &str,
    host_rid: &str,
    rust_toolchain: &str,
    dotnet_identity: &str,
) -> Result<String> {
    let source_sha256 = helper_source_digest(root)?;
    Ok(helper_key_for_digest(
        tfm,
        host_os,
        host_arch,
        host_rid,
        rust_toolchain,
        dotnet_identity,
        &source_sha256,
    ))
}

fn helper_key_for_digest(
    tfm: &str,
    host_os: &str,
    host_arch: &str,
    host_rid: &str,
    rust_toolchain: &str,
    dotnet_identity: &str,
    source_sha256: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(b"cargo-dotnet-interop-helper-v2\0");
    for value in [
        tfm,
        host_os,
        host_arch,
        host_rid,
        rust_toolchain,
        dotnet_identity,
        source_sha256,
        env!("CARGO_PKG_VERSION"),
    ] {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn copy_helper_sources(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        bail!(
            "interop-helper source is not a regular directory: {}",
            source.display()
        );
    }
    let source = fs::canonicalize(source)?;
    fs::create_dir(destination)?;
    let destination = fs::canonicalize(destination)?;
    copy_helper_sources_inner(&source, &source, &destination, &destination)
}

fn copy_helper_sources_inner(
    source_root: &Path,
    source: &Path,
    destination_root: &Path,
    destination: &Path,
) -> Result<()> {
    if fs::canonicalize(source)? != source || !source.starts_with(source_root) {
        bail!("interop-helper source escaped its canonical root");
    }
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if entry.file_name() == "bin" || entry.file_name() == "obj" {
            continue;
        }
        let src = entry.path();
        let dst = destination_root.join(src.strip_prefix(source_root)?);
        let metadata = fs::symlink_metadata(&src)?;
        if rust_dotnet_sdk_core::safe_fs::metadata_is_link_or_reparse(&metadata) {
            bail!(
                "interop-helper source contains a symlink: {}",
                src.display()
            );
        } else if metadata.is_dir() {
            fs::create_dir(&dst)?;
            copy_helper_sources_inner(source_root, &src, destination_root, &dst)?;
        } else if metadata.is_file() {
            let relative = src.strip_prefix(source_root)?;
            let (_, mut input) =
                rust_dotnet_sdk_core::safe_fs::open_regular_within(source_root, relative)?;
            let permissions = input.metadata()?.permissions();
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&dst)?;
            std::io::copy(&mut input, &mut output)?;
            output.set_permissions(permissions)?;
            output.sync_all()?;
        } else {
            bail!(
                "interop-helper source contains an unsupported entry: {}",
                src.display()
            );
        }
    }
    if fs::canonicalize(source)? != source || fs::canonicalize(destination)? != destination {
        bail!("interop-helper source changed while it was snapshotted");
    }
    Ok(())
}

fn helper_source_digest(root: &Path) -> Result<String> {
    crate::content_cache::tree_digest_excluding_root_entries(root, &["bin", "obj"])
}

fn dotnet_tool(ctx: &Context) -> rust_dotnet_sdk_core::dotnet::DotnetTool {
    if std::env::var_os("DOTNET_HOST_PATH").is_some_and(|path| !path.is_empty()) {
        return rust_dotnet_sdk_core::dotnet::DotnetTool::resolve(Some(helper_tfm(ctx.dotnet)));
    }
    if let Some((path_add, _)) = &ctx.dotnet_heal {
        return rust_dotnet_sdk_core::dotnet::DotnetTool::from_executable(path_add.join(
            if cfg!(windows) {
                "dotnet.exe"
            } else {
                "dotnet"
            },
        ));
    }
    rust_dotnet_sdk_core::dotnet::DotnetTool::resolve(Some(helper_tfm(ctx.dotnet)))
}

fn configure_dotnet_command(command: &mut Command, ctx: &Context) -> Result<()> {
    if let Some((path_add, dotnet_root)) = &ctx.dotnet_heal {
        let mut paths = vec![path_add.clone()];
        if let Some(current) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&current));
        }
        command.env(
            "PATH",
            std::env::join_paths(paths).context("constructing PATH for the selected dotnet")?,
        );
        command.env("DOTNET_ROOT", dotnet_root);
    }
    command.env("DOTNET_CLI_TELEMETRY_OPTOUT", "1");
    command.env("DOTNET_NOLOGO", "1");
    command.env("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1");
    Ok(())
}

/// CoreCLR 10 can consume the existing net8 helper, while Unity must receive only a
/// netstandard2.1 dependency closure. Keeping this selection next to the copy step prevents a
/// successful Unity-target build from accidentally staging the CoreCLR helper beside it.
fn helper_tfm(dotnet: DotnetVersion) -> &'static str {
    match dotnet {
        DotnetVersion::Net10 => "net8.0",
        DotnetVersion::UnityNetStandard21 => "netstandard2.1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_selects_netstandard_helper() {
        assert_eq!(helper_tfm(DotnetVersion::Net10), "net8.0");
        assert_eq!(
            helper_tfm(DotnetVersion::UnityNetStandard21),
            "netstandard2.1"
        );
    }

    #[test]
    fn cargo_graph_selection_preserves_metadata_relevant_forwarded_flags() {
        let crate_dir = Path::new("/tmp/selected-crate");
        let selection = CargoGraphSelection::parse(
            crate_dir,
            &[
                "--manifest-path=../workspace/Cargo.toml".into(),
                "-pconsumer".into(),
                "--features=managed,serde".into(),
                "--all-features".into(),
                "--no-default-features".into(),
                "--config".into(),
                "net.offline=true".into(),
                "--locked".into(),
                "--offline".into(),
            ],
        )
        .unwrap();
        assert_eq!(
            selection.manifest,
            crate_dir.join("../workspace/Cargo.toml")
        );
        assert_eq!(selection.packages, ["consumer"]);
        assert_eq!(selection.features, ["managed,serde"]);
        assert!(selection.all_features);
        assert!(selection.no_default_features);
        assert_eq!(
            selection.metadata_options,
            ["--config", "net.offline=true", "--locked", "--offline",]
        );
        assert!(
            CargoGraphSelection::parse(crate_dir, &["--all".into()])
                .unwrap()
                .workspace
        );
        assert!(package_name_pattern_matches("consumer-*", "consumer-api"));
        assert!(package_name_pattern_matches("consumer-[ab]", "consumer-a"));
        assert!(!package_name_pattern_matches("consumer-[!a]", "consumer-a"));
    }

    #[cfg(unix)]
    #[test]
    fn source_qualified_and_full_package_id_specs_are_resolved_by_cargo() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let consumer = workspace.join("consumer");
        fs::create_dir_all(consumer.join("src")).unwrap();
        fs::write(consumer.join("src/lib.rs"), "pub fn marker() {}\n").unwrap();
        fs::write(
            workspace.join("Cargo.toml"),
            "[workspace]\nmembers=['consumer']\nresolver='3'\n",
        )
        .unwrap();
        fs::write(
            consumer.join("Cargo.toml"),
            "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        let manifest = workspace.join("Cargo.toml");
        let options = Vec::new();
        let mut bootstrap = cargo_metadata::MetadataCommand::new();
        bootstrap.manifest_path(&manifest).current_dir(&workspace);
        bootstrap.exec().unwrap();
        let by_name = cargo_pkgid(
            "cargo", &workspace, &manifest, None, None, &options, "consumer",
        )
        .unwrap();
        let source_qualified = format!("file://{}#consumer@0.1.0", consumer.display());
        assert_eq!(
            cargo_pkgid(
                "cargo",
                &workspace,
                &manifest,
                None,
                None,
                &options,
                &source_qualified,
            )
            .unwrap(),
            by_name
        );
        assert_eq!(
            cargo_pkgid(
                "cargo", &workspace, &manifest, None, None, &options, &by_name,
            )
            .unwrap(),
            by_name
        );

        let mut metadata = cargo_metadata::MetadataCommand::new();
        metadata
            .manifest_path(&manifest)
            .current_dir(&workspace)
            .no_deps()
            .other_options(options);
        let metadata = metadata.exec().unwrap();
        let mut selection =
            CargoGraphSelection::parse(&workspace, &["--package".into(), source_qualified])
                .unwrap();
        selection.packages = vec![by_name];
        assert_eq!(
            selected_package_ids(&metadata, &selection).unwrap().len(),
            1
        );
    }

    #[test]
    fn positional_member_with_package_selector_resolves_one_pipeline_authority() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let member_a = workspace.join("member-a");
        let member_b = workspace.join("member-b");
        let mycorrhiza = temp.path().join("mycorrhiza");
        for root in [&member_a, &member_b, &mycorrhiza] {
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(root.join("src/lib.rs"), "pub fn marker() {}\n").unwrap();
        }
        fs::write(
            workspace.join("Cargo.toml"),
            "[workspace]\nmembers=['member-a','member-b']\nresolver='3'\n",
        )
        .unwrap();
        fs::write(
            member_a.join("Cargo.toml"),
            "[package]\nname='member-a'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        fs::write(
            member_b.join("Cargo.toml"),
            format!(
                "[package]\nname='member-b'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmycorrhiza={{path={mycorrhiza:?}}}\n"
            ),
        )
        .unwrap();
        fs::write(
            mycorrhiza.join("Cargo.toml"),
            "[package]\nname='mycorrhiza'\nversion='0.0.2'\nedition='2024'\n",
        )
        .unwrap();

        let flags = vec!["-p".into(), "member-b".into()];
        let (selected, normalized) =
            resolve_single_selected_crate(&member_a, &flags, "cargo", None, None).unwrap();
        assert_eq!(selected, member_b.canonicalize().unwrap());
        assert_eq!(
            normalized,
            [
                "--manifest-path".to_string(),
                member_b
                    .join("Cargo.toml")
                    .canonicalize()
                    .unwrap()
                    .display()
                    .to_string(),
            ]
        );

        let mut command = cargo_metadata::MetadataCommand::new();
        command.manifest_path(member_a.join("Cargo.toml"));
        let metadata = command.exec().unwrap();
        assert!(
            selected_graph_contains_mycorrhiza(
                &metadata,
                &CargoGraphSelection::parse(&member_b, &normalized).unwrap(),
            )
            .unwrap()
        );
        assert!(
            !selected_graph_contains_mycorrhiza(
                &metadata,
                &CargoGraphSelection::parse(&member_a, &[]).unwrap(),
            )
            .unwrap()
        );
    }

    #[test]
    fn workspace_selection_fails_closed_with_or_without_helper_dependency() {
        let temp = tempfile::tempdir().unwrap();
        for (name, dependency) in [
            ("without-helper", "".to_string()),
            (
                "with-helper",
                "[dependencies]\nmycorrhiza='0.0.2'\n".to_string(),
            ),
        ] {
            let root = temp.path().join(name);
            let member = root.join("member");
            fs::create_dir_all(member.join("src")).unwrap();
            fs::write(member.join("src/lib.rs"), "pub fn marker() {}\n").unwrap();
            fs::write(
                root.join("Cargo.toml"),
                "[workspace]\nmembers=['member']\nresolver='3'\n",
            )
            .unwrap();
            fs::write(
                member.join("Cargo.toml"),
                format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2024'\n{dependency}"),
            )
            .unwrap();
            let error = resolve_single_selected_crate(
                &member,
                &["--workspace".into()],
                "cargo",
                None,
                None,
            )
            .unwrap_err();
            assert!(
                format!("{error:#}").contains("exactly one selected package"),
                "{error:#}"
            );
        }
    }

    #[test]
    fn helper_graph_excludes_dependencies_for_a_different_build_target() {
        let temp = tempfile::tempdir().unwrap();
        let sdk_crate = temp.path().join("mycorrhiza");
        let consumer = temp.path().join("consumer");
        for root in [&sdk_crate, &consumer] {
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(root.join("src/lib.rs"), "pub fn marker() {}\n").unwrap();
        }
        fs::write(
            sdk_crate.join("Cargo.toml"),
            "[package]\nname='mycorrhiza'\nversion='0.0.2'\nedition='2024'\n",
        )
        .unwrap();
        fs::write(
            consumer.join("Cargo.toml"),
            format!(
                "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[target.'cfg(target_os = \"windows\")'.dependencies]\nmycorrhiza={{path={:?}}}\n",
                sdk_crate
            ),
        )
        .unwrap();
        let selection = CargoGraphSelection::parse(&consumer, &[]).unwrap();
        let read_metadata = |options: Vec<String>| {
            let mut command = cargo_metadata::MetadataCommand::new();
            command
                .manifest_path(consumer.join("Cargo.toml"))
                .current_dir(&consumer)
                .other_options(options);
            command.exec().unwrap()
        };
        assert!(
            selected_graph_contains_mycorrhiza(
                &read_metadata(vec!["--offline".into()]),
                &selection,
            )
            .unwrap(),
            "unfiltered Cargo metadata includes dependencies for every target"
        );
        let mut filtered =
            metadata_platform_options(Path::new("x86_64-unknown-linux-gnu")).to_vec();
        filtered.push("--offline".into());
        assert!(
            !selected_graph_contains_mycorrhiza(&read_metadata(filtered), &selection).unwrap(),
            "helper delivery must mirror the build target instead of the host/all-target graph"
        );
    }

    #[test]
    fn helper_key_invalidates_on_source_compiler_host_and_tfm() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("Helper.cs"), "class Helper {}\n").unwrap();
        let base = helper_key_for(
            temp.path(),
            "net8.0",
            "linux",
            "x86_64",
            "linux-x64",
            "nightly-a",
            "10.0.100",
        )
        .unwrap();
        for changed in [
            helper_key_for(
                temp.path(),
                "net10.0",
                "linux",
                "x86_64",
                "linux-x64",
                "nightly-a",
                "10.0.100",
            )
            .unwrap(),
            helper_key_for(
                temp.path(),
                "net8.0",
                "macos",
                "aarch64",
                "osx-arm64",
                "nightly-a",
                "10.0.100",
            )
            .unwrap(),
            helper_key_for(
                temp.path(),
                "net8.0",
                "linux",
                "x86_64",
                "linux-x64",
                "nightly-b",
                "10.0.100",
            )
            .unwrap(),
            helper_key_for(
                temp.path(),
                "net8.0",
                "linux",
                "x86_64",
                "linux-x64",
                "nightly-a",
                "10.0.200",
            )
            .unwrap(),
        ] {
            assert_ne!(base, changed);
        }
        fs::write(temp.path().join("Helper.cs"), "class Changed {}\n").unwrap();
        assert_ne!(
            base,
            helper_key_for(
                temp.path(),
                "net8.0",
                "linux",
                "x86_64",
                "linux-x64",
                "nightly-a",
                "10.0.100"
            )
            .unwrap()
        );
    }

    #[test]
    fn nested_workspace_member_uses_only_its_selected_installed_sdk_graph() {
        let temp = tempfile::tempdir().unwrap();
        let sdk_crate = temp.path().join("installed-sdk/crates/mycorrhiza");
        let workspace = temp.path().join("workspace");
        let consumer = workspace.join("consumer");
        let unrelated = workspace.join("unrelated");
        for root in [&sdk_crate, &consumer, &unrelated] {
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(root.join("src/lib.rs"), "pub fn marker() {}\n").unwrap();
        }
        fs::write(
            sdk_crate.join("Cargo.toml"),
            "[package]\nname='mycorrhiza'\nversion='0.0.2'\nedition='2024'\n",
        )
        .unwrap();
        fs::write(
            workspace.join("Cargo.toml"),
            "[workspace]\nmembers=['consumer','unrelated']\nresolver='3'\n",
        )
        .unwrap();
        fs::write(
            consumer.join("Cargo.toml"),
            "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        fs::write(
            unrelated.join("Cargo.toml"),
            format!(
                "[package]\nname='unrelated'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmycorrhiza={{path={:?}}}\n",
                sdk_crate
            ),
        )
        .unwrap();

        let read_metadata = || {
            let mut command = cargo_metadata::MetadataCommand::new();
            command
                .manifest_path(consumer.join("Cargo.toml"))
                .current_dir(&consumer)
                .other_options(["--offline".into()]);
            command.exec().unwrap()
        };
        let metadata = read_metadata();
        let consumer_selection = CargoGraphSelection::parse(&consumer, &[]).unwrap();
        assert!(!selected_graph_contains_mycorrhiza(&metadata, &consumer_selection).unwrap());
        assert!(workspace.join("Cargo.lock").is_file());
        assert!(!consumer.join("Cargo.lock").exists());

        let workspace_selection =
            CargoGraphSelection::parse(&workspace, &["--workspace".into(), "--offline".into()])
                .unwrap();
        assert!(selected_graph_contains_mycorrhiza(&metadata, &workspace_selection).unwrap());
        let package_selection = CargoGraphSelection::parse(
            &workspace,
            &["-p".into(), "consumer".into(), "--offline".into()],
        )
        .unwrap();
        assert!(!selected_graph_contains_mycorrhiza(&metadata, &package_selection).unwrap());

        fs::write(
            consumer.join("Cargo.toml"),
            format!(
                "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmycorrhiza={{path={:?}}}\n",
                sdk_crate
            ),
        )
        .unwrap();
        let metadata = read_metadata();
        assert!(selected_graph_contains_mycorrhiza(&metadata, &consumer_selection).unwrap());
    }

    #[test]
    fn optional_mycorrhiza_dependency_follows_selected_feature_set() {
        let temp = tempfile::tempdir().unwrap();
        let sdk_crate = temp.path().join("installed-home/crates/mycorrhiza");
        let consumer = temp.path().join("consumer");
        for root in [&sdk_crate, &consumer] {
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(root.join("src/lib.rs"), "pub fn marker() {}\n").unwrap();
        }
        fs::write(
            sdk_crate.join("Cargo.toml"),
            "[package]\nname='mycorrhiza'\nversion='0.0.2'\nedition='2024'\n",
        )
        .unwrap();
        fs::write(
            consumer.join("Cargo.toml"),
            format!(
                "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[features]\ndefault=[]\nmanaged=['dep:mycorrhiza']\n[dependencies]\nmycorrhiza={{path={:?}, optional=true}}\n",
                sdk_crate
            ),
        )
        .unwrap();

        let metadata = |options: &[&str]| {
            let mut command = cargo_metadata::MetadataCommand::new();
            command
                .manifest_path(consumer.join("Cargo.toml"))
                .current_dir(&consumer)
                .other_options(
                    options
                        .iter()
                        .map(|value| (*value).to_string())
                        .collect::<Vec<_>>(),
                );
            command.exec().unwrap()
        };
        let default_selection = CargoGraphSelection::parse(&consumer, &[]).unwrap();
        assert!(
            !selected_graph_contains_mycorrhiza(&metadata(&["--offline"]), &default_selection)
                .unwrap()
        );
        let feature_flags = vec!["--features".into(), "managed".into(), "--offline".into()];
        let feature_selection = CargoGraphSelection::parse(&consumer, &feature_flags).unwrap();
        assert!(
            selected_graph_contains_mycorrhiza(
                &metadata(&["--features", "managed", "--offline"]),
                &feature_selection,
            )
            .unwrap()
        );
        assert_eq!(feature_selection.metadata_options, vec!["--offline"]);
        assert_eq!(feature_selection.features, ["managed"]);

        fs::write(
            temp.path().join("Cargo.toml"),
            "[workspace]\nmembers=['consumer']\nresolver='3'\n",
        )
        .unwrap();
        let _ = fs::remove_file(consumer.join("Cargo.lock"));
        let workspace_flags = vec![
            "--manifest-path".into(),
            temp.path().join("Cargo.toml").display().to_string(),
            "-p".into(),
            "consumer".into(),
            "--features".into(),
            "managed".into(),
            "--offline".into(),
        ];
        let workspace_selection =
            CargoGraphSelection::parse(temp.path(), &workspace_flags).unwrap();
        let mut discovery = cargo_metadata::MetadataCommand::new();
        discovery
            .manifest_path(temp.path().join("Cargo.toml"))
            .current_dir(temp.path())
            .no_deps()
            .other_options(vec!["--offline".into()]);
        let discovery = discovery.exec().unwrap();
        let selected = selected_package_ids(&discovery, &workspace_selection).unwrap();
        let options =
            resolution_feature_options(&discovery, &selected, &workspace_selection).unwrap();
        assert_eq!(
            options,
            [
                "--no-default-features",
                "--features",
                "consumer/default,consumer/managed",
            ]
        );
        let mut resolved = cargo_metadata::MetadataCommand::new();
        resolved
            .manifest_path(temp.path().join("Cargo.toml"))
            .current_dir(temp.path())
            .other_options(
                options
                    .into_iter()
                    .chain(["--offline".into()])
                    .collect::<Vec<_>>(),
            );
        assert!(
            selected_graph_contains_mycorrhiza(&resolved.exec().unwrap(), &workspace_selection,)
                .unwrap()
        );
    }
}
