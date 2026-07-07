//! `cargo-dotnet` — compile/run arbitrary Rust crates on .NET (rustc_codegen_clr).
//!
//! A `cargo install`-able clap binary that replaces the bash front-end
//! (`feasibility/cargo-dotnet`). On the NATIVE backend it owns the WHOLE pipeline in
//! pure Rust — CLI + cargo-subcommand convention + standard-flag passthrough, plus the
//! inner build/run/pack: PAL injection into rust-src ([`palinject`]), the
//! [`dotnet_overlays`] apply ([`overlays`]), `build-std` ([`buildstd`]), artifact
//! location ([`artifact`]), run ([`run`]) and NuGet packing ([`pack`]). It shells out
//! only to the external tools any build tool must (cargo/rustc/ilasm/dotnet/the linker)
//! — never to a bash pipeline core. The DOCKER backend (dev-only) still delegates to the
//! in-repo bash front-end, which owns the container mount model.

mod artifact;
mod buildstd;
mod cli;
mod context;
mod doctor;
mod docker;
mod host;
mod mode;
mod overlays;
mod pack;
mod palinject;
mod passthrough;
mod pipeline;
mod publish;
mod run;
mod rustflags;
mod scaffold;
mod setup;
mod test;

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

use cli::{Cmd, DotnetCli};

fn main() -> ExitCode {
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

    let result = match &cli.cmd {
        Cmd::Build(args) => pipeline::run(args, false),
        Cmd::Run(args) => pipeline::run(args, true),
        Cmd::New(args) => scaffold::run(args),
        Cmd::Doctor(args) => doctor::run(args),
        Cmd::Test(args) => test::run(args),
        Cmd::Setup(args) => setup::run(args),
        Cmd::Pack(args) => pack::run(args),
        Cmd::Publish(args) => publish::run(args),
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
        Cmd::Build(args) | Cmd::Run(args) | Cmd::Test(args) => args.prog_args = prog_args,
        Cmd::New(_) | Cmd::Doctor(_) | Cmd::Setup(_) | Cmd::Pack(_) | Cmd::Publish(_) => {}
    }
}
