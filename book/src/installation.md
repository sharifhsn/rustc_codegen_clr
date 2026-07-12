# Install the toolchain

## Requirements

Install:

- the Rust nightly selected by `rust-toolchain.toml`, including `rustc-dev` and `rust-src`;
- the .NET 10 SDK (the default profile), or the SDK matching an explicitly selected profile;
- a C toolchain for C-output mode; and
- Mono `ilasm` only when using the ILASM fallback (`DIRECT_PE=0`).

From the repository root, build the backend and the `cargo dotnet` frontend:

```bash
cargo build --release -p cilly --bins
cargo build --release
cargo build --release --manifest-path tools/cargo-dotnet/Cargo.toml
```

Run the setup command from the tool you just built:

```bash
./tools/cargo-dotnet/target/release/cargo-dotnet dotnet setup
```

Then verify the installation:

```bash
cargo dotnet doctor
```

`doctor` reports missing SDK components, stale backend artifacts, and common project-wiring errors.
The backend is selected per build; it does not permanently replace rustc's native backend.

The default output targets .NET 10. To keep a project on an older supported runtime, pass
`--dotnet 8` or `--dotnet 9`, or set `DOTNET_VERSION` in the build environment. See
[.NET runtime profiles](runtime-profiles.md) for the compatibility contract.

> The repository currently uses a dated nightly for compatibility. Prefer the checked-in
> `rust-toolchain.toml` over an arbitrary current nightly.
