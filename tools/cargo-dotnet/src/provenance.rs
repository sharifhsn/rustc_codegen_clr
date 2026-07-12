//! Deterministic, relocatable release-provenance documents.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use cargo_metadata::MetadataCommand;
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Component {
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub license: Option<String>,
    pub license_file: Option<LicenseFile>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LicenseFile {
    pub file_name: String,
    pub sha256: String,
}

#[derive(Serialize)]
struct Sbom<'a> {
    bom_format: &'static str,
    spec_version: &'static str,
    version: u32,
    components: &'a [Component],
}

#[derive(Serialize)]
struct Licenses<'a> {
    schema: u32,
    components: &'a [Component],
}

pub fn cargo_inventory(manifest: &Path) -> Result<Vec<Component>> {
    let metadata = MetadataCommand::new()
        .manifest_path(manifest)
        .other_options(vec!["--locked".to_owned()])
        .exec()
        .context("provenance: `cargo metadata --locked` failed")?;
    let mut components = metadata
        .packages
        .into_iter()
        .map(|package| {
            let license_file = package
                .license_file
                .map(|path| {
                    let path = path.into_std_path_buf();
                    Ok::<_, anyhow::Error>(LicenseFile {
                        file_name: path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("LICENSE")
                            .to_owned(),
                        sha256: hash_bytes(
                            &fs::read(&path)
                                .with_context(|| format!("read license file {}", path.display()))?,
                        ),
                    })
                })
                .transpose()?;
            Ok(Component {
                name: package.name,
                version: package.version.to_string(),
                source: package.source.map(|source| source.to_string()),
                license: package.license,
                license_file,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    components.sort();
    Ok(components)
}

pub fn sbom_json(components: &[Component]) -> Result<Vec<u8>> {
    deterministic_json(&Sbom {
        bom_format: "CycloneDX",
        spec_version: "1.5",
        version: 1,
        components,
    })
}

pub fn licenses_json(components: &[Component]) -> Result<Vec<u8>> {
    deterministic_json(&Licenses {
        schema: 1,
        components,
    })
}

/// Strip host-specific paths from the build receipt while preserving identities and hashes.
pub fn artifact_provenance(receipt: &Path) -> Result<Vec<u8>> {
    let value: Value = serde_json::from_slice(
        &fs::read(receipt).with_context(|| format!("read {}", receipt.display()))?,
    )?;
    let object = value
        .as_object()
        .context("artifact receipt must be a JSON object")?;
    let mut projected = Map::new();
    projected.insert("schema".into(), Value::from(1));
    for key in [
        "source",
        "profile",
        "dotnet",
        "toolchain",
        "cargo_arguments",
        "pal_tree_sha256",
        "overlays_tree_sha256",
        "managed_identity",
        "local_input_closure",
    ] {
        if let Some(value) = object.get(key) {
            projected.insert(key.into(), strip_paths(value));
        }
    }
    for key in [
        "private_sysroot_receipt",
        "backend",
        "linker",
        "target_spec",
        "artifact",
        "pdb",
        "xml_docs",
    ] {
        if let Some(value) = object.get(key) {
            projected.insert(key.into(), strip_paths(value));
        }
    }
    deterministic_json(&Value::Object(projected))
}

fn strip_paths(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .filter(|(key, _)| key.as_str() != "path" && key.as_str() != "cargo_home")
                .map(|(key, value)| (key.clone(), strip_paths(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(strip_paths).collect()),
        _ => value.clone(),
    }
}

pub fn package_receipt(package: &Path, entries: &BTreeMap<String, String>) -> Result<Vec<u8>> {
    let bytes = fs::read(package).with_context(|| format!("read {}", package.display()))?;
    let mut value = Map::new();
    value.insert("schema".into(), Value::from(1));
    value.insert(
        "package".into(),
        Value::from(
            package
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("package.nupkg"),
        ),
    );
    value.insert("sha256".into(), Value::from(hash_bytes(&bytes)));
    value.insert("entries".into(), serde_json::to_value(entries)?);
    deterministic_json(&Value::Object(value))
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn deterministic_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn require_nonempty(name: &str, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        bail!("pack provenance: {name} is empty");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_documents_are_sorted_and_byte_deterministic() {
        let components = vec![
            Component {
                name: "z".into(),
                version: "1.0.0".into(),
                source: None,
                license: Some("MIT".into()),
                license_file: None,
            },
            Component {
                name: "a".into(),
                version: "2.0.0".into(),
                source: Some("registry+example".into()),
                license: None,
                license_file: None,
            },
        ];
        let mut sorted = components;
        sorted.sort();
        let first = sbom_json(&sorted).unwrap();
        assert_eq!(first, sbom_json(&sorted).unwrap());
        assert!(
            String::from_utf8(first).unwrap().find("\"a\"").unwrap()
                < String::from_utf8(sbom_json(&sorted).unwrap())
                    .unwrap()
                    .find("\"z\"")
                    .unwrap()
        );
        assert_eq!(
            licenses_json(&sorted).unwrap(),
            licenses_json(&sorted).unwrap()
        );
    }

    #[test]
    fn artifact_projection_removes_absolute_paths() {
        let path = std::env::temp_dir().join(format!(
            "cargo-dotnet-provenance-{}.json",
            std::process::id()
        ));
        fs::write(&path, br#"{"schema":1,"artifact":{"path":"/private/build/a.dll","sha256":"abc","bytes":3},"cargo_home":"/private/cargo"}"#).unwrap();
        let projected = artifact_provenance(&path).unwrap();
        assert!(!String::from_utf8(projected).unwrap().contains("/private/"));
        let _ = fs::remove_file(path);
    }
}
