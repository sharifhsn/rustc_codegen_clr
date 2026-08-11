# Install the toolchain

## Requirements

Install:

- [rustup](https://rustup.rs/); and
- the [.NET 10 SDK](https://dotnet.microsoft.com/download/dotnet/10.0).

The 0.0.2 release tag has not been published yet. On Linux x64 or macOS Apple Silicon, provision the
current candidate from a checkout; this installs `cargo dotnet` and may add the pinned rustup
toolchain and required components, but does not replace the default toolchain.

```bash
git clone https://github.com/sharifhsn/rustc_codegen_clr
cd rustc_codegen_clr
cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml -- setup --from-repo "$PWD"
cargo dotnet doctor
```

Once a signed 0.0.2 tag and matching bundles are published, the release page will provide verified
one-line installers for each supported host. Windows checkout setup still delegates to a POSIX
shell script, so Windows users should wait for that signed bundle.

`doctor` reports missing SDK components and common project-wiring errors. The backend is selected
per build; it does not permanently replace rustc's native backend.

The 0.0.2 release candidate targets .NET 10 only and emits managed PE files directly.

## Build from a checkout

Compiler contributors can provision directly from a repository checkout:

```bash
cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml -- setup --from-repo "$PWD"
cargo dotnet doctor
```

Prefer the checked-in `rust-toolchain.toml` over an arbitrary current nightly.
