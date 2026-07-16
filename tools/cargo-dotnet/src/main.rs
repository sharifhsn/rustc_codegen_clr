//! `cargo-dotnet` — compile/run arbitrary Rust crates on .NET (rustc_codegen_clr).
//!
//! A `cargo install`-able clap binary that replaces the bash front-end
//! (`feasibility/cargo-dotnet`). On the NATIVE backend it owns the WHOLE pipeline in
//! pure Rust — CLI + cargo-subcommand convention + standard-flag passthrough, plus the
//! inner build/run/pack: PAL injection into rust-src ([`palinject`]), the
//! `dotnet_overlays` apply ([`overlays`]), `build-std` ([`buildstd`]), artifact
//! location ([`artifact`]), run ([`run`]), NuGet packing ([`pack`]) and consuming a
//! third-party NuGet package via reflection-generated bindings ([`nuget`]). It shells out
//! only to the external tools any build tool must (cargo/rustc/ilasm/dotnet/the linker)
//! — never to a bash pipeline core. The DOCKER backend (dev-only) still delegates to the
//! in-repo bash front-end, which owns the container mount model.

mod artifact;
mod attach;
mod build_lock;
mod buildstd;
mod bundle;
mod capabilities;
mod cli;
mod context;
mod docker;
mod doctor;
mod host;
mod interop_helpers;
mod metadata_inputs;
mod mode;
mod native_bindgen;
mod nuget;
mod overlays;
mod pack;
mod palinject;
mod parallel_trace;
mod passthrough;
mod pipeline;
mod private_sysroot;
mod profiles;
mod provenance;
mod publish;
mod push;
mod receipt;
mod restore;
mod run;
mod rustflags;
mod scaffold;
mod setup;
mod test;
mod unity;
mod unity_attach;
mod unity_native;
mod unity_package;
mod xmldoc;

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

use cli::{Cmd, DotnetCli};

fn main() -> ExitCode {
    // Cargo invokes RUSTC_WRAPPER as `<wrapper> <rustc> <args...>`. Re-exec rustc with
    // the private sysroot forced even for Cargo's discovery probes (`--print sysroot`),
    // not merely crate compilations that happen to inherit RUSTFLAGS.
    if let Some(sysroot) = std::env::var_os("CARGO_DOTNET_PRIVATE_SYSROOT") {
        let mut args = std::env::args_os().skip(1);
        if let Some(rustc) = args.next() {
            let status = std::process::Command::new(rustc)
                .args(args)
                .arg("--sysroot")
                .arg(sysroot)
                .status();
            return match status {
                Ok(status) => ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8),
                Err(error) => {
                    eprintln!("cargo dotnet rustc wrapper: {error}");
                    ExitCode::from(1)
                }
            };
        }
    }

    // DUAL INVOCATION (both forms exist in the tree — see cli.rs):
    //   `cargo dotnet <cmd>`  -> argv = [cargo-dotnet, dotnet, <cmd>, ...]  (real cargo dispatch)
    //   `cargo-dotnet <cmd>`  -> argv = [cargo-dotnet, <cmd>, ...]          (dev.sh / MSBuild / direct)
    // Normalise by dropping a leading `dotnet` token, then parse DotnetCli directly.
    let mut argv: Vec<OsString> = std::env::args_os().collect();
    if argv.get(1).map(|a| a == "dotnet").unwrap_or(false) {
        argv.remove(1);
    }

    // PROGRAM-ARG SPLIT (run): everything after the FIRST literal `--` is program args
    // for the .NET apphost — NOT clap flags. We split it off the raw argv here so clap
    // never sees it (which avoids the trailing_var_arg/positional/`last` ambiguities),
    // then thread it into the parsed BuildArgs.prog_args below. Inner-cargo flags
    // (--locked/--offline/…) come BEFORE the `--` and are still parsed by clap into
    // `extra`. (This mirrors `cargo run <flags> -- <prog args>`.)
    let prog_args = split_program_args(&mut argv);

    let mut cli = DotnetCli::parse_from(argv);
    inject_prog_args(&mut cli.cmd, prog_args);

    if requires_supported_host(&cli.cmd) {
        if let Err(error) = host::ensure_supported(&host::HostFacts::detect()) {
            eprintln!("cargo dotnet: {error}");
            return ExitCode::from(1);
        }
    }

    let result = match &cli.cmd {
        Cmd::Profiles(args) => profiles::run(args),
        Cmd::Capabilities(args) => capabilities::run(args),
        Cmd::Restore(args) => restore::run(args),
        Cmd::Build(args) => pipeline::run(args, false),
        Cmd::Run(args) => pipeline::run(args, true),
        Cmd::New(args) => scaffold::run(args),
        Cmd::Attach(args) => attach::run(args),
        Cmd::Doctor(args) => doctor::run(args),
        Cmd::Unity(args) => unity::run(args),
        Cmd::Test(args) => test::run(args),
        Cmd::Setup(args) => setup::run(args),
        Cmd::Bundle(args) => bundle::run(args),
        Cmd::Pack(args) => pack::run(args),
        Cmd::Push(args) => push::run(args),
        Cmd::Publish(args) => publish::run(args),
        Cmd::AddNuget(args) => nuget::run(args),
        Cmd::AddNative(args) => nuget::run_native(args),
        Cmd::AddNativeFile(args) => nuget::run_native_file(args),
        Cmd::Bindgen(args) => native_bindgen::run(args),
        Cmd::MetadataInputs(args) => metadata_inputs::run(args),
        Cmd::ValidateManagedIdentities(args) => {
            context::validate_managed_identity_set(&args.crate_dirs)
        }
        Cmd::ManagedAssemblyName(args) => {
            context::print_managed_assembly_name(args.path.as_deref())
        }
    };

    match result {
        Ok(code) => {
            // Map the child exit code to a process ExitCode (0..=255).
            ExitCode::from((code & 0xff) as u8)
        }
        Err(e) => {
            eprintln!("cargo dotnet: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn requires_supported_host(cmd: &Cmd) -> bool {
    if let Cmd::Bundle(args) = cmd {
        return !matches!(args.command, cli::BundleCommand::Verify { .. });
    }
    matches!(
        cmd,
        Cmd::Build(_)
            | Cmd::Restore(_)
            | Cmd::Run(_)
            | Cmd::Test(_)
            | Cmd::Setup(_)
            | Cmd::Pack(_)
            | Cmd::Publish(_)
            | Cmd::Push(_)
            | Cmd::AddNuget(_)
            | Cmd::AddNative(_)
            | Cmd::AddNativeFile(_)
            | Cmd::Bindgen(_)
    )
}

/// Remove everything at and after the FIRST literal `--` from `argv`, returning the
/// tokens AFTER it as program args (lossy-UTF8). `argv` is left holding only the
/// clap-parseable portion. If there is no `--`, `argv` is unchanged and the result is
/// empty.
fn split_program_args(argv: &mut Vec<OsString>) -> Vec<String> {
    if let Some(pos) = argv.iter().position(|a| a == "--") {
        let prog: Vec<String> = argv
            .split_off(pos)
            .into_iter()
            .skip(1) // drop the `--` token itself
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        prog
    } else {
        Vec::new()
    }
}

/// Thread the pre-split program args into the parsed Build/Run args. (Setup/Pack/Publish
/// take no program args.)
fn inject_prog_args(cmd: &mut Cmd, prog_args: Vec<String>) {
    match cmd {
        Cmd::Build(args) | Cmd::Run(args) | Cmd::Test(args) | Cmd::Restore(args) => {
            args.prog_args = prog_args
        }
        Cmd::Profiles(_)
        | Cmd::Capabilities(_)
        | Cmd::New(_)
        | Cmd::Attach(_)
        | Cmd::Doctor(_)
        | Cmd::Unity(_)
        | Cmd::Setup(_)
        | Cmd::Bundle(_)
        | Cmd::Pack(_)
        | Cmd::Publish(_)
        | Cmd::Push(_)
        | Cmd::AddNuget(_)
        | Cmd::AddNative(_)
        | Cmd::AddNativeFile(_)
        | Cmd::Bindgen(_)
        | Cmd::MetadataInputs(_)
        | Cmd::ValidateManagedIdentities(_)
        | Cmd::ManagedAssemblyName(_) => {}
    }
}
