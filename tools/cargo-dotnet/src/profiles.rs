//! Product host compatibility is a named, inspectable contract rather than an inferred TFM.

use anyhow::Result;
use serde::Serialize;

use crate::cli::ProfilesArgs;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Support {
    Supported,
    Preview,
    Planned,
    Unsupported,
}

impl Support {
    const fn label(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Preview => "preview",
            Self::Planned => "planned",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Serialize)]
struct CompatibilityProfile {
    name: &'static str,
    support: Support,
    host: &'static str,
    managed_contract: &'static str,
    native_assets: &'static str,
    host_rids: &'static [&'static str],
    evidence: &'static str,
}

const PROFILES: &[CompatibilityProfile] = &[
    CompatibilityProfile {
        name: "net10-coreclr",
        support: Support::Supported,
        host: ".NET 10 CoreCLR on Linux x64, macOS arm64, or Windows x64",
        managed_contract: "net10.0 / Microsoft.NETCore.App 10",
        native_assets: "host RID when P/Invoke is used",
        host_rids: &["linux-x64", "osx-arm64", "win-x64"],
        evidence: "release SDK onboarding and managed consumer acceptance",
    },
    CompatibilityProfile {
        name: "excel-dna-net10-windows",
        support: Support::Preview,
        host: "64-bit desktop Excel on Windows x64 through Excel-DNA 1.9",
        managed_contract: "net10.0-windows / CoreCLR 10",
        native_assets: "win-x64 when P/Invoke is used",
        host_rids: &["win-x64"],
        evidence: "packed XLL build passes; real Excel launch proof remains",
    },
    CompatibilityProfile {
        name: "maui-windows-net10",
        support: Support::Planned,
        host: ".NET MAUI Windows on CoreCLR 10",
        managed_contract: "net10.0-windows10.0.19041.0",
        native_assets: "win-x64 or win-arm64 per package",
        host_rids: &["win-x64", "win-arm64"],
        evidence: "scaffold contract passes; Windows build and runtime fixture remain",
    },
    CompatibilityProfile {
        name: "winui3-net10-windows",
        support: Support::Planned,
        host: "unpackaged WinUI 3 desktop app on Windows 10 1809 or newer",
        managed_contract: "net10.0-windows10.0.19041.0 / CoreCLR 10",
        native_assets: "win-x64 or win-arm64 per package",
        host_rids: &["win-x64", "win-arm64"],
        evidence: "scaffold contract exists; Windows build and runtime fixture remain",
    },
    CompatibilityProfile {
        name: "unity-netstandard2.1",
        support: Support::Planned,
        host: "Unity 6 Editor plus Mono and IL2CPP players",
        managed_contract: "netstandard2.1-compatible API surface; not net10.0",
        native_assets: "Unity plugin layout per player platform",
        host_rids: &[],
        evidence: "requires a restricted BCL contract and Editor plus player execution",
    },
    CompatibilityProfile {
        name: "maui-android-net10",
        support: Support::Planned,
        host: ".NET MAUI Android (Mono first; CoreCLR separately experimental)",
        managed_contract: "Android-compatible managed IL with trimming constraints",
        native_assets: "android-arm64/x64 ABI directories",
        host_rids: &["android-arm64", "android-x64"],
        evidence: "APK/emulator runtime and packaging proof required",
    },
    CompatibilityProfile {
        name: "maui-apple-net10",
        support: Support::Planned,
        host: ".NET MAUI iOS and Mac Catalyst",
        managed_contract: "fully AOT- and trimming-compatible managed IL",
        native_assets: "signed static frameworks per Apple target",
        host_rids: &[
            "ios-arm64",
            "iossimulator-arm64",
            "maccatalyst-arm64",
            "maccatalyst-x64",
        ],
        evidence: "simulator/device NativeAOT and packaging proof required",
    },
    CompatibilityProfile {
        name: "vsto-net10-in-process",
        support: Support::Unsupported,
        host: "VSTO add-in process",
        managed_contract: "VSTO remains .NET Framework 4.8; modern .NET coexistence is unsupported",
        native_assets: "not applicable",
        host_rids: &[],
        evidence: "use Excel-DNA or a thin VSTO shim with an out-of-process .NET 10 service",
    },
];

pub(crate) fn is_known(name: &str) -> bool {
    PROFILES.iter().any(|profile| profile.name == name)
}

pub(crate) fn package_contract(name: &str) -> Option<(&'static str, &'static [&'static str])> {
    PROFILES
        .iter()
        .find(|profile| profile.name == name)
        .map(|profile| (profile.support.label(), profile.host_rids))
}

pub fn run(args: &ProfilesArgs) -> Result<i32> {
    if args.json {
        println!("{}", serde_json::to_string_pretty(PROFILES)?);
        return Ok(0);
    }

    println!("Compatibility profiles (support is evidence-gated):");
    for profile in PROFILES {
        println!("\n{} [{}]", profile.name, profile.support.label());
        println!("  host: {}", profile.host);
        println!("  managed: {}", profile.managed_contract);
        println!("  native: {}", profile.native_assets);
        println!(
            "  RIDs: {}",
            if profile.host_rids.is_empty() {
                "none declared".to_owned()
            } else {
                profile.host_rids.join(", ")
            }
        );
        println!("  evidence: {}", profile.evidence);
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_a_unique_stable_name_and_evidence() {
        let mut names = std::collections::BTreeSet::new();
        for profile in PROFILES {
            assert!(names.insert(profile.name));
            assert!(!profile.evidence.is_empty());
            assert!(profile.name.bytes().all(|byte| byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'-'
                || byte == b'.'));
        }
    }

    #[test]
    fn only_coreclr_is_fully_supported_until_host_execution_lands() {
        let supported: Vec<_> = PROFILES
            .iter()
            .filter(|profile| matches!(profile.support, Support::Supported))
            .map(|profile| profile.name)
            .collect();
        assert_eq!(supported, ["net10-coreclr"]);
    }
}
