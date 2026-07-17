//! Versioned serialization envelope for linkable `cilly` assemblies.
//!
//! The [`Assembly`] postcard representation is schema-versioned inside the envelope.

use crate::Assembly;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Prefix identifying the current, schema-v9 `cilly` assembly artifact before payload decoding.
pub const ASSEMBLY_ARTIFACT_MAGIC: &[u8; 8] = b"CILLYAR9";
/// Current serialization-envelope version.
pub const ASSEMBLY_ARTIFACT_VERSION: u16 = 9;

/// Compatibility alias for the canonical runtime profile owned by the SDK crate.
pub use rust_dotnet_sdk_core::runtime::DotnetVersion as DotnetRuntime;

/// Immutable ABI choices that affect the IR emitted independently by each rustc process.
///
/// Final-link and emitter settings deliberately do not live here: one V2 assembly can be exported
/// to multiple targets, and allocator/emitter policy can be selected after all inputs are loaded.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactAbiConfig {
    dotnet_runtime: DotnetRuntime,
    no_unwind: bool,
}

impl Default for ArtifactAbiConfig {
    fn default() -> Self {
        Self {
            dotnet_runtime: DotnetRuntime::Net10,
            no_unwind: false,
        }
    }
}

impl ArtifactAbiConfig {
    /// Selects the runtime surface used while lowering this artifact.
    #[must_use]
    pub const fn with_dotnet_runtime(mut self, runtime: DotnetRuntime) -> Self {
        self.dotnet_runtime = runtime;
        self
    }

    /// Selects whether Rust unwind cleanup regions are omitted from this artifact.
    #[must_use]
    pub const fn with_no_unwind(mut self, no_unwind: bool) -> Self {
        self.no_unwind = no_unwind;
        self
    }

    /// Constructs the ABI contract from a caller-provided immutable environment snapshot.
    pub fn from_environment(
        environment: &HashMap<String, String>,
    ) -> Result<Self, ArtifactAbiConfigCaptureError> {
        let dotnet_runtime = match environment.get("DOTNET_VERSION").map(String::as_str) {
            None | Some("10" | "net10" | "net10.0") => DotnetRuntime::Net10,
            Some("unity" | "unity-netstandard2.1" | "netstandard2.1") => {
                DotnetRuntime::UnityNetStandard21
            }
            Some(value) => {
                return Err(ArtifactAbiConfigCaptureError::InvalidValue {
                    variable: "DOTNET_VERSION",
                    value: value.to_owned(),
                    expected: "10 or unity-netstandard2.1",
                });
            }
        };

        Ok(Self {
            dotnet_runtime,
            no_unwind: parse_bool(environment, "NO_UNWIND", false)?,
        })
    }

    /// .NET runtime API surface selected for this artifact.
    #[must_use]
    pub const fn dotnet_runtime(&self) -> DotnetRuntime {
        self.dotnet_runtime
    }

    /// Whether generated MIR cleanup/unwind regions are disabled.
    #[must_use]
    pub const fn no_unwind(&self) -> bool {
        self.no_unwind
    }

    /// Verifies that another artifact was produced with the same ABI contract.
    ///
    /// All differing fields are reported together so the operator does not have to fix one
    /// environment variable at a time.
    pub fn ensure_compatible(&self, found: &Self) -> Result<(), ArtifactAbiConfigMismatch> {
        let mut differences = Vec::new();
        macro_rules! compare {
            ($field:ident) => {
                if self.$field != found.$field {
                    differences.push(ArtifactAbiConfigDifference {
                        field: stringify!($field),
                        expected: format!("{:?}", self.$field),
                        found: format!("{:?}", found.$field),
                    });
                }
            };
        }
        compare!(dotnet_runtime);
        compare!(no_unwind);

        if differences.is_empty() {
            Ok(())
        } else {
            Err(ArtifactAbiConfigMismatch { differences })
        }
    }
}

impl std::fmt::Display for ArtifactAbiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "runtime={}, no_unwind={}",
            self.dotnet_runtime, self.no_unwind,
        )
    }
}

fn parse_bool(
    environment: &HashMap<String, String>,
    variable: &'static str,
    default: bool,
) -> Result<bool, ArtifactAbiConfigCaptureError> {
    match environment.get(variable).map(String::as_str) {
        None => Ok(default),
        Some("0" | "false" | "False" | "FALSE") => Ok(false),
        Some("1" | "true" | "True" | "TRUE") => Ok(true),
        Some(value) => Err(ArtifactAbiConfigCaptureError::InvalidValue {
            variable,
            value: value.to_owned(),
            expected: "a boolean (0, 1, false, or true)",
        }),
    }
}

/// Failure to parse an artifact ABI contract from an environment snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactAbiConfigCaptureError {
    /// A variable had an unsupported value.
    InvalidValue {
        /// Environment-variable name.
        variable: &'static str,
        /// Invalid value.
        value: String,
        /// Human-readable accepted shape.
        expected: &'static str,
    },
}

impl std::fmt::Display for ArtifactAbiConfigCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidValue {
                variable,
                value,
                expected,
            } => write!(
                f,
                "{variable} has invalid value {value:?}; expected {expected}"
            ),
        }
    }
}

impl std::error::Error for ArtifactAbiConfigCaptureError {}

/// One field that differs between two linked artifact ABI contracts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactAbiConfigDifference {
    field: &'static str,
    expected: String,
    found: String,
}

impl ArtifactAbiConfigDifference {
    /// Name of the incompatible field.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Value established by the first versioned artifact.
    #[must_use]
    pub fn expected(&self) -> &str {
        &self.expected
    }

    /// Value found in the incompatible artifact.
    #[must_use]
    pub fn found(&self) -> &str {
        &self.found
    }
}

/// Field-level report for incompatible linked artifact configurations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactAbiConfigMismatch {
    differences: Vec<ArtifactAbiConfigDifference>,
}

impl ArtifactAbiConfigMismatch {
    /// All differences, in stable contract-field order.
    #[must_use]
    pub fn differences(&self) -> &[ArtifactAbiConfigDifference] {
        &self.differences
    }
}

impl std::fmt::Display for ArtifactAbiConfigMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "incompatible artifact ABI configuration")?;
        for difference in &self.differences {
            write!(
                f,
                "; {}: expected {}, found {}",
                difference.field, difference.expected, difference.found
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for ArtifactAbiConfigMismatch {}

/// Versioned payload stored after [`ASSEMBLY_ARTIFACT_MAGIC`].
#[derive(Clone, Deserialize, Serialize)]
pub struct AssemblyArtifact {
    version: u16,
    abi_config: ArtifactAbiConfig,
    assembly: Assembly,
}

impl AssemblyArtifact {
    /// Wraps an assembly in the current artifact version and immutable ABI contract.
    #[must_use]
    pub const fn new(assembly: Assembly, abi_config: ArtifactAbiConfig) -> Self {
        Self {
            version: ASSEMBLY_ARTIFACT_VERSION,
            abi_config,
            assembly,
        }
    }

    /// Envelope format version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Immutable artifact ABI contract serialized with the assembly.
    #[must_use]
    pub const fn abi_config(&self) -> &ArtifactAbiConfig {
        &self.abi_config
    }

    /// Serialized assembly payload.
    #[must_use]
    pub const fn assembly(&self) -> &Assembly {
        &self.assembly
    }

    /// Consumes the envelope into its ABI contract and assembly.
    #[must_use]
    pub fn into_parts(self) -> (ArtifactAbiConfig, Assembly) {
        (self.abi_config, self.assembly)
    }

    /// Serializes this envelope with its identifying magic prefix.
    ///
    /// # Errors
    ///
    /// Returns postcard's serialization error if the payload cannot be encoded.
    pub fn encode(&self) -> Result<Vec<u8>, postcard::Error> {
        let payload = postcard::to_stdvec(self)?;
        let mut encoded = Vec::with_capacity(ASSEMBLY_ARTIFACT_MAGIC.len() + payload.len());
        encoded.extend_from_slice(ASSEMBLY_ARTIFACT_MAGIC);
        encoded.extend_from_slice(&payload);
        Ok(encoded)
    }
}

/// Decodes a current versioned artifact.
///
/// # Errors
///
/// Older magic prefixes and prefix-less bytes are rejected. Rebuild all inputs with the current
/// backend rather than attempting schema migration.
pub fn decode_assembly_artifact(encoded: &[u8]) -> Result<AssemblyArtifact, ArtifactDecodeError> {
    if let Some(payload) = encoded.strip_prefix(ASSEMBLY_ARTIFACT_MAGIC) {
        let artifact: AssemblyArtifact =
            postcard::from_bytes(payload).map_err(ArtifactDecodeError::InvalidVersionedEnvelope)?;
        if artifact.version != ASSEMBLY_ARTIFACT_VERSION {
            return Err(ArtifactDecodeError::UnsupportedVersion {
                found: artifact.version,
                supported: ASSEMBLY_ARTIFACT_VERSION,
            });
        }
        Ok(artifact)
    } else {
        Err(ArtifactDecodeError::IncompatibleArtifact)
    }
}

/// Failure to decode a `cilly` assembly artifact.
#[derive(Debug)]
pub enum ArtifactDecodeError {
    /// The magic prefix was present, but the envelope version is unsupported.
    UnsupportedVersion {
        /// Version stored in the artifact.
        found: u16,
        /// Version supported by this linker.
        supported: u16,
    },
    /// The magic prefix was present, but the envelope payload was malformed.
    InvalidVersionedEnvelope(postcard::Error),
    /// The bytes are not a current CILLYAR9 artifact.
    IncompatibleArtifact,
}

impl std::fmt::Display for ArtifactDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "unsupported cilly artifact version {found}; this linker requires schema version \
                 {supported}. Rebuild all input crates/artifacts with the current backend"
            ),
            Self::InvalidVersionedEnvelope(error) => {
                write!(f, "invalid versioned cilly artifact envelope: {error}")
            }
            Self::IncompatibleArtifact => write!(
                f,
                "incompatible cilly artifact; expected CILLYAR9 schema {}. Rebuild all input crates/artifacts with the current backend",
                ASSEMBLY_ARTIFACT_VERSION
            ),
        }
    }
}

impl std::error::Error for ArtifactDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedVersion { .. } | Self::IncompatibleArtifact => None,
            Self::InvalidVersionedEnvelope(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Access, BasicBlock, CILRoot, ClassDef, ClassRef, ExceptionRegion, MethodDef, MethodImpl,
        Type, cilnode::MethodKind, class::FixedArrayLayout,
    };
    #[test]
    fn versioned_artifact_round_trips_abi_config_and_assembly() {
        let mut assembly = Assembly::default();
        assembly.main_module();
        let config = ArtifactAbiConfig::default()
            .with_dotnet_runtime(DotnetRuntime::Net10)
            .with_no_unwind(true);
        let encoded = AssemblyArtifact::new(assembly, config.clone())
            .encode()
            .unwrap();
        assert!(encoded.starts_with(ASSEMBLY_ARTIFACT_MAGIC));

        let decoded = decode_assembly_artifact(&encoded).unwrap();
        assert_eq!(decoded.abi_config(), &config);
        let (decoded_config, assembly) = decoded.into_parts();
        assert_eq!(decoded_config, config);
        assert_eq!(assembly.class_defs().len(), 1);
    }

    #[test]
    fn versioned_artifact_round_trips_fixed_array_layout_provenance() {
        let mut assembly = Assembly::default();
        let element_name = assembly.alloc_string("SerializedExpandedElement");
        let element = assembly.alloc_class_ref(ClassRef::new(element_name, None, true, [].into()));
        assembly
            .class_def(ClassDef::new(
                element_name,
                true,
                0,
                None,
                vec![],
                vec![],
                Access::Public,
                std::num::NonZeroU32::new(24),
                std::num::NonZeroU32::new(8),
                false,
            ))
            .unwrap();
        let array =
            ClassRef::fixed_array_with_layout(Type::ClassRef(element), 2, 32, 8, &mut assembly);
        let array_name = assembly.class_ref(array).name();
        assembly
            .class_def(
                ClassDef::new(
                    array_name,
                    true,
                    0,
                    None,
                    vec![],
                    vec![],
                    Access::Public,
                    std::num::NonZeroU32::new(32),
                    std::num::NonZeroU32::new(8),
                    true,
                )
                .with_fixed_array_layout(FixedArrayLayout::new(
                    Type::ClassRef(element),
                    2,
                    32,
                    32,
                    8,
                )),
            )
            .unwrap();

        let encoded = AssemblyArtifact::new(assembly, ArtifactAbiConfig::default())
            .encode()
            .unwrap();
        let decoded = decode_assembly_artifact(&encoded).unwrap();
        let (_, assembly) = decoded.into_parts();
        assert!(
            assembly
                .validate_fixed_array_layouts()
                .unwrap_err()
                .contains("representation-expanded element")
        );
    }

    #[test]
    fn versioned_artifact_round_trips_canonical_exception_regions() {
        let mut assembly = Assembly::default();
        let owner = assembly.main_module();
        let sig = assembly.sig([], Type::Void);
        let name = assembly.alloc_string("artifact_region_body");
        let normal_root = assembly.alloc_root(CILRoot::Nop);
        let cleanup_root = assembly.alloc_root(CILRoot::ReThrow);
        assembly.new_method(MethodDef::new(
            Access::Public,
            owner,
            name,
            sig,
            MethodKind::Static,
            MethodImpl::RegionBody {
                blocks: vec![BasicBlock::new(vec![normal_root], 0, None)],
                cleanup_blocks: vec![BasicBlock::new(vec![cleanup_root], 10, None)],
                exception_regions: vec![ExceptionRegion::new(0, 10)],
                locals: vec![],
            },
            vec![],
        ));

        let encoded = AssemblyArtifact::new(assembly, ArtifactAbiConfig::default())
            .encode()
            .unwrap();
        let decoded = decode_assembly_artifact(&encoded).unwrap();
        let (_, assembly) = decoded.into_parts();
        let method = assembly
            .method_defs()
            .values()
            .find(|method| &assembly[method.name()] == "artifact_region_body")
            .unwrap();
        let MethodImpl::RegionBody {
            blocks,
            cleanup_blocks,
            exception_regions,
            ..
        } = method.implementation()
        else {
            panic!("canonical region body did not round-trip")
        };
        assert_eq!(blocks[0].block_id(), 0);
        assert_eq!(cleanup_blocks[0].block_id(), 10);
        assert_eq!(exception_regions, &[ExceptionRegion::new(0, 10)]);
    }

    #[test]
    fn abi_config_mismatch_reports_every_differing_field() {
        let expected = ArtifactAbiConfig::default();
        let found = ArtifactAbiConfig::default()
            .with_dotnet_runtime(DotnetRuntime::UnityNetStandard21)
            .with_no_unwind(true);

        let error = expected.ensure_compatible(&found).unwrap_err();
        let fields: Vec<_> = error
            .differences()
            .iter()
            .map(ArtifactAbiConfigDifference::field)
            .collect();
        assert_eq!(fields, ["dotnet_runtime", "no_unwind"]);
        assert_eq!(
            error.to_string(),
            "incompatible artifact ABI configuration; dotnet_runtime: expected Net10, found UnityNetStandard21; \
             no_unwind: expected false, found true"
        );
    }

    #[test]
    fn magic_prefixed_unsupported_payload_version_never_falls_back_to_legacy() {
        let mut artifact = AssemblyArtifact::new(Assembly::default(), ArtifactAbiConfig::default());
        artifact.version = ASSEMBLY_ARTIFACT_VERSION + 1;
        let encoded = artifact.encode().unwrap();

        let error = decode_assembly_artifact(&encoded).err().unwrap();
        assert!(matches!(
            error,
            ArtifactDecodeError::UnsupportedVersion {
                found,
                supported: ASSEMBLY_ARTIFACT_VERSION
            } if found == ASSEMBLY_ARTIFACT_VERSION + 1
        ));
    }

    #[test]
    fn prefixless_legacy_artifact_is_rejected_for_clean_rebuild() {
        let encoded = postcard::to_stdvec(&Assembly::default()).unwrap();
        let error = decode_assembly_artifact(&encoded).err().unwrap();
        assert!(matches!(error, ArtifactDecodeError::IncompatibleArtifact));
        assert!(error.to_string().contains("CILLYAR9"));
        assert!(error.to_string().contains("Rebuild all input crates"));
    }

    #[test]
    fn environment_snapshot_contains_only_abi_settings() {
        let environment = HashMap::from([
            ("DOTNET_VERSION".to_owned(), "net10.0".to_owned()),
            ("NO_UNWIND".to_owned(), "true".to_owned()),
        ]);

        let config = ArtifactAbiConfig::from_environment(&environment).unwrap();
        assert_eq!(config.dotnet_runtime(), DotnetRuntime::Net10);
        assert!(config.no_unwind());
    }

    #[test]
    fn environment_snapshot_accepts_unity_profile() {
        let environment = HashMap::from([(
            "DOTNET_VERSION".to_owned(),
            "unity-netstandard2.1".to_owned(),
        )]);
        let config = ArtifactAbiConfig::from_environment(&environment).unwrap();
        assert_eq!(config.dotnet_runtime(), DotnetRuntime::UnityNetStandard21);
        assert_eq!(config.dotnet_runtime().tfm(), "netstandard2.1");
        assert!(!config.dotnet_runtime().supports_subword_interlocked());
    }
}
