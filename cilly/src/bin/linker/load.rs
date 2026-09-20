use ar::Archive;

use cilly::{
    ArtifactAbiConfig, ArtifactAbiConfigMismatch, ArtifactDecodeError, Assembly, AssemblyArtifact,
    AssemblyLinkError, decode_assembly_artifact,
};
use std::io::Read;

/// Assemblies and their validated immutable artifact ABI loaded for one link.
pub struct LoadedAssemblies {
    assembly: Assembly,
    abi_config: Option<ArtifactAbiConfig>,
}

impl LoadedAssemblies {
    /// Consumes all loaded state for the linker pipeline.
    pub fn into_parts(self) -> (Assembly, Option<ArtifactAbiConfig>) {
        (self.assembly, self.abi_config)
    }
}

#[derive(Default)]
struct AssemblyAccumulator {
    assembly: Assembly,
    abi_config: Option<ArtifactAbiConfig>,
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
        decoded: AssemblyArtifact,
        source: &str,
    ) -> Result<(), ArtifactLoadError> {
        let (config, assembly) = decoded.into_parts();
        let install_config = self.abi_config.is_none();
        if let Some(expected) = &self.abi_config {
            expected.ensure_compatible(&config).map_err(|error| {
                ArtifactLoadError::IncompatibleAbiConfig {
                    source: source.to_owned(),
                    error,
                }
            })?;
        }
        self.assembly
            .try_link_in_place(assembly)
            .map_err(|error| ArtifactLoadError::Link {
                source: source.to_owned(),
                error,
            })?;
        if install_config {
            self.abi_config = Some(config);
        }
        Ok(())
    }

    fn finish(self) -> LoadedAssemblies {
        LoadedAssemblies {
            assembly: self.assembly,
            abi_config: self.abi_config,
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
    Link {
        source: String,
        error: AssemblyLinkError,
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
            Self::Link { source, error } => {
                write!(
                    f,
                    "cilly artifact {source:?} conflicts with earlier inputs: {error}"
                )
            }
        }
    }
}

impl std::error::Error for ArtifactLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode { error, .. } => Some(error),
            Self::IncompatibleAbiConfig { error, .. } => Some(error),
            Self::Link { error, .. } => Some(error),
        }
    }
}

fn load_ar(r: &mut impl std::io::Read, merged: &mut AssemblyAccumulator) -> std::io::Result<()> {
    let mut archive = Archive::new(r);
    // Iterate over all entries in the archive:
    while let Some(entry_result) = archive.next_entry() {
        let mut entry = entry_result?;
        let name: String = String::from_utf8_lossy(entry.header().identifier()).into();
        let Some(ext) = name.split('.').next_back() else {
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
        } else if name.contains(".so") {
            eprintln!("shr:{name}");
        }
    }
    Ok(())
}

/// Loads, validates, and merges all assembly artifacts while retaining their ABI contract.
pub fn load_assemblies_with_config(raw_files: &[&String], archives: &[String]) -> LoadedAssemblies {
    println!("==> Preparing to load assmeblies");
    let mut merged = AssemblyAccumulator::default();
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
        load_ar(&mut asm_file, &mut merged)
            .unwrap_or_else(|error| panic!("Could not load archive {asm_path:?}: {error}"));
    }
    println!("==> Loaded assmeblies");
    merged.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cilly::{Access, ArtifactAbiConfig, AssemblyArtifact, ClassDef, DotnetRuntime, Int, Type};

    fn collision_assembly(field_type: Type) -> Assembly {
        let mut assembly = Assembly::default();
        let name = assembly.alloc_string("ArtifactLoadCollision");
        let field = assembly.alloc_string("value");
        assembly
            .class_def(ClassDef::new(
                name,
                true,
                0,
                None,
                vec![(field_type, field, Some(0))],
                vec![],
                Access::Private,
                None,
                None,
                true,
            ))
            .unwrap();
        assembly
    }

    #[test]
    fn accumulator_rejects_field_level_config_mismatch_before_linking() {
        let expected = ArtifactAbiConfig::default();
        let found = ArtifactAbiConfig::default()
            .with_dotnet_runtime(DotnetRuntime::UnityNetStandard21)
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
        assert!(diagnostic.contains("dotnet_runtime: expected Net10, found UnityNetStandard21"));
        assert!(diagnostic.contains("no_unwind: expected false, found true"));
    }

    #[test]
    fn accumulator_rejects_prefixless_legacy_artifact() {
        let legacy = postcard::to_stdvec(&Assembly::default()).unwrap();
        let mut accumulator = AssemblyAccumulator::default();
        let error = accumulator.merge_encoded(&legacy, "legacy.bc").unwrap_err();
        assert!(error.to_string().contains("incompatible cilly artifact"));
        assert!(error.to_string().contains("Rebuild all input crates"));
    }

    #[test]
    fn accumulator_link_error_preserves_loaded_assembly_and_reports_source() {
        let first = AssemblyArtifact::new(
            collision_assembly(Type::Int(Int::I32)),
            ArtifactAbiConfig::default(),
        );
        let second = AssemblyArtifact::new(
            collision_assembly(Type::Int(Int::I64)),
            ArtifactAbiConfig::default(),
        );
        let mut accumulator = AssemblyAccumulator::default();
        accumulator.merge_decoded(first, "first.bc").unwrap();
        let before_counts = accumulator.assembly.arena_counts();
        let before_bytes = postcard::to_stdvec(&accumulator.assembly).unwrap();
        let before_config = accumulator.abi_config.clone();

        let error = accumulator.merge_decoded(second, "second.bc").unwrap_err();
        assert!(matches!(error, ArtifactLoadError::Link { .. }));
        assert!(error.to_string().contains("second.bc"));
        assert_eq!(accumulator.assembly.arena_counts(), before_counts);
        assert_eq!(
            postcard::to_stdvec(&accumulator.assembly).unwrap(),
            before_bytes
        );
        assert_eq!(accumulator.abi_config, before_config);
    }
}
