//! Host facts + tool discovery (dotnet / ilasm). Ports the SHARED HOST HELPERS in
//! `feasibility/cargo-dotnet:103-150` (`detect_host_os`, `ensure_dotnet_on_path`,
//! `resolve_ilasm`).

use std::env;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
pub use rust_dotnet_sdk_core::host::HostFacts;

pub fn ensure_supported(facts: &HostFacts) -> Result<()> {
    match facts.os {
        "linux" | "macos" | "windows" => Ok(()),
        other => bail!(
            "{other} hosts are not supported by this cargo-dotnet release; use Linux, macOS, or Windows."
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

/// Prefer the user-local .NET installation when the `dotnet` found on PATH cannot run the
/// requested major runtime but `$HOME/.dotnet` can. This handles common side-by-side setups such
/// as Homebrew .NET 8 on PATH plus the project-required .NET 10 installed by `dotnet-install`.
pub fn dotnet_env_adds_for(runtime_major: &str) -> Option<(PathBuf, PathBuf)> {
    let runtime_marker = format!("Microsoft.NETCore.App {runtime_major}.");
    if on_path("dotnet")
        && Command::new("dotnet")
            .arg("--list-runtimes")
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains(&runtime_marker)
            })
    {
        return None;
    }
    let home = home_dir()?;
    let root = home.join(".dotnet");
    let shared = root.join("shared/Microsoft.NETCore.App");
    let has_runtime = std::fs::read_dir(shared).ok()?.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|version| version.starts_with(&format!("{runtime_major}.")))
    });
    (root
        .join(if cfg!(windows) {
            "dotnet.exe"
        } else {
            "dotnet"
        })
        .is_file()
        && has_runtime)
        .then(|| (root.clone(), root))
}

/// Require `dotnet` to be reachable (after the self-heal), else a helpful error.
/// `heal` is the result of [`dotnet_env_adds_for`] — `Some` means a self-heal path
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
/// `ILASM_PATH`. Order: explicit `$ILASM_PATH`; the installed cargo-dotnet SDK; the selected
/// runtime's versioned tool directory under `$HOME/.dotnet`; a non-Mono `ilasm` on PATH (returns
/// `None`, letting cilly's bare default fire).
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
    let bundled_name = format!("ilasm{}", facts.exe_ext);
    if let Some(sdk_home) = env::var_os("CARGO_DOTNET_HOME").map(PathBuf::from) {
        let tool = sdk_home.join("bin").join(&bundled_name);
        if tool.is_file() {
            return Ok(Some(tool));
        }
    }
    if let Some(home) = home_dir() {
        let bundled = home.join(".cargo-dotnet/bin").join(&bundled_name);
        if bundled.is_file() {
            return Ok(Some(bundled));
        }
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
        "ilasm not found (no ILASM_PATH, no bundled SDK tool, none at \
         $HOME/.dotnet/{}/ilasm{}, none on PATH). \
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
    fn windows_is_a_supported_release_host() {
        assert!(ensure_supported(&HostFacts::for_test("windows")).is_ok());
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
