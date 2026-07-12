# Quickstart

Create an application and run it on .NET:

```bash
cargo dotnet new hello-dotnet --app
cd hello-dotnet
cargo dotnet run
```

The generated project is an ordinary Cargo crate with the metadata needed by `cargo dotnet`.
Its `src/main.rs` can begin as normal Rust:

```rust
fn main() {
    println!("Hello from Rust on .NET!");
}
```

Useful commands:

```bash
cargo dotnet build          # release build by default
cargo dotnet build --debug  # opt into a debug build
cargo dotnet test
cargo dotnet doctor
```

For a minimal checked-in example, see
[`cargo_tests/hello_world`](../../cargo_tests/hello_world/).

## What the command does

`cargo dotnet` prepares an isolated Rust standard library for the .NET target, invokes rustc with
the codegen backend, and links the serialized intermediate assemblies into a managed executable or
library. Successful outputs include an identity receipt so MSBuild can reject stale artifacts after
a failed rebuild.
