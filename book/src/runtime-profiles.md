# .NET runtime profiles

The public SDK has one supported runtime profile: .NET 10. This keeps target frameworks, linker
metadata, CoreCLR tools, generated MSBuild projects, and NuGet assets aligned.

Select a profile on the command line:

```bash
cargo dotnet run --dotnet 10
```

Or pin it for scripts and CI:

```bash
export DOTNET_VERSION=10
cargo dotnet run
```

The selected profile is used consistently for the target framework (`net10.0`), core-library assembly
references, IL assembly, generated MSBuild projects, NuGet assets, and NativeAOT publishing.
`cargo dotnet doctor` checks that the matching SDK and tools are present.

## Compatibility contract

The profile does not change Rust language semantics, source syntax, or Cargo dependency resolution.
It selects the managed runtime contract and enables lowering and BCL calls that .NET 10 supports.

## Recommended policy

- Use .NET 10 for new applications.
- Pin `DOTNET_VERSION=10` in CI and release scripts.
- Treat .NET 8 and 9 compatibility code inside the compiler as implementation detail, not a public
  SDK contract.

Future runtime support will be added only with matching build, install, and execution coverage.
