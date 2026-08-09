pub mod host {
    use std::env;
    use std::path::PathBuf;

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct HostFacts {
        pub os: &'static str,
        pub arch: &'static str,
        pub dylib_ext: &'static str,
        pub exe_ext: &'static str,
        pub host_rid: &'static str,
    }

    impl HostFacts {
        pub fn for_test(os: &'static str) -> Self {
            let arch = if os == "macos" { "aarch64" } else { "x86_64" };
            Self::for_target(os, arch).unwrap_or_else(|| Self::unsupported(os, arch))
        }

        /// Return facts only for the exact host triples carried by the public SDK.
        pub fn for_target(os: &'static str, arch: &'static str) -> Option<Self> {
            let (dylib_ext, exe_ext, host_rid) = match (os, arch) {
                ("linux", "x86_64") => ("so", "", "linux-x64"),
                ("macos", "aarch64") => ("dylib", "", "osx-arm64"),
                ("windows", "x86_64") => ("dll", ".exe", "win-x64"),
                _ => return None,
            };
            Some(Self {
                os,
                arch,
                dylib_ext,
                exe_ext,
                host_rid,
            })
        }

        fn unsupported(os: &'static str, arch: &'static str) -> Self {
            let (dylib_ext, exe_ext) = match os {
                "macos" => ("dylib", ""),
                "windows" => ("dll", ".exe"),
                _ => ("so", ""),
            };
            Self {
                os,
                arch,
                dylib_ext,
                exe_ext,
                host_rid: "unsupported",
            }
        }

        pub fn is_supported(self) -> bool {
            Self::for_target(self.os, self.arch).is_some()
        }

        pub fn detect() -> Self {
            Self::for_target(env::consts::OS, env::consts::ARCH)
                .unwrap_or_else(|| Self::unsupported(env::consts::OS, env::consts::ARCH))
        }
        pub fn backend_dylib_name(&self) -> String {
            if self.os == "windows" {
                format!("rustc_codegen_clr.{}", self.dylib_ext)
            } else {
                format!("librustc_codegen_clr.{}", self.dylib_ext)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::HostFacts;

        #[test]
        fn accepts_only_public_release_hosts() {
            assert_eq!(
                HostFacts::for_target("linux", "x86_64").unwrap().host_rid,
                "linux-x64"
            );
            assert_eq!(
                HostFacts::for_target("macos", "aarch64").unwrap().host_rid,
                "osx-arm64"
            );
            assert_eq!(
                HostFacts::for_target("windows", "x86_64").unwrap().host_rid,
                "win-x64"
            );
            for (os, arch) in [
                ("linux", "aarch64"),
                ("macos", "x86_64"),
                ("windows", "aarch64"),
                ("freebsd", "x86_64"),
            ] {
                assert!(HostFacts::for_target(os, arch).is_none(), "{os}-{arch}");
            }
        }
    }
    pub fn home_dir() -> Option<PathBuf> {
        env::var_os("HOME")
            .or_else(|| env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }
}

pub mod identity {
    #[derive(Debug, Clone)]
    pub struct ManagedIdentity {
        pub schema: u16,
        pub package_id: String,
        pub assembly_name: String,
        pub root_namespace: String,
        pub module_type: String,
    }
    impl ManagedIdentity {
        pub fn module_full_name(&self) -> String {
            format!("{}.{}", self.root_namespace, self.module_type)
        }
    }

    /// The complete user-authored managed project contract. Identity is compiler/linker-facing;
    /// namespaces and compatibility profile are host/tooling-facing, but they are validated and
    /// versioned together from one Cargo metadata table.
    #[derive(Debug, Clone)]
    pub struct ManagedProjectConfig {
        pub identity: ManagedIdentity,
        pub public_namespaces: Vec<String>,
        pub compatibility_profile: String,
    }
}

pub mod runtime {
    use serde::{Deserialize, Serialize};

    /// Managed runtime/API profile emitted by rustc_codegen_clr.
    ///
    /// Keep the variant order stable: this enum is serialized inside the cilly artifact ABI
    /// envelope. Adding or reordering variants requires an artifact schema bump.
    #[derive(
        Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
    )]
    pub enum DotnetVersion {
        #[default]
        Net10,
        /// Unity's managed `netstandard2.1` profile (not a CoreCLR runtime).
        UnityNetStandard21,
    }
    impl DotnetVersion {
        pub fn tfm(self) -> &'static str {
            match self {
                Self::Net10 => "net10.0",
                Self::UnityNetStandard21 => "netstandard2.1",
            }
        }
        pub fn as_env(self) -> &'static str {
            match self {
                Self::Net10 => "10",
                Self::UnityNetStandard21 => "unity-netstandard2.1",
            }
        }
        /// The `.ver` triplet for a BCL assembly reference.
        pub const fn assembly_ver(self) -> &'static str {
            match self {
                Self::Net10 => "10:0:0:0",
                Self::UnityNetStandard21 => "4:0:0:0",
            }
        }

        /// The parsed `.ver` tuple used by direct PE assembly-reference rows.
        pub const fn assembly_ver_tuple(self) -> (u16, u16, u16, u16) {
            match self {
                Self::Net10 => (10, 0, 0, 0),
                Self::UnityNetStandard21 => (4, 0, 0, 0),
            }
        }

        /// `Microsoft.NETCore.App` framework-version floor for runtime configuration.
        pub const fn framework_version(self) -> &'static str {
            match self {
                Self::Net10 => "10.0.0",
                Self::UnityNetStandard21 => "2.1.0",
            }
        }

        /// Runtime major version, or zero for Unity's netstandard profile.
        pub const fn major(self) -> u32 {
            match self {
                Self::Net10 => 10,
                Self::UnityNetStandard21 => 0,
            }
        }

        /// Whether this runtime exposes native subword `Interlocked` overloads.
        pub const fn supports_subword_interlocked(self) -> bool {
            self.major() >= 9
        }
    }

    impl std::fmt::Display for DotnetVersion {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Net10 => f.write_str(".NET 10"),
                Self::UnityNetStandard21 => f.write_str("Unity netstandard2.1"),
            }
        }
    }
    impl std::str::FromStr for DotnetVersion {
        type Err = String;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s.trim() {
                "10" | "net10" | "net10.0" => Ok(Self::Net10),
                "unity" | "unity-netstandard2.1" | "netstandard2.1" => Ok(Self::UnityNetStandard21),
                other => Err(format!(
                    "--dotnet: unsupported value {other:?}; rust-dotnet 0.0.2 supports .NET 10 or Unity netstandard2.1"
                )),
            }
        }
    }
}
