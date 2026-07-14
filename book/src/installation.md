# Install the toolchain

## Requirements

Install:

- [rustup](https://rustup.rs/); and
- the [.NET 10 SDK](https://dotnet.microsoft.com/download/dotnet/10.0).

The release installer downloads the matching SDK bundle, verifies it, and installs the
`cargo dotnet` command without changing the system Rust installation.

```bash
curl -fsSL https://github.com/sharifhsn/rustc_codegen_clr/releases/download/rust-dotnet-v0.0.1/install.sh | sh
```

On Windows x64, run this in PowerShell:

```powershell
irm https://github.com/sharifhsn/rustc_codegen_clr/releases/download/rust-dotnet-v0.0.1/install.ps1 | iex
```

Then verify the installation:

```bash
cargo dotnet doctor
```

`doctor` reports missing SDK components and common project-wiring errors. The backend is selected
per build; it does not permanently replace rustc's native backend.

The public SDK targets .NET 10 only. Its bundled CoreCLR ILAsm is used automatically when the
legacy ILAsm fallback is needed.

## Build from a checkout

Compiler contributors can provision directly from a repository checkout:

```bash
cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml -- setup --from-repo "$PWD"
cargo dotnet doctor
```

Prefer the checked-in `rust-toolchain.toml` over an arbitrary current nightly.
