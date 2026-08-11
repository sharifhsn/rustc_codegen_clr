# rustc_codegen_clr

[![CI](https://github.com/sharifhsn/rustc_codegen_clr/actions/workflows/fork-gate.yml/badge.svg)](https://github.com/sharifhsn/rustc_codegen_clr/actions/workflows/fork-gate.yml)
[![Release](https://img.shields.io/github/v/release/sharifhsn/rustc_codegen_clr?include_prereleases)](https://github.com/sharifhsn/rustc_codegen_clr/releases)

An experimental rustc codegen backend that compiles Rust to managed .NET assemblies.

This fork preserves the lineage of [FractalFir's original research
backend](https://github.com/FractalFir/rustc_codegen_clr) and develops an independently tested,
fail-closed .NET SDK path around it. It is not presented as FractalFir's current roadmap or as an
official continuation maintained by him.

> [!WARNING]
> This is compiler research, not a production toolchain. Crashes, unsupported APIs, and
> miscompilations are possible. Validate important behavior against native Rust.

## Try the 0.0.2 release candidate

Prerequisites: [rustup](https://rustup.rs/) and the [.NET 10 SDK](https://dotnet.microsoft.com/download/dotnet/10.0).

The 0.0.2 release tag has not been published yet. On Linux x64 or macOS Apple Silicon, provision
the current candidate from this checkout:

```bash
git clone https://github.com/sharifhsn/rustc_codegen_clr
cd rustc_codegen_clr
cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml -- setup --from-repo "$PWD"
cargo dotnet doctor
```

This installs the captured source snapshot under `~/.cargo-dotnet` and `cargo-dotnet` under Cargo's
bin directory. Setup may install the pinned rustup toolchain and its required components, but it
does not replace the default toolchain. Once an immutable, signed 0.0.2 tag and its host bundles
exist, the release page will provide the shorter installer path. The historical 0.0.1 tag and
assets remain unchanged.

Windows x64 is covered by the release-candidate build and runtime gates, but the checkout setup
still delegates to a POSIX shell script. Windows users should wait for the signed host bundle rather
than treating the source command above as a supported installer.

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

## What works on current main

- Rust applications compiled to managed .NET executables
- Rust libraries and plugins consumed from C#
- .NET BCL and NuGet APIs called from Rust
- Native C ABI libraries called from Rust through generated or handwritten `#[link]` declarations,
  explicit safe-wrapper helpers, and CLR P/Invoke
- Managed generics, interfaces, delegates, tasks, async streams, events, arrays, and collections
- Ordinary `#[dotnet_export] async fn` APIs consumed as C# `Task`/`Task<T>` methods
- C#-friendly exported Rust APIs and deterministic NuGet packages
- MSBuild integration, Portable PDBs, Source Link, and NativeAOT publishing
- Safe, idempotent existing-project wiring with `cargo dotnet attach HOST.csproj --rust-crate PATH`
- A Windows Excel-DNA scaffold that packages worksheet functions backed by managed Rust into a
  64-bit `.xll` (`cargo dotnet new ./risk-engine --excel`)
- Executable ASP.NET Core and Worker scaffolds (`--webapi`, `--worker`) whose MSBuild projects
  rebuild and reference a schema-1 managed Rust backend automatically
- A macOS Apple-Silicon Unity preview: `cargo dotnet new ./game --unity`,
  `cargo dotnet unity doctor --project ./game`, and `cargo dotnet unity build ./game` produce a `netstandard2.1` managed
  facade with Unity-safe no-unwind exports. On pinned Unity `6000.3.19f1`, the acceptance fixture
  proves managed Rust and native Rust P/Invoke in EditMode, PlayMode, and launched Mono and IL2CPP
  macOS players. A second clean-project gate installs the generated UPM package and launches both
  backends without Rust source-tree paths. Other Unity versions, operating systems, and
  architectures remain unclaimed.
- Windows-first WinUI 3 and MAUI scaffolds (`--winui`, `--maui`), kept at planned status until
  Windows workload build-and-launch evidence exists; mobile MAUI targets are not claimed
- Evidence-gated host contracts visible through `cargo dotnet profiles`, with honest preview and
  unsupported Office/Unity/MAUI combinations

The 0.0.2 release candidate supports one deliberately narrow configuration:

| Component | Supported |
|---|---|
| .NET | .NET 10 |
| Linux | x64 |
| macOS | Apple Silicon |
| Windows | x64 |
| Rust | pinned `nightly-2026-06-17` |

Unity is a separate preview profile, not part of the .NET 10 SDK matrix: Unity `6000.3.19f1`,
macOS Apple Silicon, managed `netstandard2.1`; native staging is currently macOS-only. Windows,
Linux, Android, iOS, Web, and consoles are not claimed.

The compiler retains some older-runtime compatibility code, but .NET 8 and 9 are not supported by
the 0.0.2 candidate. A single release runtime profile keeps generated target frameworks, linker metadata,
CoreCLR tools, examples, and diagnostics consistent.

## Documentation

- [`QUICKSTART.md`](QUICKSTART.md) — installation and first run
- [`docs/CARGO_DOTNET.md`](docs/CARGO_DOTNET.md) — command reference and troubleshooting
- [`book/src/office/excel.md`](book/src/office/excel.md) — Excel-DNA preview workflow
- [`docs/UNITY_RUST_STRATEGY.md`](docs/UNITY_RUST_STRATEGY.md) — phased architecture and evidence
  plan for Rust-first Unity games with managed Rust and optional native Rust kernels
- [`docs/QUICKSTART_INTEROP.md`](docs/QUICKSTART_INTEROP.md) — Rust and C# interop examples
- [`docs/INTEROP_COOKBOOK.md`](docs/INTEROP_COOKBOOK.md) — supported interop patterns
- [`book/src/interop/native-from-rust.md`](book/src/interop/native-from-rust.md) — restore a native
  SQLite package and call it from Rust through P/Invoke
- [`docs/TRANSLATION_STATUS.md`](docs/TRANSLATION_STATUS.md) — compiler coverage and semantic limits
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — compiler pipeline and design
- [`docs/REVIEW_GUIDE.md`](docs/REVIEW_GUIDE.md) — bounded compiler, interop, and semantic review paths
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contributor setup and test selection

Questions and project ideas are welcome in [GitHub Discussions](https://github.com/sharifhsn/rustc_codegen_clr/discussions).
Please report compiler bugs, installation failures, and miscompilations through
[GitHub Issues](https://github.com/sharifhsn/rustc_codegen_clr/issues).

## Build from source

The setup command below is currently supported on Linux x64 and macOS Apple Silicon:

```bash
git clone https://github.com/sharifhsn/rustc_codegen_clr
cd rustc_codegen_clr
cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml -- setup --from-repo "$PWD"
cargo dotnet doctor
```

The repository pins the rustc nightly and required compiler components in `rust-toolchain.toml`.

## License

Dual-licensed under [MIT](LICENSE) or [Apache-2.0](LICENSE-Apache).
