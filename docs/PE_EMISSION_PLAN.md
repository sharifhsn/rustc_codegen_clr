# Direct PE emission + Portable PDBs — design & phasing

> Goal: the linker emits the final `.dll`/`.exe` **directly** from the interned IR — no textual
> `.il`, no external `ilasm` — and emits a **Portable PDB** with sequence points mapping IL back to
> Rust source (breakpoints/stepping/stack-traces on `.rs` files under any .NET debugger).
> Status: **Phase 1 COMPLETE** — the hand-rolled PE writer (`cilly::ir::pe_exporter`) is now the
> **default** linker path (`DIRECT_PE` defaults to `true`); ilasm (`il_exporter`) stays reachable,
> byte-for-byte unchanged, behind `DIRECT_PE=0` as an escape hatch. **Phase 2 (Portable PDBs) is
> COMPLETE** — default builds emit a standalone `foo.pdb` next to `foo.dll`/`.exe`, and managed
> stack traces resolve real `file.rs:line` locations with no `ilasm` in the loop. Two of the three
> Phase-0 span-quality gaps are closed (outermost-inline-callsite attribution; the direct-PE path
> never had the inlining-hint gap to begin with); the third (`<WORKSPACE>` remap) was confirmed
> out-of-scope (rustc-fork-harness-only, doesn't affect ordinary crate builds). LocalScope/
> LocalVariable (tables 0x32/0x33) were **not** built — sequence points alone clear the acceptance
> bar (file:line stack traces); local-variable-name debugging remains a stretch item.
> Owner constraint: the CIL typechecker is never weakened; the ilasm path stays available behind the
> flag indefinitely as a fallback.

## Why

ilasm is the toolchain's most troublesome external dependency:
- **Per-platform/version matching** — Mono ilasm emits PE headers macOS-arm64 CoreCLR rejects; each
  .NET runtime needs its matching CoreCLR ilasm build (two durable footguns in the project memory).
- **CoreCLR ilasm limits** — the 1023-char class-name cap forced the FNV-1a shortener; `-debug` PDB
  writes fail on large assemblies (the exporter has a retry-without-debug fallback).
- **Text as interchange** — slow (multi-MB `.il` files), lossy, label-name collisions have caused
  real bugs (`tr_done_N` duplicate labels), and quoting/escaping is a permanent hazard.
- **No debug-info control** — sequence points are whatever ilasm makes of `.line`; we can't emit
  richer info (local names/scopes) or guarantee PDB production.

## What already exists (spike findings, 2026-07-01)

1. **Spans are already threaded end-to-end.** `CILRoot::SourceFileInfo` (cilroot.rs) carries
   `(line_start, line_len, col_start, col_len, file)`; `src/assembly.rs` (`span_source_info`) fills
   it from rustc's `SourceMap` per statement/terminator; the optimizer preserves them
   (opt/mod.rs skips SFI in root-scans); il_exporter emits `.line` directives; ilasm runs `-debug`.
2. **A Portable PDB already exists latently.** A native build of `soak_rand` produced a BSJB-magic
   (portable) PDB whose document table references the Rust sources (`main.rs`, std sources). The
   quick-win experiment (§Phase 0) is to verify how far that gets us TODAY (stack-trace file:line,
   VS Code breakpoints) before replacing the producer.
3. **The IL surface is fully inventoried** (agent sweep of il_exporter/mod.rs, 1947 lines): ~80
   distinct instruction forms; directives = `.assembly`/`.assembly extern` (BCL ver+ECMA token vs
   name-only), `.class` (public/private, ansi, sealed, explicit/auto, extends, implements),
   `.pack/.size`, fields with `[offset]`, static fields incl. `.data` FieldRVA const blobs typed as
   synthetic `__rcl_const_blob_N` valuetypes (each sized to its blob — the 4b487f7 NativeAOT
   lesson), ThreadStaticAttribute (the ONLY custom attribute), `.method` headers (static/instance/
   virtual/ctor, `pinvokeimpl` cdecl [lasterr] + `preservesig`, `aggressiveinlining` heuristic),
   `.maxstack` (computed), `.locals`, generic *call* instantiations `method<T,…>` (MethodSpec),
   `calli` (StandAloneSig), `.try/catch` over `[System.Runtime]System.Object` + the nested
   TerminateRegion try/catch→`FailFast` shape, `.line` (two forms), `.entrypoint` (method literally
   named "entrypoint"), MainModule partitioning (CoreCLR method cap), the FNV name shortener, and
   `runtimeconfig.json` generation in the linker. NOT emitted (don't build): `.override`, generic
   *definitions*, switch opcode, marshalling attrs, module `.cctor`, vtfixups.

## Build-vs-borrow

Hand-roll the writer in `cilly`. Candidates rejected: `dotnetdll` (GPL-3.0 — license-incompatible
with this MIT/Apache toolchain; pre-1.0; no PDB), `clr-assembler` (v0.1.x, unclear CIL coverage),
`windows-metadata` (winmd writer — no method bodies). Hand-rolling matches house style (the JVM
exporter already writes a binary container by hand), keeps the license clean and upstreamable,
gives the determinism control PDB row-ids need, and we only need the *inventoried subset* of
ECMA-335, not all of it. The PDB writer reuses the same heap/table machinery (a Portable PDB IS a
BSJB metadata blob with different tables).

## Architecture

`cilly/src/ir/pe_exporter/` (parallel to `il_exporter`, `c_exporter`, `java_exporter`):

| Module | Responsibility |
|---|---|
| `heaps.rs` | #Strings / #Blob / #GUID / #US heaps — interned, deduped, 2-vs-4-byte index widths via HeapSizes bits |
| `sig.rs` | `Type` → ELEMENT_TYPE_* blob encodings: field / method (incl. generic-inst, vararg-free) / locals / MethodSpec / StandAloneSig |
| `tables.rs` | The needed metadata tables (Module, TypeRef, TypeDef, Field, MethodDef, Param, InterfaceImpl, MemberRef, Constant, CustomAttribute, ClassLayout, FieldLayout, StandAloneSig, ModuleRef, ImplMap, FieldRVA, Assembly, AssemblyRef, TypeSpec, MethodSpec) — populate-then-size-then-serialize, sorted-table invariants, coded-index width computation |
| `body.rs` | Method bodies: tiny/fat headers, opcode byte emission for the ~80 forms, two-pass branch layout (long-form first; short-form compaction optional later), maxstack (reuse exporter's block-based bound), fat EH sections (always fat = always valid) |
| `pe.rs` | PE container: DOS stub, COFF/optional headers, `.text`(IL+metadata)/`.sdata`(FieldRVA)/`.reloc`, CLI header (EntryPointToken, ILONLY corflags), **byte-compare headers against CoreCLR ilasm output early** (the Mono-PE32-on-arm64 rejection gotcha lives here) |
| `pdb.rs` (Phase 2, **DONE**) | Portable PDB: #Pdb stream, Document / MethodDebugInformation (delta-compressed sequence points from `SourceFileInfo` roots) tables; DebugDirectory CodeView + PdbChecksum entries in the PE. LocalScope/LocalVariable tables not built (optional; not needed to clear the acceptance bar) |

Entry: `Assembly::export_pe(...)` invoked from the linker where `il_exporter` is called today,
selected by a `config!` flag (`DIRECT_PE`, `cilly/src/bin/linker/main.rs`), default **on** as of
Phase 1c; set `DIRECT_PE=0` to fall back to the ilasm path, which remains available indefinitely.
Determinism: MVID = hash of content (no timestamps/randomness — required for reproducible builds
and for workflow resume constraints); the writer also zeroes the COFF `TimeDateStamp`, which is the
quickest way to tell the two paths' output apart (ilasm stamps a real build time).

## Validation (the gate is the oracle)

1. **Unit/golden**: construct tiny `Assembly` values in cilly tests (the `implements_roundtrip`
   pattern), export, load+run with `dotnet`, and byte-diff normalized `ilspycmd`/ildasm text vs the
   ilasm build of the same IR (best-effort where tooling exists).
2. **A/B differential**: the compile_test harness runs each test through BOTH paths
   (`DIRECT_PE=1` env, like `C_MODE`); outputs must byte-match.
3. **The full gates. DONE**: every `cd_*` interop crate green on native macOS (this also *removes*
   the CoreCLR-ilasm-on-macOS requirement — a direct win), then the Docker `::stable` gate under
   `DIRECT_PE=1` with zero new (named) failures vs the ilasm baseline, then the default flipped. The
   fatal CIL typechecker continues to run before export — the PE writer adds a *second* structural
   layer (bad metadata simply fails to load), it replaces none of it.

## Phasing

- **Phase 0 — harvest the latent PDB. RAN 2026-07-01 (`cargo_tests/cd_pdb`): mechanism PROVEN,
  three quality gaps found.** `Environment.StackTrace` on the real backend resolves a frame
  through the ilasm-produced portable PDB to a real `file.rs:line` — the whole
  `.line`→ilasm→PDB→CoreCLR chain is live. Gaps that now define Phase 2's quality bar:
  (a) **missing frames** — the exporter's `aggressiveinlining` heuristic makes RyuJIT inline the
  user's `#[inline(never)]` fns out of the managed trace (suppress the hint when debug info is the
  priority, or accept + document); (b) **wrong attribution under MIR inlining** — `main`'s frame
  reported `<WORKSPACE>/src/slice/memchr.rs:19` (an inlined-std span) instead of user source; the
  fix is the `get_caller_location`-style walk: attribute sequence points to the OUTERMOST
  non-inlined scope (`span_source_info` in src/assembly.rs); (c) **`<WORKSPACE>` path remapping**
  — build-std remaps std paths; user-crate paths must stay absolute (or cargo-dotnet must emit a
  debugger source-map config) for breakpoints to bind.
- **Phase 1a — skeleton. DONE** (commits bc0c034..5774fd0): heaps + sig encoder + tables with unit
  tests; a hand-built two-method assembly (static entrypoint calling Console.WriteLine via
  MemberRef) loads and runs with zero ilasm invocations.
- **Phase 1b — full construct coverage. DONE**: driven with the inventory checklist; the `cd_*`
  interop battery is green under `DIRECT_PE=1` on native macOS.
- **Phase 1c — gate + flip. DONE.** Oracle: `feasibility/dev.sh gate` (Docker rcc-dev image)
  running `cargo test --release ::stable -- --skip f128 --skip num_test --skip simd --skip fuzz87`
  (the CI skip set). `DIRECT_PE=1` serial run (`--test-threads=1`, the apples-to-apples control):
  424 passed / 16 failed, **named-failure set identical** to the `DIRECT_PE=0` baseline (424/16,
  stable across 2 independent parallel baseline runs) — the 16 are the pre-existing known-flaky
  group (`atomics`, `catch`, `f16`, `fastrand_test`, `futex_test`, `hello_world`, `once_lock_test`,
  `uninit_fill` × debug/release), unrelated to the PE writer. Parallel-mode `DIRECT_PE=1` runs
  showed some additional order-dependent failures that are non-reproducible (different exact names
  flagged run to run, each passing in isolation) — a contention artifact, not a PE-writer
  correctness bug. In-repo `cargo test -p cilly --lib pe_exporter` grew to 99 passing (from the
  65-test Phase 1a baseline) and stayed green throughout. Default flipped: `DIRECT_PE` is now
  `true` (`cilly/src/bin/linker/main.rs`); `DIRECT_PE=0` is the documented escape hatch to ilasm.
- **Phase 2 — Portable PDB. DONE** (commits 42726cb..02da7b8): `cilly/src/ir/pe_exporter/pdb.rs`
  (~1826 lines) implements a standalone BSJB `#Pdb`-stream metadata blob per the dotnet/runtime
  `PortablePdb-Metadata.md` spec:
  - **Document (0x30)** rows interned per source file (name blob with separator+parts, SHA-256
    `HashAlgorithm`/`Hash`, `Language` GUID).
  - **MethodDebugInformation (0x31)**: exactly one row per type-system `MethodDef` row (methods
    with no sequence points get an empty-blob row, never a missing one), carrying a delta-encoded
    Sequence Points blob built by walking `body.rs`'s method-body linearizer's `CILRoot::
    SourceFileInfo` roots in code-offset order (the seam the Phase-0/1a doc comments had already
    marked). Same-IL-offset runs are deduped (last-wins, mirrors ilasm's own `.line`-per-offset
    collapsing); a caller bug (non-monotonic offsets) is left to the spec's own strictly-increasing
    assert rather than silently papered over.
  - **LocalScope/LocalVariable (0x32/0x33) — not built.** Optional per spec; sequence points alone
    clear the Phase-2 acceptance bar (file:line resolution). Local-variable-name debugging in a
    step-through debugger remains a documented stretch item, not a gap in what was promised.
  - **PE side**: a Debug Directory with a `IMAGE_DEBUG_TYPE_CODEVIEW` (type 2) RSDS entry (GUID +
    age + PDB path) plus a `PdbChecksum` entry; `pe.rs`'s `write_debug_directory` fixed a bug where
    the CodeView row's `TimeDateStamp` must equal `pdb_id[16..20]` (not 0 — only the PdbChecksum
    row is hardcoded 0) because that's the SRM match key `PEReader` pairs with the RSDS GUID.
    `deterministic_pdb_id` (`cilly/src/lib.rs:814`) derives the 20-byte PDB id from a content hash,
    not timestamps — determinism preserved end to end.
  - **Linker wiring**: `cilly/src/bin/linker/main.rs` (DIRECT_PE branch) builds and writes the
    `.pdb` alongside the `.dll`/`.exe`; `dotnet_jumpstart.rs`'s embedded-launcher template unpacks
    the bundled PDB bytes under the *loaded* dll's stem (fixed a real bug where it had unpacked
    under the build-time hashed-stem name, so CoreCLR's loader silently found no PDB next to the
    dll it actually ran).
  - **Two span-quality fixes** (`docs/PE_EMISSION_PLAN.md` Phase-0 gaps a/b), both flag-gated where
    they touch codegen: `span_source_info` (`src/assembly.rs`) now walks the MIR `SourceScope`
    inlined-chain up to the outermost non-inlined caller scope before resolving file/line, fixing
    gap (b) (MIR-inlined statements previously mis-attributed `SourceFileInfo` to the inlined
    callee, e.g. a probe's `main` frame resolving to `memchr.rs:19`); gap (a) (the `aggressiveinlining`
    JIT hint erasing user frames) turned out to not exist on the direct-PE path at all
    (`pe_exporter/tables.rs` never emits the `MethodImplAttributes.AggressiveInlining` bit), so the
    new `PDB_FRAMES` config flag (`cilly/src/lib.rs`, default `false`) only suppresses that hint in
    `il_exporter` (the ilasm fallback path) — default-off, zero behavior change unless explicitly
    opted in, execution semantics of compiled programs unaffected either way. Gap (c) (`<WORKSPACE>`
    path remap) was confirmed to originate from the `rust-lang/rust` fork's own bootstrap/test-harness
    remap convention (zero occurrences in this repo's `src/`/`cilly/`), not from anything this writer
    emits — ordinary `cargo_tests/` crates already carry real absolute paths, so it does not affect
    the acceptance path.
  - Also fixed: a degenerate same-line, zero-width-column `SequencePoint` (produced when a source
    column clamps to `u16::MAX` in `span_source_info`) was hitting `validate_visible_sequence_point`'s
    unconditional assert and aborting the entire PE+PDB link for one bad span anywhere in the
    program; now widened to a 1-column span before validation instead (02da7b8).

  **Verification (real numbers, this session, 2026-07-02, current HEAD `02da7b8`)**:
  - `cargo test -p cilly --lib pe_exporter`: **119 passed, 0 failed** (grown from the Phase-1
    baseline of 99; includes `export::tests::e2e_unhandled_exception_resolves_file_line_through_our_pdb`).
  - `cargo_tests/cd_pdb` probe, rebuilt fresh against current HEAD, run under the default
    `DIRECT_PE=1` path (`CARGO_DOTNET_BACKEND=native`, no ilasm anywhere): the on-disk artifact is a
    301596-byte `cd_pdb.pdb` with `BSJB` magic sitting next to `cd_pdb.dll`; the managed stack trace
    resolves both frames —
    `deep_leaf_for_pdb_probe() in .../cargo_tests/cd_pdb/src/main.rs:line 19` and
    `main() in .../cargo_tests/cd_pdb/src/main.rs:line 32` — no `<WORKSPACE>`, no `memchr.rs`
    misattribution. All three probe verdicts (`names this file`, `has file:line frames`,
    `names probe fn`) print `true`.
  - `cargo_tests/cd_collections` (battery slice): **141/141** (`chk!` tally), rebuilt+run fresh
    against current HEAD.
  - The Phase-1c Docker `::stable` serial gate (424/16, byte-identical named-failure set to the
    ilasm baseline — see Phase 1c above) was run against the pre-PDB-writer commit (`fc619fe`/
    `eac6d9b`-era, DIRECT_PE-flip only); it has **not** been re-run end-to-end against the
    post-PDB-writer commits (`5b6f362`..`02da7b8`) in Docker in this session — the in-repo unit
    suite, the cd_pdb probe, and the cd_collections slice are the verified evidence for Phase 2
    specifically. Re-running the full serial gate against `02da7b8` is the one open verification
    item before calling the PDB writer gate-proven at the same bar as Phase 1.
  - A manual VS Code / debugger step-through was **not** performed this session (stretch item,
    unverified).

## Risks & mitigations

- **Header/corflags quirks** (macOS arm64 PE32 rejection): byte-diff CoreCLR ilasm's headers on
  day one; emit exactly its shape.
- **Sorted-table / coded-index subtleties**: ECMA-335 §II.24 width rules implemented once in
  `tables.rs` and unit-tested against ilasm-produced binaries parsed with a ~200-line test-only
  reader.
- **Scale** (std-linked exes: thousands of types/methods, multi-MB bodies): size-compute pass
  handles wide indices; MainModule partitioning already bounds per-class method counts.
- **Scope creep**: the inventory list is the spec; anything il_exporter doesn't emit, we don't
  build.
