# Build and distribute

## MSBuild integration

Import `msbuild/RustDotnet.targets` from a C# project and identify the Rust crate. The targets build
the reachable local Cargo dependency graph, invalidate when sources or declared external inputs
change, and refuse to run a stale assembly after a failed build.

Use [`feasibility/fixtures/msbuild_transitive_inputs`](../../feasibility/fixtures/msbuild_transitive_inputs/)
as the canonical small integration example.

## NuGet packages

Package a library with:

```bash
cargo dotnet pack
```

The package path is printed on success. Packages contain managed identity metadata, checksums,
provenance, API documentation, symbols, and RID-specific native/runtime assets where applicable.
Test a package from a clean consumer with a normal `PackageReference`; do not validate it only from
the producer's target directory.

The packaging pipeline rejects duplicate assembly/type identities and invalid asset layouts.
Two independently named Rust libraries can be loaded together; the executable proof is
[`cd_multi_library_collision`](../../cargo_tests/cd_multi_library_collision/).

Public release support additionally requires the repository's release gates in
[`docs/RELEASE_BLOCKERS.md`](../../docs/RELEASE_BLOCKERS.md).
