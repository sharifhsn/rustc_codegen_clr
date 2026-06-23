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

## Status log

- _(updated per workflow as the campaign runs)_
