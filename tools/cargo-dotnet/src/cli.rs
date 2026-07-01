//! The clap derive CLI surface for `cargo dotnet`.
//!
//! Two parse paths share these structs (see `main.rs`):
//!   * `cargo dotnet <cmd>` (real cargo dispatch) -> argv `[cargo-dotnet, dotnet, <cmd>, ...]`
//!   * `cargo-dotnet <cmd>` (direct, e.g. dev.sh / MSBuild) -> argv `[cargo-dotnet, <cmd>, ...]`
//!
//! `main` peeks argv[1] and drops a leading `dotnet`, then parses `DotnetCli` directly.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// The `cargo`-subcommand wrapper. Under genuine `cargo dotnet` dispatch, cargo
/// prepends the subcommand name (`dotnet`) as argv[1]; this enum absorbs it. It is
/// kept for the idiomatic `cargo dotnet --help` path, but `main` normalises argv and
/// parses `DotnetCli` directly (so the direct `cargo-dotnet <cmd>` form also works).
#[derive(Parser)]
#[command(name = "cargo", bin_name = "cargo")]
pub enum CargoCli {
    Dotnet(DotnetCli),
}

/// The actual CLI: `cargo dotnet <cmd> ...`.
#[derive(Parser)]
#[command(
    name = "cargo-dotnet",
    bin_name = "cargo dotnet",
    version,
    about = "Compile and run arbitrary Rust crates on .NET (rustc_codegen_clr).",
    long_about = "Compile and run arbitrary Rust crates on .NET via the rustc_codegen_clr backend.\n\n\
                  build/run a crate, setup the toolchain, or pack a .NET assembly into a NuGet package.\n\
                  Standard cargo flags (--features, --manifest-path, -p, --locked, ...) are forwarded\n\
                  to the inner build-std cargo invocation; extra flags + program args go after `--`."
)]
pub struct DotnetCli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Build a Rust crate into a .NET assembly (exe apphost or C#-referenceable .dll).
    Build(BuildArgs),
    /// Build a Rust crate and run it on .NET (forwards exit code; args after `--`).
    Run(BuildArgs),
    /// Scaffold a ready-to-run interop project (--app / --lib / --plugin).
    New(NewArgs),
    /// Diagnose the toolchain, or translate a .NET runtime failure into an actionable fix.
    Doctor(DoctorArgs),
    /// Build a crate's #[test]s with the backend and run them on .NET.
    Test(BuildArgs),
    /// Provision the toolchain + install home, then install this binary to ~/.cargo/bin.
    Setup(SetupArgs),
    /// Build a crate's cdylib and produce a NuGet .nupkg of its .NET assembly.
    Pack(PackArgs),
}

/// Shared args for `build` and `run`. Declaration ORDER matters: the modelled flags
/// and the positional `path` come BEFORE `rest` (the trailing passthrough), so the
/// `trailing_var_arg` Vec cannot swallow them.
#[derive(clap::Args)]
pub struct BuildArgs {
    /// The crate dir to build (default `.`).
    pub path: Option<PathBuf>,

    /// Release profile (the project default; release is used unless `--debug`).
    #[arg(long, conflicts_with = "debug")]
    pub release: bool,
    /// Debug profile (opt out of the release default).
    #[arg(long)]
    pub debug: bool,
    /// `cargo clean` first (rebuilds std; bulletproof but slow).
    #[arg(long)]
    pub clean: bool,
    /// Unfiltered build log.
    #[arg(short, long)]
    pub verbose: bool,
    /// Execution backend: `native` (default installed) or `docker` (in-repo dev).
    #[arg(long, env = "CARGO_DOTNET_BACKEND")]
    pub backend: Option<String>,
    /// Target .NET runtime version: `8` or `9` (default 8). Selects the matching CoreCLR ilasm,
    /// sets `DOTNET_VERSION` for the codegen backend + linker, and stamps the runtimeconfig / TFM /
    /// `.assembly extern .ver`.
    #[arg(long, value_name = "8|9", default_value = "8", env = "DOTNET_VERSION")]
    pub dotnet: String,

    // ---- standard cargo flag groups (clap-cargo) — forwarded to the inner cargo ----
    #[command(flatten)]
    pub features: clap_cargo::Features,
    #[command(flatten)]
    pub manifest: clap_cargo::Manifest,
    #[command(flatten)]
    pub workspace: clap_cargo::Workspace,

    /// Unknown cargo flags forwarded verbatim to the inner cargo (e.g. --locked,
    /// --offline, --frozen, --target-dir, --message-format). Hyphen values are allowed
    /// so flags pass through unmolested. Program args (after a literal `--`) are split
    /// off in `main` BEFORE clap sees them and threaded via `prog_args` — so this Vec
    /// only ever holds inner-cargo flags, never program args.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub extra: Vec<String>,

    /// Program args (everything after the first literal `--`), forwarded to the .NET
    /// program on `run`. Populated by `main`'s pre-clap argv split, NOT by clap (so it
    /// never collides with the positional `path` or the `extra` cargo flags).
    #[clap(skip)]
    pub prog_args: Vec<String>,
}

impl BuildArgs {
    /// Resolve the profile honouring the project's release-by-default convention:
    /// release UNLESS `--debug` was passed. (`--release` is accepted but redundant.)
    pub fn is_release(&self) -> bool {
        !self.debug
    }
}

/// Which scaffold template `cargo dotnet new` emits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Template {
    /// A Rust-on-.NET binary (models cd_collections).
    App,
    /// A Rust cdylib + C# consumer via export_rust_containers! (models cd_containers).
    Lib,
    /// A #[dotnet_class] managed type + C# host (models cd_typedef).
    Plugin,
}

impl Template {
    /// A short human label for the post-scaffold message.
    pub fn label(self) -> &'static str {
        match self {
            Template::App => "app (Rust-on-.NET binary)",
            Template::Lib => "lib (Rust cdylib consumed from C#)",
            Template::Plugin => "plugin (#[dotnet_class] managed type)",
        }
    }
}

#[derive(clap::Args)]
pub struct NewArgs {
    /// The directory (and, unless `--name` is given, the crate name) to scaffold into.
    pub path: PathBuf,

    /// A Rust-on-.NET binary using `mycorrhiza::prelude` (the default template).
    #[arg(long, conflicts_with_all = ["lib", "plugin"])]
    pub app: bool,
    /// A Rust cdylib exported to C# via `export_rust_containers!()` + a C# consumer.
    #[arg(long, conflicts_with_all = ["app", "plugin"])]
    pub lib: bool,
    /// A `#[dotnet_class]` managed type + a C# host that constructs it.
    #[arg(long, conflicts_with_all = ["app", "lib"])]
    pub plugin: bool,

    /// Override the crate name (default: the final path component).
    #[arg(long)]
    pub name: Option<String>,
}

impl NewArgs {
    /// Resolve the selected template. `--app` is the default when none is given.
    pub fn template(&self) -> anyhow::Result<Template> {
        match (self.app, self.lib, self.plugin) {
            (_, true, _) => Ok(Template::Lib),
            (_, _, true) => Ok(Template::Plugin),
            _ => Ok(Template::App), // --app or nothing.
        }
    }
}

#[derive(clap::Args)]
pub struct DoctorArgs {
    /// A .NET runtime error to translate: a path to a build/run log, or the message text
    /// itself. If omitted, and stdin is piped, the piped text is diagnosed; otherwise a
    /// full toolchain/backend-install environment check runs.
    pub input: Option<String>,
}

#[derive(clap::Args)]
pub struct SetupArgs {
    /// The repo checkout to build the backend from (default: this checkout, dev mode).
    #[arg(long)]
    pub from_repo: Option<PathBuf>,
    /// The install home (default: $CARGO_DOTNET_HOME or ~/.cargo-dotnet).
    #[arg(long)]
    pub home: Option<PathBuf>,
    /// The pinned nightly toolchain to provision.
    #[arg(long)]
    pub toolchain: Option<String>,
    #[arg(long)]
    pub skip_toolchain: bool,
    #[arg(long)]
    pub skip_dotnet: bool,
    #[arg(long)]
    pub skip_ilasm: bool,
    #[arg(long)]
    pub force: bool,
}

#[derive(clap::Args)]
pub struct PackArgs {
    /// The crate dir to pack (default `.`).
    pub path: Option<PathBuf>,
    #[arg(long, conflicts_with = "debug")]
    pub release: bool,
    #[arg(long)]
    pub debug: bool,
    /// Override the NuGet package id (default: the crate name).
    #[arg(long)]
    pub id: Option<String>,
    /// Override the NuGet package version (default: the crate version).
    #[arg(long)]
    pub version: Option<String>,
    /// Output directory (default: <crate>/target/nupkg).
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Target .NET runtime version for the package: `8` or `9` (default 8). Sets the build's
    /// `DOTNET_VERSION` + ilasm and the NuGet TFM (`lib/<tfm>/`), which must agree with the dll.
    #[arg(long, value_name = "8|9", default_value = "8", env = "DOTNET_VERSION")]
    pub dotnet: String,
}
