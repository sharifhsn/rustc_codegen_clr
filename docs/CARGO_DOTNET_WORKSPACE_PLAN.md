# `cargo dotnet` workspace boundaries

Status: the root workspace split required for first-class P/Invoke is complete.

## Result

The repository now uses one Cargo workspace and one dependency graph for the backend, reusable SDK
crates, and `cargo-dotnet`. Stable CLI development remains an explicit supported command even though
the repository's default toolchain is the pinned rustc-private nightly:

```bash
cargo +stable check -p cargo-dotnet
cargo +nightly-2026-06-17 check -p rustc_codegen_clr
```

`tools/cargo-dotnet` no longer declares a nested workspace or owns a second lockfile.

## Ownership map

```text
crates/rust-dotnet-sdk-core
  host/RID facts, .NET runtime model, managed identity

crates/rust-dotnet-assets
  NuGet graph parsing, RID selection, native/runtime/resource staging,
  collision detection, clean-clone restoration, package projection

crates/rust-dotnet-bindgen
  C-header parsing, deterministic declaration generation, stale-output checks

crates/rust-dotnet-pinvoke
  optional no_std/alloc/std FFI helpers for strings, status, handles, and callbacks

tools/cargo-dotnet
  CLI syntax, user messages, process orchestration, project mutation,
  build/run/test/doctor/setup/pack/push

rustc_codegen_clr
  pinned-rustc adapter that collects standard #[link] foreign declarations

cilly
  serialized import contract, linker resolution, verifier, PE/IL exporters
```

The dependency direction is one-way: `cargo-dotnet` may use stable reusable crates; neither the
backend nor `cilly` depends on the CLI. The helper crate does not fetch packages or transport
compiler metadata.

## What moved

### SDK core

- `HostFacts`, including supported host RID and executable/library naming;
- `ManagedIdentity`; and
- the public .NET 10 runtime profile model.

The process-heavy `Context`, bundle verification, build locks, command execution, and user-facing
diagnostics remain in the CLI because they are orchestration, not reusable domain types.

### Asset layer

- the complete former `nuget_assets` implementation;
- its RID matrix fixtures and focused tests;
- native-only package support through an optional primary managed DLL;
- direct SDK `native` group plus `runtimeTargets` parsing; and
- selection of an appropriate .NET host for restore in side-by-side installations.

The move is physical: `rust-dotnet-assets` does not include source from `tools/cargo-dotnet` by path.
`add-nuget` explicitly requires a managed DLL; `add-native` requires a native asset. Both use the
same durable package manifest and restaging pipeline.

### P/Invoke helper

`rust-dotnet-pinvoke` is `no_std` capable and intentionally contains only explicit wrapper
primitives. Raw standard Rust FFI remains the declaration contract, so adopting the helper is
optional.

### Header generator

`rust-dotnet-bindgen` owns upstream rust-bindgen/libclang integration and deterministic source
generation. `cargo-dotnet` supplies command-line policy and paths; the compiler still consumes only
ordinary Rust declarations.

## Product surface enabled by the split

```bash
cargo dotnet add-native <PACKAGE> <VERSION> --library <PINVOKE_NAME>
cargo dotnet add-native-file <FILE> --library <PINVOKE_NAME> --rid <RID>
cargo dotnet bindgen <HEADER> --library <PINVOKE_NAME>
cargo dotnet build
cargo dotnet run
cargo dotnet pack
```

The complete compiler, asset, fixture, documentation, and release-gate design is recorded in
[`NEXT_MILESTONE_NATIVE_INTEROP.md`](NEXT_MILESTONE_NATIVE_INTEROP.md).

## Guardrails kept

- The public `cargo dotnet` command names and installed-SDK layout did not change.
- The compiler backend does not depend on a host-stable CLI crate.
- Direct managed Rust-to-C#/F# interop remains the preferred managed boundary; P/Invoke is for
  Rust consuming native C ABI libraries.
- `NATIVE_PASSTHROUGH` remains an internal GCC/`nm` experiment and is not presented as the portable
  public surface.
- C++ ABI, variadics, inferred ownership, and cross-RID native compilation remain outside the C
  P/Invoke contract. Header generation and explicit ownership/marshalling helpers are supported.

## Follow-up extraction criteria

Do not split more code merely for symmetry. A new stable crate is warranted only when at least two
real consumers need the same policy and the boundary can avoid CLI process orchestration or
rustc-private types. Likely candidates are project-manifest editing and diagnostic report models;
neither is required for the current native-interoperability journey.
