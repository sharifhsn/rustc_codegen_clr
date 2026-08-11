# Quickstart: run Rust on .NET

`rustc_codegen_clr` is experimental compiler infrastructure. The unpublished 0.0.2 release
candidate intentionally has one supported runtime profile: .NET 10.

## Prerequisites

- [rustup](https://rustup.rs/)
- [.NET 10 SDK](https://dotnet.microsoft.com/download/dotnet/10.0)
- Linux x64, macOS Apple Silicon, or Windows x64

## Provision from source (Linux/macOS)

```bash
git clone https://github.com/sharifhsn/rustc_codegen_clr
cd rustc_codegen_clr
cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml -- setup --from-repo "$PWD"
cargo dotnet doctor
```

There is no `rust-dotnet-v0.0.2` release tag yet, so release-asset installer URLs are intentionally
not advertised. A published release will provide host-specific bundles and checksums; until then,
the command above captures and installs an inspectable source snapshot on Linux x64 or macOS Apple
Silicon. Windows x64 is exercised by the release-candidate gates, but checkout setup still delegates
to a POSIX shell script; use the signed Windows bundle once it is published.

If `cargo dotnet` is not found afterward, open a new terminal or add Cargo's bin directory to PATH:

- Linux/macOS: `$HOME/.cargo/bin`
- Windows: `%USERPROFILE%\.cargo\bin`

## Check the installation

```bash
cargo dotnet doctor
```

`doctor` checks the SDK bundle, pinned nightly, .NET 10 runtime, compiler backend, linker, and the
current workspace. Its `--json` output is suitable for bug reports.

## Create and run an application

```bash
cargo dotnet new hello-dotnet --app
cargo dotnet run hello-dotnet
```

Release mode is the default. Use `--debug` for a debug build:

```bash
cargo dotnet run hello-dotnet --debug
```

Existing Cargo crates work the same way:

```bash
cargo dotnet build ./my-crate
cargo dotnet run ./my-crate -- arg1 arg2
cargo dotnet test ./my-crate
```

## Create a Rust library for C#

```bash
cargo dotnet new hello-library --lib
dotnet run --project hello-library/csharp
```

The scaffold contains the Rust library, generated managed assembly, MSBuild wiring, and a C#
consumer. The `--plugin` template creates the corresponding interface/plugin shape.

## Work offline after restoring

```bash
cargo dotnet restore ./my-crate
cargo dotnet run ./my-crate --offline --frozen
```

The restore receipt detects dependency or cache changes and tells you when another online restore
is required.

## Build from a checkout

After a release is published, bundles are the normal user path. On Linux x64 or macOS Apple
Silicon, a checkout can be provisioned directly:

```bash
cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml -- setup --from-repo "$PWD"
cargo dotnet doctor
```

## Next steps

- [`docs/CARGO_DOTNET.md`](docs/CARGO_DOTNET.md) — complete command reference
- [`docs/QUICKSTART_INTEROP.md`](docs/QUICKSTART_INTEROP.md) — call .NET from Rust and Rust from C#
- [`examples/issue-dashboard`](examples/issue-dashboard/README.md) — application-shaped example
- [`docs/TRANSLATION_STATUS.md`](docs/TRANSLATION_STATUS.md) — known compiler limits
