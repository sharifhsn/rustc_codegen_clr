//! Host facts + tool discovery (dotnet / ilasm). Ports the SHARED HOST HELPERS in
//! `feasibility/cargo-dotnet:103-150` (`detect_host_os`, `ensure_dotnet_on_path`,
//! `resolve_ilasm`).

use std::env;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Host facts derived from the build target of this very binary (`std::env::consts`),
/// which on this tooling crate equals the host. `dylib_ext` is the codegen backend
/// cdylib extension; `exe_ext` the apphost suffix; `host_rid` the .NET runtime id for
/// the CoreCLR ILAsm NuGet package.
pub struct HostFacts {
    pub os: &'static str,
    pub dylib_ext: &'static str,
    pub exe_ext: &'static str,
    /// The .NET runtime id for the CoreCLR ILAsm NuGet package. Used by a native
    /// `setup` port (currently staged to the bash core); kept on HostFacts so the
    /// host-detection logic stays in one place.
    #[allow(dead_code)]
    pub host_rid: &'static str,
}

impl HostFacts {
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
        HostFacts {
            os: env::consts::OS,
            dylib_ext,
            exe_ext,
            host_rid,
        }
    }

    #[cfg(test)]
    pub fn for_test(os: &'static str) -> Self {
        let (dylib_ext, exe_ext) = match os {
            "macos" => ("dylib", ""),
            "windows" => ("dll", ".exe"),
            _ => ("so", ""),
        };
        HostFacts {
            os,
            dylib_ext,
            exe_ext,
            host_rid: "test-x64",
        }
    }

    /// Filename Cargo gives the host codegen-backend dynamic library.
    #[must_use]
    pub fn backend_dylib_name(&self) -> String {
        if self.os == "windows" {
            format!("rustc_codegen_clr.{}", self.dylib_ext)
        } else {
            format!("librustc_codegen_clr.{}", self.dylib_ext)
        }
    }
}

pub const UNSUPPORTED_WINDOWS_HOST: &str = "Windows hosts are not supported by this cargo-dotnet release; use Linux or macOS. Windows support will be enabled after Windows build, test, packaging, and MSBuild acceptance exists.";

pub fn ensure_supported(facts: &HostFacts) -> Result<()> {
    match facts.os {
        "linux" | "macos" => Ok(()),
        "windows" if env::var_os("CARGO_DOTNET_EXPERIMENTAL_WINDOWS").is_some() => Ok(()),
        "windows" => bail!(UNSUPPORTED_WINDOWS_HOST),
        other => bail!(
            "{other} hosts are not supported by this cargo-dotnet release; use Linux or macOS."
        ),
    }
}

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn on_path(cmd: &str) -> bool {
    // Mirror `command -v`: probe each PATH entry for an executable file.
    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            let cand = dir.join(cmd);
            if cand.is_file() {
                return true;
            }
            #[cfg(windows)]
            {
                let cand_exe = dir.join(format!("{cmd}.exe"));
                if cand_exe.is_file() {
                    return true;
                }
            }
        }
    }
    false
}

/// Returns the `(PATH, DOTNET_ROOT)` env additions needed to reach `dotnet` if it is
/// not already on PATH but `$HOME/.dotnet/dotnet` exists (the documented self-heal in
/// `ensure_dotnet_on_path`). The caller applies these to the child env.
pub fn dotnet_env_adds() -> Option<(PathBuf, PathBuf)> {
    if on_path("dotnet") {
        return None;
    }
    let home = home_dir()?;
    let dotnet = home.join(".dotnet/dotnet");
    if dotnet.is_file() {
        Some((home.join(".dotnet"), home.join(".dotnet")))
    } else {
        None
    }
}

/// Require `dotnet` to be reachable (after the self-heal), else a helpful error.
/// `heal` is the result of [`dotnet_env_adds`] — `Some` means a self-heal path
/// was found, so dotnet is reachable.
pub fn ensure_dotnet(heal: &Option<(PathBuf, PathBuf)>) -> Result<()> {
    if on_path("dotnet") || heal.is_some() {
        return Ok(());
    }
    bail!(
        "dotnet not found on PATH or at $HOME/.dotnet/dotnet \
         (run `cargo dotnet setup`, or add $HOME/.dotnet to PATH)"
    )
}

/// Resolve a CoreCLR (NOT Mono) ilasm and return its absolute path to export as
/// `ILASM_PATH`. Order: explicit `$ILASM_PATH`; the selected runtime's versioned tool directory
/// under `$HOME/.dotnet`; a non-Mono `ilasm` on PATH (returns `None`, letting cilly's bare default
/// fire).
/// Ports `resolve_ilasm` (cargo-dotnet:137-150).
pub fn resolve_ilasm(
    facts: &HostFacts,
    dotnet: crate::context::DotnetVersion,
) -> Result<Option<PathBuf>> {
    if let Ok(explicit) = env::var("ILASM_PATH") {
        if !explicit.is_empty() {
            let p = PathBuf::from(&explicit);
            if !p.is_file() {
                bail!("ILASM_PATH='{explicit}' is not an executable");
            }
            return Ok(Some(p));
        }
    }
    if let Some(home) = home_dir() {
        // Each runtime needs its MATCHING CoreCLR ilasm (an older ilasm's PE can be rejected by a
        // newer runtime): ilasm-tool for .NET 8, ilasm9-tool for .NET 9, ilasm10-tool for .NET 10.
        let tool = home.join(format!(
            ".dotnet/{}/ilasm{}",
            dotnet.ilasm_tool_dir(),
            facts.exe_ext
        ));
        if tool.is_file() {
            return Ok(Some(tool));
        }
    }
    if on_path("ilasm") {
        // Reject Mono's ilasm (PE32 output the CoreCLR loader rejects).
        if let Ok(out) = Command::new("ilasm").arg("--version").output() {
            let text = String::from_utf8_lossy(&out.stdout).to_lowercase()
                + &String::from_utf8_lossy(&out.stderr).to_lowercase();
            if text.contains("mono") {
                bail!(
                    "the `ilasm` on PATH is Mono's, which emits PE32 images the native CoreCLR \
                     loader rejects. Run `cargo dotnet setup` (installs the CoreCLR ILAsm to \
                     $HOME/.dotnet/{}/ilasm), or set ILASM_PATH to a CoreCLR ilasm.",
                    dotnet.ilasm_tool_dir()
                );
            }
        }
        // A non-Mono `ilasm` on PATH — cilly's bare `ilasm` default is fine.
        return Ok(None);
    }
    bail!(
        "ilasm not found (no ILASM_PATH, none at $HOME/.dotnet/{}/ilasm{}, none on PATH). \
         Run `cargo dotnet setup` to install the matching CoreCLR ILAsm NuGet tool.",
        dotnet.ilasm_tool_dir(),
        facts.exe_ext
    )
}

/// Require `rustc` and `cargo` reachable (the native-build preflight).
pub fn ensure_rust_toolchain() -> Result<()> {
    if !on_path("rustc") {
        bail!(
            "rustc not found on PATH (native backend needs the project's pinned nightly with \
             rust-src + rustc-dev; run `cargo dotnet setup`)"
        );
    }
    if !on_path("cargo") {
        bail!("cargo not found on PATH");
    }
    Ok(())
}

/// The inner cargo to invoke: `$CARGO` if set, else `cargo` (Book §External Tools).
pub fn inner_cargo() -> String {
    env::var("CARGO")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "cargo".to_string())
}

/// Resolve a path argument to an absolute path, defaulting to `.`. Errors if the dir
/// does not exist or is not a crate (no Cargo.toml).
pub fn resolve_crate_dir(path: &Option<PathBuf>) -> Result<PathBuf> {
    let raw = path.clone().unwrap_or_else(|| PathBuf::from("."));
    let abs = std::fs::canonicalize(&raw)
        .with_context(|| format!("no such directory: {}", raw.display()))?;
    if !abs.join("Cargo.toml").is_file() {
        bail!("not a crate dir (no Cargo.toml): {}", abs.display());
    }
    Ok(abs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_hosts_are_explicit() {
        assert!(ensure_supported(&HostFacts::for_test("linux")).is_ok());
        assert!(ensure_supported(&HostFacts::for_test("macos")).is_ok());
    }

    #[test]
    fn windows_has_stable_actionable_diagnostic() {
        assert_eq!(
            ensure_supported(&HostFacts::for_test("windows"))
                .unwrap_err()
                .to_string(),
            UNSUPPORTED_WINDOWS_HOST
        );
    }

    #[test]
    fn backend_dylib_filename_is_platform_native() {
        assert_eq!(
            HostFacts::for_test("linux").backend_dylib_name(),
            "librustc_codegen_clr.so"
        );
        assert_eq!(
            HostFacts::for_test("macos").backend_dylib_name(),
            "librustc_codegen_clr.dylib"
        );
        assert_eq!(
            HostFacts::for_test("windows").backend_dylib_name(),
            "rustc_codegen_clr.dll"
        );
    }
}
