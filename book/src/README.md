# Rust on .NET

`rustc_codegen_clr` is an experimental Rust compiler backend that emits .NET assemblies. It lets
you build Rust applications for CoreCLR, expose typed Rust APIs to C#, and call managed APIs from
Rust.

The project is suitable for experimentation and controlled integrations. It is not yet a general
production replacement for Rust's native backends: the compiler tracks a specific nightly, the
supported host matrix is currently Linux and macOS, and public release reproducibility is still a
release gate.

This guide focuses on the supported workflow through `cargo dotnet`. You will learn how to:

- create and run a Rust-on-.NET application;
- export typed Rust functions and data to C#;
- call .NET collections and BCL APIs from Rust;
- integrate a Rust crate into MSBuild; and
- package a Rust library as a NuGet package.

If you want compiler internals instead, start with the repository's
[architecture guide](../../docs/ARCHITECTURE.md).
