# Translation status

This page is the current support boundary for `rustc_codegen_clr`. It describes the supported
managed output, not historical experiments or future research.

## Supported profiles

- **.NET 10** on Linux x64, macOS Apple Silicon, and Windows x64.
- **Unity preview** on the pinned Unity version documented in [`UNITY_RUST_STRATEGY.md`](UNITY_RUST_STRATEGY.md),
  using a `netstandard2.1` managed facade. Native staging is currently macOS-only.
- Rust nightly is pinned in `rust-toolchain.toml`; arbitrary nightlies are not supported.

The compiler emits managed PE assemblies and Portable PDBs through the direct PE emitter. There is
no public ILAsm, C, Java, or JavaScript output mode. Older .NET runtime implementations remain
internal code and are not compatibility promises.

## What is working

The backend lowers ordinary Rust MIR into a single interned CIL IR, then verifies and optimizes it
before linking. The tested surface includes:

- Rust applications and libraries consumed by .NET and C#.
- Primitive, struct, enum, array, collection, delegate, interface, event, task, async-stream, and
  managed-generic interop patterns covered by the `cargo_tests` fixtures.
- .NET BCL and NuGet calls from Rust through `mycorrhiza` and generated bindings.
- `#[dotnet_export]` and related declaration macros for C#-friendly APIs, including XML docs.
- Native libraries through Rust foreign declarations and the `rust-dotnet-pinvoke` convenience API.
- Portable PDB, Source Link, MSBuild, NuGet, NativeAOT, and Unity packaging workflows where the
  corresponding acceptance fixture exists.

Use [`BCL_COVERAGE.md`](BCL_COVERAGE.md), [`INTEROP_CSHARP.md`](INTEROP_CSHARP.md), and
[`INTEROP_COOKBOOK.md`](INTEROP_COOKBOOK.md) for concrete examples and exact fixture names.

## Known boundaries

Unsupported or incomplete behavior is reported as a compiler diagnostic rather than silently
falling back to another exporter. Important limits include portions of the Rust standard library,
some unwind and platform-specific operations, and advanced managed type shapes not represented by
the current declaration model. Check the nearest acceptance fixture before relying on a capability.

The CIL verifier is fatal. When investigating a suspected lowering bug, set `OPTIMIZE_CIL=0` and
compare the managed result with native Rust using a deterministic observable test.

## Validation map

The decisive checks are:

```text
cargo check --workspace
cargo test -p cilly
cargo test --manifest-path tools/cargo-dotnet/Cargo.toml
feasibility/onboarding_acceptance.sh
feasibility/event_acceptance.sh
feasibility/pdb_consumer_acceptance.sh
```

The Unity acceptance scripts are opt-in because they require a local Unity installation. Product
claims should be added here only after the matching fixture passes on the supported host/profile.

For compiler architecture and lowering invariants, see [`ARCHITECTURE.md`](ARCHITECTURE.md). For
installation and command usage, see [`CARGO_DOTNET.md`](CARGO_DOTNET.md) and the mdBook.
