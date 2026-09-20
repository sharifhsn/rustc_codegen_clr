//! `cargo dotnet test` — build a crate's `#[test]`s with the backend and run them on .NET.
//!
//! A thin composition over the existing native pipeline stages (PAL inject → overlays →
//! build-std → locate → run): it compiles one libtest harness with the dotnet backend and
//! runs it on .NET forwarding any libtest args (after `--`). An explicit `--lib` or `--test NAME`
//! selector is exact; without one, Cargo's JSON must identify exactly one runnable harness.
//!
//! This lets a library author validate their crate on the REAL target — `#[test]`s run
//! through the standard libtest harness, executed by the .NET runtime. Program args after
//! `--` (e.g. a test-name filter, `--nocapture`, `--test-threads=1`) are forwarded to the
//! harness verbatim, exactly like `cargo test -- <args>`.

use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result, bail};

use crate::cli::BuildArgs;
use crate::context::Context;
use crate::mode::Backend;
use crate::{artifact, buildstd, nuget, overlays, run};

/// Run `cargo dotnet test`.
pub fn run(args: &BuildArgs) -> Result<i32> {
    // Test is a native-pipeline feature (it needs to locate + run the harness binary the
    // build produced). The docker dev backend does not model a distinct test verb.
    let mode = crate::mode::detect()?;
    let backend = Backend::resolve(args.backend.as_deref(), &mode)?;
    if backend == Backend::Docker {
        bail!(
            "`cargo dotnet test` runs on the native backend only. \
             Re-run with CARGO_DOTNET_BACKEND=native (or --backend native)."
        );
    }

    let ctx = resolve_test_context(args)?;
    run_native_tests(&ctx, &args.prog_args)
}

fn resolve_test_context(args: &BuildArgs) -> Result<Context> {
    Context::resolve(args, true)
}

/// The native test pipeline: build with `--tests`, locate the harness binary, run it.
fn run_native_tests(ctx: &Context, libtest_args: &[String]) -> Result<i32> {
    let _build_lock = crate::build_lock::BuildLock::acquire_crate(ctx)?;
    let private_sysroot = crate::private_sysroot::prepare(ctx)?;
    overlays::apply(ctx)?;
    if ctx.is_offline() {
        crate::restore::verify(ctx, &private_sysroot)?;
    }
    // Re-stage any `add-nuget` runtime closure missing from a fresh clone before spending a
    // full test build on a crate that would otherwise fail the harness run with
    // `FileNotFoundException` — see `pipeline::run_native`'s identical call for the full doc.
    nuget::ensure_staged(ctx)?;
    let nuget_lease = nuget::acquire_project_lease(&ctx.crate_dir)?;
    let json = buildstd::build_tests_with_sysroot(ctx, &private_sysroot)?;
    let (exe, target) =
        select_exact_harness(&json, &ctx.flags.extra_cargo, &ctx.selected_package_id)?;
    ensure_libtest_harness(&ctx.crate_dir.join("Cargo.toml"), &target)?;
    // A previously valid receipt must not survive a failed attempt to materialize the runtime
    // closure for this invocation. Publish the replacement only after every sidecar is ready.
    crate::receipt::invalidate_for_artifact(&exe)?;
    // The `#[test]` harness itself is an ordinary apphost — it needs the same staged
    // NuGet runtime closure next to it as any other build/run artifact (see
    // `pipeline::run_native`'s identical copy_assets call for the full doc).
    if let Some(out_dir) = exe.parent() {
        nuget_lease.copy_assets(out_dir)?;
    }
    crate::receipt::write_with_test_target(
        ctx,
        &artifact::Artifact::Executable(exe.clone()),
        &private_sysroot,
        target,
    )?;
    eprintln!("== running #[test] harness on .NET: {} ==", exe.display());
    // The located executable IS the libtest harness; forward the libtest args.
    run::run(&artifact::Artifact::Executable(exe), libtest_args, ctx)
}

fn ensure_libtest_harness(manifest: &Path, target: &artifact::TestTargetIdentity) -> Result<()> {
    let source = fs::read_to_string(manifest)
        .with_context(|| format!("read selected Cargo manifest {}", manifest.display()))?;
    let document: toml::Value = toml::from_str(&source)
        .with_context(|| format!("parse selected Cargo manifest {}", manifest.display()))?;
    let explicitly_disabled = if target.kind.iter().any(|kind| kind == "lib") {
        document
            .get("lib")
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get("harness"))
            .and_then(toml::Value::as_bool)
            == Some(false)
    } else if target.kind.iter().any(|kind| kind == "test") {
        let matches = document
            .get("test")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_table)
            .filter(|table| {
                table.get("name").and_then(toml::Value::as_str) == Some(target.name.as_str())
            })
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            bail!(
                "selected Cargo manifest declares test target {} more than once",
                target.name
            );
        }
        matches
            .first()
            .and_then(|table| table.get("harness"))
            .and_then(toml::Value::as_bool)
            == Some(false)
    } else {
        bail!(
            "selected Cargo artifact {} is neither a lib nor test target",
            target.name
        );
    };
    if explicitly_disabled {
        bail!(
            "cargo dotnet test requires a libtest harness, but target {} declares harness=false; run it through a standalone observable-oracle runner",
            target.name
        );
    }
    Ok(())
}

fn select_exact_harness(
    json: &str,
    cargo_flags: &[String],
    selected_package_id: &str,
) -> Result<(std::path::PathBuf, artifact::TestTargetIdentity)> {
    let lib = cargo_flags.iter().any(|flag| flag == "--lib");
    let named = crate::receipt::last_cargo_option_value(cargo_flags, "--test")?;
    let explicit = lib || named.is_some();
    let mut matches = artifact::executable_test_targets(json)
        .into_iter()
        .filter(|(_, target)| target.package_id == selected_package_id)
        .filter(|(_, target)| {
            !explicit
                || (lib && target.kind.iter().any(|kind| kind == "lib"))
                || named.as_deref().is_some_and(|name| {
                    target.name == name && target.kind.iter().any(|kind| kind == "test")
                })
        });
    let selected = matches.next().with_context(|| {
        if explicit {
            "cargo JSON did not identify the explicitly selected test apphost target"
        } else {
            "cargo JSON did not identify a runnable test apphost target"
        }
    })?;
    if matches.next().is_some() {
        if explicit {
            bail!("cargo produced multiple executables for the explicitly selected test harness");
        }
        bail!(
            "cargo produced multiple test harness executables; select exactly one with `--lib` or `--test NAME`"
        );
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{ensure_libtest_harness, select_exact_harness};
    use crate::artifact::TestTargetIdentity;

    #[test]
    fn selectorless_test_accepts_one_unique_cargo_harness() {
        let json = r#"
{"reason":"compiler-artifact","package_id":"path+file:///tmp/crate#0.0.0","target":{"name":"crate_name","kind":["lib"]},"executable":"/tmp/lib"}
"#;
        let (path, target) =
            select_exact_harness(json, &[], "path+file:///tmp/crate#0.0.0").unwrap();
        assert_eq!(path, std::path::PathBuf::from("/tmp/lib"));
        assert_eq!(target.name, "crate_name");
        assert_eq!(target.kind, ["lib"]);
        assert_eq!(target.package_id, "path+file:///tmp/crate#0.0.0");
    }

    #[test]
    fn selectorless_test_fails_closed_for_zero_or_multiple_harnesses() {
        let zero = select_exact_harness("", &[], "path+file:///tmp/crate#0.0.0").unwrap_err();
        assert!(zero.to_string().contains("did not identify a runnable"));

        let json = r#"
{"reason":"compiler-artifact","package_id":"path+file:///tmp/crate#0.0.0","target":{"name":"crate_name","kind":["lib"]},"executable":"/tmp/lib"}
{"reason":"compiler-artifact","package_id":"path+file:///tmp/crate#0.0.0","target":{"name":"integration","kind":["test"]},"executable":"/tmp/integration"}
"#;
        let multiple = select_exact_harness(json, &[], "path+file:///tmp/crate#0.0.0").unwrap_err();
        assert!(multiple.to_string().contains("multiple test harness"));
        assert!(multiple.to_string().contains("--lib"));
    }

    #[test]
    fn exact_named_selector_ignores_other_cargo_test_executables() {
        let json = r#"
{"reason":"compiler-artifact","package_id":"path+file:///tmp/crate#0.0.0","target":{"name":"crate_name","kind":["lib"]},"executable":"/tmp/lib"}
{"reason":"compiler-artifact","package_id":"path+file:///tmp/crate#0.0.0","target":{"name":"wanted","kind":["test"]},"executable":"/tmp/wanted"}
{"reason":"compiler-artifact","package_id":"path+file:///tmp/crate#0.0.0","target":{"name":"other","kind":["test"]},"executable":"/tmp/other"}
"#;
        let (path, target) = select_exact_harness(
            json,
            &["--test".into(), "wanted".into()],
            "path+file:///tmp/crate#0.0.0",
        )
        .unwrap();
        assert_eq!(path, std::path::PathBuf::from("/tmp/wanted"));
        assert_eq!(target.name, "wanted");
        assert_eq!(target.kind, ["test"]);
    }

    #[test]
    fn explicit_selector_fails_closed_if_cargo_reports_multiple_matching_executables() {
        let json = r#"
{"reason":"compiler-artifact","package_id":"path+file:///tmp/crate#0.0.0","target":{"name":"crate_name","kind":["lib"]},"executable":"/tmp/one"}
{"reason":"compiler-artifact","package_id":"path+file:///tmp/crate#0.0.0","target":{"name":"crate_name","kind":["lib"]},"executable":"/tmp/two"}
"#;
        let error = select_exact_harness(json, &["--lib".into()], "path+file:///tmp/crate#0.0.0")
            .unwrap_err();
        assert!(error.to_string().contains("multiple executables"));
    }

    #[test]
    fn exact_selector_rejects_same_named_target_from_another_package() {
        let json = r#"
{"reason":"compiler-artifact","package_id":"path+file:///tmp/other#0.0.0","target":{"name":"wanted","kind":["test"]},"executable":"/tmp/wrong-package"}
"#;
        let error = select_exact_harness(
            json,
            &["--test".into(), "wanted".into()],
            "path+file:///tmp/selected#0.0.0",
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("explicitly selected test apphost target")
        );
    }

    #[test]
    fn selected_manifest_rejects_harness_false_before_receipt_or_execution() {
        let directory = tempfile::tempdir().unwrap();
        let manifest = directory.path().join("Cargo.toml");
        fs::write(
            &manifest,
            r#"
[package]
name = "suite"
version = "0.0.0"

[[test]]
name = "ordinary"
path = "tests/ordinary.rs"

[[test]]
name = "standalone"
path = "tests/standalone.rs"
harness = false
"#,
        )
        .unwrap();
        let identity = |name: &str| TestTargetIdentity {
            package_id: "path+file:///suite#0.0.0".into(),
            name: name.into(),
            kind: vec!["test".into()],
        };
        ensure_libtest_harness(&manifest, &identity("ordinary")).unwrap();
        let error = ensure_libtest_harness(&manifest, &identity("standalone")).unwrap_err();
        assert!(error.to_string().contains("harness=false"), "{error:#}");
        // Cargo-auto-discovered test targets are libtest unless an explicit table overrides them.
        ensure_libtest_harness(&manifest, &identity("auto-discovered")).unwrap();
    }
}
