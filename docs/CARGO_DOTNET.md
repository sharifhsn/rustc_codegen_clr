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
cargo dotnet new PATH --app|--lib|--plugin|--excel
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

`cargo dotnet new` creates one of four project shapes:

- `--app`: a Rust application compiled to a managed executable;
- `--lib`: a Rust library plus a C# consumer and MSBuild integration; or
- `--plugin`: a Rust implementation consumed through a managed interface; or
- `--excel`: a Windows Excel-DNA `net10.0-windows` add-in whose attributed worksheet functions
  call managed Rust exports directly.

The general scaffolds target `net10.0`; the Excel host targets `net10.0-windows`. Each prints its
exact next command. `--excel` uses stable Excel-DNA 1.9.0, generates a 64-bit packed `.xll`, and is
deliberately not presented as VSTO or Office-for-macOS support.

Run `cargo dotnet profiles` for the current host-by-host support contract. Profile names describe a
real loader/runtime contract, not merely a target framework string; planned Unity and mobile
profiles are listed but rejected as supported until their runtime acceptance passes.

## Build and run

```bash
cargo dotnet build ./crate
cargo dotnet run ./crate -- arg1 arg2
cargo dotnet test ./crate
```

The driver builds a private sysroot for the pinned nightly, invokes rustc with the codegen backend,
and writes the managed PE, Portable PDB, runtime configuration, and an artifact identity receipt.
The normal direct-PE path does not require ILAsm; the legacy IL exporter remains a debugging escape
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
license/provenance metadata, and deterministic contents for identical inputs.

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
