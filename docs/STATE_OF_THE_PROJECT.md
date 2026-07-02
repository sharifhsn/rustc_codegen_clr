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
industrial-grade continuous trust, performance parity on allocation-heavy code, and debuggability.

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
| Generics interop | Rust→generic .NET (`List<T>`, `Dictionary<K,V>`), value-type generic instance methods (dict iteration, `Span<T>`, `Nullable<T>`), generic **methods** `!!N`, C#→generic Rust (`RustVec<T>`/`RustBoxVec<T>` any `T`) | `cd_generic` 18/18, `cd_rustvec` 37/37, `cd_collections` 128/128 |
| Delegates/closures | `extern fn` **and capturing closures** → `Action`/`Func`; delegates as generic-method args (`sort_by`) | `cd_delegates` 14/14, `cd_closures` |
| Interfaces | **Rust type implements a .NET interface** (`#[dotnet_class(implements=…)]`), consumed polymorphically from C# (DI-shaped) | `cd_iface` 9/9 |
| LINQ / EF | Expression trees built from Rust (parameters, binops, member access, constants via the new `box` primitive), compiled+executed, and handed to `Queryable.Where<T>(Expression<Func<T,bool>>)` | `cd_linq_expr` 30/30 |
| .NET→Rust | `#[dotnet_export]` auto-marshal, `#[dotnet_class]` (ctors/statics/fields/managed fields), reusable containers, NuGet | `cd_export` 11/11, `cd_typedef` 16/16, `cd_containers*` |
| BCL breadth | collections/DateTime/Guid/Regex/Json/… idiomatic wrappers | `cd_bcl` 313/313, `cd_json` 47/47 |
| Tooling | `cargo dotnet` full pipeline, dual-mode (installed/DEV), macOS-ARM native + Docker Linux, `--dotnet 8|9`, MSBuild `RustDotnet.targets`, `pack`→`.nupkg` | scaffolds + cd_* consumers build hands-free |
| Perf | MIR-layer inlining + SROA + const-hoist: `iter_sum` 1764→156 ms, `iter_zip` 2765→216 ms; **whole-program NativeAOT proven** (FieldRVA sizing fixed), AOT 1.6–3.5× over JIT | `bench_rs_vs_cs`, perf docs |

## Honest remaining surface

**Correctness tails.** `overflow-checks=true` build-std ICE (pre-existing, deferred); the
`adt.rs` field-offset `u16::MAX` clamp (latent, no observed repro); sub-word-atomic page-boundary
hazard on .NET 8 (eliminated on .NET 9 via native `Interlocked` overloads); the Rust-atomic-ordering →
`Interlocked`/`Volatile` **memory-model audit has never been done** (real threads + ARM64 make this the
one place a latent soundness gap could still hide); `ilverify` as an independent oracle (reports ~34k
intentional-unsafe-IL idioms; needs a triage layer).

**PAL tails.** `hard_link`; TLS drop-destructors (leak-on-exit); `timerfd`-over-loopback (unblocks
smol); fd-backed `File` for `switch_stdout`; signals beyond INT/TERM/HUP/QUIT (wall); synthetic pids;
lossy errno long-tail. Tier-0 walls unchanged — see [GAPS.md](GAPS.md) §Tier 0 (fork/exec, mmap
fidelity, real signal delivery, f128 on .NET, …).

**Interop tails (all pure-library or small-backend).** .NET events (`add_*`/`remove_*`);
`#[dotnet_class]` **virtual-method overrides** (interfaces are done); exporting Rust traits as C#
interfaces; `IEnumerable<T>` over `RustVec`; `cargo dotnet publish --aot` as a subcommand.

**Performance.** The measured **7.9× allocation floor** (`NativeMemory` malloc vs gen0 bump
allocation) — candidate fix: a pooled/arena allocator over pinned .NET memory (unattempted); EH
cleanup-block bloat under `panic=unwind` (~2× on unwind-heavy code; `NO_UNWIND` exists).

**Exporters.** IL: production (ilasm path, now the fallback). C: ~80% prototype (33 cold-path
`todo!`). JVM: skeleton. **Direct PE writer** (`cilly::ir::pe_exporter`, no ilasm): **Phase 1
COMPLETE and now the default linker path** (`DIRECT_PE` defaults to `true`; `DIRECT_PE=0` falls
back to ilasm) — see [PE_EMISSION_PLAN.md](PE_EMISSION_PLAN.md). **Portable PDBs (Phase 2)** are
next: sequence points from the already-threaded MIR spans → breakpoints/stepping on Rust source
under a .NET debugger.

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
3. **Direct PE emission (Phase 1 DONE) + Portable PDBs (Phase 2, NEXT)** — the ilasm dependency
   (per-runtime assembler, PE32 arm64 mismatch, 1023-char class-name cap) is now bypassed by default:
   the hand-rolled ECMA-335 writer (`cilly::ir::pe_exporter`) is the default linker path (`DIRECT_PE`
   defaults to `true`; `DIRECT_PE=0` escape-hatches back to ilasm). Next: thread the already-present
   MIR spans through into sequence points → **breakpoints/stepping on Rust source under a .NET
   debugger**. The largest remaining DX gap.
4. **Deferred big bets** — pooled allocator vs the 7.9× alloc floor; memory-model litmus audit;
   upstreaming universal fixes to FractalFir; a tier-3 `*-unknown-dotnet` rustc target as the long-game
   end state.
