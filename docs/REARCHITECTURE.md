# Architecture rework execution ledger

Status: active — Phase 1 implemented; Phase 2 in progress  
Started: 2026-07-09  
Branch: `codex/rearchitecture`

This document is the execution contract for a staged rework of the compiler and `cilly`
architecture. It complements `ARCHITECTURE.md`; it does not replace the description of the current
design. Each phase below must preserve the invariants and pass its gates before a dependent phase
starts.

## Objectives

1. Make the assembly lifecycle explicit and require verification after the final linker mutation.
2. Replace ad-hoc cross-assembly reconstruction with exhaustive, memoized relocation.
3. Parse build configuration once and carry one immutable contract through codegen and linking.
4. Resolve generated builtin dependencies to a fixed point instead of relying on a pass count.
5. Make method/codegen-unit construction transactional, then use the isolation for parallel codegen.
6. Reduce the nightly-sensitive rustc-facing crate surface without weakening the `cilly` boundary.
7. Represent exception regions without eagerly copying cleanup CFGs, then optimize their final
   lowering only after equivalence is proven.

## Non-negotiable invariants

- MIR lowering remains faithful before optimization.
- `Type`, `CILNode`, `CILRoot`, and rustc `TyKind` handling remains explicitly exhaustive. A new
  variant must continue to force compile-time updates at correctness-critical sites.
- The single interned CIL-tree IR remains the compiler IR; this work does not reintroduce V1/V2 or
  add another MIR-like layer.
- Direct PE, textual IL, C, and the existing Java path retain their current behavior unless a phase
  explicitly adds a gated capability.
- Unsupported behavior fails loudly or produces the existing intentional runtime stub. A refactor
  must not convert an existing loud failure into silent output.
- A linked artifact is never emitted after an unchecked mutation.
- Serialized assembly format changes are versioned or rejected with a specific diagnostic; mixed
  incompatible build contracts never link silently.
- Existing user work and local-only `graphify-out/` artifacts are not committed as part of the
  rework.

## Baseline

Recorded before the first source edit:

| Gate | Result |
|---|---|
| `cargo test -p cilly --lib` | 218 passed |
| `cargo check -p rustc_codegen_clr` | 0 errors; 2 pre-existing unused-variable warnings in `builtins/simd/eq.rs` |

The final campaign gate remains the repository's pinned Docker/`::stable` workflow plus focused
interop, C-mode, direct-PE, and performance probes appropriate to the changed surface.

## Phase 1 — safety boundaries

### 1A. Fixed-point builtin resolution

Implemented 2026-07-09. One growable worklist now replaces the linker's two hard-coded passes,
processes patcher-created references to a fixed point, and reports explicit resolution statistics.

- Replace the linker's two hard-coded `patch_missing_methods` calls with one fixed-point API.
- Process each method reference at most once per resolution run.
- Continue until generated implementations introduce no unseen references.
- Return resolution statistics and retain intentional `MethodImpl::Missing` behavior.

Gate: `cilly` unit suite plus tests where a generated builtin introduces another missing reference.

### 1B. Verified assembly lifecycle

Implemented 2026-07-09. `Assembly::verify_for_export` consumes mutable linker state into an
`ExportReadyAssembly`; the linker seals after its final mutation and direct-PE rendering rechecks
after its remaining export-only interning before returning bytes.

- Separate mutable construction/linking state from emission-ready state with consuming APIs or
  phase wrappers.
- Run the definitive fatal verifier after builtin patching, DCE, optimization, and alignment repair.
- Allow earlier checks for diagnostic locality, but make final verification non-optional for normal
  exporters.

Gate: negative test proving a post-link mutation cannot reach an exporter without verification.

### 1C. Immutable build contract

Implemented 2026-07-09 as the first consumer-complete slice. Codegen captures one immutable
`BuildConfig`, serializes it in a magic/version artifact envelope, and the linker rejects both
cross-artifact and artifact/process mismatches before builtin synthesis. Legacy raw assemblies use
an explicit decoder path and warning. Remaining configuration `LazyLock` consumers are temporarily
safe because the linker proves their process environment equal to the serialized contract; routing
them directly through the value is follow-up cleanup.

- Replace duplicated environment reads with one parsed configuration value.
- Serialize target-affecting configuration into codegen artifacts.
- Reject incompatible input artifacts at link time.
- Retire or fail on no-op flags; make optimizer disabling use one documented mechanism.

Gate: configuration round-trip, mismatch rejection, and codegen/linker parity tests.

## Phase 2 — identity, relocation, and storage

### 2A. Memoized relocation

In progress. Before switching implementations, regressions were added for two confirmed silent
losses in the old constructor-based translator: unresolved `BasicBlock::handler_id` and
`ClassDef::is_valuetype_authoritative`. Both are now preserved by the existing linker as a safety
floor for the memoized replacement.

- Introduce a relocation context with source-to-destination maps for every interned arena.
- Exhaustively relocate all fields in method/class definitions and custom metadata.
- Remove field-by-field reconstruction sites that can silently omit newly added state.

Gate: round-trip/link property tests covering every metadata-bearing field, including generic
parameters, parameter flags, interfaces, events, properties, attributes, overrides, and special
names.

### 2B. Hardened interning and compaction

- Store each interned value once; indices in the hash table point into canonical storage.
- Preserve deterministic IDs within a serialized assembly.
- Reuse relocation to compact all reachable arenas after final DCE.
- Measure allocation count, peak memory, link time, and serialized size before changing defaults.

Gate: interner property tests, deterministic serialization, and no regression in linker fixtures.

## Phase 3 — transactional and parallel codegen

- Build a method or codegen unit in isolated state and commit it only after local validation.
- Merge codegen-unit shards deterministically through the relocation API.
- Add parallel execution only after serial shard output is byte/semantic equivalent.

Gate: forced-failure transaction test, deterministic serial-vs-parallel output, rustc package check,
and representative backend execution tests.

## Phase 4 — rustc-facing boundary consolidation

- Keep `cilly` independent of `rustc_private`.
- Consolidate the tightly coupled context/type/place/operand/call ladder where doing so reduces
  nightly-port and API surface cost.
- Delete empty compatibility modules and move parked experiments behind explicit feature or module
  boundaries.

Gate: package graph check, nightly-port documentation update, and unchanged backend behavior.

## Phase 5 — exception-region representation

- Represent protected regions and cleanup entry points once at method scope.
- Initially lower the new representation to byte-equivalent existing handler shapes.
- Only then experiment with shared landing pads, state dispatch, or cold outlining.

Gate: unwind differential tests, `catch_unwind`, nounwind/double-panic abort behavior, direct-PE vs
IL equivalence, IL-size measurements, RyuJIT inlining observations, and the performance corpus.

## Validation cadence

Every slice:

1. focused unit tests;
2. `cargo fmt --all -- --check`;
3. `cargo test -p cilly --lib` when `cilly` changes;
4. `cargo check -p rustc_codegen_clr` when rustc-facing code changes;
5. a scoped commit with no unrelated formatting or generated artifacts.

At phase boundaries, run the broader target/backend probes. Before declaring the campaign complete,
run the full pinned gate and update `ARCHITECTURE.md`, porting notes, configuration documentation,
and the local Graphify index.

## Decision log

- 2026-07-09: preserve the single CIL-tree IR and explicit exhaustive matches.
- 2026-07-09: establish final verification and fixed-point resolution before changing interning or
  codegen concurrency.
- 2026-07-09: treat exception sharing as the final, measurement-driven phase because it has the
  greatest semantic and runtime risk.
- 2026-07-09: use a versioned envelope around the unchanged postcard `Assembly` payload, retaining
  an explicit legacy path while allowing the inner IR schema to evolve deliberately.
- 2026-07-09: sequence relocation before the single-storage interner so every identity/storage
  change goes through one exhaustive, memoized mapping boundary.
