use ar::Archive;

use cilly::{
    ArtifactAbiConfig, ArtifactAbiConfigMismatch, ArtifactDecodeError, ArtifactFormat, Assembly,
    DecodedAssemblyArtifact, IString, decode_assembly_artifact,
};
use std::io::Read;
pub struct LinkableFile {
    name: IString,
    file: Box<[u8]>,
}

impl LinkableFile {
    pub fn new(name: IString, file: Box<[u8]>) -> Self {
        Self { name, file }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn file(&self) -> &[u8] {
        &self.file
    }
}

/// Assemblies and their validated immutable artifact ABI loaded for one link.
pub struct LoadedAssemblies {
    assembly: Assembly,
    abi_config: Option<ArtifactAbiConfig>,
    linkables: Vec<LinkableFile>,
    legacy_artifacts: usize,
}

impl LoadedAssemblies {
    /// Consumes all loaded state for the linker pipeline.
    pub fn into_parts(
        self,
    ) -> (
        Assembly,
        Option<ArtifactAbiConfig>,
        Vec<LinkableFile>,
        usize,
    ) {
        (
            self.assembly,
            self.abi_config,
            self.linkables,
            self.legacy_artifacts,
        )
    }
}

#[derive(Default)]
struct AssemblyAccumulator {
    assembly: Assembly,
    abi_config: Option<ArtifactAbiConfig>,
    legacy_artifacts: usize,
}

impl AssemblyAccumulator {
    fn merge_encoded(&mut self, encoded: &[u8], source: &str) -> Result<(), ArtifactLoadError> {
        let decoded =
            decode_assembly_artifact(encoded).map_err(|error| ArtifactLoadError::Decode {
                source: source.to_owned(),
                error,
            })?;
        self.merge_decoded(decoded, source)
    }

    fn merge_decoded(
        &mut self,
        decoded: DecodedAssemblyArtifact,
        source: &str,
    ) -> Result<(), ArtifactLoadError> {
        let (assembly, config, format) = decoded.into_parts();
        if let Some(config) = config {
            if let Some(expected) = &self.abi_config {
                expected.ensure_compatible(&config).map_err(|error| {
                    ArtifactLoadError::IncompatibleAbiConfig {
                        source: source.to_owned(),
                        error,
                    }
                })?;
            } else {
                self.abi_config = Some(config);
            }
        }
        if format == ArtifactFormat::LegacyRawAssembly {
            self.legacy_artifacts += 1;
        }
        self.assembly = std::mem::take(&mut self.assembly).link(assembly);
        Ok(())
    }

    fn finish(self, linkables: Vec<LinkableFile>) -> LoadedAssemblies {
        LoadedAssemblies {
            assembly: self.assembly,
            abi_config: self.abi_config,
            linkables,
            legacy_artifacts: self.legacy_artifacts,
        }
    }
}

#[derive(Debug)]
enum ArtifactLoadError {
    Decode {
        source: String,
        error: ArtifactDecodeError,
    },
    IncompatibleAbiConfig {
        source: String,
        error: ArtifactAbiConfigMismatch,
    },
}

impl std::fmt::Display for ArtifactLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode { source, error } => {
                write!(f, "could not decode cilly artifact {source:?}: {error}")
            }
            Self::IncompatibleAbiConfig { source, error } => write!(
                f,
                "cilly artifact {source:?} cannot be linked with earlier inputs: {error}"
            ),
        }
    }
}

impl std::error::Error for ArtifactLoadError {}

fn load_ar(
    r: &mut impl std::io::Read,
    merged: &mut AssemblyAccumulator,
) -> std::io::Result<Vec<LinkableFile>> {
    let mut archive = Archive::new(r);
    let mut linkables = Vec::new();
    // Iterate over all entries in the archive:
    while let Some(entry_result) = archive.next_entry() {
        let mut entry = entry_result?;
        let name: String = String::from_utf8_lossy(entry.header().identifier()).into();
        let ext = if let Some(ext) = name.split('.').next_back() {
            ext
        } else {
            continue;
        };
        if ext.contains("bc") || ext.contains("cilly") {
            let mut asm_bytes = Vec::with_capacity(0x100);
            entry
                .read_to_end(&mut asm_bytes)
                .expect("ERROR: Could not load the assembly file!");
            merged.merge_encoded(&asm_bytes, &name).map_err(|error| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
            })?;
        } else if ext.contains("o") {
            let mut file_bytes = Vec::with_capacity(0x100);
            entry
                .read_to_end(&mut file_bytes)
                .expect("ERROR: Could not load the assembly file!");
            linkables.push(LinkableFile::new(name.clone().into(), file_bytes.into()));
        } else if name.contains(".so") {
            eprintln!("shr:{name}");
        }
    }
    Ok(linkables)
}

/// Loads, validates, and merges all assembly artifacts while retaining their ABI contract.
pub fn load_assemblies_with_config(raw_files: &[&String], archives: &[String]) -> LoadedAssemblies {
    println!("==> Preparing to load assmeblies");
    let mut merged = AssemblyAccumulator::default();
    let mut linkables = Vec::new();
    for asm_path in raw_files {
        let mut asm_file =
            std::fs::File::open(asm_path).expect("ERROR:Could not open the assembly file!");
        let mut asm_bytes = Vec::with_capacity(0x10000);
        asm_file
            .read_to_end(&mut asm_bytes)
            .expect("ERROR: Could not load the assembly file!");
        merged
            .merge_encoded(&asm_bytes, asm_path)
            .unwrap_or_else(|error| panic!("ERROR: {error}"));
    }
    for asm_path in archives {
        let mut asm_file =
            std::fs::File::open(asm_path).expect("ERROR: Could not open the assembly file!");
        linkables.extend(
            load_ar(&mut asm_file, &mut merged)
                .unwrap_or_else(|error| panic!("Could not load archive {asm_path:?}: {error}")),
        );
    }
    if merged.legacy_artifacts != 0 {
        eprintln!(
            "linker: loaded {} legacy raw-Assembly artifact(s); artifact ABI compatibility \
             could not be validated for those inputs",
            merged.legacy_artifacts
        );
    }
    println!("==> Loaded assmeblies");
    merged.finish(linkables)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cilly::{ArtifactAbiConfig, AssemblyArtifact, DotnetRuntime};

    #[test]
    fn accumulator_rejects_field_level_config_mismatch_before_linking() {
        let expected = ArtifactAbiConfig::default();
        let found = ArtifactAbiConfig::default()
            .with_dotnet_runtime(DotnetRuntime::Net9)
            .with_no_unwind(true);
        let first = AssemblyArtifact::new(Assembly::default(), expected)
            .encode()
            .unwrap();
        let second = AssemblyArtifact::new(Assembly::default(), found)
            .encode()
            .unwrap();
        let mut accumulator = AssemblyAccumulator::default();
        accumulator.merge_encoded(&first, "first.bc").unwrap();

        let error = accumulator.merge_encoded(&second, "second.bc").unwrap_err();
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("second.bc"));
        assert!(diagnostic.contains("dotnet_runtime: expected Net10, found Net9"));
        assert!(diagnostic.contains("no_unwind: expected false, found true"));
    }

    #[test]
    fn accumulator_accepts_legacy_artifact_but_marks_config_as_unvalidated() {
        let legacy = postcard::to_stdvec(&Assembly::default()).unwrap();
        let mut accumulator = AssemblyAccumulator::default();
        accumulator.merge_encoded(&legacy, "legacy.bc").unwrap();
        let loaded = accumulator.finish(Vec::new());
        let (_, config, _, legacy_artifacts) = loaded.into_parts();

        assert_eq!(legacy_artifacts, 1);
        assert!(config.is_none());
    }
}
