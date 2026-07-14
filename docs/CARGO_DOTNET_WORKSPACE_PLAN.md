# `cargo dotnet` workspace and P/Invoke plan

## Purpose

Keep Rust-on-.NET product concerns in one repository-level Cargo workspace while
preserving a clear division between the compiler backend, the installed `cargo`
`dotnet` command, dependency/asset handling, and a future Rust-facing P/Invoke
API.

This is an incremental extraction plan. It does **not** propose a broad compiler
rewrite or a breaking change to `cargo dotnet`.

## Current constraint

`tools/cargo-dotnet` is presently its own Cargo workspace. That is intentional:
the installed CLI must build with the host stable toolchain, while the backend
uses a pinned rustc-private nightly. Moving it into the root workspace is only
acceptable if both workflows remain reliable:

```bash
cargo +stable check -p cargo-dotnet
cargo +nightly-2026-06-17 check -p rustc_codegen_clr
```

The target is one repository workspace and shared lockfile. If Cargo cannot
preserve the host-tool isolation in practice, retain a small nested workspace
for the CLI rather than make normal installation depend on rustc-private nightly.
The product boundary is more important than the directory shape.

## Target boundaries

```text
cargo-dotnet
  ├─ rust-dotnet-sdk-core
  ├─ rust-dotnet-assets
  └─ invokes rustc_codegen_clr as the compiler backend

rust-dotnet-pinvoke
  └─ defines the Rust declaration contract

rustc_codegen_clr + cilly
  └─ recognize that contract and emit CLR P/Invoke metadata
```

The compiler backend must never depend on `cargo-dotnet`. The CLI is the
user-facing orchestrator; it may depend on reusable support crates.

Proposed layout:

```text
crates/
  rust-dotnet-sdk-core/        # toolchain discovery, paths, project metadata
  rust-dotnet-assets/          # NuGet/native asset resolution and staging
  rust-dotnet-pinvoke/         # Rust API, ABI types, wrapper helpers
  rust-dotnet-pinvoke-macros/  # optional attribute macros
tools/
  cargo-dotnet/                # CLI parsing, UX, and orchestration
```

`rustc_codegen_clr`, `cilly`, `mycorrhiza`, and `dotnet_macros` remain in their
current homes. Do not split compiler internals merely to mirror this product
layout.

## Milestones

### 1. Prove the workspace/toolchain contract

Make `cargo-dotnet` a root member only after a small spike proves that stable
CLI installation/development and pinned-nightly backend development both work.
Add separate CI jobs for the two commands above. Do not change the public CLI
or its installation instructions in this milestone.

### 2. Extract `rust-dotnet-sdk-core`

Move reusable, stable concepts out of the CLI:

- installed SDK home and layout;
- backend and toolchain discovery;
- supported host/RID facts;
- project metadata and managed assembly identity; and
- diagnostics shared with `cargo dotnet doctor`.

Keep command parsing, user-facing messages, and process execution in
`cargo-dotnet`.

### 3. Extract `rust-dotnet-assets`

Promote the existing NuGet asset machinery into the reusable asset layer:

- RID-specific runtime-target selection;
- collision detection and owned-asset manifests;
- output staging and cleanup; and
- package-path validation.

Generalize the input from only NuGet packages to a resolved asset source. This
must accommodate NuGet `runtimes/<rid>/native/` assets, local native libraries,
and system-library names without duplicating staging logic.

### 4. Keep `cargo-dotnet` deliberately thin

The CLI remains responsible for `new`, `build`, `run`, `test`, `doctor`,
`setup`, `pack`, and `push`; argument parsing; progress and errors; exit codes;
and orchestration. It should not retain reusable RID selection, package staging,
or P/Invoke marshalling policy.

### 5. Add first-class P/Invoke

`rust-dotnet-pinvoke` is a separate Rust project in this workspace, tightly
integrated with the backend and CLI. Its first public surface supports only:

- explicit C ABI primitive values, pointers, and `#[repr(C)]` structs;
- library name, entry point, calling convention, and last-error metadata;
- compile-time rejection of unsupported declaration shapes; and
- explicit UTF-8/UTF-16 and ownership helpers.

The backend lowers that declaration contract to the existing body-less CLR
P/Invoke (`ImplMap`) machinery. `cargo dotnet` uses `rust-dotnet-assets` to
select and stage the required library for each supported RID.

Do not initially promise automatic bindgen, arbitrary C++ APIs, callbacks,
variadics, automatic ownership conversion, or broad marshalling inference.

### 6. Validate one complete user journey

Ship a cross-platform SQLite fixture before declaring the feature ready:

1. Rust declares a native import.
2. `cargo dotnet` resolves or stages SQLite for Linux x64, macOS Apple Silicon,
   and Windows x64.
3. The emitted managed assembly calls it successfully.
4. `cargo dotnet pack` retains the required runtime assets.

This end-to-end acceptance is the feature gate. Individual crate builds are
necessary but not sufficient.

## Guardrails

- Preserve `cargo dotnet` command names, configuration, and installation flow
  during extraction.
- Move existing focused tests with every extraction and add a CLI acceptance
  test for unchanged behavior.
- Make each milestone independently reviewable and releasable.
- Do not advertise the existing `NATIVE_PASSTHROUGH` implementation as the
  public P/Invoke solution: it is WIP and Linux/GCC-oriented.
- Keep direct managed Rust-to-C#/F# interop as the default. P/Invoke is for
  Rust code that consumes a native library, not the preferred bridge for C#
  code consuming Rust.
