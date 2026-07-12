# .NET runtime profiles

`cargo dotnet` treats the target .NET runtime a little like a Rust edition: the repository can move
its default forward without silently changing projects that pin an older profile. The supported
profiles are `8`, `9`, and `10`; new commands default to .NET 10.

Select a profile on the command line:

```bash
cargo dotnet run --dotnet 10
cargo dotnet build --dotnet 9
cargo dotnet publish --dotnet 8
```

Or pin it for scripts and CI:

```bash
export DOTNET_VERSION=10
cargo dotnet run
```

The selected profile is used consistently for the target framework (`net8.0`, `net9.0`, or
`net10.0`), core-library assembly references, IL assembly, generated MSBuild projects, NuGet assets,
and NativeAOT publishing. `cargo dotnet doctor` checks that the matching SDK and tools are present.

## Compatibility contract

A higher profile includes capabilities introduced by lower profiles. For example, native sub-word
`Interlocked` operations are available in both the .NET 9 and .NET 10 profiles. Backend code asks for
that capability instead of comparing one exact version, which prevents a feature from accidentally
disappearing when a newer profile is added.

The profile does not change Rust language semantics, source syntax, or Cargo dependency resolution.
It only selects the managed runtime contract and enables lowering or BCL calls that runtime supports.
Code intended to run on several .NET generations should target the oldest required profile and avoid
newer-only BCL wrappers. Libraries distributed as NuGet packages should publish once per target
framework they promise to support.

## Recommended policy

- Use the default .NET 10 profile for new applications.
- Pin `DOTNET_VERSION=10` in CI and release scripts so a future default change is explicit.
- Select .NET 8 or 9 only when the deployment environment requires it.
- Test every profile you distribute; successful compilation for .NET 10 does not prove a package can
  load on .NET 8.

This is deliberately smaller than Rust editions. Adding a profile is an additive enum case and a set
of capability checks, not a fork of the compiler or standard library.
