//! Versioned serialization envelope for linkable `cilly` assemblies.
//!
//! The [`Assembly`] postcard representation is schema-versioned inside the envelope. A short magic
//! prefix distinguishes new artifacts from the historical raw-`Assembly` format, allowing the
//! decoder to retain an explicit legacy path without guessing after a versioned decode failure.

use crate::Assembly;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Prefix identifying the current, schema-v3 `cilly` assembly artifact before payload decoding.
pub const ASSEMBLY_ARTIFACT_MAGIC: &[u8; 8] = b"CILLYAR3";
/// Magic emitted by schema-v2 artifacts, before canonical method-scope exception regions.
const ASSEMBLY_ARTIFACT_V2_MAGIC: &[u8; 8] = b"CILLYAR2";
/// Magic emitted by schema-v1 artifacts, whose `BiMap` payload duplicated value storage.
const ASSEMBLY_ARTIFACT_V1_MAGIC: &[u8; 8] = b"CILLYART";
/// Current serialization-envelope version.
pub const ASSEMBLY_ARTIFACT_VERSION: u16 = 3;

/// Final output target selected by a backend or linker process.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum OutputTarget {
    /// .NET CIL/PE output.
    #[default]
    DotNet,
    /// C source output.
    C,
    /// JVM bytecode output.
    Java,
}

impl std::fmt::Display for OutputTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DotNet => f.write_str(".NET"),
            Self::C => f.write_str("C"),
            Self::Java => f.write_str("Java"),
        }
    }
}

/// .NET runtime surface available to generated code and linker-provided builtins.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum DotnetRuntime {
    /// .NET 8 API surface.
    #[default]
    Net8,
    /// .NET 9 API surface.
    Net9,
}

impl DotnetRuntime {
    /// Target-framework moniker for `runtimeconfig.json` / `.nuspec`.
    #[must_use]
    pub const fn tfm(self) -> &'static str {
        match self {
            Self::Net8 => "net8.0",
            Self::Net9 => "net9.0",
        }
    }

    /// The `.ver` triplet for a BCL `.assembly extern` stamp.
    #[must_use]
    pub const fn assembly_ver(self) -> &'static str {
        match self {
            Self::Net8 => "8:0:0:0",
            Self::Net9 => "9:0:0:0",
        }
    }

    /// The parsed `.ver` tuple used by the direct PE exporter's `AssemblyRef` rows.
    #[must_use]
    pub const fn assembly_ver_tuple(self) -> (u16, u16, u16, u16) {
        match self {
            Self::Net8 => (8, 0, 0, 0),
            Self::Net9 => (9, 0, 0, 0),
        }
    }

    /// `Microsoft.NETCore.App` framework-version floor for `runtimeconfig.json`.
    #[must_use]
    pub const fn framework_version(self) -> &'static str {
        match self {
            Self::Net8 => "8.0.0",
            Self::Net9 => "9.0.0",
        }
    }

    /// Runtime major version.
    #[must_use]
    pub const fn major(self) -> u32 {
        match self {
            Self::Net8 => 8,
            Self::Net9 => 9,
        }
    }
}

impl std::fmt::Display for DotnetRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Net8 => f.write_str(".NET 8"),
            Self::Net9 => f.write_str(".NET 9"),
        }
    }
}

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
            dotnet_runtime: DotnetRuntime::Net8,
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
    pub const fn with_no_unwind(mut self,
        no_unwind: bool) -> Self {
        self.no_unwind = no_unwind;
        self
    }

    /// Constructs the ABI contract from a caller-provided immutable environment snapshot.
    pub fn from_environment(
        environment: &HashMap<String, String>,
    ) -> Result<Self, ArtifactAbiConfigCaptureError> {
        let dotnet_runtime = match environment.get("DOTNET_VERSION").map(String::as_str) {
            None | Some("8" | "net8" | "net8.0") => DotnetRuntime::Net8,
            Some("9" | "net9" | "net9.0") => DotnetRuntime::Net9,
            Some(value) => {
                return Err(ArtifactAbiConfigCaptureError::InvalidValue {
                    variable: "DOTNET_VERSION",
                    value: value.to_owned(),
                    expected: "8 or 9 (also net8, net8.0, net9, or net9.0)",
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

/// Serialization format identified while decoding an artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactFormat {
    /// Magic-prefixed [`AssemblyArtifact`].
    Versioned,
    /// Historical raw postcard [`Assembly`].
    LegacyRawAssembly,
}

/// A decoded versioned artifact or explicitly recognized legacy raw assembly.
pub enum DecodedAssemblyArtifact {
    /// Current magic-prefixed artifact.
    Versioned(AssemblyArtifact),
    /// Legacy raw `Assembly`; no ABI contract was serialized with it.
    Legacy(Assembly),
}

impl DecodedAssemblyArtifact {
    /// Identified serialization format.
    #[must_use]
    pub const fn format(&self) -> ArtifactFormat {
        match self {
            Self::Versioned(_) => ArtifactFormat::Versioned,
            Self::Legacy(_) => ArtifactFormat::LegacyRawAssembly,
        }
    }

    /// Serialized ABI contract, absent for legacy raw assemblies.
    #[must_use]
    pub fn abi_config(&self) -> Option<&ArtifactAbiConfig> {
        match self {
            Self::Versioned(artifact) => Some(artifact.abi_config()),
            Self::Legacy(_) => None,
        }
    }

    /// Consumes the decoded value into assembly, optional ABI contract, and format.
    #[must_use]
    pub fn into_parts(self) -> (Assembly, Option<ArtifactAbiConfig>, ArtifactFormat) {
        match self {
            Self::Versioned(artifact) => {
                let (config, assembly) = artifact.into_parts();
                (assembly, Some(config), ArtifactFormat::Versioned)
            }
            Self::Legacy(assembly) => (assembly, None, ArtifactFormat::LegacyRawAssembly),
        }
    }
}

/// Decodes a versioned artifact, or a historical raw `Assembly` when the magic prefix is absent.
///
/// # Errors
///
/// Versioned artifacts never fall back to the legacy decoder: an unsupported or corrupt envelope
/// reports that exact failure. Prefix-less bytes report a legacy decode failure.
pub fn decode_assembly_artifact(
    encoded: &[u8],
) -> Result<DecodedAssemblyArtifact, ArtifactDecodeError> {
    if encoded.starts_with(ASSEMBLY_ARTIFACT_V1_MAGIC) {
        return Err(ArtifactDecodeError::UnsupportedVersion {
            found: 1,
            supported: ASSEMBLY_ARTIFACT_VERSION,
        });
    }
    if encoded.starts_with(ASSEMBLY_ARTIFACT_V2_MAGIC) {
        return Err(ArtifactDecodeError::UnsupportedVersion {
            found: 2,
            supported: ASSEMBLY_ARTIFACT_VERSION,
        });
    }
    if let Some(payload) = encoded.strip_prefix(ASSEMBLY_ARTIFACT_MAGIC) {
        let artifact: AssemblyArtifact =
            postcard::from_bytes(payload).map_err(ArtifactDecodeError::InvalidVersionedEnvelope)?;
        if artifact.version != ASSEMBLY_ARTIFACT_VERSION {
            return Err(ArtifactDecodeError::UnsupportedVersion {
                found: artifact.version,
                supported: ASSEMBLY_ARTIFACT_VERSION,
            });
        }
        Ok(DecodedAssemblyArtifact::Versioned(artifact))
    } else {
        postcard::from_bytes(encoded)
            .map(DecodedAssemblyArtifact::Legacy)
            .map_err(ArtifactDecodeError::InvalidLegacyAssembly)
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
    /// Prefix-less bytes were not a historical raw `Assembly`.
    InvalidLegacyAssembly(postcard::Error),
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
            Self::InvalidLegacyAssembly(error) => {
                write!(f, "invalid legacy raw cilly Assembly: {error}")
            }
        }
    }
}

impl std::error::Error for ArtifactDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedVersion { .. } => None,
            Self::InvalidVersionedEnvelope(error) | Self::InvalidLegacyAssembly(error) => {
                Some(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cilnode::MethodKind, Access, BasicBlock, CILRoot, ExceptionRegion, MethodDef, MethodImpl,
        Type,
    };
    #[test]
    fn versioned_artifact_round_trips_abi_config_and_assembly() {
        let mut assembly = Assembly::default();
        assembly.main_module();
        let config = ArtifactAbiConfig::default()
            .with_dotnet_runtime(DotnetRuntime::Net9)
            .with_no_unwind(true);
        let encoded = AssemblyArtifact::new(assembly, config.clone())
            .encode()
            .unwrap();
        assert!(encoded.starts_with(ASSEMBLY_ARTIFACT_MAGIC));

        let decoded = decode_assembly_artifact(&encoded).unwrap();
        assert_eq!(decoded.format(), ArtifactFormat::Versioned);
        assert_eq!(decoded.abi_config(), Some(&config));
        let (assembly, decoded_config, _) = decoded.into_parts();
        assert_eq!(decoded_config, Some(config));
        assert_eq!(assembly.class_defs().len(), 1);
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
        let (assembly, _, _) = decoded.into_parts();
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
            .with_dotnet_runtime(DotnetRuntime::Net9)
            .with_no_unwind(true);

        let error = expected.ensure_compatible(&found).unwrap_err();
        let fields: Vec<_> = error
            .differences()
            .iter()
            .map(ArtifactAbiConfigDifference::field)
            .collect();
        assert_eq!(
            fields,
            ["dotnet_runtime", "no_unwind"]
        );
        assert_eq!(
            error.to_string(),
            "incompatible artifact ABI configuration; dotnet_runtime: expected Net8, found Net9; \
             no_unwind: expected false, found true"
        );
    }

    #[test]
    fn legacy_raw_assembly_decode_is_explicit() {
        let mut assembly = Assembly::default();
        assembly.main_module();
        let encoded = postcard::to_stdvec(&assembly).unwrap();

        let decoded = decode_assembly_artifact(&encoded).unwrap();
        assert_eq!(decoded.format(), ArtifactFormat::LegacyRawAssembly);
        assert_eq!(decoded.abi_config(), None);
        let (assembly, config, format) = decoded.into_parts();
        assert_eq!(config, None);
        assert_eq!(format, ArtifactFormat::LegacyRawAssembly);
        assert_eq!(assembly.class_defs().len(), 1);
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
                found: 4,
                supported: 3
            }
        ));
    }

    #[test]
    fn v1_header_is_rejected_before_deserializing_its_old_bimap_shape() {
        let mut encoded = ASSEMBLY_ARTIFACT_V1_MAGIC.to_vec();
        encoded.extend_from_slice(b"payload shape intentionally irrelevant");

        let error = decode_assembly_artifact(&encoded).err().unwrap();
        assert!(matches!(
            error,
            ArtifactDecodeError::UnsupportedVersion {
                found: 1,
                supported: 3
            }
        ));
        assert!(error.to_string().contains("Rebuild all input crates"));
    }

    #[test]
    fn v2_header_is_rejected_before_deserializing_its_pre_region_body_shape() {
        let mut encoded = ASSEMBLY_ARTIFACT_V2_MAGIC.to_vec();
        encoded.extend_from_slice(b"payload shape intentionally irrelevant");

        let error = decode_assembly_artifact(&encoded).err().unwrap();
        assert!(matches!(
            error,
            ArtifactDecodeError::UnsupportedVersion {
                found: 2,
                supported: 3
            }
        ));
        assert!(error.to_string().contains("Rebuild all input crates"));
    }

    #[test]
    fn environment_snapshot_contains_only_abi_settings() {
        let environment = HashMap::from([
            ("DOTNET_VERSION".to_owned(), "net9.0".to_owned()),
            ("NO_UNWIND".to_owned(), "true".to_owned()),
            ("C_MODE".to_owned(), "ignored-linker-setting".to_owned()),
        ]);

        let config = ArtifactAbiConfig::from_environment(&environment).unwrap();
        assert_eq!(config.dotnet_runtime(), DotnetRuntime::Net9);
        assert!(config.no_unwind());
    }
}
