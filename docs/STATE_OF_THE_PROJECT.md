# State of the project — 2026-07-01 snapshot

> **This is the authoritative dated snapshot.** The deep maps ([TRANSLATION_STATUS.md](TRANSLATION_STATUS.md),
> [GAPS.md](GAPS.md), [ERGONOMICS_STATUS.md](ERGONOMICS_STATUS.md), [COMPAT_SURVEY.md](COMPAT_SURVEY.md))
> are campaign documents written mid-flight; several of their "blocked / not started" claims were closed
> by later work and **when they disagree with this doc, this doc wins** (§ Corrections below).
> Branch: `gaps-campaign` (pushed to `mine`), toolchain pinned `nightly-2026-06-17`, .NET 8 + 9.

## Where the project is, in one paragraph

The capability war is essentially over. Rust compiles to .NET with a **fatal CIL type-verifier** on by
default and **no known reachable silent-miscompile path on safe stable code**; `core` tests run 2657/0,
~96% of rust-lang `tests/ui` run-pass passes, and a 137-crate ecosystem survey is ~85%+ byte-identical
to native (the residual clusters are root-caused). Real `std` runs on a real .NET PAL — files, network,
**threads with real locks/TLS/Parker** (rayon/parking_lot-class crates work), process spawning **with
output capture**, `panic=unwind`, async/await + the tokio core. Interop is closed in both directions
through the hardest cases: generics both ways (including generic *methods* `!!N` and value-type
generics), delegates **including capturing closures**, **implementing .NET interfaces from Rust**, LINQ
(in-memory *and* the EF-style `IQueryable.Where(Expression<Func<...>>)` handoff over hand-built
expression trees), `Task` bridging both directions, and a `box` IR primitive. The DX is one command
(`cargo dotnet new|build|run|test|pack`, MSBuild auto-build, NuGet), and whole-program **NativeAOT is
proven** (1.6–3.5× over JIT on real workloads). What remains is not capability: it is distribution,
industrial-grade continuous trust, and performance parity on allocation-heavy code.

## The debugging story

Default builds now emit a real `foo.pdb` next to `foo.dll`/`.exe`, written entirely by this repo's
own hand-rolled Portable PDB writer (`cilly::ir::pe_exporter::pdb`, `docs/PE_EMISSION_PLAN.md`
Phase 2, DONE) — no `ilasm` anywhere in the default path (`DIRECT_PE` defaults to `true` and now
also builds the PDB; `DIRECT_PE=0` is the ilasm escape hatch, unchanged). Unhandled-exception stack
traces resolve to real Rust source: a fresh rebuild of the `cargo_tests/cd_pdb` probe against
current `HEAD` under the default path prints frames reading
`deep_leaf_for_pdb_probe() in .../cd_pdb/src/main.rs:line 19` and
`main() in .../cd_pdb/src/main.rs:line 32` — the produced `.pdb` is a 301596-byte `BSJB`-magic
file, and both a MIR-inlining mis-attribution bug (a frame previously resolving into
`memchr.rs` instead of the user's own source) and a PDB-file-naming bug in the embedded-launcher
template (unpacking the PDB under the wrong filename, so CoreCLR's loader silently found none) were
found and fixed while closing this out. `cargo test -p cilly --lib pe_exporter` is 119/0 (grown
from the 99-test Phase-1 baseline). **Remaining limitations**: no LocalScope/LocalVariable tables
(local-variable names are not resolvable while stepping — only file:line sequence points); nobody
has yet driven an actual VS Code breakpoint/step-through session against the writer's output (the
underlying primitive — sequence points — is the same one stack-trace resolution already proves
works, but the interactive-debugger UX itself is unverified); and the post-PDB-writer commits have
not yet been re-run through the full Docker `::stable` serial gate (the 424/16-identical-to-ilasm
gate result on record predates the PDB writer, from the Phase-1-only `DIRECT_PE` flip) — the
verified-in-this-session evidence for Phase 2 itself is the unit suite, the `cd_pdb` probe, and a
`cd_collections` 141/141 slice, all rebuilt fresh against current `HEAD`.

## Verified capability ledger

Every claim below is backed by a runnable proof in `cargo_tests/` (the `chk!` equal-tally convention,
run on the **real .NET backend**, `CARGO_DOTNET_BACKEND=native`) and/or the Docker `::stable` gate.

| Area | State | Proof |
|---|---|---|
| CIL type-verifier | **Fatal by default** (invariant I1); never weakened — extended twice with *sound* rules (WF-9 marker guard, box `PlatformObject`→`System.Object`) | gate green with checker fatal |
| Silent-miscompile surface | P2/P3 audits: 242 sites audited, EH/Terminate seams closed, 0 reachable silent-wrong on safe stable | `docs/P3_TOTALITY.md`, seam audit |
| core/alloc/std test suites | coretests **2657/0** (6 pathological ZST-slice skips); std suites via I2 harness | `BROKEN_TESTS.md`, success lists |
| rust-lang `tests/ui` | ~**96%** of run-pass on stable through the backend | I3 harness |
| Ecosystem differential | 137-crate survey ~85% byte-identical; 31-crate sweep all byte-identical; soak 94/97 | `docs/COMPAT_SURVEY.md` + later fixes |
| Threads/sync | Real `Thread`/`Mutex`(SemaphoreSlim)/TLS(`ThreadLocal<nint>`)/**Parker** → rayon/parking_lot-class unblocked | `pal_threads` + compat Class D fix |
| Full-I/O PAL | fs (copy/set_len/canonicalize/permissions), net (TCP/UDP/UDS), process `status()` **and** `output()` capture | `pal_*` probes all green (only `hard_link` open) |
| async | Rust async/await + tokio core on the PAL; `mycorrhiza::task` bridges .NET `Task` both directions (incl. `Task<T>` production) | `pal_tokio_net`, `cd_async` 7/7 |
| Generics interop | Rust→generic .NET (`List<T>`, `Dictionary<K,V>`), value-type generic instance methods (dict iteration, `Span<T>`, `Nullable<T>`), generic **methods** `!!N`, C#→generic Rust (`RustVec<T>`/`RustBoxVec<T>` any `T`) | `cd_generic` 18/18, `cd_rustvec` 44/44, `cd_collections` 128/128 |
| Delegates/closures | `extern fn` **and capturing closures** → `Action`/`Func`; delegates as generic-method args (`sort_by`) | `cd_delegates` 14/14, `cd_closures` |
| Interfaces | **Rust type implements a .NET interface** (`#[dotnet_class(implements=…)]`), consumed polymorphically from C# (DI-shaped) | `cd_iface` 9/9 |
| LINQ / EF | Expression trees built from Rust (parameters, binops, member access, constants via the new `box` primitive), compiled+executed, and handed to `Queryable.Where<T>(Expression<Func<T,bool>>)` | `cd_linq_expr` 30/30 |
| .NET→Rust | `#[dotnet_export]` auto-marshal, `#[dotnet_class]` (ctors/statics/fields/managed fields), reusable containers, NuGet | `cd_export` 11/11, `cd_typedef` 16/16, `cd_containers*` |
| BCL breadth | collections/DateTime/Guid/Regex/Json/… idiomatic wrappers | `cd_bcl` 313/313, `cd_json` 47/47 |
| Tooling | `cargo dotnet` full pipeline, dual-mode (installed/DEV), macOS-ARM native + Docker Linux, `--dotnet 8|9`, MSBuild `RustDotnet.targets`, `pack`→`.nupkg` | scaffolds + cd_* consumers build hands-free |
| Perf | MIR-layer inlining + SROA + const-hoist: `iter_sum` 1764→156 ms, `iter_zip` 2765→216 ms; **whole-program NativeAOT proven** (FieldRVA sizing fixed), AOT 1.6–3.5× over JIT | `bench_rs_vs_cs`, perf docs |
| Direct PE + PDB writer | Hand-rolled ECMA-335 PE writer is the **default** linker path, no `ilasm`; hand-rolled Portable PDB writer emits `foo.pdb`, stack traces resolve `file.rs:line` | `cargo test -p cilly --lib pe_exporter` 119/0, `cd_pdb` probe, `docs/PE_EMISSION_PLAN.md` |

## Honest remaining surface

**Correctness tails.** `overflow-checks=true` build-std ICE (pre-existing, deferred); the
`adt.rs` field-offset `u16::MAX` clamp (latent, no observed repro); sub-word-atomic page-boundary
hazard on .NET 8 (eliminated on .NET 9 via native `Interlocked` overloads); `ilverify` as an
independent oracle (reports ~34k intentional-unsafe-IL idioms; needs a triage layer). The
Rust-atomic-ordering → `Interlocked`/`Volatile` memory-model audit is **done** (d874a84, see
below) — one residual is now known and deferred rather than latent-unknown.

**PAL tails.** `hard_link`; TLS drop-destructors (leak-on-exit); `timerfd`-over-loopback (unblocks
smol); fd-backed `File` for `switch_stdout`; signals beyond INT/TERM/HUP/QUIT (wall); synthetic pids;
lossy errno long-tail. Tier-0 walls unchanged — see [GAPS.md](GAPS.md) §Tier 0 (fork/exec, mmap
fidelity, real signal delivery, f128 on .NET, …).

**Interop tails (all pure-library or small-backend).** .NET events (`add_*`/`remove_*`);
`#[dotnet_class]` **virtual-method overrides** (interfaces are done); exporting Rust traits as C#
interfaces; `IEnumerable<T>` over `RustVec`; `cargo dotnet publish --aot` as a subcommand.

**Performance.** The `NativeMemory` malloc vs gen0-bump allocation-model gap was re-measured and
is smaller than previously documented: the malloc/free floor itself is 28–34 ns/op (§3 of
`PERF_GUIDANCE.md`), and on the target `bench_rs_vs_cs` alloc-churn workload the measured gap to
C# is **1.9×**, not the previously-cited 7.9× (that figure was stale). A size-classed pool
allocator (`POOL_ALLOC=1`, opt-in) was built and benchmarked to close it — see *Performance
findings* below; verdict **PARKED-NEGATIVE**, left off by default. EH cleanup-block bloat under
`panic=unwind` (~2× on unwind-heavy code; `NO_UNWIND` exists) is unchanged.

**Memory-model soundness.** The Rust-atomic-ordering → `Interlocked`/`Volatile` lowering audit
(deferred big bet, now closed) found and fixed two real cells weaker than Rust's memory model
requires — `atomic_load` (missing the `volatile.` acquire-fence prefix entirely) and
`atomic_store` (missing the trailing full fence `SeqCst` needs to forbid StoreLoad reordering) —
plus an independent optimizer soundness gap (two V2 peephole folds silently dropped the
`volatile` flag on local-address accesses). All three fixed; verified with litmus-testing on
real weak-memory ARM64 hardware, native-calibrated, zero violations across 3×300,000-iteration
backend runs plus a fuller MP/SB/LB/IRIW sweep. One known-unsound residual was surfaced (not
introduced) and is explicitly deferred: `AtomicU8`/`AtomicBool::swap` races on .NET 8 (fixed on
.NET 9). Full accounting, ECMA-335 citations, and calibration caveats: `docs/MEMORY_MODEL.md`.

**Exporters.** IL: production (ilasm path, now the fallback). C: ~80% prototype (33 cold-path
`todo!`). JVM: skeleton. **Direct PE writer** (`cilly::ir::pe_exporter`, no ilasm): **Phase 1
COMPLETE and now the default linker path** (`DIRECT_PE` defaults to `true`; `DIRECT_PE=0` falls
back to ilasm), and **Phase 2 (Portable PDBs) is also COMPLETE** — default builds now emit a
standalone `foo.pdb` next to `foo.dll`/`.exe` with no `ilasm` involved — see
[PE_EMISSION_PLAN.md](PE_EMISSION_PLAN.md). LocalScope/LocalVariable tables (local-variable-name
debugging) remain a documented stretch item; a manual VS Code step-through has not been done.

## Corrections to the older docs (read this before trusting them)

| Doc | Stale claim | Reality (this snapshot) |
|---|---|---|
| GAPS.md WF-C / TRANSLATION_STATUS §5 | "typechecker off-by-default, non-fatal, not a release gate" | **Fatal by default** since P1 (`main` f3ae738); flags wired; negative-tested |
| ERGONOMICS_STATUS 🟡/⬜ | "dict iteration / `Span<T>` / valuetype `Nullable<T>` blocked by one backend gap" | Backend gap **closed** (d8af417, d80df45); all three shipped + proven |
| ERGONOMICS_STATUS 🟡 | "delegate tail: closure captures / generic-method args deferred" | **Shipped** (886de8c capturing closures; d80df45 delegate-as-generic-arg) |
| ERGONOMICS_STATUS 🟡 | "`#[dotnet_class]` interface impl not done" | **Shipped** (92631eb) — Rust types implement .NET interfaces |
| ERGONOMICS_STATUS ⬜ | "LINQ-style adapters not started" | In-memory LINQ **and** EF expression-tree `IQueryable.Where` handoff shipped (886de8c…025066a) |
| TRANSLATION_STATUS / soak | "`regex` fails (deep allocator issue)" | **Fixed** (b542de5 — 128-bit niche `get_discr`); regex byte-identical |
| COMPAT_SURVEY Class D | "rayon/parking_lot/dashmap blocked on parker/futex/TLS" | Parker keystone + generic sync routing **landed**; class unblocked |
| GAPS.md WF-F deferred list | "Condvar/RwLock/Once still no_threads" | Routed to std's generic queue-based impls over the real Parker/Mutex |
| TRANSLATION_STATUS §6 | ".NET→Rust ergonomic tail = managed-String/Result/NuGet remaining" | All shipped (WF-8 + ergonomics campaign); see ledger above |

## In-flight roadmap (2026-07)

1. **Ship & distribute** — toolchain pinned ✅, branch pushed ✅, this truth pass ✅; next: getrandom
   auto-shim, standalone hello-world demo repo, prebuilt-toolchain `cargo dotnet setup`.
2. **CI industrialization** — fork CI running the gate + fatal checker on pinned nightly; weekly
   nightly-drift canary; manual heavy jobs (soak/coretests).
3. **Direct PE emission (Phase 1 DONE) + Portable PDBs (Phase 2 DONE)** — the ilasm dependency
   (per-runtime assembler, PE32 arm64 mismatch, 1023-char class-name cap) is now bypassed by default:
   the hand-rolled ECMA-335 writer (`cilly::ir::pe_exporter`) is the default linker path (`DIRECT_PE`
   defaults to `true`; `DIRECT_PE=0` escape-hatches back to ilasm), and it now also writes the
   `foo.pdb` — sequence points from the already-threaded MIR spans resolve real `file.rs:line` in
   managed stack traces with zero `ilasm` involvement. Remaining: a manual VS Code
   breakpoints/stepping session (mechanism should work — sequence points are the same primitive —
   but nobody has clicked through it yet), and LocalScope/LocalVariable tables for local-variable-
   name inspection while stepping.
4. **Deferred big bets — both closed this campaign.** Memory-model litmus audit: **done**, two real
   fence-soundness bugs found and fixed (d874a84), documented in `docs/MEMORY_MODEL.md`. Pooled
   allocator vs the alloc-model gap: **built, measured, PARKED-NEGATIVE** (0610062, 89be4cc) — ~1.03×
   on target workloads, short of the pre-committed 1.5× bar, because the malloc/free floor it targets
   is only ~3% of total per-iteration time; documented in `docs/PERF_GUIDANCE.md` §6. Remaining:
   upstreaming universal fixes to FractalFir; a tier-3 `*-unknown-dotnet` rustc target as the
   long-game end state.
