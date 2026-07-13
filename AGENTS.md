# AGENTS.md

Guidance for coding agents working in `rustc_codegen_clr`.

## Start here

Read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) before changing compiler code. The project is an
experimental rustc codegen backend that emits managed .NET assemblies or, in an alternate mode, C
source. It is loaded by rustc through `-Z codegen-backend`; it is not a standalone compiler.

The public 0.0.1 SDK supports .NET 10 on Linux x64, macOS Apple Silicon, and Windows x64. Internal
older-runtime branches are not a public compatibility promise.

## Toolchain

`rust-toolchain.toml` pins `nightly-2026-06-17` with `rust-src`, `rustc-dev`, and LLVM tools. The
backend uses rustc-private APIs and will normally fail against an arbitrary nightly. Follow
[`feasibility/PORT_NOTES.md`](feasibility/PORT_NOTES.md) when updating the pin.

## Common checks

```bash
cargo check --workspace
cargo test -p cilly
cargo test --manifest-path tools/cargo-dotnet/Cargo.toml
cargo build --release --workspace
cargo test ::stable
C_MODE=1 cargo test ::stable
```

Use `cargo dotnet` for product-shaped runs:

```bash
cargo dotnet doctor
cargo dotnet build ./crate
cargo dotnet run ./crate
```

When compiling and then running an artifact, join the commands with `&&`; never execute a stale
binary after a failed build. Clean a fixture when switching between native and backend builds.

## Current compiler pipeline

1. `src/lib.rs` implements rustc's `CodegenBackend` entry point.
2. `src/assembly.rs` walks monomorphized MIR items.
3. `src/statement.rs` and `src/terminator/` lower MIR into cilly's interned CIL-tree IR.
4. `cilly/src/ir/opt/` and `cilly/src/ir/typecheck.rs` optimize and verify that IR.
5. The `linker` merges serialized assemblies and emits a managed PE/PDB or C source.

There is one interned IR under `cilly/src/ir/`. The former V1/V2 split and `Assembly::from_v1` no
longer exist. Do not restore that historical boundary in docs or code.

## Workspace map

| Path | Role |
|---|---|
| `src/` | rustc-facing MIR lowering and backend plumbing |
| `cilly/` | IR, optimizer, verifier, linker, and exporters |
| `tools/cargo-dotnet/` | installed setup/build/run/scaffold/package CLI |
| `mycorrhiza/` | Rust-facing managed types and .NET APIs |
| `dotnet_macros/` | export and interop proc macros |
| `cargo_tests/` | full end-to-end Rust/.NET fixtures |
| `test/` | small compiler regression programs |
| `feasibility/` | product acceptance and reproducible development harness |

The crate name `rustc_codgen_clr_operand` is intentionally misspelled and must not be renamed as
incidental cleanup.

## Design constraints

- Lower MIR faithfully before optimizing. Set `OPTIMIZE_CIL=0` first when diagnosing a suspected
  miscompilation.
- Preserve pure, isolated lowering where possible so unsupported MIR can fail without leaving a
  partially mutated assembly.
- Keep the CIL verifier fatal. Do not turn a verifier failure into a warning to make a test pass.
- Compare generated behavior with native Rust using an observable, deterministic oracle.
- Direct PE is the default. ILAsm is a legacy debugging/export fallback.
- Treat type layout, fat pointers, ZSTs, enum unions, atomics, unwinding, and managed generics as
  semantic boundaries; read the relevant architecture section before editing them.

## Tests

Add small lowering regressions under `test/`. Use `cargo_tests/` when behavior requires `build-std`,
managed dependencies, a C# consumer, or a full Cargo crate. For user-facing changes, run
`feasibility/onboarding_acceptance.sh` and the closest focused acceptance script.

Before handing off a change, run the smallest decisive test, `cargo fmt --all -- --check`, and
`git diff --check`. Do not bundle broad formatting changes with a logic change.
