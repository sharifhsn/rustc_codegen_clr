# rust-dotnet 0.0.1

This is the first installable preview of `rustc_codegen_clr`: a rustc codegen backend that emits
managed .NET assemblies.

## Supported release configuration

- .NET 10
- Linux x64
- macOS Apple Silicon
- Windows x64
- Rust nightly `nightly-2026-06-17` (installed automatically by rustup when needed)

The release includes host-specific SDK bundles and small installers. It does not bundle rustup or
the .NET SDK; install those first.

## Try it

After installation:

```text
cargo dotnet doctor
cargo dotnet new hello-dotnet --app
cargo dotnet run hello-dotnet
```

This is compiler research, not a production toolchain. Validate important results against native
Rust and report crashes, unsupported behavior, or output differences through GitHub Issues.
