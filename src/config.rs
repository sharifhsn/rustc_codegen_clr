//! Backend configuration captured from one immutable environment snapshot.
//!
//! Only [`ArtifactAbiConfig`] is serialized. Output selection, recovery policy, diagnostics, and
//! test controls are local to the rustc process and never make otherwise-compatible artifacts
//! reject one another.

use cilly::{ArtifactAbiConfig, ArtifactAbiConfigCaptureError, DotnetRuntime, OutputTarget};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::sync::OnceLock;

/// A removed setting and, when one exists, its supported replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetiredSetting {
    name: &'static str,
    replacement: Option<&'static str>,
}

impl RetiredSetting {
    const fn removed(name: &'static str) -> Self {
        Self {
            name,
            replacement: None,
        }
    }

    const fn renamed(name: &'static str, replacement: &'static str) -> Self {
        Self {
            name,
            replacement: Some(replacement),
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    #[must_use]
    pub const fn replacement(self) -> Option<&'static str> {
        self.replacement
    }
}

/// Historical no-ops and misspelled names which must no longer fail silently.
pub const RETIRED_SETTINGS: &[RetiredSetting] = &[
    RetiredSetting::renamed("ASCI_IDENT", "ASCII_IDENTS"),
    RetiredSetting::removed("CHECK_ALLOCATIONS"),
    RetiredSetting::removed("CHECK_REFS"),
    RetiredSetting::removed("C_SANITIZE"),
    RetiredSetting::removed("ENFORCE_CIL_VALID"),
    RetiredSetting::removed("ESCAPE_NAMES"),
    RetiredSetting::removed("INLINE_SIMPLE_FUNCTIONS"),
    RetiredSetting::renamed("JS_MODE", "JAVA_MODE"),
    RetiredSetting::removed("MAX_STATIC_SIZE"),
    RetiredSetting::removed("NEW_UNSIZE"),
    RetiredSetting::removed("REMOVE_UNSUED_LOCALS"),
    RetiredSetting::removed("SPLIT_LOCAL_STRUCTS"),
    RetiredSetting::removed("TRACE_CIL_OPS"),
    RetiredSetting::removed("VALIDTE_VALUES"),
];

const ACTIVE_SETTINGS: &[&str] = &[
    "ABORT_ON_ERROR",
    "C_MODE",
    "DOTNET_VERSION",
    "DRY_RUN",
    "DUMP_LAYOUT",
    "DUMP_LAYOUT_OUT",
    "DUMP_MIR",
    "DUMP_MIR_OUT",
    "INSERT_MIR_DEBUG_COMMENTS",
    "JAVA_MODE",
    "NO_UNWIND",
    "PRINT_LOCAL_TYPES",
    "RANDOMIZE_LAYOUT",
    "RCL_ICE_LOG",
    "TEST_WITH_MONO",
    "TRACE_FN",
    "TRACE_VAL",
];

/// Complete immutable configuration used by one rustc-facing backend process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendConfig {
    abi: ArtifactAbiConfig,
    output_target: OutputTarget,
    abort_on_error: bool,
    insert_mir_debug_comments: bool,
    print_local_types: bool,
    test_with_mono: bool,
    randomize_layout: bool,
    dry_run: bool,
    dump_mir: Option<String>,
    dump_mir_out: String,
    trace_fn: Option<String>,
    dump_layout: Option<String>,
    dump_layout_out: String,
    trace_val: Option<String>,
    rcl_ice_log: bool,
}

impl BackendConfig {
    /// Captures the process environment once, then parses only the recognized settings.
    pub fn capture() -> Result<Self, BackendConfigError> {
        let snapshot: HashMap<OsString, OsString> = std::env::vars_os().collect();
        let environment = recognized_environment(&snapshot)?;
        Self::from_environment(&environment)
    }

    fn from_environment(environment: &HashMap<String, String>) -> Result<Self, BackendConfigError> {
        let retired = retired_environment_variables(environment);
        if !retired.is_empty() {
            return Err(BackendConfigError::RetiredSettings(retired));
        }

        let c_mode = parse_bool(environment, "C_MODE", false)?;
        let java_mode = parse_bool(environment, "JAVA_MODE", false)?;
        let output_target = match (c_mode, java_mode) {
            (false, false) => OutputTarget::DotNet,
            (true, false) => OutputTarget::C,
            (false, true) => OutputTarget::Java,
            (true, true) => {
                return Err(BackendConfigError::ConflictingOutputModes {
                    enabled: vec!["C_MODE", "JAVA_MODE"],
                });
            }
        };

        Ok(Self {
            abi: ArtifactAbiConfig::from_environment(environment)?,
            output_target,
            abort_on_error: parse_bool(environment, "ABORT_ON_ERROR", false)?,
            insert_mir_debug_comments: parse_bool(environment, "INSERT_MIR_DEBUG_COMMENTS", false)?,
            print_local_types: parse_bool(environment, "PRINT_LOCAL_TYPES", false)?,
            test_with_mono: parse_bool(environment, "TEST_WITH_MONO", false)?,
            randomize_layout: parse_bool(environment, "RANDOMIZE_LAYOUT", false)?,
            dry_run: parse_bool(environment, "DRY_RUN", false)?,
            dump_mir: optional_nonempty(environment, "DUMP_MIR"),
            dump_mir_out: setting_or(environment, "DUMP_MIR_OUT", "/tmp/dump_mir.txt"),
            trace_fn: optional_nonempty(environment, "TRACE_FN"),
            dump_layout: optional_nonempty(environment, "DUMP_LAYOUT"),
            dump_layout_out: setting_or(environment, "DUMP_LAYOUT_OUT", "/tmp/dump_layout.txt"),
            trace_val: optional_nonempty(environment, "TRACE_VAL"),
            rcl_ice_log: environment.contains_key("RCL_ICE_LOG"),
        })
    }

    #[must_use]
    pub const fn artifact_abi(&self) -> &ArtifactAbiConfig {
        &self.abi
    }

    #[must_use]
    pub const fn output_target(&self) -> OutputTarget {
        self.output_target
    }

    #[must_use]
    pub const fn abort_on_error(&self) -> bool {
        self.abort_on_error
    }

    #[must_use]
    pub const fn no_unwind(&self) -> bool {
        self.abi.no_unwind()
    }

    #[must_use]
    pub const fn c_mode(&self) -> bool {
        matches!(self.output_target, OutputTarget::C)
    }

    #[must_use]
    pub const fn insert_mir_debug_comments(&self) -> bool {
        self.insert_mir_debug_comments
    }

    #[must_use]
    pub const fn print_local_types(&self) -> bool {
        self.print_local_types
    }

    #[must_use]
    pub const fn test_with_mono(&self) -> bool {
        self.test_with_mono
    }

    #[must_use]
    pub const fn randomize_layout(&self) -> bool {
        self.randomize_layout
    }

    #[must_use]
    pub const fn dry_run(&self) -> bool {
        self.dry_run
    }

    #[must_use]
    pub fn dump_mir(&self) -> Option<&str> {
        self.dump_mir.as_deref()
    }

    #[must_use]
    pub fn dump_mir_out(&self) -> &str {
        &self.dump_mir_out
    }

    #[must_use]
    pub fn trace_fn(&self) -> Option<&str> {
        self.trace_fn.as_deref()
    }

    #[must_use]
    pub fn dump_layout(&self) -> Option<&str> {
        self.dump_layout.as_deref()
    }

    #[must_use]
    pub fn dump_layout_out(&self) -> &str {
        &self.dump_layout_out
    }

    #[must_use]
    pub fn trace_val(&self) -> Option<&str> {
        self.trace_val.as_deref()
    }

    #[must_use]
    pub const fn rcl_ice_log(&self) -> bool {
        self.rcl_ice_log
    }
}

fn recognized_environment(
    snapshot: &HashMap<OsString, OsString>,
) -> Result<HashMap<String, String>, BackendConfigError> {
    ACTIVE_SETTINGS
        .iter()
        .copied()
        .chain(RETIRED_SETTINGS.iter().map(|setting| setting.name))
        .filter_map(|name| snapshot.get(OsStr::new(name)).map(|value| (name, value)))
        .map(|(name, value)| {
            value
                .to_str()
                .map(|value| (name.to_owned(), value.to_owned()))
                .ok_or(BackendConfigError::InvalidUnicode { variable: name })
        })
        .collect()
}

static ACTIVE_CONFIG: OnceLock<BackendConfig> = OnceLock::new();

/// Installs a validated process-wide configuration.
pub fn install(config: BackendConfig) -> &'static BackendConfig {
    install_into(&ACTIVE_CONFIG, config).unwrap_or_else(|error| panic!("{error}"))
}

fn install_into(
    slot: &OnceLock<BackendConfig>,
    config: BackendConfig,
) -> Result<&BackendConfig, BackendConfigInstallError> {
    match slot.set(config) {
        Ok(()) => Ok(slot.get().expect("configuration was just installed")),
        Err(attempted) => {
            let active = slot
                .get()
                .expect("OnceLock::set returned the rejected configuration");
            if active == &attempted {
                Ok(active)
            } else {
                Err(BackendConfigInstallError {
                    active: active.clone(),
                    attempted,
                })
            }
        }
    }
}

/// Returns the process-wide configuration, capturing lazily in test-harness-only processes.
#[must_use]
pub fn current() -> &'static BackendConfig {
    ACTIVE_CONFIG.get_or_init(|| {
        BackendConfig::capture()
            .unwrap_or_else(|error| panic!("invalid backend configuration: {error}"))
    })
}

/// Finds retired names in stable documentation order.
#[must_use]
pub fn retired_environment_variables(environment: &HashMap<String, String>) -> Vec<RetiredSetting> {
    RETIRED_SETTINGS
        .iter()
        .copied()
        .filter(|setting| environment.contains_key(setting.name))
        .collect()
}

fn parse_bool(
    environment: &HashMap<String, String>,
    variable: &'static str,
    default: bool,
) -> Result<bool, BackendConfigError> {
    match environment.get(variable).map(String::as_str) {
        None => Ok(default),
        Some("0" | "false" | "False" | "FALSE") => Ok(false),
        Some("1" | "true" | "True" | "TRUE") => Ok(true),
        Some(value) => Err(BackendConfigError::InvalidBoolean {
            variable,
            value: value.to_owned(),
        }),
    }
}

fn optional_nonempty(
    environment: &HashMap<String, String>,
    variable: &'static str,
) -> Option<String> {
    environment
        .get(variable)
        .filter(|value| !value.is_empty())
        .cloned()
}

fn setting_or(
    environment: &HashMap<String, String>,
    variable: &'static str,
    default: &'static str,
) -> String {
    environment
        .get(variable)
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

/// Failure to construct a coherent backend configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendConfigError {
    ArtifactAbi(ArtifactAbiConfigCaptureError),
    InvalidBoolean {
        variable: &'static str,
        value: String,
    },
    InvalidUnicode {
        variable: &'static str,
    },
    ConflictingOutputModes {
        enabled: Vec<&'static str>,
    },
    RetiredSettings(Vec<RetiredSetting>),
}

impl std::fmt::Display for BackendConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ArtifactAbi(error) => error.fmt(formatter),
            Self::InvalidBoolean { variable, value } => write!(
                formatter,
                "boolean environment variable {variable} has invalid value `{value}`; expected 0/1 or false/true"
            ),
            Self::InvalidUnicode { variable } => {
                write!(
                    formatter,
                    "environment setting {variable} is not valid Unicode"
                )
            }
            Self::ConflictingOutputModes { enabled } => write!(
                formatter,
                "conflicting output modes: {}; enable at most one",
                enabled.join(", ")
            ),
            Self::RetiredSettings(settings) => {
                formatter.write_str("retired environment setting(s): ")?;
                for (index, setting) in settings.iter().enumerate() {
                    if index != 0 {
                        formatter.write_str(", ")?;
                    }
                    formatter.write_str(setting.name)?;
                    if let Some(replacement) = setting.replacement {
                        write!(formatter, " (use {replacement})")?;
                    }
                }
                formatter.write_str("; remove or replace them")
            }
        }
    }
}

impl std::error::Error for BackendConfigError {}

impl From<ArtifactAbiConfigCaptureError> for BackendConfigError {
    fn from(value: ArtifactAbiConfigCaptureError) -> Self {
        Self::ArtifactAbi(value)
    }
}

/// A second backend instance attempted to replace the process-wide snapshot.
#[derive(Debug, Eq, PartialEq)]
pub struct BackendConfigInstallError {
    active: BackendConfig,
    attempted: BackendConfig,
}

impl std::fmt::Display for BackendConfigInstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "attempted to replace the process-wide backend configuration; active={:?}, attempted={:?}",
            self.active, self.attempted
        )
    }
}

impl std::error::Error for BackendConfigInstallError {}

/// Whether the target runtime is .NET 9+.
#[must_use]
pub fn dotnet9() -> bool {
    current().artifact_abi().dotnet_runtime().major() >= 9
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(environment: &[(&str, &str)]) -> BackendConfig {
        BackendConfig::from_environment(
            &environment
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn reports_retired_settings_with_replacements_in_stable_order() {
        let environment = HashMap::from([
            ("JS_MODE".to_owned(), "1".to_owned()),
            ("ASCI_IDENT".to_owned(), "0".to_owned()),
            ("CHECK_ALLOCATIONS".to_owned(), "0".to_owned()),
        ]);
        let retired = retired_environment_variables(&environment);
        assert_eq!(
            retired
                .iter()
                .map(|setting| setting.name())
                .collect::<Vec<_>>(),
            ["ASCI_IDENT", "CHECK_ALLOCATIONS", "JS_MODE"]
        );
        let error = BackendConfig::from_environment(&environment).unwrap_err();
        assert!(error.to_string().contains("ASCI_IDENT (use ASCII_IDENTS)"));
        assert!(error.to_string().contains("JS_MODE (use JAVA_MODE)"));
    }

    #[test]
    fn parses_backend_and_abi_settings_from_one_snapshot() {
        let config = config_with(&[
            ("C_MODE", "true"),
            ("NO_UNWIND", "1"),
            ("DOTNET_VERSION", "net9.0"),
            ("DRY_RUN", "True"),
            ("DUMP_MIR", "needle"),
        ]);
        assert!(config.c_mode());
        assert!(config.no_unwind());
        assert!(config.dry_run());
        assert_eq!(config.dump_mir(), Some("needle"));
        assert_eq!(config.artifact_abi().dotnet_runtime(), DotnetRuntime::Net9);
        assert!(!config.abort_on_error());
    }

    #[test]
    fn rejects_conflicting_output_targets() {
        let error = BackendConfig::from_environment(&HashMap::from([
            ("C_MODE".to_owned(), "1".to_owned()),
            ("JAVA_MODE".to_owned(), "true".to_owned()),
        ]))
        .unwrap_err();
        assert!(matches!(
            error,
            BackendConfigError::ConflictingOutputModes { .. }
        ));
    }

    #[test]
    fn local_install_accepts_equal_and_rejects_different_snapshots() {
        let slot = OnceLock::new();
        let first = config_with(&[]);
        assert_eq!(install_into(&slot, first.clone()), Ok(&first));
        assert_eq!(install_into(&slot, first.clone()), Ok(&first));

        let different = config_with(&[("ABORT_ON_ERROR", "1")]);
        let error = install_into(&slot, different).unwrap_err();
        assert_eq!(error.active, first);
        assert!(error.attempted.abort_on_error());
    }

    #[test]
    fn max_static_size_is_rejected_as_a_retired_no_op() {
        let error = BackendConfig::from_environment(&HashMap::from([(
            "MAX_STATIC_SIZE".to_owned(),
            "32".to_owned(),
        )]))
        .unwrap_err();
        assert!(error.to_string().contains("MAX_STATIC_SIZE"));
    }
}
