# GAPS.md — what's missing, and the workflow campaign to close it

> Companion to [docs/TRANSLATION_STATUS.md](TRANSLATION_STATUS.md). That doc maps what *works*; this
> one is the honest backlog of what doesn't, tiered by whether it's a permanent **wall**, a
> **tractable** engineering gap, or **polish**, with a rough effort estimate and the workflow that
> attacks it. The project is experimental (the README says *"DO NOT USE IT FOR ANYTHING SERIOUS"*);
> this is the map from "impressive demo" toward "trustworthy toolchain."

**Effort key:** S ≈ hours · M ≈ 1–2 days · L ≈ 3–5 days · XL ≈ 1–3 weeks · ∞ = no solution on stock CoreCLR.

**Execution rule:** every workflow below mutates the backend (`cilly/`, `src/`), rebuilds, and runs the
canonical Docker `::stable` gate (baseline **426 pass / 12 fail**). They therefore run **serialized**,
one at a time, each verified + committed before the next. Commits land locally on a feature branch;
push to `mine` (the fork, github.com/sharifhsn) only when explicitly asked — never `origin`
(FractalFir's upstream).

---

## Tier 0 — Walls (document only, NOT attempted)

No architectural solution on stock CoreCLR. Listed so they're never mistaken for "todo."

| Gap | Why it's a wall | Effort |
|-----|-----------------|--------|
| `fork`/`vfork`/`execve` | Can't clone or replace a running CLR image in-process | ∞ |
| `f128` (quadruple float) | No 128-bit float in the .NET BCL; would need softfloat emulation | ∞ (C-mode only) |
| Zero-cost open generics overlapping managed refs | CLI forbids explicit layout on generics | ∞ (name-mangling fallback exists) |
| Arbitrary novel `inline asm` / `global_asm` | No general asm→CIL lowering; only a few templates recognized | ∞ (common cases coverable) |
| Static borrow-safety across the boundary | Rust ownership can't be enforced once a value enters managed code | ∞ |
| `mmap MAP_FIXED`/shared/file-backed, `mprotect`, `brk`/`sbrk` | No VM-placement/protection/program-break model on .NET | ∞ |
| Real signal delivery (beyond INT/TERM/HUP/QUIT) | No .NET delivery path; `EINTR` never fires | ∞ |
| Abstract-ns `AF_UNIX`, `SCM_RIGHTS`, `ucred`, true inode/dev/nlink, `/proc` | No managed source of truth | ∞ (compiled out via cfg) |

These are surfaced with clear `todo!`/panic messages or `cfg`-compiled-out so end-user code never silently
hits them. **No workflow targets Tier 0.**

---

## Tier 1 — Tractable gaps (workflow campaign)

### WF-A — Integer/bit intrinsics tail  ·  effort M  ·  **first**
Wire the `todo!` integer/bitwise intrinsic arms for the type widths not yet covered (mainly 128-bit, via
the already-supported `System.Int128`/`System.UInt128`): `bswap`, `ctlz`, `cttz`, `ctpop`, `bitreverse`,
`rotate_left`/`rotate_right`, `saturating_add`/`saturating_sub`, `float_to_int_unchecked`.
Files: `src/terminator/intrinsics/`. Verify: targeted tests + `::stable` gate.

### WF-B — SIMD tail  ·  effort L
`simd_shuffle` (const-index immediate), float transcendentals (`fsqrt`/`floor`/`ceil`/`round`/`trunc`/
`fma`), float `reduce_min`/`reduce_max`, SIMD `ctlz`/`cttz`/`ctpop`/`bswap`. Files:
`cilly/src/ir/builtins/simd/`, dispatch in `src/terminator/intrinsics/`. Verify: `test/intrinsics/simd.rs`
(CI skips `simd`, so run explicitly) + gate. Goal: shrink the `--skip simd` set.

### WF-C — Correctness posture (soundness)  ·  effort L  ·  **highest value**
Turn on `TYPECHECK_CIL` + `VERIFY_METHODS` across `::stable`, collect + categorize every CIL violation,
clear the benign `FieldAssignWrongType` noise, and triage the real ones (`FieldOwnerMismatch`,
`CallArgTypeWrong`). Then assess flipping `ALLOW_MISCOMPILATIONS` from its current `true` default toward a
hard gate. This is the single most important step toward "trustworthy." Files: `src/config.rs`,
`cilly/src/ir/typecheck.rs`, wherever violations originate.

### WF-D — std/core/alloc test-failure triage  ·  effort L
Pipeline over the `BROKEN_TESTS.md` list: minimize → root-cause → fix the **class-level** bugs
(candidates: IPv6 address string formatting, iterator `try_fold`/`try_rfold` codegen, `slice`/`Vec`/
`VecDeque` edge cases, `i128` saturating abs/neg, `try_reserve`). Each class fix should unbreak several
tests at once. Verify per-class + gate; update `BROKEN_TESTS.md`.

### WF-E — std PAL fidelity  ·  effort M
Reduce the documented PAL leaks to the extent the runtime allows: richer `stat`/`fstat` fields, better
errno fidelity, `readdir` `d_type`. Explicitly bounded by Tier-0 walls (no inode identity). Files:
`dotnet_pal/`, `cilly/src/ir/builtins/posix.rs`. Verify: `pal_*` probes + gate.

### WF-G — Interop tail  ·  effort M (5d) + XL (WF-9)
(1) **5d managed-array return** — new IR ops `NewArr`/`StElem` so a Rust fn can return a managed array to
C# (~15 exhaustive-match sites). (2) **Generic interop bridge (WF-9)** — generic Rust types ↔ generic
.NET; this is the linchpin and is XL — likely scope-only this campaign. Plus `dotnet_typedef!`
follow-ups (managed-type fields, ctors with non-primitive types). Verify: extend `cd_interop_tier2` +
gate.

### WF-F — Threading / TLS  ·  effort XL  ·  **the architectural one**
Real per-thread thread-locals (`[ThreadStatic]`/`ThreadLocal<T>`) + route std's sync (`Mutex`/`Condvar`/
`RwLock`) to a genuine pthread/.NET-Monitor backend, replacing the single-thread-correct global map.
Unblocks (a) safe multi-threaded Rust on the PAL and (b) the clean global `target-family=["unix"]` flip.
Highest risk; sequenced last and may land as **scope + first slice** rather than complete. Files:
`dotnet_pal/`, std PAL injection in `feasibility/dev.sh`/overlays, `cilly` BCL hooks.

---

## Sequencing & honesty

Order: **A → B → C → D → E → G → F**. Rationale: bank the clean, low-risk completeness wins first
(A, B), then the highest-value soundness work (C), then the visible test-pass gains (D), then PAL polish
(E) and the interop tail (G), and finally the large architectural lift (F), which is the most likely to
land partially.

"Complete all this work to whatever extent possible" is taken literally: each workflow goes as far as it
can and **reports honestly** what it finished, what it couldn't, and why — partial progress on F or G is
expected and fine. Tier-0 walls are out of scope by definition. This file is updated as each workflow
lands (status + commit).

## Performance findings (benchmark + WF-OPT outcome)

A head-to-head vs hand-written C# on the same .NET 8 (`cargo_tests/bench_rs_vs_cs/`, byte-identical
logic, identical result sinks → fair) measured the *actual* cost of going through this backend:

| Workload | Rust → .NET | C# | Ratio |
|---|---|---|---|
| `numeric` (tight int loop, zero alloc) | 165 ms | 92 ms | **1.8×** |
| `alloc_churn` (iterator fill/sum) | 3182 ms | 108 ms | 29.5× |
| `alloc_churn_indexed` (plain `while` loops) | 858 ms | 108 ms | **7.9×** |

The 30× allocation gap decomposes into two **independent** multipliers:
- **Iterator/closure codegen ≈ 3.7×** (3182 → 858 ms just by dropping `.iter()`/`.enumerate()`).
- **Allocation model ≈ 7.9×** (`NativeMemory` malloc/free vs .NET's gen0 bump allocator) — the GC
  *wins* on allocation **throughput**. The earlier "Rust avoids GC pressure so it's faster"
  intuition is a **latency/pause** argument, not throughput, and did not show here.

**WF-OPT (optimizer) — attempted, parked with a negative result.** The hypothesis (the 1.8× came from
a redundant per-iteration `conv.u8` no-op u64 zero-extend) was implemented and **reverted**: it changed
zero IL in the hot loop (the loop uses `wrapping_*`/`+=`, which lower through the checked path at
`src/binop/checked/mod.rs:~165`, not the plain-`Add` arm at `src/binop/mod.rs:215` that was edited), and
even at the right site **RyuJIT almost certainly already elides a trailing no-op `conv.u8`** — so the
pure-compute 1.8× is largely **RyuJIT-vs-LLVM fundamental**, with a low optimizer ceiling. Nothing
committed; gate untouched at baseline.

**Decision: optimizer/compute-throughput work is parked.** The genuinely high-value optimizer levers
(deferred, not abandoned) are the *bigger* ones FractalFir flagged and the bench confirms:
- **Iterator/closure lowering** (the measured 3.7×) — recoverable, impactful, a real lift.
- **Exception-handler reduction** (his measured ~2× via `NO_UNWIND`; cleanup-block bloat also defeats
  RyuJIT's >5-basic-block inline limit).
- A safe IL-hygiene win: generalize the nested-`IntCast` collapse in `cilly/src/ir/opt/opt_node.rs:43`
  to fire whenever `target == target2` (same-target re-cast = guaranteed no-op, no type table needed) —
  shrinks IL (helps inlining + the typechecker) even though the bench would stay flat.

**Re-open performance only with a pause-sensitive (latency) workload** — large/long-lived working set,
tail-latency-sensitive — where the deterministic-drop / unmanaged-heap model could actually win. Raw
throughput vs C# is not where this backend competes today.

## Status log

- **WF-A (intrinsics tail)** — DONE, commit `e3cdd4b`. Wired signed-`i128` saturating + `bitreverse`
  usize/isize + `float_to_int_unchecked` u128/i128; verified the other `todo!`s are unreachable
  fallbacks. `cargo_tests/intr_bits` green; Docker gate 426/12.
- **Benchmark** — DONE, commit `a9d8a6d`. See *Performance findings* above.
- **WF-OPT (optimizer)** — PARKED (negative result, nothing committed). See *Performance findings*.
- **WF-B (SIMD tail)** — DONE. 14 ops wired via per-lane spill-and-index (`simd_shuffle`,
  `ctpop`/`ctlz`/`cttz`/`bswap`/`bitreverse`, `fsqrt`/`floor`/`ceil`/`trunc`/`round`/`round_ties_even`,
  `fma`/`relaxed_fma`); walls (`gather`/`scatter`/`masked_*`/`funnel_*`) left as explicit `todo!`.
  `cargo_tests/simd_tail` 18/18 on real .NET; Docker gate 426/12. Float `reduce_min`/`reduce_max` left
  unwired — blocked by a pre-existing, orthogonal `f32::min` `TypeLoadException`, not this change.
- **WF-D (test-failure triage)** — DONE, with a bonus high-impact find. The targeted clusters
  (IPv6 fmt, iterator `try_fold`, `i128` saturating, sub-word `leading/trailing_ones`) all repro
  **clean** vs native → those `BROKEN_TESTS.md` lines are **stale** (fail test-harness-wide, not
  codegen; `i128` saturating was transitively fixed by WF-A) — now flagged "Verified STALE". The real
  bug found+fixed: the **entire `Assert`-terminator panic family** (bounds check, div/rem-by-zero, all
  arithmetic/neg overflow, null/misaligned-ptr deref, invalid-enum, coroutine-resume) lowered to a
  surrogate that discarded operands and called an unbodied `abort` → runtime crash `missing methiod
  abort` instead of a catchable native panic. Fixed to call the exact `#[track_caller]` panic lang
  items (`src/terminator/mod.rs`); panic messages now byte-match native. Unbreaks `swap_panics`×4 +
  `vec::test_index_out_of_bounds`; makes div0/overflow panics native-correct. 7 repro crates added;
  Docker gate 426/12 (no regressions).
- **WF-E (PAL fidelity)** — DONE. The metadata timestamp/size/symlink "leaks" were already closed by
  B2 (docs were stale → corrected). Real win: **errno fidelity** — enriched exception→errno
  (`FileNotFound`/`DirectoryNotFound`→`ENOENT`, `UnauthorizedAccess`→`EACCES`, `PathTooLong`→
  `ENAMETOOLONG`), errno-wrapped fs hooks (`mkdir`/`rmdir`/`unlink`/`rename`/`open`), std-side
  `last_os_error()` → `File::open(missing)` now `NotFound` (was `ErrorKind::Other`). Walls left honest
  (inode/dev/nlink=0 not faked, `ctime`=creation-time documented, `EACCES` Unix-host-best-effort,
  `create_dir` recursive = BCL wall). `pal_fsmeta` extended + green; gate 426/12.
- **WF-G part 1 (managed-array return)** — DONE. Two new IR ops `CILNode::NewArr` + `CILRoot::StElem`
  (real `newarr`/`stelem`) threaded through every exhaustive match (interner/visitor/typecheck/opt/
  il-exporter + asm `shallow_methodef_gc`); reuses the existing marker→`PlatformArray` return path. A
  Rust `#[no_mangle]` fn now returns a first-class managed `System.Int32[]` to C# (not a ptr/len pair).
  Verified: `cd_interop_tier2`'s `make_ints()` → `int[]{10,20,30}` consumed in C# (8/8 checks); gate
  426/12. C-mode/JVM array return left as documented `todo!` (only .NET needed); arrays-of-structs/
  strings + the **generic-interop bridge remain XL / scope-only**.
- **WF-C (typechecker — SAFE measurement pass)** — DONE. **Key finding: the config flags are dead
  wiring.** `TYPECHECK_CIL`/`VERIFY_METHODS`/`ENFORCE_CIL_VALID`/`CHECK_REFS`/`ALLOW_MISCOMPILATIONS`
  are declared in `src/config.rs` but **never read** — the checker already runs unconditionally and
  only warns (`src/lib.rs:304`→`cilly/src/ir/asm.rs:184` eprintln; `src/assembly.rs` span_warn). So
  "flip `ALLOW_MISCOMPILATIONS`" is **impossible without first wiring the flags** — a prerequisite
  refactor, not a config change. Measured ~105 violations/build (std-systemic): **~75% `WriteWrongAddr`
  = false positives of the checker's own model** on code that runs green (a hard gate would reject std
  day-one). The **only ~5 plausible real silent miscompiles**: fat-pointer **nesting** mismatches
  (`FatPtr<u8>` vs `FatPtr<FatPtr<u8>>`) in `LocalAssignmentWrong`(3) + `String::push_str_slice`(2) —
  the DST/fat-ptr class. **Flip verdict: NOT feasible now**; staged path = A clear checker false
  positives (void-`StLoc` DONE here = task #43) → B investigate the ~5 fat-ptr-nesting violations → C
  wire flags + make fatal behind-flag + green `::stable` → THEN the owner's default-flip decision is
  meaningful. Landed: an 11-line log-only void-`StLoc` guard (`cilly/src/ir/typecheck.rs`); gate 426/12,
  config defaults unchanged.
- **WF-TC (typechecker soundness hunt)** — DONE, decisive. Differential-tested (native vs backend,
  byte-for-byte) the ~5 fat-pointer-nesting flags + the 3 dominant false-positive families. **Verdict:
  ZERO real miscompiles** — all are checker-model false positives. Decisive fact: `fat_ptr_to`
  (`type.rs`) builds *every* fat pointer with identical layout `{void* @0, usize @8, size 16}`; the
  inner type only changes the interned name, so `FatPtr<u8>` ≡ `FatPtr<FatPtr<u8>>` at the bit level
  and the checker's name-compare mis-flags them. `WriteWrongAddr` ppX/X + void-addr: `StInd` picks the
  store from `tpe`, not the address pointee (`tpe:Void` still correctly throws). `catch_unwind *Data→
  *u8`: matches `__rust_try`'s erased ABI. **Zero source changed** (checker relaxations recommended but
  owner-gated — left unapplied; an advisory checker needn't be correct to be safe). Evidence crates
  `cargo_tests/cd_fatptr` + `cd_fpfam`; gate 426/12. Honest scope: only these 5 patterns were
  differential-tested; the un-triaged tail of the ~111 advisory warnings stays flagged, not silenced.
  RECOMMENDED-NOT-APPLIED owner-gated checker fixes (for a deliberate future pass) are recorded in the
  WF-TC result — narrow layout-based fat-ptr equivalence + the two `StInd` disjuncts + the `*u8`-sink arm.
- **WF-F (threading/TLS)** — scoped + first-slice attempted (WF-F1): **spawn/join already fully real**;
  the only blockers to correct multithreading are `Mutex` (no_threads `Cell<bool>`) + per-thread TLS
  (process-global statics). Mutex-first plan ready (real `SemaphoreSlim`-backed `Mutex` via new cilly
  hooks = self-contained/low-risk; per-thread TLS = gate-risky 2nd slice). WF-F1 landed NOTHING (the
  new threading codegen can't ship unverified, and the rebuild+gate+repeated-probe loop wasn't
  completed in-pass). HELD for a dedicated implement pass.
- Branch `gaps-campaign` (off `main` = pushed Tier-2 state); pushed to `mine/gaps-campaign`.
