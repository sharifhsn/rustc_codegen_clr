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
    let mut fails = 0u32;
    for c in &checks {
        c.render(&mut out);
        if !c.ok && c.hard {
            fails += 1;
        }
    }
    print!("{out}");

    // Workspace-wiring lints: sibling Rust crates missing a <RustCrate> reference, and
    // TFM/RustDotnetVersion mismatches. Best-effort — scan errors are reported as a single
    // soft warning rather than aborting the whole `doctor` run.
    let wiring_checks = workspace_wiring_checks(&args.workspace);
    if !wiring_checks.is_empty() {
        let mut wout = String::new();
        wout.push_str(&format!(
            "\ncargo dotnet doctor — workspace wiring ({}):\n\n",
            args.workspace.display()
        ));
        for c in &wiring_checks {
            c.render(&mut wout);
            if !c.ok && c.hard {
                fails += 1;
            }
        }
        print!("{wout}");
    }

    if fails == 0 {
        println!("\nAll required checks passed. You should be able to `cargo dotnet run`.");
        Ok(0)
    } else {
        println!(
            "\n{fails} required check(s) failed — most are fixed by `cargo dotnet setup` \
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
    let heal = host::dotnet_env_adds();
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
// Workspace wiring lints (RustCrate csproj wiring + TFM/RustDotnetVersion mismatches).
//
// These are static-text scans, not MSBuild evaluation: they look for the `<RustCrate
// Include="...">` item and `<RustDotnetVersion>`/`<TargetFramework>` properties by simple
// XML string matching, which is good enough to catch the common mistakes (a sibling Rust
// crate nobody references; a hardcoded TargetFramework that has drifted from
// RustDotnetVersion) without pulling in a full MSBuild evaluator.
//
// NOTE on the third lint the backlog item asked about — "stale generated bindings": there
// is no existing per-project generated-bindings artifact with a staleness signal to check.
// `mycorrhiza/src/bindings.rs` is a hand-committed, one-time `spinacz`-generated file (see
// its module doc), not something rebuilt per project with a hash/timestamp marker; the only
// other "generated bindings" in the tree is `cargo_tests/spinacz/out.rs`, a one-off demo
// output with the same lack of a freshness marker. Inventing a new hashing/timestamp scheme
// here would be new infrastructure, which the task instructions say to skip rather than add.
// So only checks (1) and (2) are implemented.
// ---------------------------------------------------------------------------------

/// Directory names never worth descending into while scanning a workspace.
const SKIP_DIRS: &[&str] = &[
    "target", ".git", "node_modules", "bin", "obj", ".vs", ".idea", "graphify-out",
];

fn workspace_wiring_checks(root: &Path) -> Vec<Check> {
    if !root.is_dir() {
        return vec![Check::warn(
            "workspace wiring scan",
            format!("--workspace {} is not a directory; skipping", root.display()),
        )];
    }

    let mut cargo_tomls = Vec::new();
    let mut csprojs = Vec::new();
    walk(root, 0, &mut cargo_tomls, &mut csprojs);

    let mut checks = Vec::new();

    // Read each csproj once: (path, contents, RustCrate Include paths (resolved absolute),
    // RustDotnetVersion property value if present, TargetFramework property value if present).
    struct CsProjFacts {
        path: PathBuf,
        rust_crate_dirs: Vec<PathBuf>,
        rust_dotnet_version: Option<String>,
        target_framework: Option<String>,
    }
    let mut csproj_facts = Vec::new();
    for csproj in &csprojs {
        let Ok(text) = std::fs::read_to_string(csproj) else { continue };
        let proj_dir = csproj.parent().unwrap_or(Path::new("."));
        let rust_crate_dirs = extract_rust_crate_includes(&text)
            .into_iter()
            .map(|rel| normalize(&proj_dir.join(rel)))
            .collect();
        csproj_facts.push(CsProjFacts {
            path: csproj.clone(),
            rust_crate_dirs,
            rust_dotnet_version: extract_property(&text, "RustDotnetVersion"),
            target_framework: extract_property(&text, "TargetFramework"),
        });
    }

    // (1) Missing RustCrate wiring: a sibling dir with a Cargo.toml (a Rust crate) that is
    // not named by ANY csproj's <RustCrate Include> anywhere in the scanned tree, and that
    // itself is not a C# project dir (no .csproj alongside it — that would just be the
    // Rust-only cargo_tests probe crates that have no C# consumer by design, e.g. rust-on-.NET
    // "app" style crates). We only flag a Rust crate that sits NEXT TO a .csproj sibling
    // (same parent dir) — the strongest signal that it was meant to be consumed by it.
    let referenced: std::collections::HashSet<PathBuf> =
        csproj_facts.iter().flat_map(|f| f.rust_crate_dirs.iter().cloned()).collect();
    for crate_dir in &cargo_tomls {
        let crate_dir = crate_dir.parent().unwrap_or(Path::new(".")).to_path_buf();
        let norm = normalize(&crate_dir);
        if referenced.contains(&norm) {
            continue;
        }
        // Only flag if a sibling .csproj exists in the same small "project family" — either
        // directly in the parent directory, or one level down (the repo convention is
        // `<name>/rustlib` + `<name>/csharp/*.csproj`, i.e. the csproj's OWN parent is a
        // sibling of the crate dir). The parent itself must NOT be the scan root (a large
        // directory holding many unrelated crates, like `cargo_tests/`) — otherwise every
        // crate in a big flat probe-crate tree would spuriously match some unrelated csproj
        // living two levels down elsewhere in that same tree. Otherwise this is plausibly a
        // Rust-only crate never meant to be referenced from C#.
        let Some(parent) = norm.parent() else { continue };
        let parent_is_scan_root = normalize(parent) == normalize(root);
        let has_sibling_csproj = !parent_is_scan_root
            && (has_csproj_under(parent)
                || csprojs.iter().any(|c| {
                    c.parent()
                        .and_then(Path::parent)
                        .map(|gp| normalize(gp) == normalize(parent))
                        .unwrap_or(false)
                }));
        if has_sibling_csproj {
            checks.push(Check::warn(
                format!("RustCrate wiring: {}", norm.display()),
                format!(
                    "a Rust crate exists here with a sibling C# project, but no <RustCrate \
                     Include=\"...\"/> in any scanned .csproj resolves to it — add \
                     `<RustCrate Include=\"{}\" />` to the consuming .csproj, or it will never \
                     be built/referenced.",
                    pretty_relative(&norm, root)
                ),
            ));
        }
    }

    // (2) TFM / RustDotnetVersion mismatches per csproj.
    for f in &csproj_facts {
        let rdv = f.rust_dotnet_version.as_deref();
        let tfm = f.target_framework.as_deref();
        match (rdv, tfm) {
            (Some(rdv), Some(tfm)) => {
                let expected = format!("net{rdv}.0");
                // TargetFramework is allowed to literally be "net$(RustDotnetVersion).0" (the
                // scaffolded template) — that is not a hardcoded value, so nothing to compare.
                if tfm.contains("$(RustDotnetVersion)") {
                    continue;
                }
                if tfm != expected {
                    checks.push(Check::fail(
                        format!("TFM/RustDotnetVersion: {}", f.path.display()),
                        format!(
                            "<RustDotnetVersion>{rdv}</RustDotnetVersion> implies \
                             <TargetFramework>{expected}</TargetFramework>, but the csproj \
                             hardcodes <TargetFramework>{tfm}</TargetFramework> — the Rust \
                             assembly's `.assembly extern .ver` / runtimeconfig will target {rdv} \
                             while the consumer runs on a different TFM. Either set \
                             TargetFramework to net$(RustDotnetVersion).0, or align \
                             RustDotnetVersion with {tfm}."
                        ),
                    ));
                }
            }
            (None, Some(tfm)) if !tfm.contains("$(RustDotnetVersion)") => {
                // No explicit RustDotnetVersion → RustDotnet.props defaults it to "8". Flag
                // only if the hardcoded TFM disagrees with that default (net9.0, net10.0, …),
                // since that is the actual failure mode (dotnet build targets a runtime the
                // Rust side wasn't built for).
                if tfm != "net8.0" {
                    checks.push(Check::warn(
                        format!("TFM/RustDotnetVersion: {}", f.path.display()),
                        format!(
                            "<TargetFramework>{tfm}</TargetFramework> is set but \
                             <RustDotnetVersion> is not — RustDotnet.props defaults it to \"8\" \
                             (net8.0), which disagrees with {tfm}. Set \
                             <RustDotnetVersion>{}</RustDotnetVersion> explicitly (matching {tfm}), \
                             or change TargetFramework to net$(RustDotnetVersion).0.",
                            tfm.trim_start_matches("net").trim_end_matches(".0")
                        ),
                    ));
                }
            }
            _ => {}
        }
    }

    if checks.is_empty() {
        checks.push(Check::pass(
            "workspace wiring",
            format!("no RustCrate/TFM issues found under {}", root.display()),
        ));
    }
    checks
}

/// Recursively collect `Cargo.toml` and `*.csproj` paths under `dir`, skipping build/VCS dirs.
fn walk(dir: &Path, depth: u32, cargo_tomls: &mut Vec<PathBuf>, csprojs: &mut Vec<PathBuf>) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            walk(&path, depth + 1, cargo_tomls, csprojs);
        } else if name == "Cargo.toml" {
            cargo_tomls.push(path);
        } else if name.ends_with(".csproj") {
            csprojs.push(path);
        }
    }
}

/// Whether any `.csproj` exists directly inside `dir` (non-recursive).
fn has_csproj_under(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .any(|e| e.path().extension().map(|x| x == "csproj").unwrap_or(false))
        })
        .unwrap_or(false)
}

fn normalize(p: &Path) -> PathBuf {
    // Best-effort lexical normalization (no filesystem canonicalize, so this still works for
    // paths that don't exist yet / across symlink quirks): resolve `.` and `..` components.
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn pretty_relative(p: &Path, root: &Path) -> String {
    p.strip_prefix(&normalize(root)).unwrap_or(p).display().to_string()
}

/// Extract every `<RustCrate Include="...">` path (attribute-order-agnostic, simple regex-free
/// scan — good enough for the hand-written csproj files this tool targets).
fn extract_rust_crate_includes(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(tag_start) = rest.find("<RustCrate") {
        let after = &rest[tag_start..];
        let tag_end = after.find('>').unwrap_or(after.len());
        let tag = &after[..tag_end];
        if let Some(inc) = extract_attr(tag, "Include") {
            out.push(inc);
        }
        rest = &after[tag_end.min(after.len())..];
        if rest.is_empty() {
            break;
        }
        rest = &rest[1..]; // skip past the '>' we just consumed
    }
    out
}

/// Extract `<PropName>value</PropName>` (first occurrence), trimmed.
fn extract_property(xml: &str, prop: &str) -> Option<String> {
    let open = format!("<{prop}>");
    let close = format!("</{prop}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let val = xml[start..end].trim();
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

/// Extract `Attr="value"` or `Attr='value'` from a tag's inner text.
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=");
    let pos = tag.find(&needle)? + needle.len();
    let rest = &tag[pos..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &rest[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
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

    // -----------------------------------------------------------------------------
    // Workspace wiring lints.
    // -----------------------------------------------------------------------------

    #[test]
    fn extracts_rust_crate_include_paths() {
        let xml = r#"<ItemGroup>
            <RustCrate Include="../rustlib" />
            <RustCrate Include='../other' Configuration="Debug" />
        </ItemGroup>"#;
        assert_eq!(extract_rust_crate_includes(xml), vec!["../rustlib", "../other"]);
    }

    #[test]
    fn extracts_property_value() {
        let xml = "<PropertyGroup><RustDotnetVersion>9</RustDotnetVersion></PropertyGroup>";
        assert_eq!(extract_property(xml, "RustDotnetVersion").as_deref(), Some("9"));
        assert_eq!(extract_property(xml, "TargetFramework"), None);
    }

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cargo_dotnet_doctor_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn clean_workspace_reports_ok() {
        let root = tmp_dir("clean");
        write(&root.join("good/rustlib/Cargo.toml"), "[package]\nname = \"probe\"\n");
        write(
            &root.join("good/csharp/probe_cs.csproj"),
            r#"<Project><PropertyGroup>
                <RustDotnetVersion>8</RustDotnetVersion>
                <TargetFramework>net$(RustDotnetVersion).0</TargetFramework>
            </PropertyGroup>
            <ItemGroup><RustCrate Include="../rustlib" /></ItemGroup></Project>"#,
        );
        let checks = workspace_wiring_checks(&root);
        assert!(checks.iter().all(|c| c.ok), "expected a clean report, got a flagged check");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_missing_rust_crate_wiring() {
        let root = tmp_dir("missing");
        write(&root.join("proj/rustlib/Cargo.toml"), "[package]\nname = \"probe\"\n");
        write(
            &root.join("proj/csharp/probe_cs.csproj"),
            "<Project><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
        );
        let checks = workspace_wiring_checks(&root);
        assert!(
            checks.iter().any(|c| !c.ok && c.label.contains("RustCrate wiring")),
            "expected a RustCrate-wiring warning, got: {:?}",
            checks.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_tfm_version_mismatch() {
        let root = tmp_dir("tfm");
        write(&root.join("proj/rustlib/Cargo.toml"), "[package]\nname = \"probe\"\n");
        write(
            &root.join("proj/csharp/probe_cs.csproj"),
            r#"<Project><PropertyGroup>
                <RustDotnetVersion>9</RustDotnetVersion>
                <TargetFramework>net8.0</TargetFramework>
            </PropertyGroup>
            <ItemGroup><RustCrate Include="../rustlib" /></ItemGroup></Project>"#,
        );
        let checks = workspace_wiring_checks(&root);
        assert!(
            checks.iter().any(|c| !c.ok && c.hard && c.label.contains("TFM/RustDotnetVersion")),
            "expected a hard TFM mismatch failure, got: {:?}",
            checks.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn does_not_flag_unrelated_crates_under_a_large_scan_root() {
        // Regression test: a flat directory holding many unrelated Rust-only crates AND
        // separate, unrelated csproj-having crates (like this repo's `cargo_tests/`) must
        // not cause every crate to be flagged just because *some* csproj exists somewhere
        // else two levels down from the scan root.
        let root = tmp_dir("largeroot");
        write(&root.join("standalone_a/Cargo.toml"), "[package]\nname = \"a\"\n");
        write(&root.join("standalone_b/Cargo.toml"), "[package]\nname = \"b\"\n");
        write(
            &root.join("unrelated_consumer/unrelated.csproj"),
            "<Project><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
        );
        let checks = workspace_wiring_checks(&root);
        assert!(checks.iter().all(|c| c.ok), "expected no false positives, got: {:?}",
            checks.iter().map(|c| (&c.label, c.ok)).collect::<Vec<_>>());
        let _ = std::fs::remove_dir_all(&root);
    }
}
