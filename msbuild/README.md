# `RustDotnet.targets` — build a Rust crate from a C# project, automatically

This directory ships the MSBuild integration that makes **consuming a Rust crate from a C# project
seamless**: a C# dev declares a dependency on a Rust crate, and a single `dotnet build` (or `dotnet run`)
auto-compiles it (via the installed [`cargo dotnet`](../docs/CARGO_DOTNET.md)) and references the produced
.NET assembly — **no manual `cargo dotnet`, no `.dll` copy, no hand-written `<Reference><HintPath>`**.

> **Supported hosts:** the current release supports this integration on Linux and macOS. On Windows,
> the targets fail immediately with the cargo-dotnet host-support diagnostic until Windows build,
> test, packaging, and MSBuild acceptance exists. This does not assert a managed target/runtime limit.

This is the recommended path in [docs/INTEROP_CSHARP.md](../docs/INTEROP_CSHARP.md) §3a. The manual
`<Reference>` + `<HintPath>` and the NuGet `cargo dotnet pack` paths are documented there too.

## Files

| File | Role |
|------|------|
| `RustDotnet.targets` | the integration: 3 targets that build each `<RustCrate>` and inject its assembly as a `<Reference>` before the C# compile resolves references. |
| `RustDotnet.props` | default properties (target spec, tool PATH, force-build flag), imported by the `.targets`. |

Both files are also copied into `$CARGO_DOTNET_HOME/msbuild/` (default `~/.cargo-dotnet/msbuild/`) by
`cargo dotnet setup`, so an **external** C# project — with no repo checkout — can import them.

## Usage

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>   <!-- only if you marshal raw (ptr, len) -->
  </PropertyGroup>

  <!-- Import the installed copy (works in any external project); repo-relative fallback. -->
  <Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
  <Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
          Condition="!Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets') and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />

  <ItemGroup>
    <RustCrate Include="../path/to/rustlib" />
  </ItemGroup>
</Project>
```

```bash
dotnet run    # builds the Rust crate + references it + runs. Zero manual steps.
```

### `<RustCrate>` item

`Include` is the path to the Rust crate directory, relative to the `.csproj` — the only mandatory input.
Multiple `<RustCrate>` items are supported (built serially). Optional metadata, all defaulted:

| metadata | default | meaning |
|----------|---------|---------|
| `Configuration` | `Release` | `Release` or `Debug` → `--release` / `--debug`. |
| `CrateName` | the `[package] name` from `Cargo.toml` (else the dir leaf) | the assembly / `.dll` basename. Override only if the auto-detected name is wrong. |
| `Clean` | (unset) | `true` → pass `--clean` (force a clean rebuild). |
| `Private` | `true` | copy the Rust `.dll` into the consumer `bin/` (needed for `dotnet run` to resolve it at runtime). |

### Additional incremental inputs

The targets automatically track crate-local Rust sources, manifests, lockfile, build script,
crate-local toolchain selection, and the targets file itself. Until Cargo-metadata-derived transitive
fingerprinting lands, declare inputs outside the selected crate explicitly in the consuming project:

```xml
<ItemGroup>
  <RustDotnetInput Include="../shared-rust/**/*.rs" />
  <RustDotnetInput Include="../shared-rust/**/Cargo.toml" />
  <RustDotnetInput Include="../templates/codegen-input.json" />
</ItemGroup>
```

This is required for path dependencies, workspace manifests, and arbitrary files read by `build.rs`.

### Project-level property overrides

| property | default | meaning |
|----------|---------|---------|
| `CargoDotnet` | auto-discovered | explicit path to the `cargo-dotnet` script. |
| `RustDotnetTarget` | `x86_64-unknown-dotnet` | the rustc target (CIL is arch-agnostic). |
| `RustDotnetForceBuild` | `false` | `true` defeats the incremental skip (always rebuild). |
| `RustDotnetToolPath` | `$(HOME)/.cargo/bin:$(HOME)/.dotnet` | dirs prepended to PATH for the build Exec. |
| `RustDotnetDotnetRoot` | `$(HOME)/.dotnet` | `DOTNET_ROOT` for the build Exec. |

The current Rust target has 64-bit pointer/layout semantics. The integration rejects explicit x86,
`Prefer32Bit`, and x86 RID consumers even though the managed assembly itself can otherwise appear
AnyCPU. Use x64, arm64, or a 64-bit AnyCPU process.

## How it works

Three targets in `RustDotnet.targets`:

1. **`FindCargoDotnet`** — resolves the installed `cargo-dotnet` (probe order: `<CargoDotnet>` >
   `$CARGO_DOTNET_HOME/cargo-dotnet` > `~/.cargo-dotnet/cargo-dotnet` > `~/.cargo/bin/cargo-dotnet`). If
   none exists, fails with an actionable error pointing at `cargo dotnet setup`.
2. **`BuildRustCrates`** — `BeforeTargets="ResolveAssemblyReferences;ResolveReferences;BeforeCompile"`
   (so it runs *before* the C# build resolves references). It batches `@(RustCrate)`, builds each crate
   serially via the inner per-crate target, then adds a `<Reference>` (with `<HintPath>` + `<Private>`)
   to each produced `.dll`.
3. **`_ResolveOneCrate` / `_BuildOneRustCrate`** (per crate) — resolves the crate name (Cargo.toml
   `[package] name`), computes the `.dll` path, and runs `cargo dotnet build`. **Incremental**: the
   target's `Inputs` are the automatic and consumer-declared inputs above. An explicit success stamp,
   written only after the expected DLL exists, is the timestamp oracle; the DLL's own timestamp is
   not trusted. A missing DLL forces the target stale even when a stamp remains. A successful build
   must also produce `<dll>.rustdotnet.receipt.json`, binding the artifact to source, toolchain,
   backend, linker, target, PAL, overlays, profile, and .NET version. Once MSBuild decides a rebuild is
   required, it deletes the old managed DLL, receipt, and stamp before invoking `cargo dotnet`; a
   failed requested rebuild cannot leave stale output or stale evidence eligible for a later compile
   or run. Cargo-dotnet currently keeps a conservative cross-process lock while the remaining
   shared-cache concurrency audit is completed; ordinary builds use private content-addressed
   sysroots and a cargo-dotnet-owned Cargo home rather than mutating ambient `rust-src` or registry
   sources.

The build `Exec` runs with the inherited PATH plus `RustDotnetToolPath` prepended (so the tool's internal
`cargo`/`rustc`/`dotnet` and any cargo `rustc-wrapper` resolve under MSBuild's non-interactive
environment) and `DOTNET_ROOT` set.

## Worked example

[`cargo_tests/cd_interop/csharp/`](../cargo_tests/cd_interop/csharp) imports `RustDotnet.targets` and
declares `<RustCrate Include="../rustlib" />`. A single `dotnet run` builds the Rust `cdylib`, references
the assembly, and the C# program calls the exported Rust functions — all six marshalling checks pass,
exit 0, with no manual steps. A second `dotnet build` skips the Rust rebuild (incremental).

## Requirements

- The installed `cargo dotnet` (G1): `feasibility/cargo-dotnet setup` once from a repo checkout.
- A .NET 8 SDK (`dotnet build`/`dotnet run`).
- The Rust crate is a `cdylib` (`[lib] crate-type = ["cdylib"]`) with `#[unsafe(no_mangle)] pub extern "C"`
  exports — see [docs/INTEROP_CSHARP.md](../docs/INTEROP_CSHARP.md).
