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
    /// Push an already-signed NuGet package under immutable release rules.
    Push(PushArgs),
    /// Publish a C# host project with NativeAOT, producing a standalone native binary
    /// that has the referenced Rust `<RustCrate>` compiled in (`dotnet publish
    /// -p:PublishAot=true`). See `docs/PERF_GUIDANCE.md` §5 for the proven recipe this wraps.
    Publish(PublishArgs),
    /// Fetch a NuGet package and generate Rust bindings for its public API (reflection-based,
    /// the same mechanism spinacz uses for the BCL), then wire the package's .dll into this
    /// crate's runtime output. Consuming the generated bindings needs no further ceremony.
    AddNuget(AddNugetArgs),
    /// Emit Cargo's local source/build-input closure for an MSBuild incremental target.
    MetadataInputs(MetadataInputsArgs),
    /// Reject duplicate CLR assembly names or public managed type names across Rust crates.
    ValidateManagedIdentities(ValidateManagedIdentitiesArgs),
    /// Print the CLR assembly name selected for one Rust crate (metadata identity or Cargo name).
    ManagedAssemblyName(ManagedAssemblyNameArgs),
}

#[derive(clap::Args)]
pub struct ManagedAssemblyNameArgs {
    /// Rust crate directory (default `.`).
    pub path: Option<PathBuf>,
}

#[derive(clap::Args)]
pub struct ValidateManagedIdentitiesArgs {
    /// Rust crate directories to validate as one managed host's reference set.
    #[arg(required = true, value_name = "CRATE_DIR")]
    pub crate_dirs: Vec<PathBuf>,
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
    /// Target .NET runtime version: `8`, `9`, or `10` (default 10).
    /// sets `DOTNET_VERSION` for the codegen backend + linker, and stamps the runtimeconfig / TFM /
    /// `.assembly extern .ver`.
    #[arg(long, value_name = "8|9|10", default_value = "10", env = "DOTNET_VERSION")]
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

    /// Directory to scan for workspace-wiring issues (sibling Rust crates missing a
    /// `<RustCrate>` reference, TFM/`RustDotnetVersion` mismatches). Default: `.`. Only
    /// consulted in environment-check mode (ignored when translating a failure).
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,
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
    /// Inspect the completed OPC/NuGet structure and fail before reporting success if required
    /// metadata or managed assets are missing or unsafe.
    #[arg(long)]
    pub validate: bool,
    /// Sign the completed package with this PKCS#12/PFX certificate.
    #[arg(long, value_name = "PFX")]
    pub sign_certificate: Option<PathBuf>,
    /// Name of the environment variable containing the certificate password.
    #[arg(long, value_name = "ENV", requires = "sign_certificate")]
    pub sign_password_env: Option<String>,
    /// Optional RFC3161 timestamp service used while signing.
    #[arg(long, value_name = "URL", requires = "sign_certificate")]
    pub timestamper: Option<String>,
    /// Expected SHA-256 signer certificate fingerprint (hex, separators ignored).
    #[arg(long, value_name = "SHA256", requires = "sign_certificate")]
    pub signer_fingerprint: Option<String>,
    /// Target .NET runtime version for the package: `8`, `9`, or `10` (default 10).
    /// `DOTNET_VERSION` + ilasm and the NuGet TFM (`lib/<tfm>/`), which must agree with the dll.
    #[arg(long, value_name = "8|9|10", default_value = "10", env = "DOTNET_VERSION")]
    pub dotnet: String,
}

#[derive(clap::Args)]
pub struct PushArgs {
    /// Exact `.nupkg` file to publish.
    pub package: PathBuf,
    /// Explicit NuGet source URL. Implicit configured sources are forbidden.
    #[arg(long)]
    pub source: String,
    /// Name of the environment variable containing the feed API key.
    #[arg(long, value_name = "ENV")]
    pub api_key_env: String,
    /// Expected SHA-256 signer certificate fingerprint.
    #[arg(long, value_name = "SHA256")]
    pub signer_fingerprint: String,
}

#[derive(clap::Args)]
pub struct AddNugetArgs {
    /// The NuGet package id, e.g. `Newtonsoft.Json`.
    pub id: String,
    /// The exact package version, e.g. `13.0.3`. NuGet's version-range resolution is not
    /// supported — pin an exact version (matches the reproducibility the rest of this tool's
    /// vendoring/overlay mechanisms assume).
    pub version: String,
    /// The consumer crate dir to wire the generated bindings + runtime asset into (default `.`).
    pub path: Option<PathBuf>,
    /// Re-fetch and re-generate even if this exact (id, version) is already cached.
    #[arg(long)]
    pub force: bool,
    /// Runtime identifier to restore for (for example `linux-x64`, `linux-musl-x64`,
    /// `win-x64`, or `osx-arm64`). The SDK selects the RID graph; cargo-dotnet preserves
    /// the resulting runtime/native/resource paths and provenance in its asset manifest.
    #[arg(long)]
    pub rid: Option<String>,
    /// Target framework used for SDK restore and the reflection bindgen executable.
    #[arg(long, value_name = "8|9|10", default_value = "10", env = "DOTNET_VERSION")]
    pub dotnet: String,
    /// Unfiltered build log for the bindgen step.
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(clap::Args)]
pub struct MetadataInputsArgs {
    /// The crate dir to inspect (default `.`).
    pub path: Option<PathBuf>,
    /// Newline-delimited absolute input manifest. Updated only when its contents change.
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(clap::Args)]
pub struct PublishArgs {
    /// A C# host project: either its directory (containing exactly one `.csproj`) or
    /// the `.csproj` file itself. Default `.`. The project must `<Import>`
    /// `RustDotnet.targets` and declare its `<RustCrate>` (see any `cargo_tests/cd_*/csharp`
    /// project, or scaffold one with `cargo dotnet new --app`) — `dotnet publish` builds
    /// the referenced Rust crate as part of the same invocation via that import.
    pub path: Option<PathBuf>,

    /// Debug configuration (default: Release — NativeAOT publishing a Debug build is
    /// supported by the SDK but rarely what you want).
    #[arg(long)]
    pub debug: bool,

    /// Target runtime identifier (default: the host RID, e.g. `osx-arm64`, `linux-x64`).
    /// Pass an explicit RID to cross-publish (the SDK's usual cross-compilation caveats
    /// apply — NativeAOT generally needs platform-matching build tools).
    #[arg(long)]
    pub rid: Option<String>,

    /// Unfiltered `dotnet publish` output (prints the invoked command line too).
    #[arg(short, long)]
    pub verbose: bool,

    /// Extra flags forwarded verbatim to `dotnet publish` (e.g. `-p:InvariantGlobalization=true`).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub extra: Vec<String>,
}
