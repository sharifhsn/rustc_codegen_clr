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

Native C ABI dependencies use standard Rust FFI and CLR P/Invoke. The SDK can restore and stage a
host-specific NuGet native package with:

```text
cargo dotnet add-native SQLitePCLRaw.lib.e_sqlite3 3.53.3 --library e_sqlite3 ./my-crate
cargo dotnet bindgen sqlite3.h --library e_sqlite3 --path ./my-crate
cargo dotnet run ./my-crate
```

Retained asynchronous callbacks use `CallbackRegistration`: callbacks require `Fn + Send + Sync`,
unregister failures preserve a retryable live guard, and callback storage is released only after
native unregistration guarantees that no callback is still in flight. Thread-safe
aborting and status-returning trampoline macros are included.

See the mdBook page “Call native libraries from Rust” for the complete declaration and supported
ABI boundary.

This is compiler research, not a production toolchain. Validate important results against native
Rust. Report crashes, unsupported behavior, installation problems, or output differences through
[GitHub Issues](https://github.com/sharifhsn/rustc_codegen_clr/issues), and bring questions or ideas
to [GitHub Discussions](https://github.com/sharifhsn/rustc_codegen_clr/discussions).
