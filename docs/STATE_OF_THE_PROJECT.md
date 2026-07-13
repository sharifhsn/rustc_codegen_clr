# State of the project — July 2026

`rustc_codegen_clr` can compile substantial Rust programs into managed .NET assemblies. The core
compiler and interop mechanisms are established; the current focus is making the SDK installable,
understandable, and useful outside this checkout.

## Public preview contract

The 0.0.1 SDK supports:

- .NET 10;
- Linux x64, macOS Apple Silicon, and Windows x64; and
- the pinned `nightly-2026-06-17` rustc toolchain.

The compiler retains internal .NET 8/9 compatibility machinery, but those profiles are not exposed
as supported choices in 0.0.1. One public profile prevents target-framework, linker, runtimeconfig,
ILAsm, and example drift.

This is an experimental compiler preview. It is not production-ready, does not promise a stable
compiler ABI, and may still crash, reject valid Rust, or miscompile unsupported edge cases.

## Compiler and runtime

- The fatal CIL verifier is enabled by default.
- The main exporter writes managed PE files directly; ILAsm is a legacy fallback.
- Portable PDBs include Rust sequence points, source paths, local metadata, and optional Source
  Link mappings.
- The .NET PAL covers files, networking, threads, locks, TLS, process execution and output,
  unwinding, async Rust, and core tokio/rayon-shaped workloads.
- The alternate C exporter shares the compiler IR but remains a secondary prototype.

Repository test campaigns currently report 2,657 passing core tests, roughly 96% of the relevant
rustc run-pass suite, and a 137-crate ecosystem differential survey with roughly 85% byte-identical
results. These are broad confidence signals, not a guarantee for arbitrary programs.

## Interop

Implemented Rust/.NET interop includes:

- Rust applications, class libraries, and managed plugin shapes;
- primitive, string, struct, enum, nullable, option, result, and managed-handle boundaries;
- generic types and methods, interfaces, delegates, capturing closures, and events;
- `Task<T>`, `ValueTask<T>`, and `IAsyncEnumerable<T>` consumption;
- managed collections, LINQ and expression trees, arrays, `Memory<T>`, and BCL wrappers;
- generated bindings from NuGet's resolved dependency graph; and
- C#-friendly exports, XML documentation, NuGet packages, MSBuild integration, and NativeAOT.

The main remaining interop gaps are uncommon delegate shapes, automatic owned callback-value
marshalling, some nullable-reference metadata, multidimensional arrays, and broader automatic
adapter generation.

## Developer experience

`cargo dotnet` provides:

- setup and integrity-checked host SDK bundles;
- environment and failure diagnostics;
- app, library, and plugin scaffolds;
- build, run, and test workflows;
- restore receipts and offline/frozen builds;
- deterministic NuGet packing and feed push support;
- Portable PDB and Source Link output; and
- NativeAOT publishing for managed hosts.

Release bundles remove the compiler checkout from the consumer machine. Rustup and the .NET 10 SDK
remain normal host prerequisites.

## Known limits

- `hard_link`, full Unix signal semantics, fork/exec fidelity, mmap fidelity, and f128 do not map
  cleanly to portable managed APIs.
- TLS destructors and several long-tail errno/PAL behaviors remain incomplete.
- `overflow-checks=true` has a known build-std ICE.
- Interactive IDE stepping and Source Link retrieval have not received the same end-to-end coverage
  as PDB metadata and managed stack traces.
- Allocation-heavy code can remain slower than equivalent GC-optimized C#.
- The C exporter has unsupported cold paths; the JVM exporter is only a skeleton.

## Near-term priorities

1. Publish and clean-install the three host SDK bundles.
2. Make first-user failures reproducible and actionable through GitHub Issues.
3. Expand Windows MSBuild/packaging and interactive debugger coverage.
4. Improve correctness and ecosystem compatibility based on real external programs.
5. Pursue an upstream Rust `*-unknown-dotnet` target as the longer-term integration path.
