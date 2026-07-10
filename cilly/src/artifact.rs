//! Versioned serialization envelope for linkable `cilly` assemblies.
//!
//! The [`Assembly`] postcard representation remains unchanged inside the envelope. A short magic
//! prefix distinguishes new artifacts from the historical raw-`Assembly` format, allowing the
//! decoder to retain an explicit legacy path without guessing after a versioned decode failure.

use crate::Assembly;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Prefix identifying the current, schema-v2 `cilly` assembly artifact before payload decoding.
pub const ASSEMBLY_ARTIFACT_MAGIC: &[u8; 8] = b"CILLYAR2";
/// Magic emitted by schema-v1 artifacts, whose `BiMap` payload duplicated value storage.
const ASSEMBLY_ARTIFACT_V1_MAGIC: &[u8; 8] = b"CILLYART";
/// Current serialization-envelope version.
pub const ASSEMBLY_ARTIFACT_VERSION: u16 = 2;

/// Output target whose linker/runtime semantics the artifact was generated for.
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
///
/// This is the serde-stable artifact counterpart of the existing runtime-only
/// [`crate::DotnetVersion`]. The follow-up configuration-consumer migration should collapse the
/// duplicate by making `DotnetVersion` derive serde or by replacing its environment-backed global
/// with this contract value.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum DotnetRuntime {
    /// .NET 8 API surface.
    #[default]
    Net8,
    /// .NET 9 API surface.
    Net9,
}

impl std::fmt::Display for DotnetRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Net8 => f.write_str(".NET 8"),
            Self::Net9 => f.write_str(".NET 9"),
        }
    }
}

/// Immutable subset of build configuration that changes generated IR or link semantics.
///
/// Debugging, verification, optimizer-fuel, and final-emitter controls deliberately do not live
/// here: artifacts produced with different values for those controls remain link-compatible.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildConfig {
    target: OutputTarget,
    dotnet_runtime: DotnetRuntime,
    no_unwind: bool,
    abort_on_error: bool,
    guaranteed_align: u8,
    max_static_size: u64,
    pool_alloc: bool,
    panic_managed_backtrace: bool,
    native_passthrough: bool,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            target: OutputTarget::DotNet,
            dotnet_runtime: DotnetRuntime::Net8,
            no_unwind: false,
            abort_on_error: false,
            guaranteed_align: 8,
            max_static_size: 16,
            pool_alloc: false,
            panic_managed_backtrace: false,
            native_passthrough: false,
        }
    }
}

impl BuildConfig {
    /// Constructs an immutable build contract from already parsed values.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        target: OutputTarget,
        dotnet_runtime: DotnetRuntime,
        no_unwind: bool,
        abort_on_error: bool,
        guaranteed_align: u8,
        max_static_size: u64,
        pool_alloc: bool,
        panic_managed_backtrace: bool,
        native_passthrough: bool,
    ) -> Self {
        Self {
            target,
            dotnet_runtime,
            no_unwind,
            abort_on_error,
            guaranteed_align,
            max_static_size,
            pool_alloc,
            panic_managed_backtrace,
            native_passthrough,
        }
    }

    /// Captures all contract-relevant environment variables from one immutable environment
    /// snapshot.
    ///
    /// # Errors
    ///
    /// Returns a precise error for invalid values or mutually exclusive output modes.
    pub fn capture() -> Result<Self, BuildConfigCaptureError> {
        let environment: HashMap<String, String> = std::env::vars().collect();
        Self::from_environment(&environment)
    }

    fn from_environment(
        environment: &HashMap<String, String>,
    ) -> Result<Self, BuildConfigCaptureError> {
        let c_mode = parse_bool(environment, "C_MODE", false)?;
        let java_mode = parse_bool(environment, "JAVA_MODE", false)?;
        let target = match (c_mode, java_mode) {
            (false, false) => OutputTarget::DotNet,
            (true, false) => OutputTarget::C,
            (false, true) => OutputTarget::Java,
            (true, true) => {
                return Err(BuildConfigCaptureError::ConflictingOutputModes {
                    enabled: vec!["C_MODE", "JAVA_MODE"],
                });
            }
        };
        let dotnet_runtime = match environment.get("DOTNET_VERSION").map(String::as_str) {
            None | Some("8" | "net8" | "net8.0") => DotnetRuntime::Net8,
            Some("9" | "net9" | "net9.0") => DotnetRuntime::Net9,
            Some(value) => {
                return Err(BuildConfigCaptureError::InvalidValue {
                    variable: "DOTNET_VERSION",
                    value: value.to_owned(),
                    expected: "8 or 9 (also net8, net8.0, net9, or net9.0)",
                });
            }
        };

        Ok(Self {
            target,
            dotnet_runtime,
            no_unwind: parse_bool(environment, "NO_UNWIND", false)?,
            abort_on_error: parse_bool(environment, "ABORT_ON_ERROR", false)?,
            guaranteed_align: parse_number(environment, "GUARANTEED_ALIGN", 8)?,
            max_static_size: parse_number(environment, "MAX_STATIC_SIZE", 16)?,
            pool_alloc: parse_bool(environment, "POOL_ALLOC", false)?,
            panic_managed_backtrace: parse_bool(environment, "PANIC_MANAGED_BT", false)?,
            native_passthrough: parse_bool(environment, "NATIVE_PASSTROUGH", false)?,
        })
    }

    /// Output target selected for this artifact.
    #[must_use]
    pub const fn target(&self) -> OutputTarget {
        self.target
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

    /// Whether unsupported codegen operations abort instead of producing recovery stubs.
    #[must_use]
    pub const fn abort_on_error(&self) -> bool {
        self.abort_on_error
    }

    /// Minimum alignment assumed for generated values.
    #[must_use]
    pub const fn guaranteed_align(&self) -> u8 {
        self.guaranteed_align
    }

    /// Largest static value size handled by the inline/static path.
    #[must_use]
    pub const fn max_static_size(&self) -> u64 {
        self.max_static_size
    }

    /// Whether linker-provided allocator shims use the experimental pool.
    #[must_use]
    pub const fn pool_alloc(&self) -> bool {
        self.pool_alloc
    }

    /// Whether panic shims preserve managed backtraces.
    #[must_use]
    pub const fn panic_managed_backtrace(&self) -> bool {
        self.panic_managed_backtrace
    }

    /// Whether native libraries are bundled through the native-pass-through path.
    #[must_use]
    pub const fn native_passthrough(&self) -> bool {
        self.native_passthrough
    }

    /// Verifies that another artifact was produced with the same link-semantic contract.
    ///
    /// All differing fields are reported together so the operator does not have to fix one
    /// environment variable at a time.
    pub fn ensure_compatible(&self, found: &Self) -> Result<(), BuildConfigMismatch> {
        let mut differences = Vec::new();
        macro_rules! compare {
            ($field:ident) => {
                if self.$field != found.$field {
                    differences.push(BuildConfigDifference {
                        field: stringify!($field),
                        expected: format!("{:?}", self.$field),
                        found: format!("{:?}", found.$field),
                    });
                }
            };
        }
        compare!(target);
        compare!(dotnet_runtime);
        compare!(no_unwind);
        compare!(abort_on_error);
        compare!(guaranteed_align);
        compare!(max_static_size);
        compare!(pool_alloc);
        compare!(panic_managed_backtrace);
        compare!(native_passthrough);

        if differences.is_empty() {
            Ok(())
        } else {
            Err(BuildConfigMismatch { differences })
        }
    }
}

impl std::fmt::Display for BuildConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "target={}, runtime={}, no_unwind={}, abort_on_error={}, guaranteed_align={}, \
             max_static_size={}, pool_alloc={}, panic_managed_backtrace={}, native_passthrough={}",
            self.target,
            self.dotnet_runtime,
            self.no_unwind,
            self.abort_on_error,
            self.guaranteed_align,
            self.max_static_size,
            self.pool_alloc,
            self.panic_managed_backtrace,
            self.native_passthrough,
        )
    }
}

fn parse_bool(
    environment: &HashMap<String, String>,
    variable: &'static str,
    default: bool,
) -> Result<bool, BuildConfigCaptureError> {
    match environment.get(variable).map(String::as_str) {
        None => Ok(default),
        Some("0" | "false" | "False" | "FALSE") => Ok(false),
        Some("1" | "true" | "True" | "TRUE") => Ok(true),
        Some(value) => Err(BuildConfigCaptureError::InvalidValue {
            variable,
            value: value.to_owned(),
            expected: "a boolean (0, 1, false, or true)",
        }),
    }
}

fn parse_number<T>(
    environment: &HashMap<String, String>,
    variable: &'static str,
    default: T,
) -> Result<T, BuildConfigCaptureError>
where
    T: std::str::FromStr,
{
    match environment.get(variable) {
        None => Ok(default),
        Some(value) => value
            .parse()
            .map_err(|_| BuildConfigCaptureError::InvalidValue {
                variable,
                value: value.clone(),
                expected: "a non-negative integer in range",
            }),
    }
}

/// Failure to parse a build contract from the environment snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildConfigCaptureError {
    /// A variable had an unsupported value.
    InvalidValue {
        /// Environment-variable name.
        variable: &'static str,
        /// Invalid value.
        value: String,
        /// Human-readable accepted shape.
        expected: &'static str,
    },
    /// More than one mutually exclusive output mode was selected.
    ConflictingOutputModes {
        /// Enabled mode variables.
        enabled: Vec<&'static str>,
    },
}

impl std::fmt::Display for BuildConfigCaptureError {
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
            Self::ConflictingOutputModes { enabled } => write!(
                f,
                "conflicting output modes: {}; enable at most one",
                enabled.join(", ")
            ),
        }
    }
}

impl std::error::Error for BuildConfigCaptureError {}

/// One field that differs between two linked build contracts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildConfigDifference {
    field: &'static str,
    expected: String,
    found: String,
}

impl BuildConfigDifference {
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
pub struct BuildConfigMismatch {
    differences: Vec<BuildConfigDifference>,
}

impl BuildConfigMismatch {
    /// All differences, in stable contract-field order.
    #[must_use]
    pub fn differences(&self) -> &[BuildConfigDifference] {
        &self.differences
    }
}

impl std::fmt::Display for BuildConfigMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "incompatible build configuration")?;
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

impl std::error::Error for BuildConfigMismatch {}

/// Versioned payload stored after [`ASSEMBLY_ARTIFACT_MAGIC`].
#[derive(Clone, Deserialize, Serialize)]
pub struct AssemblyArtifact {
    version: u16,
    build_config: BuildConfig,
    assembly: Assembly,
}

impl AssemblyArtifact {
    /// Wraps an assembly in the current artifact version and immutable build contract.
    #[must_use]
    pub const fn new(assembly: Assembly, build_config: BuildConfig) -> Self {
        Self {
            version: ASSEMBLY_ARTIFACT_VERSION,
            build_config,
            assembly,
        }
    }

    /// Envelope format version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Immutable build contract serialized with the assembly.
    #[must_use]
    pub const fn build_config(&self) -> &BuildConfig {
        &self.build_config
    }

    /// Serialized assembly payload.
    #[must_use]
    pub const fn assembly(&self) -> &Assembly {
        &self.assembly
    }

    /// Consumes the envelope into its build contract and assembly.
    #[must_use]
    pub fn into_parts(self) -> (BuildConfig, Assembly) {
        (self.build_config, self.assembly)
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
    /// Legacy raw `Assembly`; no build contract was serialized with it.
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

    /// Serialized build contract, absent for legacy raw assemblies.
    #[must_use]
    pub fn build_config(&self) -> Option<&BuildConfig> {
        match self {
            Self::Versioned(artifact) => Some(artifact.build_config()),
            Self::Legacy(_) => None,
        }
    }

    /// Consumes the decoded value into assembly, optional build contract, and format.
    #[must_use]
    pub fn into_parts(self) -> (Assembly, Option<BuildConfig>, ArtifactFormat) {
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

    #[test]
    fn versioned_artifact_round_trips_config_and_assembly() {
        let mut assembly = Assembly::default();
        assembly.main_module();
        let config = BuildConfig {
            target: OutputTarget::C,
            dotnet_runtime: DotnetRuntime::Net9,
            no_unwind: true,
            pool_alloc: true,
            ..BuildConfig::default()
        };
        let encoded = AssemblyArtifact::new(assembly, config.clone())
            .encode()
            .unwrap();
        assert!(encoded.starts_with(ASSEMBLY_ARTIFACT_MAGIC));

        let decoded = decode_assembly_artifact(&encoded).unwrap();
        assert_eq!(decoded.format(), ArtifactFormat::Versioned);
        assert_eq!(decoded.build_config(), Some(&config));
        let (assembly, decoded_config, _) = decoded.into_parts();
        assert_eq!(decoded_config, Some(config));
        assert_eq!(assembly.class_defs().len(), 1);
    }

    #[test]
    fn build_config_mismatch_reports_every_differing_field() {
        let expected = BuildConfig::default();
        let found = BuildConfig {
            target: OutputTarget::Java,
            dotnet_runtime: DotnetRuntime::Net9,
            no_unwind: true,
            max_static_size: 32,
            ..BuildConfig::default()
        };

        let error = expected.ensure_compatible(&found).unwrap_err();
        let fields: Vec<_> = error
            .differences()
            .iter()
            .map(BuildConfigDifference::field)
            .collect();
        assert_eq!(
            fields,
            ["target", "dotnet_runtime", "no_unwind", "max_static_size"]
        );
        assert_eq!(
            error.to_string(),
            "incompatible build configuration; target: expected DotNet, found Java; \
             dotnet_runtime: expected Net8, found Net9; no_unwind: expected false, found true; \
             max_static_size: expected 16, found 32"
        );
    }

    #[test]
    fn legacy_raw_assembly_decode_is_explicit() {
        let mut assembly = Assembly::default();
        assembly.main_module();
        let encoded = postcard::to_stdvec(&assembly).unwrap();

        let decoded = decode_assembly_artifact(&encoded).unwrap();
        assert_eq!(decoded.format(), ArtifactFormat::LegacyRawAssembly);
        assert_eq!(decoded.build_config(), None);
        let (assembly, config, format) = decoded.into_parts();
        assert_eq!(config, None);
        assert_eq!(format, ArtifactFormat::LegacyRawAssembly);
        assert_eq!(assembly.class_defs().len(), 1);
    }

    #[test]
    fn magic_prefixed_unsupported_payload_version_never_falls_back_to_legacy() {
        let mut artifact = AssemblyArtifact::new(Assembly::default(), BuildConfig::default());
        artifact.version = ASSEMBLY_ARTIFACT_VERSION + 1;
        let encoded = artifact.encode().unwrap();

        let error = decode_assembly_artifact(&encoded).err().unwrap();
        assert!(matches!(
            error,
            ArtifactDecodeError::UnsupportedVersion {
                found: 3,
                supported: 2
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
                supported: 2
            }
        ));
        assert!(error.to_string().contains("Rebuild all input crates"));
    }

    #[test]
    fn environment_snapshot_rejects_conflicting_targets() {
        let environment = HashMap::from([
            ("C_MODE".to_owned(), "1".to_owned()),
            ("JAVA_MODE".to_owned(), "true".to_owned()),
            ("DUMP_FN".to_owned(), "ignored-debug-setting".to_owned()),
        ]);

        let error = BuildConfig::from_environment(&environment).unwrap_err();
        assert_eq!(
            error,
            BuildConfigCaptureError::ConflictingOutputModes {
                enabled: vec!["C_MODE", "JAVA_MODE"]
            }
        );
    }
}
