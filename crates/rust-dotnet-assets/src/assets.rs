use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{StagedPackageAsset, StagedPackageAssetKind};

/// The SDK's asset classification.  Keep this separate from the physical source path: a
/// `runtimeTargets` entry is selected by the SDK for one RID, but its logical NuGet path is
/// still the stable identifier used for collision reporting and staging.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Compile,
    Runtime,
    Native,
    Resource,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ResolvedAsset {
    pub owner: String,
    pub kind: AssetKind,
    /// Slash-normalized path within the owning NuGet package. Never an output path.
    pub logical_path: String,
    pub source: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rid: Option<String>,
    pub fallback: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetCollision {
    pub logical_path: String,
    pub owners: Vec<String>,
}

#[derive(Debug)]
pub struct ResolvedAssets {
    pub primary_dll: Option<PathBuf>,
    pub compile_dlls: Vec<PathBuf>,
    pub runtime_dlls: Vec<PathBuf>,
    pub assets: Vec<ResolvedAsset>,
    // Retained in the resolved graph for audit/reporting and exercised by the acceptance tests;
    // production staging currently consumes the already-validated `assets` projection.
    #[allow(dead_code)]
    pub collisions: Vec<AssetCollision>,
    #[allow(dead_code)]
    pub requested_rid: Option<String>,
}

#[derive(Deserialize)]
struct AssetsFile {
    targets: serde_json::Map<String, serde_json::Value>,
    libraries: serde_json::Map<String, serde_json::Value>,
    #[serde(rename = "packageFolders")]
    package_folders: serde_json::Map<String, serde_json::Value>,
}

pub fn restore(
    id: &str,
    version: &str,
    cache_root: &Path,
    rid: Option<&str>,
    tfm: &str,
    sources: &[String],
) -> Result<ResolvedAssets> {
    let restore_dir = cache_root.join("restore");
    let packages_dir = cache_root.join("packages");
    fs::create_dir_all(&restore_dir)?;
    fs::create_dir_all(&packages_dir)?;
    let project = restore_dir.join("cargo-dotnet-restore.csproj");
    fs::write(
        &project,
        format!(
            "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>{}</TargetFramework>\
             <RestorePackagesPath>{}</RestorePackagesPath>{}</PropertyGroup><ItemGroup>\
             <PackageReference Include=\"{}\" Version=\"{}\" /></ItemGroup></Project>",
            xml_escape(tfm),
            xml_escape(&packages_dir.to_string_lossy()),
            rid.map(|rid| format!("<RuntimeIdentifier>{}</RuntimeIdentifier>", xml_escape(rid)))
                .unwrap_or_default(),
            xml_escape(id),
            xml_escape(version),
        ),
    )?;
    eprintln!("== rust-dotnet assets: restoring {id} {version} with the .NET SDK ==");
    let mut command = dotnet_command(tfm);
    command.args(["restore", "--nologo", "--verbosity", "quiet"]);
    for source in sources {
        command.args(["--source", source]);
    }
    let status = command.arg(&project).status().with_context(|| {
        format!("asset restore: failed to spawn `dotnet restore` (is the {tfm} SDK installed?)")
    })?;
    if !status.success() {
        bail!("asset restore: `dotnet restore` failed for {id} {version}");
    }
    parse(&restore_dir.join("obj/project.assets.json"), id, rid)
}

fn dotnet_command(tfm: &str) -> Command {
    if let Some(host) = std::env::var_os("DOTNET_HOST_PATH").filter(|value| !value.is_empty()) {
        return Command::new(host);
    }
    let requested_major = tfm
        .strip_prefix("net")
        .and_then(|version| version.split('.').next());
    if let (Some(home), Some(major)) = (
        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")),
        requested_major,
    ) {
        let root = PathBuf::from(home).join(".dotnet");
        let host = root.join(if cfg!(windows) {
            "dotnet.exe"
        } else {
            "dotnet"
        });
        let shared = root.join("shared/Microsoft.NETCore.App");
        let has_runtime = std::fs::read_dir(shared).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|version| version.starts_with(&format!("{major}.")))
            })
        });
        if host.is_file() && has_runtime {
            return Command::new(host);
        }
    }
    Command::new("dotnet")
}

fn parse(path: &Path, root_id: &str, requested_rid: Option<&str>) -> Result<ResolvedAssets> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let assets: AssetsFile =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let (target_name, target) = assets
        .targets
        .iter()
        .find(|(name, _)| requested_rid.is_some_and(|rid| name.ends_with(&format!("/{rid}"))))
        .or_else(|| assets.targets.iter().find(|(name, _)| !name.contains('/')))
        .or_else(|| assets.targets.iter().next())
        .context("asset restore produced no target graph")?;
    let target = target
        .as_object()
        .context("asset restore target graph is not an object")?;

    let target_rid = target_name.rsplit_once('/').map(|(_, rid)| rid);
    let selected_rid = requested_rid.or(target_rid).map(str::to_owned);
    let mut graph = BTreeSet::new();
    for (library_key, node) in target {
        let Some(library) = assets.libraries.get(library_key) else {
            continue;
        };
        if library.get("type").and_then(|v| v.as_str()) != Some("package") {
            continue;
        }
        let package_path = library
            .get("path")
            .and_then(|v| v.as_str())
            .with_context(|| format!("asset restore package {library_key} has no library path"))?;
        let node = node
            .as_object()
            .with_context(|| format!("asset restore target node {library_key} is not an object"))?;
        collect_group(
            &assets.package_folders,
            package_path,
            library_key,
            AssetKind::Compile,
            node.get("compile"),
            None,
            false,
            &mut graph,
        )?;
        collect_group(
            &assets.package_folders,
            package_path,
            library_key,
            AssetKind::Runtime,
            node.get("runtime"),
            None,
            false,
            &mut graph,
        )?;
        // RID-specific target graphs commonly project selected native assets into a direct
        // `native` group instead of retaining the richer `runtimeTargets` entries.
        collect_group(
            &assets.package_folders,
            package_path,
            library_key,
            AssetKind::Native,
            node.get("native"),
            selected_rid.as_deref(),
            false,
            &mut graph,
        )?;
        collect_runtime_targets(
            &assets.package_folders,
            package_path,
            library_key,
            node.get("runtimeTargets"),
            selected_rid.as_deref(),
            &mut graph,
        )?;
    }

    let assets = graph.into_iter().collect::<Vec<_>>();
    // Package keys carry the resolved version. Match the id prefix rather than trying to infer a
    // version from a DLL filename.
    let root_assets = assets.iter().filter(|asset| {
        asset
            .owner
            .split_once('/')
            .is_some_and(|(id, _)| id.eq_ignore_ascii_case(root_id))
    });
    let root_runtime_dlls = root_assets
        .clone()
        .filter(|asset| asset.kind == AssetKind::Runtime && is_dll(&asset.source))
        .map(|asset| asset.source.clone())
        .collect::<Vec<_>>();
    let root_compile_dlls = root_assets
        .filter(|asset| asset.kind == AssetKind::Compile && is_dll(&asset.source))
        .map(|asset| asset.source.clone())
        .collect::<Vec<_>>();
    let primary_dll = choose_primary(root_id, &root_runtime_dlls)
        .or_else(|| choose_primary(root_id, &root_compile_dlls));
    let compile_dlls = assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Compile && is_dll(&asset.source))
        .map(|asset| asset.source.clone())
        .collect();
    let runtime_dlls = assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Runtime && is_dll(&asset.source))
        .map(|asset| asset.source.clone())
        .collect();
    Ok(ResolvedAssets {
        primary_dll,
        compile_dlls,
        runtime_dlls,
        collisions: collisions(&assets),
        assets,
        requested_rid: selected_rid,
    })
}

fn collect_group(
    folders: &serde_json::Map<String, serde_json::Value>,
    package_path: &str,
    owner: &str,
    kind: AssetKind,
    group: Option<&serde_json::Value>,
    rid: Option<&str>,
    fallback: bool,
    out: &mut BTreeSet<ResolvedAsset>,
) -> Result<()> {
    let Some(group) = group.and_then(|v| v.as_object()) else {
        return Ok(());
    };
    for relative in group.keys() {
        let source = resolve_source(folders, package_path, relative)?;
        out.insert(ResolvedAsset {
            owner: owner.to_owned(),
            kind: kind.clone(),
            logical_path: normalize_logical_path(relative)?,
            source,
            rid: rid.map(str::to_owned),
            fallback,
        });
    }
    Ok(())
}

fn collect_runtime_targets(
    folders: &serde_json::Map<String, serde_json::Value>,
    package_path: &str,
    owner: &str,
    group: Option<&serde_json::Value>,
    requested_rid: Option<&str>,
    out: &mut BTreeSet<ResolvedAsset>,
) -> Result<()> {
    let Some(group) = group.and_then(|value| value.as_object()) else {
        return Ok(());
    };
    for (relative, metadata) in group {
        let metadata = metadata
            .as_object()
            .context("asset restore runtimeTargets entry is not an object")?;
        let kind = match metadata.get("assetType").and_then(|value| value.as_str()) {
            Some("runtime") => AssetKind::Runtime,
            Some("native") => AssetKind::Native,
            Some("resource") | Some("resources") => AssetKind::Resource,
            Some(other) => bail!("asset restore: unsupported runtimeTargets assetType `{other}`"),
            None => bail!("asset restore: runtimeTargets entry {relative} has no assetType"),
        };
        let rid = metadata.get("rid").and_then(|value| value.as_str());
        out.insert(ResolvedAsset {
            owner: owner.to_owned(),
            kind,
            logical_path: normalize_logical_path(relative)?,
            source: resolve_source(folders, package_path, relative)?,
            rid: rid.map(str::to_owned),
            fallback: requested_rid
                .is_some_and(|requested| rid.is_some_and(|actual| actual != requested)),
        });
    }
    Ok(())
}

fn resolve_source(
    folders: &serde_json::Map<String, serde_json::Value>,
    package_path: &str,
    relative: &str,
) -> Result<PathBuf> {
    let mut found = None;
    for folder in folders.keys() {
        let candidate = Path::new(folder).join(package_path).join(relative);
        if candidate.is_file() {
            found = Some(candidate);
            break;
        }
    }
    found.with_context(|| {
        format!("asset restore: {package_path}/{relative} is missing from package folders")
    })
}

fn normalize_logical_path(relative: &str) -> Result<String> {
    let path = relative.replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        bail!("asset restore: unsafe package-relative asset path `{relative}`");
    }
    Ok(path)
}

fn collisions(assets: &[ResolvedAsset]) -> Vec<AssetCollision> {
    let mut owners = BTreeMap::<String, BTreeSet<String>>::new();
    for asset in assets {
        owners
            .entry(asset.logical_path.clone())
            .or_default()
            .insert(asset.owner.clone());
    }
    owners
        .into_iter()
        .filter_map(|(logical_path, owners)| {
            (owners.len() > 1).then(|| AssetCollision {
                logical_path,
                owners: owners.into_iter().collect(),
            })
        })
        .collect()
}

fn is_dll(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
}

fn choose_primary(id: &str, dlls: &[PathBuf]) -> Option<PathBuf> {
    let simple = id.rsplit('.').next().unwrap_or(id);
    dlls.iter()
        .find(|path| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| {
                    stem.eq_ignore_ascii_case(id) || stem.eq_ignore_ascii_case(simple)
                })
        })
        .or_else(|| dlls.first())
        .cloned()
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const STAGING_MANIFEST: &str = "manifest.json";

#[derive(Debug, Default, Deserialize, Serialize)]
struct OwnedAssetsManifest {
    #[serde(default = "manifest_version")]
    version: u32,
    #[serde(default)]
    roots: BTreeMap<String, OwnedRootAssets>,
}

fn manifest_version() -> u32 {
    1
}

#[derive(Debug, Deserialize, Serialize)]
struct OwnedRootAssets {
    assets: Vec<OwnedAssetRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct OwnedAssetRecord {
    owner: String,
    kind: AssetKind,
    logical_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rid: Option<String>,
    fallback: bool,
    staged_path: String,
}

/// Stage a complete SDK-selected graph under a root-package-owned directory. The manifest is
/// replaced only after every source has copied successfully, so an interrupted refresh cannot
/// silently leave a partial new graph masquerading as the previous package version.
pub fn stage_assets(crate_dir: &Path, root_id: &str, assets: &[ResolvedAsset]) -> Result<()> {
    let collisions = collisions(assets);
    if !collisions.is_empty() {
        let details = collisions
            .iter()
            .map(|collision| {
                format!(
                    "{} ({})",
                    collision.logical_path,
                    collision.owners.join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        bail!("asset staging: unsafe logical asset collision for {root_id}: {details}");
    }

    let assets_dir = crate_dir.join(".cargo-dotnet-nuget-assets");
    fs::create_dir_all(&assets_dir)?;
    let manifest_path = assets_dir.join(STAGING_MANIFEST);
    let mut manifest = read_manifest(&manifest_path)?;
    let root_token = root_token(root_id);
    let owned_dir = assets_dir.join("owned");
    fs::create_dir_all(&owned_dir)?;
    let unique = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let temp_dir = assets_dir.join(format!(".staging-{root_token}-{unique}"));
    let final_dir = owned_dir.join(&root_token);

    let staged = (|| -> Result<Vec<OwnedAssetRecord>> {
        let mut records = Vec::with_capacity(assets.len());
        for asset in assets {
            let logical_path = normalize_logical_path(&asset.logical_path)?;
            let destination = temp_dir.join(&logical_path);
            fs::create_dir_all(
                destination
                    .parent()
                    .context("asset staging: asset has no parent")?,
            )?;
            fs::copy(&asset.source, &destination).with_context(|| {
                format!(
                    "asset staging: {} -> {}",
                    asset.source.display(),
                    destination.display()
                )
            })?;
            records.push(OwnedAssetRecord {
                owner: asset.owner.clone(),
                kind: asset.kind.clone(),
                logical_path: logical_path.clone(),
                rid: asset.rid.clone(),
                fallback: asset.fallback,
                staged_path: format!("owned/{root_token}/{logical_path}"),
            });
        }
        Ok(records)
    })();
    let records = match staged {
        Ok(records) => records,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(error);
        }
    };

    let backup_dir = assets_dir.join(format!(".backup-{root_token}-{unique}"));
    let had_old_root = final_dir.exists();
    if had_old_root {
        fs::rename(&final_dir, &backup_dir).with_context(|| {
            format!(
                "asset staging: moving previous graph {}",
                final_dir.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(&temp_dir, &final_dir) {
        if had_old_root {
            let _ = fs::rename(&backup_dir, &final_dir);
        }
        return Err(error)
            .with_context(|| format!("asset staging: promoting graph {}", temp_dir.display()));
    }

    manifest.version = manifest_version();
    manifest
        .roots
        .insert(root_id.to_owned(), OwnedRootAssets { assets: records });
    if let Err(error) = write_manifest_atomic(&manifest_path, &manifest) {
        let _ = fs::remove_dir_all(&final_dir);
        if had_old_root {
            let _ = fs::rename(&backup_dir, &final_dir);
        }
        return Err(error);
    }
    if had_old_root {
        fs::remove_dir_all(&backup_dir)?;
    }
    Ok(())
}

/// Materialize the staged runtime closure after a Rust build. This intentionally maps only the
/// runtime-facing layout (`runtime`, `native`, and culture resources); compile references remain
/// provenance in the manifest rather than being copied beside an executable.
pub fn copy_staged_assets(crate_dir: &Path, out_dir: &Path) -> Result<bool> {
    let assets_dir = crate_dir.join(".cargo-dotnet-nuget-assets");
    let manifest_path = assets_dir.join(STAGING_MANIFEST);
    if !manifest_path.is_file() {
        return Ok(false);
    }
    let manifest = read_manifest(&manifest_path)?;
    let mut destinations = BTreeMap::<PathBuf, &OwnedAssetRecord>::new();
    for root in manifest.roots.values() {
        for asset in &root.assets {
            if asset.kind == AssetKind::Compile {
                continue;
            }
            let deployment = deployment_path(asset)?;
            if let Some(previous) = destinations.get(&deployment) {
                // A RID-selected runtime asset intentionally replaces its portable `lib/` twin
                // at deployment time. Any other flattening collision is unsafe: it loses one
                // package's identity and must be fixed by a future package-layout policy.
                if previous.rid.is_none() && asset.rid.is_some() {
                    destinations.insert(deployment, asset);
                } else if previous.rid.is_some() && asset.rid.is_none() {
                    continue;
                } else {
                    bail!(
                        "cargo-dotnet: staged NuGet assets collide at {}: {} and {}",
                        deployment.display(),
                        previous.staged_path,
                        asset.staged_path
                    );
                }
            } else {
                destinations.insert(deployment, asset);
            }
        }
    }
    for (deployment, asset) in destinations {
        let source = assets_dir.join(&asset.staged_path);
        if !source.is_file() {
            bail!(
                "cargo-dotnet: staged asset referenced by manifest is missing: {}",
                source.display()
            );
        }
        let destination = out_dir.join(deployment);
        fs::create_dir_all(
            destination
                .parent()
                .context("cargo-dotnet: output asset has no parent")?,
        )?;
        fs::copy(&source, &destination)
            .with_context(|| format!("cp {} -> {}", source.display(), destination.display()))?;
    }
    Ok(true)
}

/// Return the subset of `recorded` `(id, version)` pairs whose staged runtime closure is missing
/// or incomplete — no `.cargo-dotnet-nuget-assets/manifest.json` at all, no entry for that id, an
/// empty asset list, a manifest entry whose staged file(s) no longer exist on disk (e.g. a
/// partial manual cleanup), or — critically — a staged root whose own package asset does not
/// carry an `owner` of exactly `{id}/{version}` (case-insensitive). That last check is what
/// catches VERSION DRIFT: `.cargo-dotnet-nuget-deps.json` is checked in and last-write-wins, so a
/// teammate bumping a package's recorded version and you pulling their commit must NOT pass this
/// check against your still-locally-staged OLD version's graph — that would silently build/pack
/// against stale dlls (worse, against dlls the checked-in `src/nuget/*.rs` bindings may no longer
/// match). This is also the fresh-clone detector: `.cargo-dotnet-nuget-assets/` is gitignored
/// while the deps manifest is checked in, so a clean checkout has every id "missing" here even
/// though `add-nuget` ran successfully at some point in the repo's history.
pub fn missing_recorded_roots(
    crate_dir: &Path,
    recorded: &[(String, String)],
) -> Result<Vec<String>> {
    let assets_dir = crate_dir.join(".cargo-dotnet-nuget-assets");
    let manifest_path = assets_dir.join(STAGING_MANIFEST);
    if !manifest_path.is_file() {
        return Ok(recorded.iter().map(|(id, _)| id.clone()).collect());
    }
    let manifest = read_manifest(&manifest_path)?;
    let mut missing = Vec::new();
    for (id, version) in recorded {
        let expected_owner = format!("{id}/{version}");
        let complete = manifest.roots.get(id).is_some_and(|root| {
            !root.assets.is_empty()
                && root
                    .assets
                    .iter()
                    .any(|asset| asset.owner.eq_ignore_ascii_case(&expected_owner))
                && root
                    .assets
                    .iter()
                    .all(|asset| assets_dir.join(&asset.staged_path).is_file())
        });
        if !complete {
            missing.push(id.clone());
        }
    }
    Ok(missing)
}

fn deployment_path(asset: &OwnedAssetRecord) -> Result<PathBuf> {
    match asset.kind {
        AssetKind::Runtime | AssetKind::Native => asset
            .logical_path
            .rsplit('/')
            .next()
            .map(PathBuf::from)
            .context("cargo-dotnet: asset has no filename"),
        AssetKind::Resource => {
            let parts = asset.logical_path.split('/').collect::<Vec<_>>();
            let Some(lib_index) = parts.iter().position(|part| *part == "lib") else {
                return parts
                    .last()
                    .map(PathBuf::from)
                    .context("cargo-dotnet: resource has no filename");
            };
            if parts.len() <= lib_index + 2 {
                bail!(
                    "cargo-dotnet: resource path lacks a culture/file suffix: {}",
                    asset.logical_path
                );
            }
            let mut output = PathBuf::new();
            for part in &parts[lib_index + 2..] {
                output.push(part);
            }
            Ok(output)
        }
        AssetKind::Compile => unreachable!("compile assets are skipped before deployment mapping"),
    }
}

fn read_manifest(path: &Path) -> Result<OwnedAssetsManifest> {
    if !path.is_file() {
        return Ok(OwnedAssetsManifest {
            version: manifest_version(),
            ..OwnedAssetsManifest::default()
        });
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let manifest: OwnedAssetsManifest =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    if manifest.version != manifest_version() {
        bail!(
            "unsupported asset staging manifest version {}",
            manifest.version
        );
    }
    Ok(manifest)
}

fn write_manifest_atomic(path: &Path, manifest: &OwnedAssetsManifest) -> Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, serde_json::to_vec_pretty(manifest)?)
        .with_context(|| format!("writing {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("promoting {} -> {}", temporary.display(), path.display()))
}

fn root_token(root_id: &str) -> String {
    let digest = Sha256::digest(root_id.to_ascii_lowercase().as_bytes());
    digest
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Read the owned staging manifest as package entries.  The manifest itself is deliberately
/// private: callers receive only checked source paths and package-relative destinations.
///
/// The runtime build path flattens files because the CLR probes next to an executable; NuGet
/// packages must *not* do that.  NuGet's RID and resource selection relies on these exact paths.
pub fn package_assets(crate_dir: &Path) -> Result<Vec<StagedPackageAsset>> {
    let assets_dir = crate_dir.join(".cargo-dotnet-nuget-assets");
    let manifest_path = assets_dir.join(STAGING_MANIFEST);
    if !manifest_path.is_file() {
        return Ok(Vec::new());
    }
    let manifest = read_manifest(&manifest_path)?;
    let mut result = BTreeMap::<String, StagedPackageAsset>::new();
    for root in manifest.roots.values() {
        for asset in &root.assets {
            if asset.kind == AssetKind::Compile {
                continue;
            }
            let logical_path = normalize_logical_path(&asset.logical_path)?;
            let source = assets_dir.join(&asset.staged_path);
            if !source.is_file() {
                bail!(
                    "pack: staged NuGet asset referenced by manifest is missing: {}",
                    source.display()
                );
            }
            let kind = match asset.kind {
                AssetKind::Runtime => StagedPackageAssetKind::Runtime,
                AssetKind::Native => StagedPackageAssetKind::Native,
                AssetKind::Resource => StagedPackageAssetKind::Resource,
                AssetKind::Compile => unreachable!("compile assets were skipped"),
            };
            let staged = StagedPackageAsset {
                logical_path: logical_path.clone(),
                source,
                kind,
                rid: asset.rid.clone(),
            };
            if let Some(previous) = result.insert(logical_path.clone(), staged) {
                bail!(
                    "pack: staged NuGet assets collide at {logical_path}: {} and {}",
                    previous.source.display(),
                    asset.staged_path
                );
            }
        }
    }
    Ok(result.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RID_FIXTURES: &[&str] = &[
        "linux-x64.project.assets.json",
        "linux-musl-x64.project.assets.json",
        "win-x64.project.assets.json",
        "osx-arm64.project.assets.json",
    ];

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedFixtureGraph {
        requested_rid: String,
        assets: Vec<ExpectedFixtureAsset>,
        collisions: Vec<ExpectedFixtureCollision>,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct ExpectedFixtureAsset {
        owner: String,
        kind: String,
        path: String,
        #[serde(default)]
        rid: Option<String>,
        #[serde(default)]
        fallback: bool,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct ExpectedFixtureCollision {
        path: String,
        owners: Vec<String>,
    }

    fn fixture_text(name: &str) -> String {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fixtures/nuget-assets")
            .join(name);
        fs::read_to_string(&fixture).unwrap_or_else(|error| {
            panic!("reading RID asset fixture {}: {error}", fixture.display())
        })
    }

    fn fixture_graph(name: &str) -> ExpectedFixtureGraph {
        let fixture: serde_json::Value = serde_json::from_str(&fixture_text(name)).unwrap();
        serde_json::from_value(fixture["cargoDotnetExpected"].clone()).unwrap()
    }

    #[test]
    fn parses_full_transitive_compile_and_runtime_graph() {
        let temp = std::env::temp_dir().join(format!("cargo-dotnet-assets-{}", std::process::id()));
        let packages = temp.join("packages");
        let obj = temp.join("obj");
        let files = [
            (
                "root.package/1.0.0/lib/net8.0/Root.Package.dll",
                b"root".as_slice(),
            ),
            (
                "dependency/2.0.0/ref/net8.0/Dependency.dll",
                b"ref".as_slice(),
            ),
            (
                "dependency/2.0.0/lib/net8.0/Dependency.dll",
                b"run".as_slice(),
            ),
        ];
        for (relative, bytes) in files {
            let path = packages.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        fs::create_dir_all(&obj).unwrap();
        let json = serde_json::json!({
            "targets": {"net8.0": {
                "Root.Package/1.0.0": {"type":"package", "dependencies":{"Dependency":"2.0.0"},
                    "compile":{"lib/net8.0/Root.Package.dll":{}}, "runtime":{"lib/net8.0/Root.Package.dll":{}}},
                "Dependency/2.0.0": {"type":"package", "compile":{"ref/net8.0/Dependency.dll":{}},
                    "runtime":{"lib/net8.0/Dependency.dll":{}}}
            }},
            "libraries": {
                "Root.Package/1.0.0":{"type":"package","path":"root.package/1.0.0"},
                "Dependency/2.0.0":{"type":"package","path":"dependency/2.0.0"}
            },
            "packageFolders": {(format!("{}/", packages.display())): {}}
        });
        let assets_path = obj.join("project.assets.json");
        fs::write(&assets_path, serde_json::to_vec(&json).unwrap()).unwrap();

        let resolved = parse(&assets_path, "Root.Package", None).unwrap();
        assert_eq!(
            resolved.primary_dll.as_ref().unwrap().file_name().unwrap(),
            "Root.Package.dll"
        );
        assert_eq!(resolved.compile_dlls.len(), 2);
        assert_eq!(resolved.runtime_dlls.len(), 2);
        assert!(resolved
            .runtime_dlls
            .iter()
            .any(|p| p.ends_with("Dependency.dll")));
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn prefers_compile_asset_when_root_has_no_runtime_asset() {
        let temp = std::env::temp_dir().join(format!(
            "cargo-dotnet-assets-compile-{}",
            std::process::id()
        ));
        let packages = temp.join("packages");
        let dll = packages.join("reference.only/1.0.0/ref/net8.0/Reference.Only.dll");
        fs::create_dir_all(dll.parent().unwrap()).unwrap();
        fs::write(&dll, b"ref").unwrap();
        let assets_path = temp.join("project.assets.json");
        let json = serde_json::json!({
            "targets":{"net8.0":{"Reference.Only/1.0.0":{"type":"package","compile":{"ref/net8.0/Reference.Only.dll":{}}}}},
            "libraries":{"Reference.Only/1.0.0":{"type":"package","path":"reference.only/1.0.0"}},
            "packageFolders":{(format!("{}/", packages.display())):{}}
        });
        fs::write(&assets_path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(
            parse(&assets_path, "Reference.Only", None)
                .unwrap()
                .primary_dll
                .as_ref()
                .unwrap(),
            &dll
        );
        fs::remove_dir_all(temp).unwrap();
    }

    /// These fixtures deliberately describe the post-R2.3 contract rather than the current
    /// `ResolvedAssets` projection. They are input/snapshot data only: R0.3 must not grow a
    /// second asset-graph implementation beside the eventual production parser.
    #[test]
    fn rid_asset_fixture_snapshots_cover_the_release_matrix() {
        let mut total_assets = 0;
        let mut total_collisions = 0;
        for fixture in RID_FIXTURES {
            let graph = fixture_graph(fixture);
            assert!(
                !graph.requested_rid.is_empty(),
                "{fixture} must declare the requested RID"
            );
            assert!(
                graph.assets.iter().any(|asset| asset.kind == "runtime"),
                "{fixture} must contain a managed runtime asset"
            );
            assert!(
                graph.assets.iter().any(|asset| asset.kind == "native"),
                "{fixture} must contain a native asset"
            );
            assert!(
                graph.assets.iter().any(|asset| asset.kind == "resource"),
                "{fixture} must contain a resource asset"
            );
            total_assets += graph.assets.len();
            total_collisions += graph.collisions.len();
        }
        assert!(
            fixture_graph("linux-musl-x64.project.assets.json")
                .assets
                .iter()
                .any(|asset| asset.fallback && asset.rid.as_deref() == Some("linux-x64")),
            "linux-musl-x64 must preserve a selected linux-x64 fallback asset"
        );
        assert_eq!(
            total_collisions, 2,
            "fixtures must retain both collision cases"
        );
        assert!(total_assets >= 30, "fixture coverage unexpectedly shrank");
    }

    /// Snapshot acceptance for the SDK-selected RID graph. It covers runtimeTargets, native
    /// assets, culture resources, fallback RID provenance, and package-owner collisions.
    #[test]
    fn rid_asset_graph_snapshots_are_preserved() {
        for fixture in RID_FIXTURES {
            let mut expected = fixture_graph(fixture);
            sort_assets(&mut expected.assets);
            sort_collisions(&mut expected.collisions);
            let mut current = parse_fixture(fixture);
            sort_collisions(&mut current.collisions);
            assert_eq!(
                current, expected,
                "{fixture}: complete SDK-resolved graph must survive cargo-dotnet parsing"
            );
        }
    }

    fn parse_fixture(name: &str) -> ExpectedFixtureGraph {
        let temp = std::env::temp_dir().join(format!(
            "cargo-dotnet-rid-fixture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let packages = temp.join("packages");
        // The placeholder appears inside a JSON string. Windows paths contain backslashes, so
        // inject their JSON-escaped representation rather than raw `D:\...` text.
        let package_root = packages.to_string_lossy().replace('\\', "\\\\");
        let text = fixture_text(name).replace("__PACKAGE_ROOT__", &package_root);
        let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
        let target = fixture["targets"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .as_object()
            .unwrap();
        for (owner, node) in target {
            let package = fixture["libraries"][owner]["path"].as_str().unwrap();
            for group in ["compile", "runtime", "runtimeTargets"] {
                for relative in node[group]
                    .as_object()
                    .into_iter()
                    .flat_map(|assets| assets.keys())
                {
                    let path = packages.join(package).join(relative);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, b"fixture").unwrap();
                }
            }
        }
        let assets_path = temp.join("project.assets.json");
        fs::create_dir_all(&temp).unwrap();
        fs::write(&assets_path, text).unwrap();
        let requested_rid = fixture["cargoDotnetExpected"]["requestedRid"]
            .as_str()
            .unwrap()
            .to_owned();
        let resolved = parse(&assets_path, "Example.Root", Some(&requested_rid)).unwrap();
        let mut assets = resolved
            .assets
            .into_iter()
            .map(|asset| ExpectedFixtureAsset {
                owner: asset.owner,
                kind: match asset.kind {
                    AssetKind::Compile => "compile",
                    AssetKind::Runtime => "runtime",
                    AssetKind::Native => "native",
                    AssetKind::Resource => "resource",
                }
                .to_owned(),
                path: asset.logical_path,
                rid: asset.rid,
                fallback: asset.fallback,
            })
            .collect::<Vec<_>>();
        sort_assets(&mut assets);
        let graph = ExpectedFixtureGraph {
            requested_rid: resolved.requested_rid.unwrap(),
            assets,
            collisions: resolved
                .collisions
                .into_iter()
                .map(|collision| ExpectedFixtureCollision {
                    path: collision.logical_path,
                    owners: collision.owners,
                })
                .collect(),
        };
        fs::remove_dir_all(temp).unwrap();
        graph
    }

    fn sort_assets(assets: &mut [ExpectedFixtureAsset]) {
        assets.sort_by(|left, right| {
            (
                &left.owner,
                &left.kind,
                &left.path,
                &left.rid,
                left.fallback,
            )
                .cmp(&(
                    &right.owner,
                    &right.kind,
                    &right.path,
                    &right.rid,
                    right.fallback,
                ))
        });
    }

    fn sort_collisions(collisions: &mut [ExpectedFixtureCollision]) {
        for collision in collisions.iter_mut() {
            collision.owners.sort();
        }
        collisions.sort_by(|left, right| left.path.cmp(&right.path));
    }

    fn unique_temp(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "cargo-dotnet-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn asset(
        owner: &str,
        kind: AssetKind,
        logical_path: &str,
        source: PathBuf,
        rid: Option<&str>,
    ) -> ResolvedAsset {
        ResolvedAsset {
            owner: owner.to_owned(),
            kind,
            logical_path: logical_path.to_owned(),
            source,
            rid: rid.map(str::to_owned),
            fallback: false,
        }
    }

    #[test]
    fn owned_staging_replaces_a_root_graph_without_stale_assets() {
        let temp = unique_temp("owned-staging");
        let crate_dir = temp.join("consumer");
        let source_dir = temp.join("source");
        fs::create_dir_all(&source_dir).unwrap();
        let old = source_dir.join("old.dll");
        let new = source_dir.join("new.dll");
        fs::write(&old, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        let old_graph = [asset(
            "Example.Root/1.0.0",
            AssetKind::Runtime,
            "lib/net8.0/Old.dll",
            old,
            None,
        )];
        let new_graph = [asset(
            "Example.Root/2.0.0",
            AssetKind::Runtime,
            "lib/net8.0/New.dll",
            new,
            None,
        )];
        stage_assets(&crate_dir, "Example.Root", &old_graph).unwrap();
        stage_assets(&crate_dir, "Example.Root", &new_graph).unwrap();

        let manifest = read_manifest(
            &crate_dir
                .join(".cargo-dotnet-nuget-assets")
                .join(STAGING_MANIFEST),
        )
        .unwrap();
        let staged = &manifest.roots["Example.Root"].assets;
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].logical_path, "lib/net8.0/New.dll");
        let output = temp.join("output");
        assert!(copy_staged_assets(&crate_dir, &output).unwrap());
        assert_eq!(fs::read(output.join("New.dll")).unwrap(), b"new");
        assert!(!output.join("Old.dll").exists());
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn staged_assets_use_rid_runtime_native_and_resource_layouts() {
        let temp = unique_temp("asset-layout");
        let crate_dir = temp.join("consumer");
        let source_dir = temp.join("source");
        fs::create_dir_all(&source_dir).unwrap();
        let portable = source_dir.join("portable.dll");
        let rid_runtime = source_dir.join("rid.dll");
        let native = source_dir.join("native.so");
        let resource = source_dir.join("resource.dll");
        fs::write(&portable, b"portable").unwrap();
        fs::write(&rid_runtime, b"rid").unwrap();
        fs::write(&native, b"native").unwrap();
        fs::write(&resource, b"resource").unwrap();
        stage_assets(
            &crate_dir,
            "Example.Root",
            &[
                asset(
                    "Example.Root/1.0.0",
                    AssetKind::Runtime,
                    "lib/net8.0/Example.Root.dll",
                    portable,
                    None,
                ),
                asset(
                    "Example.Root/1.0.0",
                    AssetKind::Runtime,
                    "runtimes/linux-x64/lib/net8.0/Example.Root.dll",
                    rid_runtime,
                    Some("linux-x64"),
                ),
                asset(
                    "Example.Root/1.0.0",
                    AssetKind::Native,
                    "runtimes/linux-x64/native/libexample.so",
                    native,
                    Some("linux-x64"),
                ),
                asset(
                    "Example.Root/1.0.0",
                    AssetKind::Resource,
                    "runtimes/linux-x64/lib/net8.0/fr/Example.Root.resources.dll",
                    resource,
                    Some("linux-x64"),
                ),
            ],
        )
        .unwrap();
        let output = temp.join("output");
        copy_staged_assets(&crate_dir, &output).unwrap();
        assert_eq!(fs::read(output.join("Example.Root.dll")).unwrap(), b"rid");
        assert_eq!(fs::read(output.join("libexample.so")).unwrap(), b"native");
        assert_eq!(
            fs::read(output.join("fr/Example.Root.resources.dll")).unwrap(),
            b"resource"
        );
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn missing_recorded_roots_flags_a_fresh_clone_with_no_assets_dir() {
        let temp = unique_temp("missing-roots-fresh-clone");
        let crate_dir = temp.join("consumer");
        // Simulate the gitignored assets dir never having been materialized (fresh clone):
        // no `.cargo-dotnet-nuget-assets/` at all, even though a deps manifest recorded it.
        let missing = missing_recorded_roots(
            &crate_dir,
            &[("Example.Root".to_string(), "1.0.0".to_string())],
        )
        .unwrap();
        assert_eq!(missing, vec!["Example.Root".to_string()]);
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn missing_recorded_roots_accepts_a_complete_staged_graph() {
        let temp = unique_temp("missing-roots-complete");
        let crate_dir = temp.join("consumer");
        let source = temp.join("source");
        fs::create_dir_all(&source).unwrap();
        let dll = source.join("Example.Root.dll");
        fs::write(&dll, b"root").unwrap();
        stage_assets(
            &crate_dir,
            "Example.Root",
            &[asset(
                "Example.Root/1.0.0",
                AssetKind::Runtime,
                "lib/net8.0/Example.Root.dll",
                dll,
                None,
            )],
        )
        .unwrap();
        let missing = missing_recorded_roots(
            &crate_dir,
            &[("Example.Root".to_string(), "1.0.0".to_string())],
        )
        .unwrap();
        assert!(missing.is_empty());
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn missing_recorded_roots_flags_a_staged_graph_whose_version_no_longer_matches_what_was_recorded(
    ) {
        let temp = unique_temp("missing-roots-version-drift");
        let crate_dir = temp.join("consumer");
        let source = temp.join("source");
        fs::create_dir_all(&source).unwrap();
        let dll = source.join("Example.Root.dll");
        fs::write(&dll, b"root").unwrap();
        // Staged (and gitignored) locally under 1.0.0, but a teammate's checked-in
        // `.cargo-dotnet-nuget-deps.json` now records 2.0.0 (e.g. after a `git pull`).
        stage_assets(
            &crate_dir,
            "Example.Root",
            &[asset(
                "Example.Root/1.0.0",
                AssetKind::Runtime,
                "lib/net8.0/Example.Root.dll",
                dll,
                None,
            )],
        )
        .unwrap();
        let missing = missing_recorded_roots(
            &crate_dir,
            &[("Example.Root".to_string(), "2.0.0".to_string())],
        )
        .unwrap();
        assert_eq!(missing, vec!["Example.Root".to_string()]);
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn missing_recorded_roots_flags_an_id_absent_from_a_manifest_covering_other_ids() {
        let temp = unique_temp("missing-roots-partial-manifest");
        let crate_dir = temp.join("consumer");
        let source = temp.join("source");
        fs::create_dir_all(&source).unwrap();
        let dll = source.join("Staged.Root.dll");
        fs::write(&dll, b"root").unwrap();
        stage_assets(
            &crate_dir,
            "Staged.Root",
            &[asset(
                "Staged.Root/1.0.0",
                AssetKind::Runtime,
                "lib/net8.0/Staged.Root.dll",
                dll,
                None,
            )],
        )
        .unwrap();
        // A second package was recorded (e.g. in `.cargo-dotnet-nuget-deps.json`) but never
        // staged into this manifest — a partial manifest relative to what's recorded.
        let missing = missing_recorded_roots(
            &crate_dir,
            &[
                ("Staged.Root".to_string(), "1.0.0".to_string()),
                ("Unstaged.Root".to_string(), "1.0.0".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(missing, vec!["Unstaged.Root".to_string()]);
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn missing_recorded_roots_flags_a_manifest_entry_whose_staged_file_was_deleted() {
        let temp = unique_temp("missing-roots-deleted-file");
        let crate_dir = temp.join("consumer");
        let source = temp.join("source");
        fs::create_dir_all(&source).unwrap();
        let dll = source.join("Example.Root.dll");
        fs::write(&dll, b"root").unwrap();
        stage_assets(
            &crate_dir,
            "Example.Root",
            &[asset(
                "Example.Root/1.0.0",
                AssetKind::Runtime,
                "lib/net8.0/Example.Root.dll",
                dll,
                None,
            )],
        )
        .unwrap();
        // Manually delete the staged dll while leaving the manifest entry behind, mirroring an
        // interrupted/partial cleanup rather than a clean `rm -rf .cargo-dotnet-nuget-assets/`.
        let staged_dll = crate_dir
            .join(".cargo-dotnet-nuget-assets/owned")
            .read_dir()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("lib/net8.0/Example.Root.dll");
        fs::remove_file(&staged_dll).unwrap();
        let missing = missing_recorded_roots(
            &crate_dir,
            &[("Example.Root".to_string(), "1.0.0".to_string())],
        )
        .unwrap();
        assert_eq!(missing, vec!["Example.Root".to_string()]);
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn staging_rejects_ambiguous_logical_asset_owners() {
        let temp = unique_temp("asset-collision");
        let source = temp.join("shared.dll");
        fs::create_dir_all(&temp).unwrap();
        fs::write(&source, b"shared").unwrap();
        let error = stage_assets(
            &temp.join("consumer"),
            "Example.Root",
            &[
                asset(
                    "Example.Root/1.0.0",
                    AssetKind::Native,
                    "runtimes/win-x64/native/shared.dll",
                    source.clone(),
                    Some("win-x64"),
                ),
                asset(
                    "Example.Dependency/2.0.0",
                    AssetKind::Native,
                    "runtimes/win-x64/native/shared.dll",
                    source,
                    Some("win-x64"),
                ),
            ],
        )
        .unwrap_err();
        assert!(error.to_string().contains("unsafe logical asset collision"));
        fs::remove_dir_all(temp).unwrap();
    }
}
