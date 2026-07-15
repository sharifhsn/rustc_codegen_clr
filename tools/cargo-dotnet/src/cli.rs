//! The clap derive CLI surface for `cargo dotnet`.
//!
//! Two parse paths share these structs (see `main.rs`):
//!   * `cargo dotnet <cmd>` (real cargo dispatch) -> argv `[cargo-dotnet, dotnet, <cmd>, ...]`
//!   * `cargo-dotnet <cmd>` (direct, e.g. dev.sh / MSBuild) -> argv `[cargo-dotnet, <cmd>, ...]`
//!
//! `main` peeks `argv[1]` and drops a leading `dotnet`, then parses `DotnetCli` directly.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// The `cargo`-subcommand wrapper. Under genuine `cargo dotnet` dispatch, cargo
/// prepends the subcommand name (`dotnet`) as `argv[1]`; this enum absorbs it. It is
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
    /// Show honest host compatibility profiles and their current support level.
    Profiles(ProfilesArgs),
    /// Validate the product capability manifest and generate a human-readable report.
    Capabilities(CapabilitiesArgs),
    /// Populate and verify the private Cargo cache and injected sysroot for later offline builds.
    Restore(BuildArgs),
    /// Build a Rust crate into a .NET assembly (exe apphost or C#-referenceable .dll).
    Build(BuildArgs),
    /// Build a Rust crate and run it on .NET (forwards exit code; args after `--`).
    Run(BuildArgs),
    /// Scaffold a Rust-on-.NET app, library, or product host.
    New(NewArgs),
    /// Attach a schema-1 managed Rust crate to an existing SDK-style C# project.
    Attach(AttachArgs),
    /// Diagnose the toolchain, or translate a .NET runtime failure into an actionable fix.
    Doctor(DoctorArgs),
    /// Build a crate's #[test]s with the backend and run them on .NET.
    Test(BuildArgs),
    /// Provision the toolchain + install home, then install this binary to ~/.cargo/bin.
    Setup(SetupArgs),
    /// Create, verify, or atomically install a checksummed repo-independent SDK bundle.
    Bundle(BundleArgs),
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
    /// Fetch and stage a native-only NuGet package for P/Invoke.
    AddNative(AddNativeArgs),
    /// Vendor a local native library into the crate for build, run, and pack.
    AddNativeFile(AddNativeFileArgs),
    /// Generate ordinary Rust P/Invoke declarations from a C header.
    Bindgen(BindgenArgs),
    /// Emit Cargo's local source/build-input closure for an MSBuild incremental target.
    MetadataInputs(MetadataInputsArgs),
    /// Reject duplicate CLR assembly names or public managed type names across Rust crates.
    ValidateManagedIdentities(ValidateManagedIdentitiesArgs),
    /// Print the CLR assembly name selected for one Rust crate (metadata identity or Cargo name).
    ManagedAssemblyName(ManagedAssemblyNameArgs),
}

#[derive(clap::Args)]
pub struct ProfilesArgs {
    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum CapabilitiesFormat {
    Markdown,
    Json,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum CapabilitiesEvidenceScope {
    Presubmit,
    Release,
}

#[derive(clap::Args)]
pub struct CapabilitiesArgs {
    /// Capability manifest to validate and report.
    #[arg(long, default_value = "acceptance/capabilities.toml")]
    pub manifest: PathBuf,
    /// Acceptance result TSV. Repeat to merge independent matrix/script evidence files.
    #[arg(long)]
    pub results: Vec<PathBuf>,
    /// Exit nonzero unless every presubmit journey has complete passing runtime/profile evidence.
    /// The report is still written, so CI retains the diagnostic artifact.
    #[arg(long)]
    pub strict: bool,
    /// Runtime/profile coverage contract used to classify PASS/PARTIAL and by `--strict`.
    #[arg(long, value_enum, default_value = "presubmit")]
    pub evidence_scope: CapabilitiesEvidenceScope,
    /// Output format.
    #[arg(long, value_enum, default_value = "markdown")]
    pub format: CapabilitiesFormat,
    /// Write the report to a file instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

#[derive(clap::Args)]
pub struct BundleArgs {
    #[command(subcommand)]
    pub command: BundleCommand,
}

#[derive(Subcommand)]
pub enum BundleCommand {
    /// Archive a provisioned CARGO_DOTNET_HOME plus this cargo-dotnet executable.
    Create {
        /// Provisioned install home (default: CARGO_DOTNET_HOME or ~/.cargo-dotnet).
        #[arg(long)]
        home: Option<PathBuf>,
        /// Destination .zip file.
        #[arg(long)]
        out: PathBuf,
    },
    /// Verify structure, host metadata, sizes, and SHA-256 hashes without installing.
    Verify {
        /// Bundle .zip to verify.
        archive: PathBuf,
    },
    /// Verify and atomically restore a bundle into CARGO_DOTNET_HOME.
    Install {
        /// Bundle .zip to install.
        archive: PathBuf,
        /// Destination install home (default: CARGO_DOTNET_HOME or ~/.cargo-dotnet).
        #[arg(long)]
        home: Option<PathBuf>,
        /// Replace an existing install home after the replacement verifies successfully.
        #[arg(long)]
        force: bool,
        /// Do not copy the bundled cargo-dotnet executable into CARGO_HOME/bin.
        #[arg(long)]
        no_install_cli: bool,
    },
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
    /// Target .NET runtime version. The 0.0.1 SDK supports `10`.
    /// sets `DOTNET_VERSION` for the codegen backend + linker, and stamps the runtimeconfig / TFM /
    /// `.assembly extern .ver`.
    #[arg(long, value_name = "10", default_value = "10", env = "DOTNET_VERSION")]
    pub dotnet: String,

    /// HTTPS Source Link template for this crate's remapped `/_/consumer/*` documents.
    /// Include exactly one `*`, for example
    /// `https://raw.githubusercontent.com/org/repo/<commit>/*`.
    #[arg(
        long,
        value_name = "HTTPS_URL_WITH_*",
        env = "CARGO_DOTNET_SOURCE_LINK_URL"
    )]
    pub source_link_url: Option<String>,

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
    /// A `#[dotnet_class]` managed type + C# host (models cd_typedef).
    Plugin,
    /// An Excel-DNA add-in whose worksheet functions call managed Rust exports.
    Excel,
    /// A .NET MAUI application with an initially Windows-only managed Rust backend.
    Maui,
    /// An unpackaged WinUI 3 application with a managed Rust backend.
    Winui,
    /// An ASP.NET Core minimal API with a managed Rust backend.
    WebApi,
    /// A .NET Generic Host worker service with a managed Rust backend.
    Worker,
}

impl Template {
    /// A short human label for the post-scaffold message.
    pub fn label(self) -> &'static str {
        match self {
            Template::App => "app (Rust-on-.NET binary)",
            Template::Lib => "lib (Rust cdylib consumed from C#)",
            Template::Plugin => "plugin (#[dotnet_class] managed type)",
            Template::Excel => "Excel-DNA add-in (worksheet functions backed by managed Rust)",
            Template::Maui => "MAUI Windows app (managed Rust backend)",
            Template::Winui => "WinUI 3 app (managed Rust backend)",
            Template::WebApi => "ASP.NET Core Web API (managed Rust backend)",
            Template::Worker => ".NET worker service (managed Rust backend)",
        }
    }
}

const NEW_TEMPLATE_FLAGS: &[&str] = &[
    "app", "lib", "plugin", "excel", "maui", "winui", "webapi", "worker",
];

#[derive(clap::Args)]
pub struct NewArgs {
    /// The directory (and, unless `--name` is given, the crate name) to scaffold into.
    pub path: PathBuf,

    /// A Rust-on-.NET binary using `mycorrhiza::prelude` (the default template).
    #[arg(long, conflicts_with_all = ["lib", "plugin", "excel", "maui", "winui", "webapi", "worker"])]
    pub app: bool,
    /// A Rust cdylib exported to C# via `export_rust_containers!()` + a C# consumer.
    #[arg(long, conflicts_with_all = ["app", "plugin", "excel", "maui", "winui", "webapi", "worker"])]
    pub lib: bool,
    /// A `#[dotnet_class]` managed type + a C# host that constructs it.
    #[arg(long, conflicts_with_all = ["app", "lib", "excel", "maui", "winui", "webapi", "worker"])]
    pub plugin: bool,
    /// A Windows Excel-DNA add-in with worksheet functions backed by managed Rust.
    #[arg(long, conflicts_with_all = ["app", "lib", "plugin", "maui", "winui", "webapi", "worker"])]
    pub excel: bool,
    /// A Windows-first .NET MAUI app. Android/iOS/Mac Catalyst remain unsupported until gated.
    #[arg(long, conflicts_with_all = ["app", "lib", "plugin", "excel", "winui", "webapi", "worker"])]
    pub maui: bool,
    /// An unpackaged WinUI 3 desktop app for Windows.
    #[arg(long, conflicts_with_all = ["app", "lib", "plugin", "excel", "maui", "webapi", "worker"])]
    pub winui: bool,
    /// An ASP.NET Core minimal API with Rust business logic compiled to managed .NET.
    #[arg(long, conflicts_with_all = ["app", "lib", "plugin", "excel", "maui", "winui", "worker"])]
    pub webapi: bool,
    /// A .NET worker service with Rust business logic compiled to managed .NET.
    #[arg(long, conflicts_with_all = ["app", "lib", "plugin", "excel", "maui", "winui", "webapi"])]
    pub worker: bool,

    /// Override the crate name (default: the final path component).
    #[arg(long)]
    pub name: Option<String>,

    /// Runtime profile embedded in generated C# projects and printed run commands.
    #[arg(
        long,
        value_name = "10",
        default_value = "10",
        value_parser = ["10"],
        env = "DOTNET_VERSION"
    )]
    pub dotnet: String,
}

impl NewArgs {
    /// Resolve the selected template. `--app` is the default when none is given.
    pub fn template(&self) -> anyhow::Result<Template> {
        let selected = [
            (self.app, Template::App),
            (self.lib, Template::Lib),
            (self.plugin, Template::Plugin),
            (self.excel, Template::Excel),
            (self.maui, Template::Maui),
            (self.winui, Template::Winui),
            (self.webapi, Template::WebApi),
            (self.worker, Template::Worker),
        ];
        let mut templates = selected
            .into_iter()
            .filter_map(|(enabled, template)| enabled.then_some(template));
        let template = templates.next().unwrap_or(Template::App);
        if templates.next().is_some() {
            anyhow::bail!(
                "select exactly one template flag: {}",
                NEW_TEMPLATE_FLAGS
                    .iter()
                    .map(|flag| format!("--{flag}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Ok(template)
    }
}

#[derive(clap::Args)]
pub struct AttachArgs {
    /// Existing SDK-style C# project to update.
    pub project: PathBuf,

    /// Rust cdylib directory containing Cargo.toml and package.metadata.dotnet schema 1.
    #[arg(long, value_name = "PATH")]
    pub rust_crate: PathBuf,

    /// Include the shipped RustVec<T>/RustBoxVec<T> C# wrappers.
    #[arg(long)]
    pub containers: bool,

    /// Print the exact managed block without modifying the project.
    #[arg(long)]
    pub dry_run: bool,
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

    /// Runtime profile whose installation and workspace wiring should be checked.
    #[arg(
        long,
        value_name = "10",
        default_value = "10",
        value_parser = ["10"],
        env = "DOTNET_VERSION"
    )]
    pub dotnet: String,

    /// Emit a stable, machine-readable JSON report instead of human-oriented text.
    /// The command's exit code is unchanged: environment reports return 1 when a
    /// required check fails, while failure-translation reports return 0.
    #[arg(long)]
    pub json: bool,
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
    /// Output directory (default: `<crate>/target/nupkg`).
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
    /// Target .NET runtime version for the package. The 0.0.1 SDK supports `10`.
    /// `DOTNET_VERSION` + ilasm and the NuGet TFM (`lib/<tfm>/`), which must agree with the dll.
    #[arg(long, value_name = "10", default_value = "10", env = "DOTNET_VERSION")]
    pub dotnet: String,
    /// HTTPS Source Link template embedded in the package's Portable PDB.
    #[arg(
        long,
        value_name = "HTTPS_URL_WITH_*",
        env = "CARGO_DOTNET_SOURCE_LINK_URL"
    )]
    pub source_link_url: Option<String>,
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
    /// NuGet package source to use for restore. Repeat to provide the complete source set; when
    /// present, these override sources from NuGet.Config (matching `dotnet restore --source`).
    #[arg(long, value_name = "PATH_OR_URL")]
    pub source: Vec<String>,
    /// Target framework used for SDK restore and the reflection bindgen executable.
    #[arg(long, value_name = "10", default_value = "10", env = "DOTNET_VERSION")]
    pub dotnet: String,
    /// Unfiltered build log for the bindgen step.
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(clap::Args)]
pub struct AddNativeArgs {
    pub id: String,
    pub version: String,
    /// Logical library name used by the Rust `#[link(name = "...")]` declaration.
    #[arg(long)]
    pub library: String,
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub rid: Option<String>,
    #[arg(long, value_name = "10", default_value = "10", env = "DOTNET_VERSION")]
    pub dotnet: String,
}

#[derive(clap::Args)]
pub struct AddNativeFileArgs {
    /// Native library file to vendor.
    pub file: PathBuf,
    /// Logical library name used by `#[link(name = "...")]`.
    #[arg(long)]
    pub library: String,
    /// Consumer crate directory (default `.`).
    #[arg(long)]
    pub path: Option<PathBuf>,
    /// Runtime identifier for this binary (default current host).
    #[arg(long)]
    pub rid: Option<String>,
}

#[derive(clap::Args)]
pub struct BindgenArgs {
    /// C header to parse, relative to the consumer crate unless absolute.
    pub header: PathBuf,
    /// Logical library name written into generated `#[link(name = "...")]` attributes.
    #[arg(long)]
    pub library: String,
    /// Consumer crate directory (default `.`).
    #[arg(long)]
    pub path: Option<PathBuf>,
    /// Generated Rust module, relative to the consumer crate.
    #[arg(short, long, default_value = "src/native.rs")]
    pub output: PathBuf,
    /// Regex selecting C functions. Repeat to build an allowlist.
    #[arg(long)]
    pub allowlist_function: Vec<String>,
    /// Regex selecting C types. Repeat to build an allowlist.
    #[arg(long)]
    pub allowlist_type: Vec<String>,
    /// Regex excluding an item. Repeat as needed.
    #[arg(long)]
    pub blocklist_item: Vec<String>,
    /// Argument forwarded to libclang, such as `-Ivendor/include`. Repeat as needed.
    #[arg(long, allow_hyphen_values = true)]
    pub clang_arg: Vec<String>,
    /// Derive `Default` where bindgen can do so safely.
    #[arg(long)]
    pub derive_default: bool,
    /// Emit bindgen layout tests. Disabled by default for product builds.
    #[arg(long)]
    pub layout_tests: bool,
    /// Fail if the checked-in output differs, without rewriting it.
    #[arg(long)]
    pub check: bool,
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
    /// project, or scaffold one with `cargo dotnet new --lib` / `--plugin`) — `dotnet publish` builds
    /// the referenced Rust crate as part of the same invocation via that import.
    pub path: Option<PathBuf>,

    /// Target .NET runtime profile. Controls the Rust contract, host TFM, and reported output.
    #[arg(long, value_name = "10", default_value = "10", env = "DOTNET_VERSION")]
    pub dotnet: String,

    /// Debug configuration (default: Release — NativeAOT publishing a Debug build is
    /// supported by the SDK but rarely what you want).
    #[arg(long)]
    pub debug: bool,

    /// Target runtime identifier (default: the host RID, e.g. `osx-arm64`, `linux-x64`).
    /// Pass an explicit RID to cross-publish (the SDK's usual cross-compilation caveats
    /// apply — NativeAOT generally needs platform-matching build tools).
    #[arg(long)]
    pub rid: Option<String>,

    /// Write the publish tree to this directory instead of the SDK's
    /// `bin/<configuration>/<tfm>/<rid>/publish` default.
    #[arg(short = 'o', long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Unfiltered `dotnet publish` output (prints the invoked command line too).
    #[arg(short, long)]
    pub verbose: bool,

    /// Extra flags forwarded verbatim to `dotnet publish` (e.g. `-p:InvariantGlobalization=true`).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub extra: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_excel_selects_the_excel_dna_template() {
        let cli =
            DotnetCli::try_parse_from(["cargo-dotnet", "new", "risk-engine", "--excel"]).unwrap();
        let Cmd::New(args) = cli.cmd else {
            panic!("new --excel did not parse to the new command")
        };
        assert_eq!(args.template().unwrap(), Template::Excel);
        assert_eq!(args.dotnet, "10");
    }

    #[test]
    fn new_product_host_flags_select_their_templates() {
        for (flag, expected) in [
            ("--maui", Template::Maui),
            ("--winui", Template::Winui),
            ("--webapi", Template::WebApi),
            ("--worker", Template::Worker),
        ] {
            let cli = DotnetCli::try_parse_from(["cargo-dotnet", "new", "demo", flag]).unwrap();
            let Cmd::New(args) = cli.cmd else {
                panic!("new {flag} did not parse to the new command")
            };
            assert_eq!(args.template().unwrap(), expected);
        }
    }

    #[test]
    fn new_rejects_multiple_template_flags() {
        let error = match DotnetCli::try_parse_from([
            "cargo-dotnet",
            "new",
            "demo",
            "--webapi",
            "--worker",
        ]) {
            Ok(_) => panic!("two template flags unexpectedly parsed"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn add_native_cli_keeps_package_version_library_and_path_explicit() {
        let cli = DotnetCli::try_parse_from([
            "cargo-dotnet",
            "add-native",
            "SQLitePCLRaw.lib.e_sqlite3",
            "3.53.3",
            "--library",
            "e_sqlite3",
            "--rid",
            "osx-arm64",
            "fixture",
        ])
        .unwrap();
        let Cmd::AddNative(args) = cli.cmd else {
            panic!("add-native did not parse to its command variant")
        };
        assert_eq!(args.id, "SQLitePCLRaw.lib.e_sqlite3");
        assert_eq!(args.version, "3.53.3");
        assert_eq!(args.library, "e_sqlite3");
        assert_eq!(args.rid.as_deref(), Some("osx-arm64"));
        assert_eq!(args.path.as_deref(), Some(std::path::Path::new("fixture")));
    }

    #[test]
    fn bindgen_cli_keeps_generation_policy_explicit() {
        let cli = DotnetCli::try_parse_from([
            "cargo-dotnet",
            "bindgen",
            "vendor/sqlite3.h",
            "--library",
            "e_sqlite3",
            "--allowlist-function",
            "sqlite3_.*",
            "--clang-arg=-Ivendor",
        ])
        .unwrap();
        let Cmd::Bindgen(args) = cli.cmd else {
            panic!("bindgen did not parse to its command variant")
        };
        assert_eq!(args.library, "e_sqlite3");
        assert_eq!(args.output, PathBuf::from("src/native.rs"));
        assert_eq!(args.allowlist_function, ["sqlite3_.*"]);
        assert_eq!(args.clang_arg, ["-Ivendor"]);
    }

    #[test]
    fn local_native_file_cli_requires_its_logical_name() {
        let cli = DotnetCli::try_parse_from([
            "cargo-dotnet",
            "add-native-file",
            "vendor/libsample.dylib",
            "--library",
            "sample",
            "--rid",
            "osx-arm64",
        ])
        .unwrap();
        let Cmd::AddNativeFile(args) = cli.cmd else {
            panic!("add-native-file did not parse to its command variant")
        };
        assert_eq!(args.library, "sample");
        assert_eq!(args.rid.as_deref(), Some("osx-arm64"));
    }
}
