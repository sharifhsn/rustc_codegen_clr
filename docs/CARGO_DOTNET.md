# `cargo dotnet` reference

`cargo dotnet` is the supported user interface for `rustc_codegen_clr`. It owns compiler setup,
the private patched sysroot, target configuration, artifact discovery, managed packaging, and
execution. Users should not need to construct `RUSTFLAGS` or configure `build-std` manually.

## Supported configuration

The 0.0.1 SDK supports .NET 10 on Linux x64, macOS Apple Silicon, and Windows x64. Commands accept
`--dotnet 10` for explicit scripts, but it is optional because 10 is the only public profile.
Passing 8 or 9 fails immediately with an actionable diagnostic.

The compiler contains compatibility implementation for older runtimes, which is useful for
development and archaeology, but it is not part of this release contract.

## Commands

```text
cargo dotnet setup --from-repo PATH
cargo dotnet profiles [--json]
cargo dotnet doctor [MESSAGE_OR_LOG] [--workspace PATH] [--json]
cargo dotnet new PATH --app|--lib|--plugin|--excel|--webapi|--worker|--winui|--maui
cargo dotnet attach HOST.csproj --rust-crate PATH [--containers] [--dry-run]
cargo dotnet build [PATH]
cargo dotnet run [PATH] [-- PROGRAM_ARGS...]
cargo dotnet test [PATH]
cargo dotnet restore [PATH] [--locked|--offline|--frozen]
cargo dotnet pack [PATH] [--out DIR] [--validate]
cargo dotnet push PACKAGE --source URL
cargo dotnet publish CSPROJ_OR_DIR [--rid RID] [--output DIR]
cargo dotnet add-nuget PACKAGE VERSION [PATH] [--source URL]
cargo dotnet add-native PACKAGE VERSION --library NAME [PATH] [--rid RID]
cargo dotnet add-native-file FILE --library NAME [--path PATH] [--rid RID]
cargo dotnet bindgen HEADER --library NAME [--path PATH] [--output FILE]
cargo dotnet bundle create --home PATH --out SDK.zip
cargo dotnet bundle verify SDK.zip
cargo dotnet bundle install SDK.zip [--home PATH] [--force]
cargo dotnet capabilities --manifest acceptance/capabilities.toml
```

Run `cargo dotnet <command> --help` for the complete flags accepted by a command.

With `--workspace <crate>`, `doctor` also inventories every `#[link(name = ...)]` block and its
effective native entry points. For staged native assets it validates the host RID, binary
architecture, and export table before launch. An unstaged library is reported as a warning because
it may intentionally come from the operating system; a staged wrong-RID, wrong-architecture, or
missing-export binary is a hard failure with the Rust symbol and native entry point named.

The backend rejects unsafe-to-project P/Invoke declarations while compiling, before CoreCLR can
load or invoke them. The portable raw boundary accepts C integer and floating-point scalars,
`usize`/`isize`, raw pointers, `()` returns, and fixed-signature `extern "C"` callbacks (including
nullable `Option<extern "C" fn(...)>` callbacks). Rust references, slices, `bool`, `char`, by-value
aggregates, variadics, and callbacks with a non-C ABI produce a diagnostic naming the library, Rust
symbol, effective `#[link_name]`, offending position and type, plus the supported replacement. Put
rich data behind a raw pointer to a `#[repr(C)]` DTO or expose a fixed-signature C shim; use the
safe types and callback guards in `rust-dotnet-pinvoke` above that raw declaration.

## Common build flags

| Flag | Meaning |
|---|---|
| `--debug` | Build the Rust and managed artifacts in debug mode |
| `--release` | Select release mode explicitly; this is the default |
| `--clean` | Remove mode-specific outputs before building |
| `--offline` | Require Cargo and NuGet inputs to be available locally |
| `--frozen` | Require both offline and locked dependency state |
| `--locked` | Require the current lockfile |
| `--source-link-url URL_WITH_*` | Map consumer PDB documents to an immutable source URL |
| `--backend native` | Use the installed host backend; this is the installed default |
| `--backend docker` | Use the contributor Docker path from a checkout |

## Installation layout

The release installer places the SDK under `CARGO_DOTNET_HOME`, defaulting to
`$HOME/.cargo-dotnet`, and the command under `CARGO_HOME/bin`, defaulting to `$HOME/.cargo/bin`.

The SDK bundle contains:

- the host compiler backend and linker;
- the pinned target specification and toolchain identity;
- the .NET PAL and Cargo overlays;
- `mycorrhiza`, `dotnet_macros`, and managed helper sources;
- MSBuild integration; and
- the matching `cargo-dotnet` executable.

It deliberately does not contain rustup or the .NET SDK.

Bundle installation verifies the adjacent `.sha256`, validates the internal per-file hashes and
host OS/architecture, restores into a temporary directory, then atomically activates the SDK.
Installed files are integrity-checked before later builds.

## Scaffolds

`cargo dotnet new` creates one of eight project shapes:

- `--app`: a Rust application compiled to a managed executable;
- `--lib`: a Rust library plus a C# consumer and MSBuild integration;
- `--plugin`: a Rust implementation consumed through a managed interface;
- `--excel`: a Windows Excel-DNA `net10.0-windows` add-in whose attributed worksheet functions
  call managed Rust exports directly;
- `--webapi`: an ASP.NET Core minimal API with a schema-1 managed Rust backend;
- `--worker`: a Generic Host worker service with the same backend contract;
- `--winui`: an unpackaged Windows WinUI 3 application; or
- `--maui`: a Windows-only MAUI application. Android, iOS, and Mac Catalyst TFMs are not generated
  until their packaging and runtime gates pass.

The general, Web API, and Worker scaffolds target `net10.0`; the desktop product hosts target their
explicit Windows TFMs. Each prints its exact next command. `--excel` uses stable Excel-DNA 1.9.0,
generates a 64-bit packed `.xll`, and is deliberately not presented as VSTO or Office-for-macOS
support. Web API and Worker have runtime acceptance on the supported CoreCLR hosts. WinUI and MAUI
remain planned profiles until Windows CI builds and launches them with the required workloads.

Run `cargo dotnet profiles` for the current host-by-host support contract. Profile names describe a
real loader/runtime contract, not merely a target framework string; planned Unity and mobile
profiles are listed but rejected as supported until their runtime acceptance passes.

For an existing SDK-style project, `cargo dotnet attach` inserts one clearly marked,
idempotent block containing the validated compatibility profile, `<RustCrate>`, and conditional
`RustDotnet.targets` imports. The Rust crate must declare managed identity schema 1. Attachment
rejects profile/TFM mismatches and pre-existing hand-authored wiring instead of rewriting or
guessing around it. `--dry-run` prints the exact block without changing the project.

## Build and run

```bash
cargo dotnet build ./crate
cargo dotnet run ./crate -- arg1 arg2
cargo dotnet test ./crate
```

The driver builds a private sysroot for the pinned nightly, invokes rustc with the codegen backend,
and writes the managed PE, Portable PDB, runtime configuration, and an artifact identity receipt.
The normal direct-PE path, including schema-1 projected assembly/type identity, does not require
ILAsm. Set `DIRECT_PE=0` only when intentionally using the legacy IL exporter as a debugging escape
hatch.

## Rust and C# interop

For C# consumption, start with a `--lib` or `--plugin` scaffold. Existing C# projects can import
`$CARGO_DOTNET_HOME/msbuild/RustDotnet.targets` and declare a `<RustCrate>` item. See
[`INTEROP_CSHARP.md`](INTEROP_CSHARP.md).

For Rust calling managed APIs, `mycorrhiza` provides typed BCL wrappers and `add-nuget` generates
bindings from NuGet's resolved asset graph. See [`QUICKSTART_INTEROP.md`](QUICKSTART_INTEROP.md) and
[`INTEROP_COOKBOOK.md`](INTEROP_COOKBOOK.md).

For Rust calling a native C ABI library, use an ordinary `#[link] unsafe extern` declaration.
`add-native` restores and stages host-RID files from a native NuGet package; see the mdBook's
[`Call native libraries from Rust`](../book/src/interop/native-from-rust.md) guide.
`add-native-file` vendors an unpublished binary by RID, while `bindgen` generates checked-in Rust
declarations from a C header and supports `--check` for CI.

## NuGet packages

```bash
cargo dotnet pack ./rust-library --out ./packages --validate
cargo dotnet push ./packages/Example.0.0.1.nupkg --source https://example.invalid/v3/index.json
```

Packages include the managed assembly, XML documentation, Portable PDB, Source Link information,
README and Cargo license/repository metadata, provenance, SBOM, and license inventory. The
`build/rustdotnet/package-metadata.json` contract records the exact package and assembly identity,
target framework, compatibility-profile evidence state and host RIDs, RIDs with included native
assets, Source Link template, sidecar inventory, and one owner/RID/path notice for every native
dependency. `--validate` cross-checks that contract against the actual NuGet entries.

Acceptance restores and executes four clean package shapes: a portable managed package, real
transitive NuGet dependencies, the bundled Mycorrhiza helper assembly, and an SDK-selected RID
layout containing managed, native, and satellite-resource assets. Identical inputs remain
byte-for-byte deterministic.

## Offline use

```bash
cargo dotnet restore ./crate
cargo dotnet build ./crate --offline --frozen
```

Restore writes a checksummed receipt covering dependency manifests, lock/configuration files,
private sysroot inputs, and package caches. Source-only edits do not invalidate it; dependency or
cache changes produce a re-restore command before compilation begins.

## Diagnostics

Start with:

```bash
cargo dotnet doctor
cargo dotnet doctor --json
```

When a C# project references a schema-1 Rust crate, `doctor --workspace` compares the crate's
compatibility profile with `RustDotnetCompatibilityProfile`, the project TFM(s), product markers,
and the current host RID. It rejects older/newer CoreCLR contracts, in-process VSTO, unproven Unity
`netstandard2.1`, MAUI/WinUI profile misuse, and every planned or unsupported profile before the
loader runs. Preview profiles remain non-fatal warnings and name their missing host-execution gate.

You can also pass an exception, linker message, or log file to `doctor`. It recognizes common
runtime/profile mismatches, unsupported managed layouts, stale artifacts, missing entry points,
and installation problems.

When reporting a compiler problem, include:

- host OS and architecture;
- `cargo dotnet --version`;
- `dotnet --info`;
- `rustup show active-toolchain`;
- `cargo dotnet doctor --json`; and
- a small Rust program plus native and managed output.

Use the GitHub miscompilation issue template when generated behavior differs from native Rust.
