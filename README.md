# rustc_codegen_clr

[![CI](https://github.com/sharifhsn/rustc_codegen_clr/actions/workflows/fork-gate.yml/badge.svg)](https://github.com/sharifhsn/rustc_codegen_clr/actions/workflows/fork-gate.yml)
[![Release](https://img.shields.io/github/v/release/sharifhsn/rustc_codegen_clr?include_prereleases)](https://github.com/sharifhsn/rustc_codegen_clr/releases)

An experimental rustc codegen backend that compiles Rust to managed .NET assemblies. The same
compiler IR can also emit C source.

> [!WARNING]
> This is compiler research, not a production toolchain. Crashes, unsupported APIs, and
> miscompilations are possible. Validate important behavior against native Rust.

## Install the 0.0.1 preview

Prerequisites: [rustup](https://rustup.rs/) and the [.NET 10 SDK](https://dotnet.microsoft.com/download/dotnet/10.0).

Linux x64 or macOS Apple Silicon:

```bash
curl -fsSL https://github.com/sharifhsn/rustc_codegen_clr/releases/download/rust-dotnet-v0.0.1/install.sh | sh
```

Windows x64 PowerShell:

```powershell
irm https://github.com/sharifhsn/rustc_codegen_clr/releases/download/rust-dotnet-v0.0.1/install.ps1 | iex
```

The installer downloads the matching host SDK bundle, verifies its checksum and host identity,
installs it under `~/.cargo-dotnet`, and installs `cargo-dotnet` under Cargo's bin directory. It does
not modify your system Rust installation.

## Run Rust on .NET

```bash
cargo dotnet doctor
cargo dotnet new hello-dotnet --app
cargo dotnet run hello-dotnet
```

The generated program is a managed .NET executable produced from Rust. Release builds are the
default; add `--debug` when needed.

For a less toy-like example:

```bash
git clone https://github.com/sharifhsn/rustc_codegen_clr
cd rustc_codegen_clr
cargo dotnet run examples/issue-dashboard
```

The issue dashboard parses JSON using managed `System.Text.Json` from Rust, then processes the
result with ordinary Rust code.

## What works

- Rust applications compiled to managed .NET executables
- Rust libraries and plugins consumed from C#
- .NET BCL and NuGet APIs called from Rust
- Managed generics, interfaces, delegates, tasks, async streams, events, arrays, and collections
- C#-friendly exported Rust APIs and deterministic NuGet packages
- MSBuild integration, Portable PDBs, Source Link, and NativeAOT publishing
- An alternate C exporter from the same compiler IR

The public 0.0.1 SDK supports one deliberately narrow configuration:

| Component | Supported |
|---|---|
| .NET | .NET 10 |
| Linux | x64 |
| macOS | Apple Silicon |
| Windows | x64 |
| Rust | pinned `nightly-2026-06-17` |

The compiler retains some older-runtime compatibility code, but .NET 8 and 9 are not supported by
the 0.0.1 SDK. A single public runtime profile keeps generated target frameworks, linker metadata,
CoreCLR tools, examples, and diagnostics consistent.

## Documentation

- [`QUICKSTART.md`](QUICKSTART.md) — installation and first run
- [`docs/CARGO_DOTNET.md`](docs/CARGO_DOTNET.md) — command reference and troubleshooting
- [`docs/QUICKSTART_INTEROP.md`](docs/QUICKSTART_INTEROP.md) — Rust and C# interop examples
- [`docs/INTEROP_COOKBOOK.md`](docs/INTEROP_COOKBOOK.md) — supported interop patterns
- [`docs/TRANSLATION_STATUS.md`](docs/TRANSLATION_STATUS.md) — compiler coverage and semantic limits
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — compiler pipeline and design
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contributor setup and test selection

Questions and project ideas are welcome in [GitHub Discussions](https://github.com/sharifhsn/rustc_codegen_clr/discussions).
Please report compiler bugs, installation failures, and miscompilations through
[GitHub Issues](https://github.com/sharifhsn/rustc_codegen_clr/issues).

## Build from source

```bash
git clone https://github.com/sharifhsn/rustc_codegen_clr
cd rustc_codegen_clr
cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml -- setup --from-repo "$PWD"
cargo dotnet doctor
```

The repository pins the rustc nightly and required compiler components in `rust-toolchain.toml`.

## License

Dual-licensed under [MIT](LICENSE) or [Apache-2.0](LICENSE-Apache).
