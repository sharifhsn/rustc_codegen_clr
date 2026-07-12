# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## What this is

`rustc_codegen_clr` is an experimental **rustc codegen backend** (a `-Z codegen-backend` plugin) that
compiles Rust to **.NET assemblies** or, in an alternate mode, to **C source**. It is compiled as a
`dylib` and loaded by `rustc`; it does not produce binaries on its own. The project is early-stage —
miscompilations and crashes are expected, and that informs much of the design (see "Design principles").

> **Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first.** It is a codebase-oriented digest of the
> author's (FractalFir's) blog series explaining *why* the project is built the way it is — the
> CIL-trees IR, the V1→V2 split, how Rust constructs map to .NET, panics/unwinding, and the many
> .NET-mapping gotchas. The articles themselves are archived locally in
> [docs/fractalfir_articles/](docs/fractalfir_articles/) ([index](docs/fractalfir_articles/README.md)).
> The summary below is the quick reference; ARCHITECTURE.md is the deeper context.

## Toolchain

The project pins a specific **nightly** (`rust-toolchain.toml`, currently bare `channel = "nightly"`)
and uses internal compiler crates (`rustc_private`, `rustc-dev`, `rust-src`). It only compiles against
a narrow window of nightly versions — if `cargo check` fails with rustc internal errors, the cause is
almost always rustc-internal API drift, not your environment. Because `rust-toolchain.toml` is unpinned,
a fresh checkout grabs *today's* nightly, so the project bit-rots if left untouched; updating to a new
rustc version is a recurring maintenance task (see git log: "Updated rustc version").

This tree has been ported to compile on **`nightly-2026-06-17`** (it had drifted ~8 months). The exact
rustc-API changes and a re-port playbook are in [feasibility/PORT_NOTES.md](feasibility/PORT_NOTES.md);
[feasibility/](feasibility/) also has a Dockerfile + harness that builds the workspace in the on-path
Linux env. Key insight for maintenance: only the thin rustc-facing crates (`…_type`, `…_operand`,
root `src/`) rot — the `cilly` IR core compiles unchanged across nightly gaps. Read PORT_NOTES before
the next bump; most fixes are mechanical renames you can resolve against the local `rustc-src`
(`$(rustc --print sysroot)/lib/rustlib/rustc-src`).

## Common commands

```bash
cargo build                       # builds the backend dylib (librustc_codegen_clr.so) + the `linker` binary
cargo build --release             # release backend (what tests/real use expect)
./build_all.sh                    # builds cilly (all targets) then the root crate, debug + release
cargo check                       # verify compatibility with the installed nightly

cargo test ::stable               # run the STABLE test suite (compiles test programs, runs them, diffs output)
cargo test binops::stable         # run a single named test group
cargo test binops::stable::release  # narrow to just the release variant
C_MODE=1 cargo test ::stable      # run the suite in C-output mode instead of .NET

./bin/rustflags.rs                # checks deps (dotnet/ilasm) and prints the RUSTFLAGS to use the backend
./bin/rustflags.rs --setup_command  # prints just the `export RUSTFLAGS=...` line
./setup_rustc_fork.sh             # clones rust-lang/rust at the matching version (for core/std/alloc test suites)
```

Running a project with the backend means setting `RUSTFLAGS` to point `-Z codegen-backend` at the built
`.so` and `-C linker` at the built `linker` binary (use `bin/rustflags.rs` to generate the exact string),
then `cargo run`. The backend makes no permanent changes to the rustc install — it is per-shell-session.

CI (`.github/workflows/rust.yml`): builds on Linux + Windows, then runs `cargo test ::stable` with a set of
`--skip` filters for known-flaky/unsupported areas (`f128`, `num_test`, `simd`, `fuzz87`, …), plus a
separate `C_MODE=1` run. The project is primarily tested on **Linux x86_64 / .NET 8 CoreCLR**.

## Requirements for running tests

.NET mode needs both the **.NET runtime** (`dotnet`) and an **IL assembler** (`ilasm`, easiest via the Mono
runtime). C mode needs a C compiler (GCC/Clang fully supported; tcc/sdcc partial). On a machine without
.NET, set `DRY_RUN=1` to compile without linking/executing.

## Configuration via environment variables

Behavior is controlled by env-var flags defined with the `config!` macro in [src/config.rs](src/config.rs)
(and `cilly`'s own `config`). The important ones:

- `C_MODE=1` — emit C source instead of .NET CIL. `JS_MODE=1` / (JVM exporter) are other backends.
- `OPTIMIZE_CIL=0` — **disable the CIL optimizer.** Do this first when debugging a miscompilation: it makes
  the generated CIL map 1:1 back to MIR statements (see "Design principles").
- `TEST_WITH_MONO=1` — also run tests under Mono. `DRY_RUN=1` — compile only, don't link/execute.
- `ABORT_ON_ERROR`, `ALLOW_MISCOMPILATIONS`, `VERIFY_METHODS`/`TYPECHECK_CIL`, `INSERT_MIR_DEBUG_COMMENTS`,
  `PRINT_LOCAL_TYPES`, `TRACE_CIL_OPS` — debugging/strictness knobs.
- `ASCI_IDENT` — force ASCII-only symbol names (needed for compilers that reject mangled Unicode idents).

## Architecture

### Compilation pipeline

1. Entry point is `__rustc_codegen_backend()` in [src/lib.rs](src/lib.rs), returning `MyBackend: CodegenBackend`.
2. `codegen_crate` iterates the crate's codegen units and calls `assembly::add_item` for each mono item.
   **The actual translation begins in `assembly::add_item` / `add_fn`** ([src/assembly.rs](src/assembly.rs)) —
   not really in `lib.rs`, which is mostly rustc plumbing (receiving MIR, serializing, linking).
3. `add_fn` walks MIR: each statement goes through `handle_statement` ([src/statement.rs](src/statement.rs))
   and each block terminator through `handle_terminator` ([src/terminator/](src/terminator/)), producing CIL.
4. `join_codegen` converts the produced IR with `cilly::Assembly::from_v1`, runs `.opt()` and `.typecheck()`,
   and serializes the result (postcard) into a `.bc`, bundled into an `.rlib`.
5. The **`linker`** binary (set via `-C linker=`) loads the serialized assemblies from rlibs, merges them,
   patches in libc/intrinsic implementations, and emits the final `.NET` executable or C output.

### Two-level IR (this is the key concept)

The `cilly` crate defines the IR. There are two generations:

- **V1 IR** — a "CIL trees" tree IR: pure value-producing `CILNode`s and side-effecting `CILRoot`s
  ([cilly/src/cil_node.rs](cilly/src/cil_node.rs), [cilly/src/cil_root.rs](cilly/src/cil_root.rs)).
  Only a root may write to a local/address; non-`Call` nodes have fixed arity validated at construction,
  so malformed ops are structurally impossible. This is what the rustc backend produces directly.
- **V2 IR** — an **interned / hash-consed** IR under [cilly/src/v2/](cilly/src/v2/), addressed by
  `Interned<T>` handles into a `BiMap`. `Assembly::from_v1` converts V1→V2; optimization
  ([cilly/src/v2/opt/](cilly/src/v2/opt/)), type checking ([cilly/src/v2/typecheck.rs](cilly/src/v2/typecheck.rs)),
  and all exporters operate on V2. V2 is what gets serialized.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) §3 for why the IR became trees and the V1/V2 rationale.

Exporters live under `cilly/src/v2/`: `il_exporter` (.NET CIL via ilasm), `c_exporter` (C source),
`java_exporter` (JVM bytecode). The same V2 assembly drives all targets — this is why C support reuses
almost the entire codebase, and why a .NET bugfix usually fixes C too.

### Workspace crates

The codegen is split across small crates so the heavy `rustc_private` dependencies are isolated:

| Crate | Role |
|-------|------|
| `rustc_codegen_clr` (root, `src/`) | The rustc backend plugin: MIR → V1 CIL, assembly assembly. |
| `cilly` | The IR itself + optimizer + typechecker + exporters + the `linker` binary. Standalone, no rustc dep. The heart of the project. |
| `rustc_codegen_clr_ctx` | `MethodCompileCtx` — per-method compilation context wrapping `TyCtxt` + `Assembly`. Threaded everywhere. |
| `rustc_codegen_clr_type` | Rust `Ty` → cilly `Type`, ADT layout, and the `TyCache`. |
| `rustc_codegen_clr_place` | MIR `Place` handling (address / get / set). |
| `rustc_codgen_clr_operand` | MIR `Operand` and constant handling. **Note: the crate name is misspelled (missing the second `e`) — this is intentional, do not "fix" it.** |
| `rustc_codegen_clr_call` | Function signatures and call ABI (`CallInfo`). |
| `mycorrhiza` | Rust/.NET interop layer — Rust-side wrappers for managed types (StringBuilder, Console, …). |
| `dotnet_aot` | Native AOT support helpers. |
| `AssemblyUtilis` | C# helper code (`.csproj`) for assembly building / managed handles. |

### Design principles (these explain non-obvious code choices)

- **Functional / pure**: each MIR element is handled by a pure function taking immutable inputs and returning
  a translated item. This makes panic recovery trivial (no half-mutated state) — important because the backend
  expects to hit unsupported code. The notable exception is the mutable `TyCache`, reused per codegen unit and
  resettable after a panic.
- **Faithful-to-MIR then optimize**: V1 translation is deliberately precise-but-inefficient — every MIR
  statement maps to a fixed, isolated block of CIL ops, so malformed CIL traces straight back to one MIR
  statement. Reordering/eliminating ops is the optimizer's job (V2 `opt`). When chasing a bug, set
  `OPTIMIZE_CIL=0` to keep that 1:1 mapping.

### How Rust constructs map to .NET, and the gotchas

This is where most of the subtlety (and bugs) live: monomorphized generics fall back to name mangling
because .NET bans explicit layout on generics; enums become `[FieldOffset]` tagged unions; fat
pointers/DSTs vs the thin-pointer `TyKind::Foreign` case (`DATA_PTR`/`METADATA`/`ENUM_TAG`); ZSTs have
no .NET equivalent; `#[track_caller]` makes `FnSig` ≠ `FnAbi`; atomics → `Interlocked`; sign-agnostic
stack and saturating-vs-wrapping casts; and **panics/unwinding** map to .NET try/catch with cleanup
blocks duplicated per handler (`NO_UNWIND` strips them, a large perf win). Each of these is explained
with the originating bug/decision in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) §5–6 — consult it
before touching type lowering, the exporters, or unwinding code.

## Tests

- `test/` holds standalone `.rs` test programs; `cargo_tests/` holds full cargo crates (glam, rapier, the
  guessing game, std builds, …). The harness in [src/compile_test.rs](src/compile_test.rs) compiles each with
  the backend, runs it under .NET (or C), and asserts output matches native Rust.
- Tests are generated by the `test_lib!`, `run_test!`, and `cargo_test!` macros, which expand to modules named
  `<name>::<stable|unstable>::{release,debug}`. That is why `cargo test ::stable` selects the stable subset.
- [BROKEN_TESTS.md](BROKEN_TESTS.md) lists known-broken stdlib tests and the minimization workflow. **Always
  `cargo clean` when switching between a native build and a backend build of the same test** — stale artifacts
  produce misleading results. The `bin/success_*.txt` / `bin/c_success_*.txt` files are the recorded
  passing-stdlib-test lists.
