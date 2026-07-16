//! `cargo dotnet doctor` — diagnose the interop toolchain + translate runtime failures.
//!
//! Three user-facing checks:
//!
//!  1. **Environment check** — verify the pieces a native `cargo dotnet` build/run needs
//!     are present: the pinned nightly components, .NET SDK, backend, linker, target spec,
//!     and MSBuild integration. CoreCLR ILAsm is an optional fallback for `DIRECT_PE=0`.
//!
//!  2. **Workspace wiring and native imports** — validate Rust/MSBuild configuration, then
//!     compare declared `#[link]`/`#[link_name]` imports with staged host-RID binaries.
//!
//!  3. **Runtime-failure translation** — scan a supplied log or piped message and turn known
//!     managed-loader and P/Invoke exceptions into concrete causes and recovery commands.

use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use serde::Serialize;

use crate::cli::DoctorArgs;
use crate::host::{self, HostFacts};
use crate::mode::{self, Mode};

/// One diagnostic line: a status + a human message (+ an optional fix hint).
#[derive(Serialize)]
struct Check {
    ok: bool,
    /// `false` for a soft warning that does not fail the overall exit code.
    hard: bool,
    label: String,
    detail: String,
}

impl Check {
    fn pass(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Check {
            ok: true,
            hard: true,
            label: label.into(),
            detail: detail.into(),
        }
    }
    fn fail(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Check {
            ok: false,
            hard: true,
            label: label.into(),
            detail: detail.into(),
        }
    }
    fn warn(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Check {
            ok: false,
            hard: false,
            label: label.into(),
            detail: detail.into(),
        }
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
        if args.json {
            println!("{}", failure_report_json(&hints)?);
            return Ok(0);
        }
        if hints.is_empty() {
            println!("cargo dotnet doctor: no known interop failure signature matched.");
            println!(
                "  (recognised: TypeLoadException, MissingMethod/EntryPointNotFound, \
                      DllNotFound, BadImageFormat, and the Mono-ilasm PE mismatch)"
            );
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
    let checks = environment_checks(&args.dotnet);
    let mut wiring_checks = workspace_wiring_checks(&args.workspace);
    wiring_checks.extend(native_import_checks(&args.workspace));
    let fails = checks
        .iter()
        .chain(&wiring_checks)
        .filter(|check| !check.ok && check.hard)
        .count() as u32;

    if args.json {
        println!(
            "{}",
            environment_report_json(
                &args.dotnet,
                &args.workspace,
                &checks,
                &wiring_checks,
                fails,
            )?
        );
        return Ok(if fails == 0 { 0 } else { 1 });
    }

    let mut out = String::new();
    out.push_str("cargo dotnet doctor — environment:\n\n");
    for c in &checks {
        c.render(&mut out);
    }
    print!("{out}");

    // Workspace-wiring lints: sibling Rust crates missing a <RustCrate> reference, and
    // TFM/RustDotnetVersion mismatches. Best-effort — scan errors are reported as a single
    // soft warning rather than aborting the whole `doctor` run.
    if !wiring_checks.is_empty() {
        let mut wout = String::new();
        wout.push_str(&format!(
            "\ncargo dotnet doctor — workspace wiring ({}):\n\n",
            args.workspace.display()
        ));
        for c in &wiring_checks {
            c.render(&mut wout);
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

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq)]
struct NativeImport {
    rust_symbol: String,
    entry_point: String,
}

fn native_import_checks(workspace: &Path) -> Vec<Check> {
    use std::collections::{BTreeMap, BTreeSet};
    use syn::visit::Visit;

    #[derive(Default)]
    struct LinkVisitor {
        libraries: BTreeMap<String, BTreeSet<NativeImport>>,
    }
    impl<'ast> Visit<'ast> for LinkVisitor {
        fn visit_item_foreign_mod(&mut self, item: &'ast syn::ItemForeignMod) {
            let mut library = None;
            for attribute in &item.attrs {
                if !attribute.path().is_ident("link") {
                    continue;
                }
                let _ = attribute.parse_nested_meta(|meta| {
                    if meta.path.is_ident("name") {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        library = Some(value.value());
                    }
                    Ok(())
                });
            }
            if let Some(library) = library {
                let imports = self.libraries.entry(library).or_default();
                for foreign_item in &item.items {
                    let syn::ForeignItem::Fn(function) = foreign_item else {
                        continue;
                    };
                    let rust_symbol = function.sig.ident.to_string();
                    let mut entry_point = rust_symbol.clone();
                    for attribute in &function.attrs {
                        if attribute.path().is_ident("link_name")
                            && let syn::Meta::NameValue(name_value) = &attribute.meta
                            && let syn::Expr::Lit(expression) = &name_value.value
                            && let syn::Lit::Str(value) = &expression.lit
                        {
                            entry_point = value.value();
                        }
                    }
                    imports.insert(NativeImport {
                        rust_symbol,
                        entry_point,
                    });
                }
            }
            syn::visit::visit_item_foreign_mod(self, item);
        }
    }

    fn visit_rs(root: &Path, visitor: &mut LinkVisitor) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit_rs(&path, visitor);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs")
                && let Ok(source) = std::fs::read_to_string(&path)
                && let Ok(file) = syn::parse_file(&source)
            {
                visitor.visit_file(&file);
            }
        }
    }

    let source_root = workspace.join("src");
    if !source_root.is_dir() {
        return Vec::new();
    }
    let mut visitor = LinkVisitor::default();
    visit_rs(&source_root, &mut visitor);
    if visitor.libraries.is_empty() {
        return Vec::new();
    }
    let staged = match crate::nuget::staged_package_assets(workspace) {
        Ok(staged) => staged,
        Err(error) => {
            return vec![Check::fail(
                "native asset manifest",
                format!("could not enumerate declared native assets: {error:#}"),
            )];
        }
    };
    let host = HostFacts::detect();
    visitor
        .libraries
        .into_iter()
        .map(|(library, imports)| inspect_native_import(&library, &imports, &staged, &host))
        .collect()
}

fn inspect_native_import(
    library: &str,
    imports: &std::collections::BTreeSet<NativeImport>,
    staged: &[crate::nuget::StagedPackageAsset],
    host: &HostFacts,
) -> Check {
    let matching = staged
        .iter()
        .filter(|asset| {
            asset.kind == crate::nuget::StagedPackageAssetKind::Native
                && crate::nuget::native_library_matches(library, &asset.source)
        })
        .collect::<Vec<_>>();
    let declared = imports
        .iter()
        .map(|import| {
            if import.rust_symbol == import.entry_point {
                import.entry_point.clone()
            } else {
                format!("{} -> {}", import.rust_symbol, import.entry_point)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if matching.is_empty() {
        return Check::warn(
            format!("native import {library}"),
            format!(
                "declares [{declared}], but no matching staged package asset exists; this is valid only when the host OS loader supplies the library"
            ),
        );
    }

    let available_rids = matching
        .iter()
        .filter_map(|asset| asset.rid.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    let host_assets = matching
        .into_iter()
        .filter(|asset| asset.rid.as_deref().is_none_or(|rid| rid == host.host_rid))
        .collect::<Vec<_>>();
    if host_assets.is_empty() {
        return Check::fail(
            format!("native import {library}"),
            format!(
                "missing RID asset for host {}; available RID(s): {}. Restore or vendor this library for {}",
                host.host_rid,
                available_rids.into_iter().collect::<Vec<_>>().join(", "),
                host.host_rid
            ),
        );
    }

    let mut failures = Vec::new();
    for asset in host_assets {
        match inspect_native_binary(&asset.source, host, imports) {
            Ok(()) => {
                return Check::pass(
                    format!("native import {library}"),
                    format!(
                        "{} matches host {} and exports [{}]",
                        asset.source.display(),
                        host.host_rid,
                        declared
                    ),
                );
            }
            Err(error) => failures.push(format!("{}: {error:#}", asset.source.display())),
        }
    }
    Check::fail(format!("native import {library}"), failures.join("; "))
}

fn inspect_native_binary(
    path: &Path,
    host: &HostFacts,
    imports: &std::collections::BTreeSet<NativeImport>,
) -> anyhow::Result<()> {
    use object::{Object as _, ObjectSymbol as _};

    let bytes = std::fs::read(path)?;
    let file = object::File::parse(bytes.as_slice())?;
    let expected_architecture = if host.host_rid.ends_with("-arm64") {
        object::Architecture::Aarch64
    } else {
        object::Architecture::X86_64
    };
    if file.architecture() != expected_architecture {
        anyhow::bail!(
            "architecture mismatch: host {} requires {:?}, binary is {:?}",
            host.host_rid,
            expected_architecture,
            file.architecture()
        );
    }

    let mut exports = std::collections::BTreeSet::new();
    if let Ok(file_exports) = file.exports() {
        for export in file_exports {
            if let Ok(name) = std::str::from_utf8(export.name()) {
                exports.insert(name.to_owned());
            }
        }
    }
    for symbol in file.dynamic_symbols() {
        if symbol.is_definition()
            && let Ok(name) = symbol.name()
        {
            exports.insert(name.to_owned());
        }
    }
    let missing = imports
        .iter()
        .filter(|import| {
            let entry = &import.entry_point;
            !exports.contains(entry)
                && !(host.os == "macos" && exports.contains(&format!("_{entry}")))
        })
        .map(|import| {
            format!(
                "Rust symbol {} expects native entry point {}",
                import.rust_symbol, import.entry_point
            )
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        anyhow::bail!(
            "missing entry point(s): {}; regenerate declarations or correct #[link_name]",
            missing.join(", ")
        );
    }
    Ok(())
}

#[derive(Serialize)]
struct FailureReport<'a> {
    schema: u32,
    mode: &'static str,
    matched: bool,
    hints: &'a [Hint],
}

fn failure_report_json(hints: &[Hint]) -> Result<String> {
    Ok(serde_json::to_string_pretty(&FailureReport {
        schema: 1,
        mode: "failure",
        matched: !hints.is_empty(),
        hints,
    })?)
}

#[derive(Serialize)]
struct EnvironmentReport<'a> {
    schema: u32,
    mode: &'static str,
    dotnet: &'a str,
    workspace: String,
    ok: bool,
    hard_failures: u32,
    environment: &'a [Check],
    workspace_wiring: &'a [Check],
}

fn environment_report_json(
    dotnet: &str,
    workspace: &Path,
    environment: &[Check],
    workspace_wiring: &[Check],
    hard_failures: u32,
) -> Result<String> {
    Ok(serde_json::to_string_pretty(&EnvironmentReport {
        schema: 1,
        mode: "environment",
        dotnet,
        workspace: workspace.display().to_string(),
        ok: hard_failures == 0,
        hard_failures,
        environment,
        workspace_wiring,
    })?)
}

// ---------------------------------------------------------------------------------
// Environment checks.
// ---------------------------------------------------------------------------------

fn environment_checks(dotnet_version: &str) -> Vec<Check> {
    let mut checks = Vec::new();
    let facts = HostFacts::detect();

    if let Ok(Mode::Installed { home }) = mode::detect() {
        checks.push(match crate::bundle::verify_installed_if_locked(&home) {
            Ok(true) => Check::pass(
                "install bundle integrity",
                "BUNDLE-LOCK.json and all declared SDK files match".to_string(),
            ),
            Ok(false) => Check::warn(
                "install bundle integrity",
                "source-checkout setup has no BUNDLE-LOCK.json; install a release bundle for checksummed SDK inputs"
                    .to_string(),
            ),
            Err(error) => Check::fail("install bundle integrity", format!("{error:#}")),
        });
    }

    // rustc / cargo on PATH.
    checks.push(match host::ensure_rust_toolchain() {
        Ok(()) => Check::pass("rustc + cargo on PATH", ""),
        Err(e) => Check::fail("rustc + cargo on PATH", e.to_string()),
    });

    // The pinned nightly toolchain (rust-src + rustc-dev are what the backend needs).
    checks.push(check_pinned_toolchain());

    // dotnet. Select the requested profile before choosing among side-by-side hosts.
    let dotnet: Result<crate::context::DotnetVersion, _> = dotnet_version.parse();
    let heal = dotnet
        .as_ref()
        .ok()
        .and_then(|dotnet| host::dotnet_env_adds_for(dotnet.as_env()));
    let dotnet_reachable = host::ensure_dotnet(&heal).is_ok();
    checks.push(match host::ensure_dotnet(&heal) {
        Ok(()) => Check::pass("dotnet runtime reachable", ""),
        Err(e) => Check::fail("dotnet runtime reachable", e.to_string()),
    });

    // The selected runtime profile must actually be installed. Merely finding a `dotnet` binary is
    // insufficient: an older SDK can otherwise survive setup and fail only after an expensive
    // Rust build with the runtime's opaque "install or update .NET" message.
    match dotnet {
        Ok(dotnet) => {
            if dotnet_reachable {
                checks.push(check_dotnet_runtime(dotnet, heal.as_ref()));
            }
            // `ilasm` is only needed for DIRECT_PE=0, so its absence is a warning rather than a
            // blocker for the default direct-PE pipeline.
            checks.push(match host::resolve_ilasm(&facts, dotnet) {
                Ok(Some(p)) => {
                    Check::pass("CoreCLR ilasm fallback", format!("using {}", p.display()))
                }
                Ok(None) => Check::pass(
                    "CoreCLR ilasm fallback",
                    "a non-Mono ilasm is on PATH".to_string(),
                ),
                Err(e) => Check::warn(
                    "CoreCLR ilasm fallback",
                    format!("optional unless DIRECT_PE=0: {e}"),
                ),
            });
        }
        Err(e) => checks.push(Check::fail("selected .NET runtime", e)),
    }

    // The backend install (dylib + linker + target spec + msbuild targets).
    checks.extend(check_backend_install(&facts));

    checks
}

fn check_dotnet_runtime(
    dotnet: crate::context::DotnetVersion,
    heal: Option<&(PathBuf, PathBuf)>,
) -> Check {
    let mut command = if let Some((path, root)) = heal {
        let mut command = Command::new(path.join(if cfg!(windows) {
            "dotnet.exe"
        } else {
            "dotnet"
        }));
        command.env("DOTNET_ROOT", root);
        command
    } else {
        Command::new("dotnet")
    };
    match command.arg("--list-runtimes").output() {
        Ok(output) if output.status.success() => {
            let runtimes = String::from_utf8_lossy(&output.stdout);
            if has_runtime_major(&runtimes, dotnet.as_env()) {
                Check::pass(
                    format!(".NET {} runtime installed", dotnet.as_env()),
                    "Microsoft.NETCore.App is available".to_string(),
                )
            } else {
                Check::fail(
                    format!(".NET {} runtime installed", dotnet.as_env()),
                    format!(
                        "Microsoft.NETCore.App {}.x is missing — install the .NET {} SDK, or select an installed profile with `--dotnet` / DOTNET_VERSION",
                        dotnet.as_env(),
                        dotnet.as_env()
                    ),
                )
            }
        }
        Ok(output) => Check::fail(
            "installed .NET runtimes",
            format!("`dotnet --list-runtimes` exited with {}", output.status),
        ),
        Err(error) => Check::fail(
            "installed .NET runtimes",
            format!("could not run `dotnet --list-runtimes`: {error}"),
        ),
    }
}

fn has_runtime_major(list: &str, major: &str) -> bool {
    list.lines().any(|line| {
        let mut fields = line.split_whitespace();
        fields.next() == Some("Microsoft.NETCore.App")
            && fields
                .next()
                .is_some_and(|version| version.starts_with(&format!("{major}.")))
    })
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
    let backend_name = facts.backend_dylib_name();
    let (root_label, dylib, linker, target_spec, targets) = match mode::detect() {
        Ok(Mode::Installed { home }) => (
            format!("installed home {}", home.display()),
            home.join("bin").join(&backend_name),
            home.join(format!("bin/linker{}", facts.exe_ext)),
            home.join("target/x86_64-unknown-dotnet.json"),
            home.join("msbuild/RustDotnet.targets"),
        ),
        Ok(Mode::Dev { repo_root }) => (
            format!("dev checkout {}", repo_root.display()),
            repo_root.join("target/release").join(&backend_name),
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
    checks.push(file_check(
        "target spec (x86_64-unknown-dotnet.json)",
        &target_spec,
        true,
    ));
    // RustDotnet.targets is only needed for the C#-consumes-Rust (--lib/--plugin) flow, so a
    // soft warning rather than a hard failure.
    checks.push(file_check(
        "msbuild/RustDotnet.targets (C# consumer)",
        &targets,
        false,
    ));
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
            format!(
                "missing: {} (only needed for the C# consumer flow)",
                path.display()
            ),
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
    "target",
    ".git",
    "node_modules",
    "bin",
    "obj",
    ".vs",
    ".idea",
    "graphify-out",
];

fn workspace_wiring_checks(root: &Path) -> Vec<Check> {
    if !root.is_dir() {
        return vec![Check::warn(
            "workspace wiring scan",
            format!(
                "--workspace {} is not a directory; skipping",
                root.display()
            ),
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
        let Ok(text) = std::fs::read_to_string(csproj) else {
            continue;
        };
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
    let referenced: std::collections::HashSet<PathBuf> = csproj_facts
        .iter()
        .flat_map(|f| f.rust_crate_dirs.iter().cloned())
        .collect();
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
        let Some(parent) = norm.parent() else {
            continue;
        };
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
        if f.rust_crate_dirs.is_empty() {
            continue;
        }
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
                // No explicit RustDotnetVersion → RustDotnet.props defaults it to "10". Flag
                // only if the hardcoded TFM disagrees with that default (net9.0, net10.0, …),
                // since that is the actual failure mode (dotnet build targets a runtime the
                // Rust side wasn't built for).
                if tfm != "net10.0" {
                    checks.push(Check::warn(
                        format!("TFM/RustDotnetVersion: {}", f.path.display()),
                        format!(
                            "<TargetFramework>{tfm}</TargetFramework> is set but \
                             <RustDotnetVersion> is not — RustDotnet.props defaults it to \"10\" \
                             (net10.0), which disagrees with {tfm}. Set \
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
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
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
    p.strip_prefix(&normalize(root))
        .unwrap_or(p)
        .display()
        .to_string()
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
#[derive(Serialize)]
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
    unsafe extern "C" {
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

    // 1a. A managed GC reference cannot participate in Rust's overlapping enum/union layout.
    // Match this before the generic TypeLoadException advice: suggesting an assembly-name change
    // for this loader error sends the user in exactly the wrong direction.
    if (lower.contains("typeloadexception") || lower.contains("could not load type"))
        && lower.contains("object field at offset")
        && (lower.contains("incorrectly aligned")
            || lower.contains("overlapped by a non-object field"))
    {
        hints.push(Hint {
            title: "TypeLoadException → managed reference in an overlapping Rust layout".to_string(),
            body: vec![
                "The CLR garbage collector forbids an object reference from overlapping a non-object field in an explicit-layout type."
                    .to_string(),
                "The usual cause is placing a managed wrapper such as Json, List, or DotNetString directly inside Result, a data-carrying enum, or a union."
                    .to_string(),
                "Fix: pattern-match Option at the managed boundary and keep the managed handle outside the Rust enum, or convert it to Rust-owned data before wrapping it in Result."
                    .to_string(),
                "This is a CLR layout boundary, not an implementation-assembly mismatch."
                    .to_string(),
            ],
        });
    // 1b. Other TypeLoadExceptions are usually the impl-assembly gotcha.
    } else if lower.contains("typeloadexception") || lower.contains("could not load type") {
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
        hints.push(Hint {
            title: "TypeLoadException → impl-assembly mismatch".to_string(),
            body,
        });
    }

    // 2a. Missing managed method.
    if lower.contains("missingmethodexception") || lower.contains("missingmethod") {
        let mut body = vec![
            "A managed method the C# side called was not found in the Rust assembly.".to_string(),
            "Common causes:".to_string(),
            "  • the Rust side did not `mycorrhiza::export_rust_containers!()` (no rcl_vec_* exported)"
                .to_string(),
            "  • the public Rust export was renamed or removed without rebuilding the C# consumer"
                .to_string(),
        ];
        if lower.contains("rcl_vec") {
            body.push(
                "The name mentions rcl_vec_* → add `mycorrhiza::export_rust_containers!()` to the \
                 cdylib's lib.rs and rebuild."
                    .to_string(),
            );
        }
        hints.push(Hint {
            title: "MissingMethod → managed Rust API mismatch".to_string(),
            body,
        });
    }

    // 2b. Native P/Invoke symbol absent from a library that did load.
    if lower.contains("entrypointnotfoundexception") || lower.contains("entry point") {
        hints.push(Hint {
            title: "EntryPointNotFound → native export name/signature mismatch".to_string(),
            body: vec![
                "The native library loaded, but it does not export the entry point recorded by the Rust declaration."
                    .to_string(),
                "Run `cargo dotnet doctor --workspace <crate>` to compare every #[link] declaration and #[link_name] against the staged host-RID binary."
                    .to_string(),
                "Fix the native export, regenerate bindgen declarations, or correct #[link_name]; changing only the Rust function identifier does not change an explicit native entry point."
                    .to_string(),
            ],
        });
    }

    // 3. DllNotFound — a native dependency was not resolvable by the host loader.
    if lower.contains("dllnotfoundexception")
        || (lower.contains("unable to load") && lower.contains("dll"))
    {
        hints.push(Hint {
            title: "DllNotFound → native library or host RID asset missing".to_string(),
            body: vec![
                "CoreCLR could not resolve a native library named by P/Invoke.".to_string(),
                "Run `cargo dotnet doctor --workspace <crate>` to distinguish no staged library from a missing RID, architecture mismatch, or missing entry point."
                    .to_string(),
                "Use `cargo dotnet add-native` for a NuGet-native package or `add-native-file` for a local binary; keep #[link(name = ...)] equal to the logical loader name."
                    .to_string(),
            ],
        });
    }

    // 4. BadImageFormat — invalid managed PE or wrong-architecture native binary.
    if lower.contains("badimageformatexception") || lower.contains("bad il format") {
        hints.push(Hint {
            title: "BadImageFormat → invalid managed PE or native architecture mismatch"
                .to_string(),
            body: vec![
                "CoreCLR rejected either a managed assembly image or a native P/Invoke dependency."
                    .to_string(),
                "For native code, run `cargo dotnet doctor --workspace <crate>` to compare the binary architecture with the host RID."
                    .to_string(),
                "For the optional DIRECT_PE=0 path, use the matching CoreCLR ILAsm; Mono ILAsm output is not a supported substitute."
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
    fn runtime_inventory_matches_only_coreclr_major() {
        let inventory = "Microsoft.AspNetCore.App 10.0.1 [/dotnet/shared/Microsoft.AspNetCore.App]\n\
                         Microsoft.NETCore.App 8.0.28 [/dotnet/shared/Microsoft.NETCore.App]\n";
        assert!(has_runtime_major(inventory, "8"));
        assert!(!has_runtime_major(inventory, "10"));
    }

    #[test]
    fn typeload_maps_to_impl_assembly() {
        let hints = diagnose_failure(
            "Unhandled exception. System.TypeLoadException: Could not load type \
             'System.Collections.Generic.Stack`1' from assembly 'System.Private.CoreLib'.",
        );
        assert_eq!(hints.len(), 1);
        assert!(hints[0].title.contains("impl-assembly"));
        // The Stack-specific sharpening fired.
        assert!(
            hints[0]
                .body
                .iter()
                .any(|l| l.contains("System.Collections"))
        );
        assert!(
            hints[0]
                .body
                .iter()
                .any(|l| l.contains("names Stack/Queue"))
        );
    }

    #[test]
    fn overlapping_object_typeload_maps_to_managed_enum_layout() {
        let hints = diagnose_failure(
            "Unhandled exception. System.TypeLoadException: Could not load type \
             'core.result.Result.tid_123' because it contains an object field at offset 8 that \
             is incorrectly aligned or overlapped by a non-object field.",
        );
        assert_eq!(hints.len(), 1);
        assert!(hints[0].title.contains("overlapping Rust layout"));
        assert!(hints[0].body.iter().any(|line| line.contains("Result")));
        assert!(
            hints[0]
                .body
                .iter()
                .any(|line| line.contains("not an implementation-assembly mismatch"))
        );
    }

    #[test]
    fn missing_method_maps_to_export() {
        let hints =
            diagnose_failure("System.MissingMethodException: Method not found: rcl_vec_new");
        assert_eq!(hints.len(), 1);
        assert!(hints[0].title.contains("managed Rust API mismatch"));
        assert!(
            hints[0]
                .body
                .iter()
                .any(|l| l.contains("export_rust_containers"))
        );
    }

    #[test]
    fn entrypoint_not_found_maps_to_export() {
        let hints = diagnose_failure("EntryPointNotFoundException: greet");
        assert_eq!(hints.len(), 1);
        assert!(hints[0].title.contains("native export"));
        assert!(
            hints[0]
                .body
                .iter()
                .any(|line| line.contains("#[link_name]"))
        );
    }

    #[test]
    fn badimage_maps_to_mono_ilasm() {
        let hints = diagnose_failure("System.BadImageFormatException: Bad IL format.");
        assert_eq!(hints.len(), 1);
        assert!(hints[0].title.contains("architecture mismatch"));
        assert!(hints[0].body.iter().any(|line| line.contains("Mono ILAsm")));
    }

    #[test]
    fn native_import_scan_reports_rust_and_explicit_entry_point_names() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();
        std::fs::write(
            temp.path().join("src/lib.rs"),
            r#"
                #[link(name = "sample")]
                unsafe extern "C" {
                    fn ordinary(value: i32) -> i32;
                    #[link_name = "native_renamed"]
                    fn rust_renamed() -> i32;
                }
            "#,
        )
        .unwrap();
        let checks = native_import_checks(temp.path());
        assert_eq!(checks.len(), 1);
        assert!(!checks[0].hard);
        assert!(checks[0].detail.contains("ordinary"));
        assert!(checks[0].detail.contains("rust_renamed -> native_renamed"));
    }

    #[test]
    fn native_import_check_distinguishes_missing_host_rid() {
        let host = HostFacts::detect();
        let other_rid = if host.host_rid == "linux-x64" {
            "osx-arm64"
        } else {
            "linux-x64"
        };
        let staged = vec![crate::nuget::StagedPackageAsset {
            logical_path: format!("runtimes/{other_rid}/native/libsample.so"),
            source: PathBuf::from("libsample.so"),
            kind: crate::nuget::StagedPackageAssetKind::Native,
            rid: Some(other_rid.to_owned()),
        }];
        let imports = [NativeImport {
            rust_symbol: "call".into(),
            entry_point: "call".into(),
        }]
        .into_iter()
        .collect();
        let check = inspect_native_import("sample", &imports, &staged, &host);
        assert!(check.hard && !check.ok);
        assert!(check.detail.contains("missing RID asset"));
        assert!(check.detail.contains(host.host_rid));
        assert!(check.detail.contains(other_rid));
    }

    #[test]
    fn binary_inspection_distinguishes_architecture_and_missing_entry_point() {
        let executable = std::env::current_exe().unwrap();
        let host = HostFacts::detect();
        let no_imports = std::collections::BTreeSet::new();
        inspect_native_binary(&executable, &host, &no_imports).unwrap();

        let missing = [NativeImport {
            rust_symbol: "rust_probe".into(),
            entry_point: "rcl_entry_that_cannot_exist_7d78614b".into(),
        }]
        .into_iter()
        .collect();
        let error = inspect_native_binary(&executable, &host, &missing).unwrap_err();
        assert!(error.to_string().contains("missing entry point"));
        assert!(error.to_string().contains("rust_probe"));

        let wrong_host = HostFacts {
            host_rid: if host.host_rid.ends_with("-arm64") {
                "test-x64"
            } else {
                "test-arm64"
            },
            ..host
        };
        let error = inspect_native_binary(&executable, &wrong_host, &no_imports).unwrap_err();
        assert!(error.to_string().contains("architecture mismatch"));
    }

    #[test]
    fn unknown_text_yields_no_hint() {
        assert!(diagnose_failure("thread 'main' panicked at 'index out of bounds'").is_empty());
    }

    #[test]
    fn failure_json_has_stable_schema_and_match_state() {
        let hints = diagnose_failure("EntryPointNotFoundException: greet");
        let report: serde_json::Value =
            serde_json::from_str(&failure_report_json(&hints).unwrap()).unwrap();
        assert_eq!(report["schema"], 1);
        assert_eq!(report["mode"], "failure");
        assert_eq!(report["matched"], true);
        assert_eq!(report["hints"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn environment_json_separates_environment_and_wiring_checks() {
        let environment = vec![Check::pass("dotnet", "reachable")];
        let wiring = vec![Check::fail("TFM", "mismatch")];
        let report: serde_json::Value = serde_json::from_str(
            &environment_report_json("10", Path::new("fixture"), &environment, &wiring, 1).unwrap(),
        )
        .unwrap();
        assert_eq!(report["schema"], 1);
        assert_eq!(report["mode"], "environment");
        assert_eq!(report["ok"], false);
        assert_eq!(report["hard_failures"], 1);
        assert_eq!(report["environment"][0]["label"], "dotnet");
        assert_eq!(report["workspace_wiring"][0]["label"], "TFM");
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
        assert_eq!(
            extract_rust_crate_includes(xml),
            vec!["../rustlib", "../other"]
        );
    }

    #[test]
    fn extracts_property_value() {
        let xml = "<PropertyGroup><RustDotnetVersion>9</RustDotnetVersion></PropertyGroup>";
        assert_eq!(
            extract_property(xml, "RustDotnetVersion").as_deref(),
            Some("9")
        );
        assert_eq!(extract_property(xml, "TargetFramework"), None);
    }

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cargo_dotnet_doctor_test_{name}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn clean_workspace_reports_ok() {
        let root = tmp_dir("clean");
        write(
            &root.join("good/rustlib/Cargo.toml"),
            "[package]\nname = \"probe\"\n",
        );
        write(
            &root.join("good/csharp/probe_cs.csproj"),
            r#"<Project><PropertyGroup>
                <RustDotnetVersion>8</RustDotnetVersion>
                <TargetFramework>net$(RustDotnetVersion).0</TargetFramework>
            </PropertyGroup>
            <ItemGroup><RustCrate Include="../rustlib" /></ItemGroup></Project>"#,
        );
        let checks = workspace_wiring_checks(&root);
        assert!(
            checks.iter().all(|c| c.ok),
            "expected a clean report, got a flagged check"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_missing_rust_crate_wiring() {
        let root = tmp_dir("missing");
        write(
            &root.join("proj/rustlib/Cargo.toml"),
            "[package]\nname = \"probe\"\n",
        );
        write(
            &root.join("proj/csharp/probe_cs.csproj"),
            "<Project><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
        );
        let checks = workspace_wiring_checks(&root);
        assert!(
            checks
                .iter()
                .any(|c| !c.ok && c.label.contains("RustCrate wiring")),
            "expected a RustCrate-wiring warning, got: {:?}",
            checks.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_tfm_version_mismatch() {
        let root = tmp_dir("tfm");
        write(
            &root.join("proj/rustlib/Cargo.toml"),
            "[package]\nname = \"probe\"\n",
        );
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
            checks
                .iter()
                .any(|c| !c.ok && c.hard && c.label.contains("TFM/RustDotnetVersion")),
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
        write(
            &root.join("standalone_a/Cargo.toml"),
            "[package]\nname = \"a\"\n",
        );
        write(
            &root.join("standalone_b/Cargo.toml"),
            "[package]\nname = \"b\"\n",
        );
        write(
            &root.join("unrelated_consumer/unrelated.csproj"),
            "<Project><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
        );
        let checks = workspace_wiring_checks(&root);
        assert!(
            checks.iter().all(|c| c.ok),
            "expected no false positives, got: {:?}",
            checks.iter().map(|c| (&c.label, c.ok)).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
