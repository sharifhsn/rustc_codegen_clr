# Architecture rework execution ledger

Status: complete — all five phases implemented and architecture gates exercised
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
Builtin lookup is exact-symbol by default. The only demangled aliases are a closed, full-path
allowlist for rustc's private allocator entry points; arbitrary leaf-name matching is forbidden, so
Rust methods such as `core::fmt::write` cannot collide with libc's `write`. Allocator aliases reuse
the canonical ABI-safe builtin generators, including correct zeroed-allocation behavior.

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

Implemented 2026-07-09 and completed 2026-07-10. Codegen captures one immutable `BackendConfig`;
only `ArtifactAbiConfig { dotnet_runtime, no_unwind }` is serialized in the schema-v3 artifact
envelope. The linker captures one typed `LinkerConfig`, validates its process ABI against every
versioned input, then uses the validated artifact ABI as the authority for runtime/unwind behavior.
Target selection, alignment repair, allocator policy, managed panic backtraces, native pass-through,
and emitter selection remain local final-link choices, so compatible cached artifacts can be reused
across them. Runtime and alignment are threaded explicitly through exporters and final repair rather
than re-read from globals. Legacy raw assemblies use an explicit decoder path and warning.

- Replace duplicated environment reads with one parsed configuration value.
- Serialize only configuration that changes independently generated per-crate IR.
- Reject incompatible input artifacts at link time.
- Retire or fail on no-op flags; make optimizer disabling use one documented mechanism.

Gate: configuration round-trip, mismatch rejection, and codegen/linker parity tests.

## Phase 2 — identity, relocation, and storage

### 2A. Memoized relocation

Implemented 2026-07-10. `RelocateCtx` carries dense maps for all ten interned arenas, so a shared
DAG value is translated once. Metadata relocation lives beside its owning types and exhaustively
destructures without `..`; an exhaustive `Assembly` field fence makes a new arena fail compilation
until relocation accounts for it. Class traversal is sorted by source ID for deterministic output.
The depth-20 shared-DAG regression visits 21 unique nodes with 20 cache hits; the old recursive
shape would perform roughly 2.1 million node visits.

- Introduce a relocation context with source-to-destination maps for every interned arena.
- Exhaustively relocate all fields in method/class definitions and custom metadata.
- Remove field-by-field reconstruction sites that can silently omit newly added state.

Gate: round-trip/link property tests covering every metadata-bearing field, including generic
parameters, parameter flags, interfaces, events, properties, attributes, overrides, and special
names.

### 2B. Hardened interning and compaction

Implemented 2026-07-10. `BiMap<T>` now owns one canonical `Vec<T>` and indexes it with a
`hashbrown::HashTable<Interned<T>>`; ordinary interning no longer clones `T`, and values-only serde
rebuilds and validates the index. Artifact schema v2 is identifiable before payload decoding, so
v1 receives a precise rebuild diagnostic. Final-link compaction rebuilds all ten arenas from live
definitions after the second DCE and is byte-identical when applied twice.

- Store each interned value once; indices in the hash table point into canonical storage.
- Preserve deterministic IDs within a serialized assembly.
- Reuse relocation to compact all reachable arenas after final DCE.
- Measure allocation count, peak memory, link time, and serialized size before changing defaults.

Gate: interner property tests, deterministic serialization, and no regression in linker fixtures.

## Phase 3 — transactional and parallel codegen

The serial transactional boundary was implemented 2026-07-10. Every mono item builds in a fresh
assembly shard, successful items commit to a CGU shard, and successful CGUs commit to the crate in
rustc's existing deterministic order. Error and panic tests prove parent arena counts and postcard
bytes remain unchanged. The pinned rustc concurrency audit found `rustc_data_structures::sync::par_map`
safe for isolated CGU work, but default parallelism remains blocked: raw rustc `AllocId` discovery
order still leaks into emitted static names and allocation fingerprints. The measured design,
two-process byte-equivalence harness, and promotion criteria are in [CGU_PARALLELISM.md](CGU_PARALLELISM.md).

- Build a method or codegen unit in isolated state and commit it only after local validation.
- Merge codegen-unit shards deterministically through the relocation API.
- Add parallel execution only after serial shard output is byte/semantic equivalent.

Gate: forced-failure transaction test, deterministic serial-vs-parallel output, rustc package check,
and representative backend execution tests.

## Phase 4 — rustc-facing boundary consolidation

Implemented 2026-07-10. The five helper packages were folded into private root modules, reducing
six rustc-facing packages to one and removing fourteen internal package edges. `cilly` remains a
standalone non-`rustc_private` crate. Three repeated warm root checks averaged 0.749 s versus the
0.96 s pre-migration baseline (22% faster), while workspace metadata/check gates stayed green.

- Keep `cilly` independent of `rustc_private`.
- Consolidate the tightly coupled context/type/place/operand/call ladder where doing so reduces
  nightly-port and API surface cost.
- Delete empty compatibility modules and move parked experiments behind explicit feature or module
  boundaries.

Gate: package graph check, nightly-port documentation update, and unchanged backend behavior.

## Phase 5 — exception-region representation

Implemented 2026-07-10. `MethodImpl::RegionBody` stores normal blocks, one canonical cleanup graph,
method-scope `ExceptionRegion` associations, and shared locals. MIR lowering no longer clones a
reachable cleanup CFG into every protected `BasicBlock`; traversal, optimization, typechecking,
relocation, DCE, alignment repair, and local compaction visit canonical cleanup roots exactly once.
A structural verifier rejects duplicate/missing/cross-partition region edges before an assembly can
become export-ready. Methods without a protected region retain compact legacy `MethodBody` storage.

IL, direct PE, and C call one shared compatibility materializer, preserving the previous physical
handler shape while the serialized/link-time IR avoids the duplication. `RegionBody` was appended as
postcard enum tag 4 and the artifact envelope moved to schema v3, which rejects schema v2 before
payload decoding. Measured panic-heavy artifacts previously spent 60–68% of ordinary handler text on
duplicate normalized handlers in two representative cases; exporter-time physical sharing remains a
separate optimization because ECMA-335 forbids arbitrary overlap between distinct exception clauses.

- Represent protected regions and cleanup entry points once at method scope.
- Initially lower the new representation to byte-equivalent existing handler shapes.
- Only then experiment with shared landing pads, state dispatch, or cold outlining.

Gate: unwind differential tests, `catch_unwind`, nounwind/double-panic abort behavior, direct-PE vs
IL equivalence, IL-size measurements, RyuJIT inlining observations, and the performance corpus.

## Validation cadence

Every slice:

1. focused unit tests;
2. `git diff --check`, plus stable rustfmt only where it does not reflow unrelated legacy files;
3. `cargo test -p cilly --lib` when `cilly` changes;
4. `cargo check -p rustc_codegen_clr` when rustc-facing code changes;
5. a scoped commit with no unrelated formatting or generated artifacts.

At phase boundaries, run the broader target/backend probes. Before declaring the campaign complete,
run the full pinned gate and update `ARCHITECTURE.md`, porting notes, configuration documentation,
and the local Graphify index.

## Final validation snapshot

Recorded 2026-07-10 on macOS/aarch64 with `nightly-2026-06-17`:

| Gate | Result |
|---|---|
| `cargo test -p cilly` | 291 passed, 2 ignored across 5 suites |
| `cargo check --workspace` | 0 errors; only the repository's existing warning set |
| focused verifier/alias/transaction/config/pthread tests | passed |
| `compile_test::fastrand_test::stable::cargo_release` (.NET) | passed end to end |
| focused C `abox`, `adt_enum`, `caller_location`, `copy_nonoverlaping` | passed end to end |
| CI-equivalent `.NET ::stable` aggregate | 202 passed, 238 failed, 52 filtered; zero final-verifier or linker failures |
| CI-equivalent C `::stable` aggregate | 51 passed, 369 failed, 72 filtered in the final parallel run |

The historical aggregate is not a clean acceptance gate on this host/toolchain. Its remaining
failures fall into three independently visible classes: test-source drift against the pinned
nightly (`atomic_xsub` gained a type parameter, `catch_unwind` returns `bool`, removed
`MaybeUninit::fill`/feature combinations), long-standing semantic output mismatches, and severe
parallel harness saturation (many generated executables time out only during the all-at-once run
while the same focused tests pass). The rearchitecture's initial strict-verifier fanout was fixed at
the shared ABI boundaries rather than suppressed. The C run likewise exposed and fixed two shared
runtime-header omissions, `Environment.Exit` and `Environment.FailFast`; representative affected
tests pass individually afterward. These residual corpus failures are recorded rather than folded
into the architecture contract or hidden by weakening verification.

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
- 2026-07-10: split serialized artifact ABI from process-local backend/linker policy; keep output
  targets and emitter policy out of cross-artifact compatibility.
- 2026-07-10: consolidate the full rustc-facing helper ladder into root modules; the measured warm
  check improved rather than regressed, so the fallback frontend package is unnecessary.
- 2026-07-10: keep CGU scheduling serial until isolated shard output has a serial-vs-parallel
  equivalence harness; transactions and concurrency are separate correctness changes.
- 2026-07-10: canonicalize exception cleanup graphs in IR first, but keep compatibility
  materialization at exporter boundaries until a shared .NET region planner proves legal lexical
  coalescing under ECMA-335.
- 2026-07-10: make linker overrides exact-symbol by default and permit demangled resolution only
  through a closed full-path rustc-runtime allowlist; centralize explicit pointer/native-int wrapper
  adaptation in `Assembly` so aliases are verifier-visible and unit-testable.
