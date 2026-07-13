# Quickstart: run Rust on .NET

`rustc_codegen_clr` is experimental compiler infrastructure. The 0.0.1 preview intentionally has
one supported runtime profile: .NET 10.

## Prerequisites

- [rustup](https://rustup.rs/)
- [.NET 10 SDK](https://dotnet.microsoft.com/download/dotnet/10.0)
- Linux x64, macOS Apple Silicon, or Windows x64

## Install

Linux or macOS:

```bash
curl -fsSL https://github.com/sharifhsn/rustc_codegen_clr/releases/download/rust-dotnet-v0.0.1/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://github.com/sharifhsn/rustc_codegen_clr/releases/download/rust-dotnet-v0.0.1/install.ps1 | iex
```

For an inspectable install, download the script first, read it, and run it locally. The installer
downloads a host-specific ZIP and checksum, then uses the standalone `cargo-dotnet` executable to
verify and install the bundle.

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

Release bundles are the normal user path. Compiler contributors can provision directly from a
checkout:

```bash
cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml -- setup --from-repo "$PWD"
cargo dotnet doctor
```

## Next steps

- [`docs/CARGO_DOTNET.md`](docs/CARGO_DOTNET.md) — complete command reference
- [`docs/QUICKSTART_INTEROP.md`](docs/QUICKSTART_INTEROP.md) — call .NET from Rust and Rust from C#
- [`examples/issue-dashboard`](examples/issue-dashboard/README.md) — application-shaped example
- [`docs/TRANSLATION_STATUS.md`](docs/TRANSLATION_STATUS.md) — known compiler limits
