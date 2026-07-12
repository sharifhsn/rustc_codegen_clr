# Install the toolchain

## Requirements

Install:

- the Rust nightly selected by `rust-toolchain.toml`, including `rustc-dev` and `rust-src`;
- the .NET 8 SDK;
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

> The repository currently uses a dated nightly for compatibility. Prefer the checked-in
> `rust-toolchain.toml` over an arbitrary current nightly.
