pub mod host {
    use std::env;
    use std::path::PathBuf;

    #[derive(Debug, Clone, Copy)]
    pub struct HostFacts {
        pub os: &'static str,
        pub dylib_ext: &'static str,
        pub exe_ext: &'static str,
        pub host_rid: &'static str,
    }

    impl HostFacts {
        pub fn for_test(os: &'static str) -> Self {
            let (dylib_ext, exe_ext) = match os {
                "macos" => ("dylib", ""),
                "windows" => ("dll", ".exe"),
                _ => ("so", ""),
            };
            Self {
                os,
                dylib_ext,
                exe_ext,
                host_rid: "test-x64",
            }
        }
        pub fn detect() -> Self {
            let (dylib_ext, exe_ext) = match env::consts::OS {
                "macos" => ("dylib", ""),
                "windows" => ("dll", ".exe"),
                _ => ("so", ""),
            };
            let host_rid = match (env::consts::OS, env::consts::ARCH) {
                ("macos", "aarch64") => "osx-arm64",
                ("macos", _) => "osx-x64",
                ("windows", _) => "win-x64",
                (_, "aarch64") => "linux-arm64",
                _ => "linux-x64",
            };
            Self {
                os: env::consts::OS,
                dylib_ext,
                exe_ext,
                host_rid,
            }
        }
        pub fn backend_dylib_name(&self) -> String {
            if self.os == "windows" {
                format!("rustc_codegen_clr.{}", self.dylib_ext)
            } else {
                format!("librustc_codegen_clr.{}", self.dylib_ext)
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
        pub legacy_main_module: bool,
    }
    impl ManagedIdentity {
        pub fn module_full_name(&self) -> Option<String> {
            (!self.legacy_main_module)
                .then(|| format!("{}.{}", self.root_namespace, self.module_type))
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
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
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
        pub fn ilasm_tool_dir(self) -> &'static str {
            match self {
                Self::Net10 => "ilasm10-tool",
                Self::UnityNetStandard21 => "ilasm-unity-tool",
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
                    "--dotnet: unsupported value {other:?}; rust-dotnet 0.0.1 supports .NET 10 or Unity netstandard2.1"
                )),
            }
        }
    }
}
