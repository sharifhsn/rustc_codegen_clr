# Roadmap to `std`: complexity/LOC framework, architecture, and plan

The work splits into **two horizons** that must not be conflated:

- **Horizon 1 — Restore the surrogate std.** Un-rot the nightly-port regressions so real
  `alloc`/`std` compile and run again on the existing Linux-x86_64 surrogate (the ~95%-of-test-
  suite state that worked at GSoC). *Bounded; debugging-heavy; small LOC.*
- **Horizon 2 — Build a proper .NET std.** Replace the fragile surrogate-libc with a real
  `.NET` target + a native `std::sys` platform layer (the wasm/SGX model). *The real
  architecture; thousands of LOC + upstreaming; months.*

You get most of the *near-term* value (real projects compile, EF Core spike becomes possible)
from Horizon 1. Horizon 2 is what makes std *correct, portable, and maintainable*.

---

## 1. The assessment framework (how to measure complexity & progress, repeatably)

Don't estimate std once — *measure* it continuously with five instruments. Each turns a vague
"does std work" into a number you can track:

| # | Instrument | What it tells you | How |
|---|---|---|---|
| **M1** | **build-std crate walk** | the current *frontier* | `cargo build -Zbuild-std=core,alloc,std` — the first crate that fails is where you are. |
| **M2** | **named-gap backlog** | the *finite* list of holes | grep the run for the backend's own rejections: `todo!`, `span_fatal`, and typecheck variants (`FieldOwnerMismatch`/`CallArgTypeWrong`/`not yet implemented`). Dedupe by message → a prioritizable backlog. (Adopt cranelift's "fail loud + specific" so every hole is named.) |
| **M3** | **differential validator** | *runtime correctness* (miscompiles) | [`feasibility/validate/`](../../feasibility/validate/) — native vs .NET diff once it compiles. |
| **M4** | **std test-suite %** | *coverage* | the `core`/`alloc`/`std` test suites (README tables track this historically). |
| **M5** | **LLVM-vs-self sysroot bisect** | *which crate miscompiles* | copy cranelift's `SysrootKind::Llvm` toggle — swap one std crate to the real backend to localize a bug. (Not yet built — ~150 LOC, high leverage.) |

**Per-subsystem complexity rubric.** Score each std subsystem on the axes below; the product
of (work-type × tier) and the runtime-LOC column give the estimate.

- **Work type:** `R` regression-fix (codegen bit-rot) · `C` codegen-feature (new lowering) ·
  `P` PAL module (a `std::sys` platform layer) · `S` runtime shim (managed support lib).
- **Tier 1–5:** 1 = mechanical, 3 = needs design, 5 = research/upstream-coupled.
- **Validate with:** which instrument (M1–M5) confirms it.

---

## 2. The filled-in assessment

| Subsystem | Status now | Work type | Tier | LOC (codegen / runtime) | Validate |
|---|---|---|---|---|---|
| core | ✅ works | — | — | — | M1/M4 |
| alloc: Box/Vec/String | 🔴 regressed | **R** (fat-ptr nesting) | 2 | ~50–150 / 0 | M1 |
| alloc: Rc/Arc/Cell | 🔴 regressed | **R** (type identity) | 3 | ~100–300 / 0 | M1 |
| collections | ⚠️ untested | C | 2 | small / 0 | M3 |
| fmt | ⚠️ suspect | R | 2 | small / 0 | M3 |
| panic/unwind | ✅ (→ .NET exc) | C (harden) | 2 | ~0 / small | M3 |
| atomics (8/16-bit) | ⚠️ lock-emulated | C + target flag | 3 | small / small | M3/run |
| thread/sync | ⚠️ emulated pthreads | **P + S** | 4 | ~200 / ~500 | M3/run |
| TLS (+ destructors) | ⚠️ emulated POSIX | **P + S** | 4 | ~100 / ~300 | run |
| env / args | ⚠️ argv/.cctor hack | **P** | 3 | ~50 / ~200 | run |
| time | ⚠️ libc | P/S | 2 | ~0 / ~150 | run |
| io / fs | ⚠️ libc P/Invoke | **P** | 4 | ~100 / ~800 | M3/run |
| net | ⚠️ libc, flaky | **P** | 4 | ~50 / ~700 | run |
| process | ⚠️ libc | P | 3 | ~50 / ~400 | run |
| **os / `sys` PAL (proper)** | 🟥 surrogate only | **P + S + target** | 5 | ~target spec / ~2.5–3.5k | M1–M5 |
| intrinsics/SIMD long tail | ⚠️ partial | C | 3–4 | ~1–3k / 0 | M2 |

---

## 3. LOC budget (order-of-magnitude; refine via M1/M2 as you go)

| | Codegen-side LOC | Runtime/PAL-side LOC | Effort | Confidence |
|---|---|---|---|---|
| **Horizon 1 — restore surrogate std** | ~100–500 (mostly root `type`, `typecheck`, `unsize`, `aggregate`) | ~0 | **2–6 weeks** (debugging-dominated, not typing) | medium-high — it's bit-rot in two known-fragile areas, same family as fixes already shipped this session (pattern_type, c_char, the v0_1_3 `FatPtrg` fix) |
| **Horizon 2 — proper .NET std PAL** | ~1–3k (target spec, intrinsic gaps, ABI/atomics) | ~3–6k (a `std::sys` PAL ≈ 2.5–3.5k by the unix/sgx reference, + `mycorrhiza` growth over today's 14k) | **months** + upstream process | low-medium — depends on upstream cooperation and how much BCL reuse vs P/Invoke |

Anchors: cg_clr is already ~55k LOC (src 12.5k, `cilly` 24.5k, `mycorrhiza` 14k); cranelift 14k /
gcc 25k *with the host-OS shortcut we can't use*; a functional std `sys` layer is ~2.8k LOC
(unix 2795, sgx 2818). So Horizon 2's net-new is comparable to *one* OS's `sys` layer plus the
managed shims behind it — not a from-scratch std.

---

## 4. Architecture decision

**Recommendation: do Horizon 1 on the surrogate now; commit to a proper `.NET` target for
Horizon 2 — but build the PAL *behind a target spec from day one* so the two aren't a rewrite.**

- The surrogate-libc approach is a dead end for *correctness and portability* (errno clobbering,
  `set_env` desync, fork UB, lock-emulated sub-word atomics, Linux-only, per-OS libc metadata) —
  but it's the fastest path to "real projects compile", which unblocks the EF Core spike and
  every other experiment. **Use it as the stepping stone, not the destination.**
- The destination is a **real target triple** (`dotnet-*`) whose `std::sys` is implemented on
  .NET APIs. A target spec is also the right place to *declare capabilities* (no 8/16-bit
  atomics, panic strategy, pointer width) so the compiler stops emitting things the runtime
  can't honor — removing whole classes of the surrogate's hacks.
- **.NET is genuinely *easier* than native backends in two places** the others found hardest:
  **unwinding** (native .NET exceptions vs cranelift's multi-year regalloc saga) and **i128**
  (`System.Int128`). Spend the saved budget on the PAL.

## 5. The structure to build

Keep the codegen dumb; push platform complexity outward into layers (mirrors cranelift/gcc's
"funnel to runtime helpers"):

```
 rustc MIR
    │  (lower — keep it dumb; span_fatal unsupported, don't emulate asm!)
    ▼
 cilly IR ──► .NET CIL / C
    │
    ├─ target spec (dotnet-*.json): capabilities, panic strategy, ptr width
    │
    └─ std::sys PAL  ── thin Rust, calls ──►  mycorrhiza runtime support library
       (threads,env,                          (CLR threads, Interlocked, ThreadStatic,
        args,tls,fs,                            .NET BCL fs/net/time/process, GC heap
        net,time,                               alloc) — the managed side, where the
        process,alloc)                          real platform work lives
```

Adopt from the mature backends, in priority order:
1. **Fail-loud + specific** everywhere (turns M2 into a real backlog) — cheap, do first.
2. **LLVM-vs-self sysroot toggle** (M5) — ~150 LOC, makes every later bug bisectable.
3. **git-commit-per-patch sysroot** — bit-rot resistance for the inevitable nightly bumps.
4. **`panic=abort` baseline**, unwinding as the (easier-here) feature.
5. **Allocator → managed heap**, **atomics/TLS → Interlocked/ThreadStatic** — cheap wins.

## 6. Phased plan with milestones

- **Phase 0 — instrument (days).** Wire M1/M2 (build-std walk + named-gap backlog) and the M5
  sysroot toggle. Deliverable: a `make std-status` that prints the frontier crate + gap list.
- **Phase 1 — restore surrogate std (Horizon 1, weeks).** Fix type-identity (`FieldOwnerMismatch`)
  and fat-ptr nesting (`CallArgTypeWrong`); finish new type-kinds. **Milestone: real
  `alloc`+`std` compile via build-std; `cargo_tests/build_std` + `std_hello_world` pass; the
  differential validator (M3) is green on `kitchen_sink` and a real CLI.** ← *unblocks EF Core spike.*
- **Phase 2 — define the `.NET` target + PAL skeleton (Horizon 2 start, weeks).** Add the target
  spec; stand up a `std::sys` PAL that delegates to `mycorrhiza`, starting with the cheap wins
  (alloc, atomics, TLS, env/args, time). **Milestone: std runs without the argv/.cctor/errno hacks.**
- **Phase 3 — the platform surface (months).** threads/sync on CLR threads, fs/net/process on the
  BCL. **Milestone: a real multi-threaded, file/network-using program validates (M3).**
- **Phase 4 — upstream (ongoing).** Land the target + `std::sys` patches upstream (tier-3), per
  `target.md`. **Milestone: `rustup target add dotnet-*` works without a forked std.**

The framework (§1) is the throughline: at every phase, M1–M5 tell you the frontier, the backlog,
the correctness, and the coverage — so "how far to std" is always a measured number, not a guess.
