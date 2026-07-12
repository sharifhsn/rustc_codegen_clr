//! `publish` — wrap the proven whole-program NativeAOT recipe as a first-class subcommand.
//!
//! AOT is codegen-proven end-to-end (see `docs/PERF_GUIDANCE.md` §5: a `PublishAot` C#
//! host referencing a Rust `cdylib` built by this backend, `ILC`-compiled to a
//! standalone native binary — `cd_interop` 6/6 under AOT, byte-identical to the JIT
//! path). Until now that recipe was manual:
//!
//!   cargo dotnet build mylib --release
//!   # host.csproj: <PublishAot>true</PublishAot> <RuntimeIdentifier>osx-arm64</RuntimeIdentifier>
//!   dotnet publish -c Release
//!
//! `cargo dotnet publish <csproj-dir>` wraps this: it takes the directory of an
//! EXISTING C# host project (one that `<Import>`s `RustDotnet.targets` and declares its
//! `<RustCrate>` — see any `cargo_tests/cd_*/csharp` for the shape, or scaffold one with
//! `cargo dotnet new`), and runs `dotnet publish` against it with the NativeAOT
//! properties set on the command line (`-p:PublishAot=true`, `-r <host-rid>`,
//! `--self-contained`). `RustDotnet.targets`' `BuildRustCrates` target (which every such
//! project already imports) builds the referenced `<RustCrate>` as an ordinary
//! `BeforeTargets="ResolveAssemblyReferences"` step of that SAME `dotnet publish`
//! invocation — so this is genuinely "the existing pipeline, then AOT-publish", not a
//! reimplementation of the build.
//!
//! We deliberately do NOT reinvent the C#-project generation here (no synthesized
//! throwaway host project): the project already needs a `Main` entrypoint and a real
//! `.csproj`, which `cargo dotnet new` (App template) or a hand-written host already
//! provide. `publish` only adds the AOT-specific `dotnet publish` invocation on top.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::cli::PublishArgs;
use crate::host::{self, HostFacts};

pub fn run(args: &PublishArgs) -> Result<i32> {
    let proj_dir = args.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let proj_dir = std::fs::canonicalize(&proj_dir)
        .with_context(|| format!("no such directory: {}", proj_dir.display()))?;

    let csproj = find_csproj(&proj_dir)?;

    // host preflight: dotnet reachable (self-healing from $HOME/.dotnet, same as the
    // build/run path). NativeAOT publish additionally needs a native C toolchain
    // (clang/cc + linker) on the host to link the produced object files — `dotnet
    // publish` reports that itself with an actionable error if missing, so we don't
    // duplicate that check here.
    let facts = HostFacts::detect();
    let dotnet_heal = host::dotnet_env_adds();
    host::ensure_dotnet(&dotnet_heal)?;

    let rid = args
        .rid
        .clone()
        .unwrap_or_else(|| facts.host_rid.to_string());
    let profile = if args.debug { "Debug" } else { "Release" };

    eprintln!(
        "== cargo dotnet publish: {} ({profile}, AOT, {rid}) ==",
        csproj.display()
    );

    let mut cmd = Command::new("dotnet");
    cmd.arg("publish")
        .arg(&csproj)
        .arg("-c")
        .arg(profile)
        .arg("-r")
        .arg(&rid)
        .arg("--self-contained")
        .arg("-p:PublishAot=true")
        // Trimming/single-file are ILC defaults under PublishAot; explicit for clarity
        // and so a consumer's csproj doesn't need to restate them.
        .arg("-p:PublishTrimmed=true");
    for extra in &args.extra {
        cmd.arg(extra);
    }
    if let Some((path_add, dotnet_root)) = &dotnet_heal {
        let cur = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{}", path_add.display(), cur));
        cmd.env("DOTNET_ROOT", dotnet_root);
    }
    if args.verbose {
        eprintln!("+ {cmd:?}");
    } else {
        // `dotnet publish` is chatty (restore + build + ILC compile logs); keep it, but
        // note that -v is what a user reaches for if this needs debugging.
    }

    let status = cmd.status().context("failed to run `dotnet publish`")?;
    let code = status.code().unwrap_or(1);
    if code != 0 {
        return Ok(code);
    }

    // Report the produced native binary path: bin/<profile>/<tfm>/<rid>/publish/<AssemblyName>.
    if let Some(bin) = locate_published_binary(
        csproj.parent().expect("a csproj always has a parent"),
        profile,
        &rid,
    ) {
        eprintln!("== published native binary: {} ==", bin.display());
    } else {
        eprintln!(
            "== publish succeeded (binary not auto-located; see bin/{profile}/*/{rid}/publish/) =="
        );
    }
    Ok(0)
}

/// Resolve the `.csproj` to publish: an explicit `--project` file, or the sole
/// `*.csproj` in the given directory (matching `dotnet publish`'s own auto-discovery
/// rule, but with an actionable error on ambiguity instead of `dotnet`'s terser one).
fn find_csproj(dir: &std::path::Path) -> Result<PathBuf> {
    if dir.is_file() {
        return Ok(dir.to_path_buf());
    }
    let candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("read dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("csproj"))
        .collect();
    match candidates.len() {
        0 => bail!(
            "publish: no .csproj found in {} (pass the path to a C# host project that \
             imports RustDotnet.targets and declares its <RustCrate> — see any \
             cargo_tests/cd_*/csharp for the shape, or `cargo dotnet new --app`)",
            dir.display()
        ),
        1 => Ok(candidates.into_iter().next().unwrap()),
        _ => bail!(
            "publish: multiple .csproj files in {} — pass the specific file as the path",
            dir.display()
        ),
    }
}

/// Best-effort locate the ILC-produced native executable under the SDK's standard
/// publish output layout: `<proj>/bin/<profile>/<tfm>/<rid>/publish/<AssemblyName>`.
/// Returns `None` (non-fatal) if the layout doesn't match — `dotnet publish`'s own
/// stdout already told the user where it wrote the binary in that case.
fn locate_published_binary(
    proj_dir: &std::path::Path,
    profile: &str,
    rid: &str,
) -> Option<PathBuf> {
    let bin_dir = proj_dir.join("bin").join(profile);
    // Multiple TFMs can coexist under `bin/<profile>`. Select the newest native output rather
    // than the first directory entry, which can report a stale net8 binary after a net10 publish.
    std::fs::read_dir(&bin_dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|tfm_dir| tfm_dir.is_dir())
        .flat_map(|tfm_dir| {
            std::fs::read_dir(tfm_dir.join(rid).join("publish"))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
        })
        .filter(|path| {
            path.is_file()
                && !matches!(
                    path.extension().and_then(|extension| extension.to_str()),
                    Some("dll") | Some("pdb") | Some("json") | Some("dSYM")
                )
        })
        .max_by_key(|path| path.metadata().and_then(|metadata| metadata.modified()).ok())
}
