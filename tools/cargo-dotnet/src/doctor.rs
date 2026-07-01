//! `cargo dotnet doctor` — diagnose the interop toolchain + translate runtime failures.
//!
//! Two jobs (ERGONOMICS_ROADMAP Theme-5 "Better interop diagnostics"):
//!
//!  1. **Environment check** — verify the pieces a native `cargo dotnet` build/run needs
//!     are actually present (the pinned nightly + rust-src + rustc-dev, a CoreCLR — not
//!     Mono — ilasm, `dotnet`, and the installed/dev backend dylib + linker + target
//!     spec + the shipped `msbuild/RustDotnet.targets`). Each missing piece prints the
//!     one actionable fix (almost always `cargo dotnet setup`).
//!
//!  2. **Runtime-failure translation** — the .NET runtime's exceptions are cryptic for a
//!     Rust dev. `cargo dotnet doctor <log-or-message>` (or piped stdin) scans the text
//!     for the known interop failure signatures and prints the *cause + fix*, e.g.:
//!       * `TypeLoadException: … Stack` / `Queue` → the impl-assembly gotcha (those live
//!         in `System.Collections`, not `System.Private.CoreLib`).
//!       * `MissingMethodException` / `EntryPointNotFoundException: rcl_vec_*` →
//!         "did you `export_rust_containers!()` / mark the fn `#[no_mangle]`?".
//!     The same matcher powers both the file/arg mode and (future) an auto-hint on a
//!     failed run.

use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

use crate::cli::DoctorArgs;
use crate::host::{self, HostFacts};
use crate::mode::{self, Mode};

/// One diagnostic line: a status + a human message (+ an optional fix hint).
struct Check {
    ok: bool,
    /// `false` for a soft warning that does not fail the overall exit code.
    hard: bool,
    label: String,
    detail: String,
}

impl Check {
    fn pass(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Check { ok: true, hard: true, label: label.into(), detail: detail.into() }
    }
    fn fail(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Check { ok: false, hard: true, label: label.into(), detail: detail.into() }
    }
    fn warn(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Check { ok: false, hard: false, label: label.into(), detail: detail.into() }
    }
    fn render(&self, out: &mut String) {
        let mark = if self.ok {
            "[ OK ]"
        } else if self.hard {
            "[FAIL]"
        } else {
            "[WARN]"
        };
        let _ = writeln!(out, "  {mark} {}", self.label);
        if !self.detail.is_empty() {
            let _ = writeln!(out, "         {}", self.detail);
        }
    }
}

pub fn run(args: &DoctorArgs) -> Result<i32> {
    // Runtime-failure translation mode: an explicit log/message arg, or piped stdin.
    if let Some(input) = read_failure_input(args)? {
        let hints = diagnose_failure(&input);
        if hints.is_empty() {
            println!("cargo dotnet doctor: no known interop failure signature matched.");
            println!("  (recognised: TypeLoadException, MissingMethod/EntryPointNotFound, \
                      DllNotFound, BadImageFormat, and the Mono-ilasm PE mismatch)");
            return Ok(0);
        }
        println!("cargo dotnet doctor — interop failure analysis:\n");
        for h in &hints {
            println!("  ● {}", h.title);
            for line in &h.body {
                println!("    {line}");
            }
            println!();
        }
        return Ok(0);
    }

    // Environment check mode.
    let checks = environment_checks();
    let mut out = String::new();
    out.push_str("cargo dotnet doctor — environment:\n\n");
    let mut hard_failures = 0u32;
    for c in &checks {
        c.render(&mut out);
        if !c.ok && c.hard {
            hard_failures += 1;
        }
    }
    print!("{out}");
    if hard_failures == 0 {
        println!("\nAll required checks passed. You should be able to `cargo dotnet run`.");
        Ok(0)
    } else {
        println!(
            "\n{hard_failures} required check(s) failed — most are fixed by `cargo dotnet setup` \
             (from a repo checkout, or with --from-repo <path>)."
        );
        Ok(1)
    }
}

// ---------------------------------------------------------------------------------
// Environment checks.
// ---------------------------------------------------------------------------------

fn environment_checks() -> Vec<Check> {
    let mut checks = Vec::new();
    let facts = HostFacts::detect();

    // rustc / cargo on PATH.
    checks.push(match host::ensure_rust_toolchain() {
        Ok(()) => Check::pass("rustc + cargo on PATH", ""),
        Err(e) => Check::fail("rustc + cargo on PATH", e.to_string()),
    });

    // The pinned nightly toolchain (rust-src + rustc-dev are what the backend needs).
    checks.push(check_pinned_toolchain());

    // dotnet.
    let heal = host::dotnet_env_additions();
    checks.push(match host::ensure_dotnet(&heal) {
        Ok(()) => Check::pass("dotnet runtime reachable", ""),
        Err(e) => Check::fail("dotnet runtime reachable", e.to_string()),
    });

    // A CoreCLR (not Mono) ilasm.
    checks.push(match host::resolve_ilasm(&facts, crate::context::DotnetVersion::Net8) {
        Ok(Some(p)) => Check::pass("CoreCLR ilasm", format!("using {}", p.display())),
        Ok(None) => Check::pass("CoreCLR ilasm", "a non-Mono ilasm on PATH".to_string()),
        Err(e) => Check::fail("CoreCLR ilasm", e.to_string()),
    });

    // The backend install (dylib + linker + target spec + msbuild targets).
    checks.extend(check_backend_install(&facts));

    checks
}

/// Verify the pinned nightly is installed and carries rust-src (needed for build-std).
fn check_pinned_toolchain() -> Check {
    let toolchain = resolve_expected_toolchain();
    // `rustc +<toolchain> --version` succeeds only if the toolchain is installed.
    match Command::new("rustc").arg(format!("+{toolchain}")).arg("--version").output() {
        Ok(out) if out.status.success() => {
            // Probe rust-src via the sysroot.
            match Command::new("rustc")
                .arg(format!("+{toolchain}"))
                .arg("--print")
                .arg("sysroot")
                .output()
            {
                Ok(so) if so.status.success() => {
                    let sysroot = String::from_utf8_lossy(&so.stdout).trim().to_string();
                    let src = Path::new(&sysroot)
                        .join("lib/rustlib/src/rust/library/std/src/lib.rs");
                    if src.is_file() {
                        Check::pass(
                            format!("pinned toolchain {toolchain}"),
                            "installed with rust-src".to_string(),
                        )
                    } else {
                        Check::fail(
                            format!("pinned toolchain {toolchain}"),
                            format!(
                                "installed but rust-src is MISSING ({}) — \
                                 `rustup component add rust-src --toolchain {toolchain}`",
                                src.display()
                            ),
                        )
                    }
                }
                _ => Check::warn(
                    format!("pinned toolchain {toolchain}"),
                    "installed; could not probe rust-src".to_string(),
                ),
            }
        }
        _ => Check::fail(
            format!("pinned toolchain {toolchain}"),
            "not installed — run `cargo dotnet setup` (installs the nightly + rust-src + rustc-dev)"
                .to_string(),
        ),
    }
}

/// The toolchain we expect: the install-home VERSION file's, or the compiled default.
fn resolve_expected_toolchain() -> String {
    match mode::detect() {
        Ok(Mode::Installed { home }) => mode::read_home_toolchain(&home),
        _ => mode::DEFAULT_TOOLCHAIN.to_string(),
    }
}

/// Check the backend artifacts + shipped msbuild targets in whichever layout applies.
fn check_backend_install(facts: &HostFacts) -> Vec<Check> {
    let mut checks = Vec::new();
    let (root_label, dylib, linker, target_spec, targets) = match mode::detect() {
        Ok(Mode::Installed { home }) => (
            format!("installed home {}", home.display()),
            home.join(format!("bin/librustc_codegen_clr.{}", facts.dylib_ext)),
            home.join(format!("bin/linker{}", facts.exe_ext)),
            home.join("target/x86_64-unknown-dotnet.json"),
            home.join("msbuild/RustDotnet.targets"),
        ),
        Ok(Mode::Dev { repo_root }) => (
            format!("dev checkout {}", repo_root.display()),
            repo_root.join(format!("target/release/librustc_codegen_clr.{}", facts.dylib_ext)),
            repo_root.join(format!("target/release/linker{}", facts.exe_ext)),
            repo_root.join("x86_64-unknown-dotnet.json"),
            repo_root.join("msbuild/RustDotnet.targets"),
        ),
        Err(e) => {
            checks.push(Check::fail("backend install", e.to_string()));
            return checks;
        }
    };

    checks.push(Check::pass("backend layout", root_label));
    checks.push(file_check("backend dylib", &dylib, true));
    checks.push(file_check("linker binary", &linker, true));
    checks.push(file_check("target spec (x86_64-unknown-dotnet.json)", &target_spec, true));
    // RustDotnet.targets is only needed for the C#-consumes-Rust (--lib/--plugin) flow, so a
    // soft warning rather than a hard failure.
    checks.push(file_check("msbuild/RustDotnet.targets (C# consumer)", &targets, false));
    checks
}

fn file_check(label: &str, path: &Path, hard: bool) -> Check {
    if path.is_file() {
        Check::pass(label.to_string(), format!("{}", path.display()))
    } else if hard {
        Check::fail(
            label.to_string(),
            format!("missing: {} — run `cargo dotnet setup`", path.display()),
        )
    } else {
        Check::warn(
            label.to_string(),
            format!("missing: {} (only needed for the C# consumer flow)", path.display()),
        )
    }
}

// ---------------------------------------------------------------------------------
// Runtime-failure translation.
// ---------------------------------------------------------------------------------

/// A translated diagnosis: a one-line title + a few actionable body lines.
pub struct Hint {
    pub title: String,
    pub body: Vec<String>,
}

/// Read the failure text to diagnose: the `input` arg is a path (read the file) or a
/// literal message; if neither is given and stdin is not a TTY, read piped stdin.
fn read_failure_input(args: &DoctorArgs) -> Result<Option<String>> {
    if let Some(s) = &args.input {
        // If it names an existing file, read it; otherwise treat it as a literal message.
        let p = PathBuf::from(s);
        if p.is_file() {
            return Ok(Some(std::fs::read_to_string(&p)?));
        }
        return Ok(Some(s.clone()));
    }
    // Piped stdin (a `... 2>&1 | cargo dotnet doctor` flow).
    if !is_stdin_tty() {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        let trimmed = buf.trim();
        if !trimmed.is_empty() {
            return Ok(Some(buf));
        }
    }
    Ok(None)
}

#[cfg(unix)]
fn is_stdin_tty() -> bool {
    // Avoid an extra dependency: check via isatty on fd 0.
    extern "C" {
        fn isatty(fd: i32) -> i32;
    }
    unsafe { isatty(0) == 1 }
}

#[cfg(not(unix))]
fn is_stdin_tty() -> bool {
    // Conservative: on non-unix assume a TTY so we don't block reading stdin.
    true
}

/// Scan `text` for known interop failure signatures and return the matching hints.
/// Pure + string-only so it is fully unit-testable.
pub fn diagnose_failure(text: &str) -> Vec<Hint> {
    let mut hints = Vec::new();
    let lower = text.to_lowercase();

    // 1. TypeLoadException — usually the impl-assembly gotcha.
    if lower.contains("typeloadexception") || lower.contains("could not load type") {
        let mut body = vec![
            "A managed type failed to load. The #1 cause in interop is the IMPL-ASSEMBLY gotcha:"
                .to_string(),
            "a collection wrapper names the wrong assembly for its type.".to_string(),
            "  • List / Dictionary / HashSet live in System.Private.CoreLib".to_string(),
            "  • Stack / Queue (and most other collections) live in System.Collections".to_string(),
            "  • concurrent collections live in System.Collections.Concurrent".to_string(),
        ];
        // If the message names Stack/Queue specifically, sharpen it.
        if mentions_word(&lower, "stack") || mentions_word(&lower, "queue") {
            body.push(
                "This message names Stack/Queue → set the impl assembly to \"System.Collections\"."
                    .to_string(),
            );
        }
        body.push(
            "Fix: correct the impl-assembly string on the wrapper (mycorrhiza/src/collections.rs)."
                .to_string(),
        );
        hints.push(Hint { title: "TypeLoadException → impl-assembly mismatch".to_string(), body });
    }

    // 2. Missing method / entry point — usually a not-exported Rust fn.
    if lower.contains("missingmethodexception")
        || lower.contains("missingmethod")
        || lower.contains("entrypointnotfoundexception")
        || lower.contains("entry point")
    {
        let mut body = vec![
            "A method/entry point the C# side called was not found in the Rust assembly.".to_string(),
            "Common causes:".to_string(),
            "  • the Rust side did not `mycorrhiza::export_rust_containers!()` (no rcl_vec_* exported)"
                .to_string(),
            "  • the exported fn is missing `#[no_mangle]` (so its symbol was mangled away)".to_string(),
            "  • a signature mismatch between the C# P/Invoke and the Rust `extern` fn".to_string(),
        ];
        if lower.contains("rcl_vec") {
            body.push(
                "The name mentions rcl_vec_* → add `mycorrhiza::export_rust_containers!()` to the \
                 cdylib's lib.rs and rebuild."
                    .to_string(),
            );
        }
        hints.push(Hint {
            title: "MissingMethod / EntryPointNotFound → un-exported Rust symbol".to_string(),
            body,
        });
    }

    // 3. DllNotFound — the Rust assembly/native lib was not found/referenced.
    if lower.contains("dllnotfoundexception")
        || (lower.contains("unable to load") && lower.contains("dll"))
    {
        hints.push(Hint {
            title: "DllNotFound → the Rust assembly was not built/referenced".to_string(),
            body: vec![
                "The C# project could not locate the Rust-produced assembly.".to_string(),
                "  • ensure the csproj imports RustDotnet.targets and has a <RustCrate Include=... />"
                    .to_string(),
                "  • ensure CARGO_DOTNET_HOME (or ~/.cargo-dotnet) points at a valid install"
                    .to_string(),
                "  • run `cargo dotnet doctor` (no args) to verify the backend install".to_string(),
            ],
        });
    }

    // 4. BadImageFormat — almost always the Mono-ilasm PE mismatch.
    if lower.contains("badimageformatexception") || lower.contains("bad il format") {
        hints.push(Hint {
            title: "BadImageFormat → Mono ilasm PE (CoreCLR rejects it)".to_string(),
            body: vec![
                "The assembly's PE image was rejected by the CoreCLR loader.".to_string(),
                "The classic cause is Mono's ilasm (emits PE32 CoreCLR won't load), or a .NET \
                 version/ilasm mismatch (a net8 ilasm's output is rejected by net9 and vice-versa)."
                    .to_string(),
                "Fix: `cargo dotnet setup` installs the matching CoreCLR ilasm, or set ILASM_PATH \
                 to a CoreCLR ilasm."
                    .to_string(),
            ],
        });
    }

    hints
}

/// Whole-word-ish containment: `needle` bounded by non-alphanumerics (so "queue"
/// matches "'Queue'" but not "queuey"). `hay` is assumed already lowercased.
fn mentions_word(hay: &str, needle: &str) -> bool {
    let bytes = hay.as_bytes();
    let n = needle.len();
    let mut start = 0;
    while let Some(pos) = hay[start..].find(needle) {
        let i = start + pos;
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let after = i + n;
        let after_ok = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = i + 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typeload_maps_to_impl_assembly() {
        let hints = diagnose_failure(
            "Unhandled exception. System.TypeLoadException: Could not load type \
             'System.Collections.Generic.Stack`1' from assembly 'System.Private.CoreLib'.",
        );
        assert_eq!(hints.len(), 1);
        assert!(hints[0].title.contains("impl-assembly"));
        // The Stack-specific sharpening fired.
        assert!(hints[0].body.iter().any(|l| l.contains("System.Collections")));
        assert!(hints[0].body.iter().any(|l| l.contains("names Stack/Queue")));
    }

    #[test]
    fn missing_method_maps_to_export() {
        let hints = diagnose_failure(
            "System.MissingMethodException: Method not found: rcl_vec_new",
        );
        assert_eq!(hints.len(), 1);
        assert!(hints[0].title.contains("un-exported"));
        assert!(hints[0].body.iter().any(|l| l.contains("export_rust_containers")));
    }

    #[test]
    fn entrypoint_not_found_maps_to_export() {
        let hints = diagnose_failure("EntryPointNotFoundException: greet");
        assert_eq!(hints.len(), 1);
        assert!(hints[0].title.contains("un-exported"));
    }

    #[test]
    fn badimage_maps_to_mono_ilasm() {
        let hints = diagnose_failure("System.BadImageFormatException: Bad IL format.");
        assert_eq!(hints.len(), 1);
        assert!(hints[0].title.contains("Mono ilasm"));
    }

    #[test]
    fn unknown_text_yields_no_hint() {
        assert!(diagnose_failure("thread 'main' panicked at 'index out of bounds'").is_empty());
    }

    #[test]
    fn word_boundary_matcher() {
        assert!(mentions_word("'queue' failed", "queue"));
        assert!(mentions_word("a stack.push", "stack"));
        assert!(!mentions_word("queuey mcqueueface", "queue"));
        assert!(mentions_word("system.collections.generic.stack`1", "stack"));
    }

    #[test]
    fn two_signatures_yield_two_hints() {
        let hints = diagnose_failure(
            "TypeLoadException: ... Queue ... later ... MissingMethodException: ...",
        );
        assert_eq!(hints.len(), 2);
    }
}
