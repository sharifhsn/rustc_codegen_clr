# Rust on .NET

`rustc_codegen_clr` is an experimental Rust compiler backend that emits .NET assemblies. It lets
you build Rust applications for CoreCLR, expose typed Rust APIs to C#, and call managed APIs from
Rust.

The project is suitable for experimentation and controlled integrations. It is not yet a general
production replacement for Rust's native backends: the compiler tracks a specific nightly, the
public 0.0.1 SDK supports only .NET 10 on Linux x64, macOS Apple Silicon, and Windows x64, and
unsupported Rust or CLR edge cases can still fail compilation or behave incorrectly. Release
bundles are built and install-tested on their matching GitHub runners.

This guide focuses on the supported workflow through `cargo dotnet`. You will learn how to:

- create and run a Rust-on-.NET application;
- export typed Rust functions and data to C#;
- call .NET collections and BCL APIs from Rust;
- integrate a Rust crate into MSBuild; and
- package a Rust library as a NuGet package.

If you want compiler internals instead, start with the repository's
[architecture guide](../../docs/ARCHITECTURE.md).
