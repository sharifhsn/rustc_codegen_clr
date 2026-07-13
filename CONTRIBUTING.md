# Contributing to rustc_codegen_clr

Thanks for helping with the project. `rustc_codegen_clr` is an experimental compiler backend, so
the most useful changes are small, evidence-backed, and accompanied by a program that distinguishes
the fixed behavior from native Rust.

Start with [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). It explains the MIR-to-CIL pipeline, the
single interned IR, and the design constraints that are easy to miss in individual files.

## Prepare a checkout

The repository pins the compatible nightly toolchain. Before editing, confirm that you are not
mixing your work with unrelated changes:

```bash
git status --short
rustup show active-toolchain
cargo check -p cilly
cargo test --manifest-path tools/cargo-dotnet/Cargo.toml
```

Those are the fast preflight. A first product-shaped check is:

```bash
cargo build --release --workspace && feasibility/onboarding_acceptance.sh
```

The first run builds a private `std` and can take several minutes; later runs reuse its
content-addressed cache. Before opening a compiler or linker change, run the broader checks:

```bash
cargo check --workspace
cargo test -p cilly
cargo test --manifest-path tools/cargo-dotnet/Cargo.toml
```

The last command uses an explicit manifest because `tools/cargo-dotnet` is a separate Cargo
workspace. .NET end-to-end tests also require the .NET SDK. The legacy IL exporter requires
`ilasm`; the normal direct-PE path does not.

For the reproducible Linux environment, use the Docker harness:

```bash
feasibility/run.sh build
feasibility/dev.sh run hello_world
feasibility/dev.sh gate
```

## Find the right test boundary

| Area changed | Focused evidence to add or run |
|---|---|
| MIR lowering or the rustc-facing backend | A minimal program under `test/`, then its generated `::<stable>::release` test |
| `cilly` IR, optimizer, typechecker, linker, or exporter | A unit/regression test in `cilly`, then `cargo test -p cilly` |
| `cargo dotnet` setup, diagnostics, scaffolding, packaging, or examples | `cargo test --manifest-path tools/cargo-dotnet/Cargo.toml`, `feasibility/onboarding_acceptance.sh`, and `feasibility/flagship_example_acceptance.sh` when application behavior changes |
| Rust/.NET interop | The closest `cargo_tests/cd_*` fixture plus its C# consumer where applicable |
| Release workflow or evidence scripts | `feasibility/release_workflow_acceptance.sh` and shell syntax checks |
| A nightly rustc API bump | Follow [`feasibility/PORT_NOTES.md`](feasibility/PORT_NOTES.md) and update its API-drift record |

Use `test/` for a small standalone Rust regression. Use `cargo_tests/` when the behavior needs a full
crate, `build-std`, managed dependencies, or a C# consumer. The harness compares generated-program
output with native Rust, so keep fixtures deterministic and make the expected behavior observable on
stdout.

When compiling and then running a generated artifact, join the commands with `&&`. A failed build
must never leave a stale executable looking like a successful test. Clean the fixture when switching
between native and backend builds if results look inconsistent.

## Debug a translation failure

Set `OPTIMIZE_CIL=0` first. The unoptimized output preserves the direct relationship between MIR
statements and CIL operations and separates lowering bugs from optimizer bugs. Useful additional
diagnostic flags are documented in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and
[`src/config.rs`](src/config.rs).

Reduce failures to the smallest program that still differs from native Rust. Include the exact
command, host, Rust nightly, .NET version, native output, backend output, and whether C mode behaves
the same way. Miscompilations need a regression test even when the code change appears mechanical.

## Before opening a pull request

- Keep the change scoped; do not combine a compiler fix with unrelated formatting or cleanup.
- Run the smallest relevant test first, then the area-level checks in the table above.
- Run `cargo fmt --all -- --check` and `git diff --check`.
- Document a new limitation honestly if it cannot be fixed in the same change.
- Call out user-visible changes to `cargo dotnet`, generated assemblies, interop APIs, or packaging.

The intentionally named `rustc_codgen_clr_operand` crate is missing the second `e` in `codegen`.
Do not rename it as incidental cleanup; it is part of the current workspace and dependency surface.

For known unsupported behavior and test-minimization guidance, see
[`BROKEN_TESTS.md`](BROKEN_TESTS.md). For the current release boundary and evidence, see
[`docs/RELEASE_BLOCKERS.md`](docs/RELEASE_BLOCKERS.md).
