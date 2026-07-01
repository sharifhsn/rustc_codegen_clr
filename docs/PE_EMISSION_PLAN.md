# Direct PE emission + Portable PDBs — design & phasing

> Goal: the linker emits the final `.dll`/`.exe` **directly** from the interned IR — no textual
> `.il`, no external `ilasm` — and emits a **Portable PDB** with sequence points mapping IL back to
> Rust source (breakpoints/stepping/stack-traces on `.rs` files under any .NET debugger).
> Status: design (this doc) + Phase 1 in progress. Owner constraint: the CIL typechecker is never
> weakened; the ilasm path stays available behind a flag until the PE path survives the full gate.

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
| `pdb.rs` (Phase 2) | Portable PDB: #Pdb stream, Document / MethodDebugInformation (delta-compressed sequence points from `SourceFileInfo` roots) / LocalScope / LocalVariable tables; DebugDirectory CodeView entry in the PE |

Entry: `Assembly::export_pe(...)` invoked from the linker where `il_exporter` is called today,
selected by a `config!` flag (working name `DIRECT_PE`), default **off** until §Gate; the ilasm
path remains as fallback. Determinism: MVID = hash of content (no timestamps/randomness — required
for reproducible builds and for workflow resume constraints).

## Validation (the gate is the oracle)

1. **Unit/golden**: construct tiny `Assembly` values in cilly tests (the `implements_roundtrip`
   pattern), export, load+run with `dotnet`, and byte-diff normalized `ilspycmd`/ildasm text vs the
   ilasm build of the same IR (best-effort where tooling exists).
2. **A/B differential**: the compile_test harness runs each test through BOTH paths
   (`DIRECT_PE=1` env, like `C_MODE`); outputs must byte-match.
3. **The full gates**: every `cd_*` interop crate on native macOS (this also *removes* the
   CoreCLR-ilasm-on-macOS requirement — a direct win), then the Docker `::stable` gate under
   `DIRECT_PE=1` with zero new failures, then flip the default. The fatal CIL typechecker continues
   to run before export — the PE writer adds a *second* structural layer (bad metadata simply fails
   to load), it replaces none of it.

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
- **Phase 1a — skeleton**: heaps + sig encoder + tables with unit tests; a hand-built
  two-method assembly (static entrypoint calling Console.WriteLine via MemberRef) loads and runs.
- **Phase 1b — full construct coverage**: drive with the inventory checklist; `cd_*` suite green
  under `DIRECT_PE=1` on native macOS.
- **Phase 1c — gate + flip**: Docker `::stable` differential green → flip default; ilasm stays
  behind `ILASM_FALLBACK=1` for one release-cycle of soak.
- **Phase 2 — PDB**: emit the Portable PDB from `SourceFileInfo` roots (sequence points), add
  LocalScope/LocalVariable from the existing named `.locals`, DebugDirectory wiring. Scriptable
  acceptance: a deliberate panic's managed stack trace prints `<crate>/src/main.rs:<line>` (uses
  `StackTrace(fNeedFileInfo: true)` machinery), plus a manual VS Code step-through. Stretch:
  richer per-node spans (today spans are per-MIR-statement — already the right granularity for
  sequence points).

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
