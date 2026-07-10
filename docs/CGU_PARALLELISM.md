# CGU parallelism feasibility audit

Audit target: `nightly-2026-06-17`, rustc `1.98.0-nightly`
(`9e2abe0c6ab27fcbb95c30695188a75776e2feb1f`).

## Verdict

**GO for an opt-in equivalence prototype. NO-GO for default enablement today.**

The pinned compiler explicitly supports concurrent CGU work through rustc's own dynamic-parallel
API. `TyCtxt`, query TLS, worker-local arenas, profiling, jobserver coordination, and panic handling
are compatible when work is scheduled with that API. The backend's isolated item/CGU assemblies
also provide the right ownership boundary.

Default enablement is blocked by deterministic output identity: allocation lowering still embeds
raw rustc `AllocId`s in emitted names/fingerprints, while rustc assigns newly discovered IDs through
an atomic counter. Parallel query discovery can therefore produce byte-different assemblies without
any data race or semantic query failure.

## Use rustc's `par_map`, not an independent pool

Resolve the pinned source root with `rustc --print sysroot`; paths below are relative to
`$SYSROOT/lib/rustlib/rustc-src/rust/compiler/`.

Pinned evidence:

- `rustc_middle/src/ty/context.rs:684-685`: `TyCtxt` explicitly implements rustc's `DynSend` and
  `DynSync` traits.
- `rustc_data_structures/src/sync/parallel.rs:223`: `par_map` accepts `DynSend` inputs/results and a
  `DynSend + DynSync` closure. It stores results in input positions before collecting them, so its
  returned sequence preserves input order.
- `rustc_codegen_ssa/src/base.rs:14,799-803`: the upstream SSA backend imports
  `IntoDynSyncSend, par_map` and invokes `backend.compile_codegen_unit(tcx, ...)` inside `par_map`.
- `rustc_middle/src/ty/context/tls.rs:34-35`: rustc's thread-pool local value carries the
  `ImplicitCtxt` into pool jobs.
- `rustc_interface/src/util.rs:285-293`: rustc constructs the scoped worker pool and installs
  session globals on each worker.
- `rustc_interface/src/interface.rs:388-390` and `rustc_session/src/session.rs:824-831`: dynamic
  synchronization is enabled by `-Z threads=N`; without it, rustc's parallel operations degrade to
  serial execution.
- `rustc_data_structures/src/profiling.rs:170-177`: `SelfProfilerRef` is documented as cloneable and
  sendable across thread boundaries. Existing per-item `tcx.prof.generic_activity_with_arg` scopes
  can remain inside workers.

The implementation import should mirror rustc:

```rust
use rustc_data_structures::sync::{IntoDynSyncSend, par_map};
```

Use `IntoDynSyncSend<CguShard>` only if the compiler cannot infer the dynamic marker implementation
for the otherwise ordinary `Send` result. Do not call external Rayon, `std::thread`, or
`rustc_thread_pool` directly: those paths do not establish rustc query TLS, `WorkerLocal`
registration, session globals, jobserver behavior, or rustc's coordinated panic semantics.

## Ownership and deterministic merge design

The current serial boundary is `src/lib.rs:251-263`; item and CGU rollback behavior is implemented
by `src/assembly_transaction.rs`, including the order-sensitive regression at lines 86-110.

Extract one worker shared by both modes:

```rust
struct CguShard {
    ordinal: usize,
    name: String,
    item_count: usize,
    size_estimate: usize,
    assembly: Assembly,
    diagnostics: Vec<ItemDiagnostic>,
}

fn compile_cgu_shard<'tcx>(
    tcx: TyCtxt<'tcx>,
    ordinal: usize,
    cgu: &'tcx CodegenUnit<'tcx>,
) -> CguShard;
```

Required scheduling rules:

1. Snapshot `(original_ordinal, &CodegenUnit)` from `cgus.codegen_units`.
2. Keep `cgu.items()` serial inside a shard and preserve its `FxIndexMap` iteration order.
3. In serial mode, map the snapshot through `compile_cgu_shard` normally.
4. In experimental parallel mode, pass the same snapshot and worker to `par_map`.
5. Carry the original ordinal even if work is size-sorted for load balancing. Sort/assert returned
   shards by that ordinal before commit.
6. Merge assemblies only on the coordinator, in the exact original CGU order, through
   `Assembly::link`. Never merge from workers, a mutex, or channel receive order.
7. Emit buffered recoverable diagnostics in `(CGU ordinal, item ordinal)` order.
8. Synthesize the entry wrapper and insert crate-wide FFI/runtime definitions only after all CGUs
   have merged.

Merge order is observable: it determines stable interned IDs, last-writer section behavior,
metadata first-registration/dedup order, and cctor/tcctor/user-init concatenation. Parallelism owns
only shard construction, not crate assembly mutation.

Initially require all of:

- an explicit backend mode such as `RCL_CGU_MODE=parallel`;
- `tcx.sess.threads().is_some_and(|threads| threads > 1)`;
- at least two non-empty CGUs;
- `ABORT_ON_ERROR=0` until multi-panic selection is made deterministic.

The existing `ABORT_ON_ERROR=1` path lets the original panic escape. Rustc's parallel guard waits
for all jobs and may observe more than one panic, so that mode should stay serial initially or be
redesigned to return indexed failures and rethrow the earliest serial-order failure.

## Determinism blocker: raw `AllocId`

Pinned rustc uses an atomic allocator:

- `rustc_middle/src/mir/interpret/mod.rs:412-447`: `AllocMap` stores `next_id: AtomicU64`, and
  `reserve()` assigns IDs with `fetch_add(Ordering::Relaxed)`.
- The same file's lines 490 and 520 expose lazy static and memory reservation.

Backend output currently consumes those schedule-sensitive IDs:

- `src/assembly.rs:554`: static compilation calls `reserve_and_set_memory_alloc`.
- `src/rvalue.rs:386`: thread-local/static references call `reserve_and_set_static_alloc`.
- `src/operand/static_data.rs:377-398`: immutable allocation fingerprints include relocation-target
  `AllocId`s, and mutable allocation names include the allocation's raw ID.

The rustc maps are synchronized; this is an output-identity problem, not unsafe concurrent access.
Serially pre-forcing a guessed query list is not a durable fix because constants, promoted values,
vtables, and provenance recursively discover allocations.

Before promotion, replace raw IDs in emitted identity with a cycle-aware semantic fingerprint:

- functions: stable symbol/instance identity;
- statics: stable definition/symbol identity;
- vtables: type plus trait-ref identity;
- immutable memory: bytes, alignment, length, and recursively fingerprinted provenance targets;
- distinct mutable anonymous allocations: a deterministic source identity, not discovery order.

## Two-process equivalence and stress harness

Run serial and parallel builds in separate rustc processes so one run cannot seed the other's
global allocation/query state.

For each fixture, build with identical source/configuration and explicit CGU count:

```text
serial:   RCL_CGU_MODE=serial   -C codegen-units=16 -Z threads=1
parallel: RCL_CGU_MODE=parallel -C codegen-units=16 -Z threads=4
```

Add an opt-in coordinator-side shard dump keyed by original ordinal. The harness must compare:

1. compacted postcard bytes for every corresponding CGU shard;
2. final merged pre-entry assembly bytes;
3. final verified `.cilly2` bytes;
4. arena counts and relocation/compaction statistics;
5. normalized IL and emitted C text;
6. .NET and C runtime stdout, stderr, and exit status.

Byte equality is the primary invariant. Runtime/semantic equality is an additional gate, not a
replacement for deterministic bytes.

Repeat the parallel build at least ten times with deterministic test-only jitter derived from
`(seed, CGU ordinal)` so completion order varies. Test `-Z threads=2,4,8` against
`-C codegen-units=1,2,4,16`. Include fixtures for:

- a one-CGU control;
- `cargo_tests/soak_serde_json` as a real multi-CGU workload;
- mutable statics, promoted immutable allocations, cyclic provenance, vtables, function pointers,
  and `TypeId`;
- cctor/tcctor/user-init and duplicate section-key ordering;
- recoverable item failures and uncaught panics;
- representative `::stable`, C-mode, and direct-PE/IL execution.

## Measured granularity

The measurements used the pinned monomorphization partitioner with
`-Z print-mono-items=yes -C codegen-units=16`.

- `test/binops.rs`: 15 functions, all in one CGU despite requesting 16. There is no useful CGU
  parallelism for this class of small test.
- `cargo_tests/soak_serde_json`: 531 unique mono-item lines and 645 CGU assignments including local
  copies across 16 CGUs. Assignment counts were:

  ```text
  149, 125, 100, 75, 29, 29, 19, 19, 17, 17, 15, 15, 13, 13, 6, 4
  ```

  The largest four contain 69.6% of assignments. Treating assignments as equal-cost gives an ideal
  upper bound of `645 / 149 = 4.33x`; real speedup will be lower because mono items vary greatly in
  cost and ordered final merging remains serial.

Parallel construction does not add a new per-item transaction, but it retains several completed
CGU assemblies concurrently and increases simultaneous rustc query/MIR working sets. Measure peak
RSS as well as wall time before choosing an automatic size threshold.

## Promotion criteria

Keep serial scheduling as the default until all of these hold:

1. No emitted name or fingerprint depends on raw `AllocId` discovery order.
2. Ten schedule-perturbed parallel runs are byte-identical to the serial baseline per shard and
   after final linking.
3. `cargo check -p rustc_codegen_clr` passes both without `-Z threads` and with `-Z threads=4`.
4. Transaction rollback, ordered diagnostics, special initializers, and section collision tests
   pass in both modes.
5. Full `::stable`, representative cargo/soak fixtures, C mode, and direct-PE/IL runtime outputs are
   identical.
6. No small-workload wall-time regression above 5%, and at least a 10% median improvement on a
   representative multi-CGU workload.
7. Peak RSS remains within an explicitly accepted budget (initial recommendation: no more than
   1.5x the serial peak on the representative workload).
8. Results remain identical across `threads=2,4,8` and `codegen-units=1,2,4,16`.

Only after these gates should parallel mode become automatic; retain an explicit serial escape
hatch for nightly-port diagnosis.
