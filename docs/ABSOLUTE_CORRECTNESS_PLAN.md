# Plan: translate everything possible, with absolute correctness

> Companion to [GAPS.md](GAPS.md) (the feature backlog) and [TRANSLATION_STATUS.md](TRANSLATION_STATUS.md)
> (what maps today). This document is the **correctness program**: how to drive `rustc_codegen_clr`
> from "compiles ~90% of std, verified by a narrow gate + sampled differentials" to "every
> translatable Rust construct is machine-checked type-safe, differentially equivalent to native rustc
> at scale, and nothing untranslatable is ever silently miscompiled."

## 0. What "absolute correctness" can and cannot mean (read this first)

**It cannot mean a formal proof.** True formal correctness — a mechanized proof that the CIL emitted
for any Rust program refines the program's semantics — requires a formal MIR semantics, a formal CIL
semantics, and a machine-checked refinement proof. That is a multi-person-year verified-compiler
research effort (CompCert/RustBelt scale), and it is **out of scope**. Claiming it would be dishonest.

**What is achievable is defense-in-depth empirical + type-level correctness**, resting on three
invariants. The plan is the work to make all three hold:

- **I1 — Soundness (no ill-typed CIL).** Every emitted method passes a *sound, fatal* type verifier
  (the cilly typechecker made correct + blocking) AND the .NET runtime's own IL verifier. If a method
  can't be proven type-consistent, the build **fails** — it is never emitted.
- **I2 — Behavioral equivalence (same result as Rust).** For every program in an exhaustive corpus
  (full std/core/alloc test suites + a large crate corpus + a differential fuzzer + edge-case probes),
  the backend's observable behavior — stdout, exit code, panic message, float bit-patterns, thread
  interleavings' invariants — is **identical to native rustc** on the same nightly.
- **I3 — Totality with loud failure (no silent gap).** Every MIR construct / intrinsic / `TyKind` is
  either (a) supported-and-tested, or (b) a documented hard wall that **fails the build with a clear
  message**. There is **zero reachable `todo!`/`unsupported`/silent-wrong path**. The impossible may
  refuse to compile; it may never miscompile.

"Everything *possible*" = everything that has *any* sound mapping to .NET. The irreducible walls (§7)
are explicitly excluded and, by I3, fail loud instead of producing wrong code.

## 1. The inversion (why the current posture must flip)

Today: a narrow `::stable` gate (skips f128/num_test/simd/fuzz*) + sampled differentials are the
certificate; the type verifier is dead-wired and advisory; `ALLOW_MISCOMPILATIONS` defaults **true**
(emit-wrong-and-continue); there is **no IL verifier**; the fuzzer and full library suites exist
(`bin/fuzz.rs`, `setup_rustc_fork.sh`) but aren't gates. None of that can support an "absolute
correctness" claim. The plan flips each:

| Axis | Today | Target |
|---|---|---|
| Type safety | dead-wired advisory checker; `ALLOW_MISCOMPILATIONS=true` | sound checker, **fatal by default**; `ALLOW_MISCOMPILATIONS` removed/false |
| IL validity | unchecked | every assembly passes `ilverify` |
| Behavioral check | `::stable` subset + sampled diffs | full std suites + crate corpus + fuzzer, all differential, **zero diffs** |
| Unsupported | 433 `todo!`/silent paths | loud compile error, **0 reachable** |
| Platforms | Linux x64 + macOS arm64 | + Windows; + AOT |

## 2. Phase P0 — Define + automate the oracle (foundation)

- **Oracle = native rustc on the pinned nightly.** Build the differential harness as first-class:
  given any crate, build native + backend, run both, assert byte-identical observable behavior
  (stdout/stderr-shape/exit/panic text; for FP, exact bit patterns). Generalize the existing
  `feasibility/cargo-dotnet` + `bin/fuzz.rs` machinery into one `differential <crate>` command.
- **Promote the real gate.** Replace the `::stable`-subset CI gate with: (full library suites diff) +
  (crate-corpus diff) + (fuzzer diff) + (typecheck-fatal) + (ilverify). The current 426/12 stays as a
  fast smoke test, not the certificate.

## 3. Phase P1 — Make the type system a fatal gate (delivers I1) — **highest priority**

Nothing else can be called "correct" until this holds.
1. **Wire the dead flags** (`TYPECHECK_CIL`/`VERIFY_METHODS`/`ENFORCE_CIL_VALID`/`ALLOW_MISCOMPILATIONS`
   are declared-but-never-read): make them actually gate the typecheck and actually abort.
2. **Make the checker SOUND — zero false positives.** Apply the WF-TC-recommended narrow fixes
   (layout-based fat-ptr equivalence; the two `StInd` disjuncts; the `*u8` erasure-sink arm) and triage
   the remaining un-triaged tail of the ~111 advisory warnings to zero — each via native-differential
   proof, never by blanket relaxation.
3. **Audit for false NEGATIVES.** A sound checker must also *catch* real errors: review every
   `CILNode`/`CILRoot` arm of `typecheck.rs` for gaps (it currently has no general `Ptr→Ptr` rule,
   etc.). Add a deliberately-miscompiled corpus that the checker MUST reject.
4. **Flip it fatal + remove `ALLOW_MISCOMPILATIONS`.** Every method must typecheck or the build fails.
5. **Add `ilverify`** (CoreCLR's IL verifier) as an independent second oracle over the final assembly —
   catches anything the cilly checker's model misses.
Effort: L–XL. This is the spine.

### P1 status (delivered)

1–4 are **done**. The flags are wired (`cilly/src/ir/asm.rs::typecheck` reads `TYPECHECK_CIL`/
`VERIFY_METHODS`/`ALLOW_MISCOMPILATIONS`; `src/lib.rs` join_codegen + the `linker` both run it). The
checker is **sound to zero false positives** across the full `::stable` build + the std/probe/soak
corpus: the full-std build (`build_std`) went from 99 advisory violations to **0** via six narrow,
each-proven-benign relaxations (fat-ptr layout-equivalence; the StInd extra-indirection + void-address
disjuncts; the StLoc `PtrCast`-noop pointer-relabel + bool↔int CIL-stack arms; the `*u8`/`*void`
erased-pointer-sink for both direct and FnPtr call args). The lone un-triaged family (D, the
`IndexRange`-cursor `ppX/pX`) was differentially proven benign by `test/iter/array_byval.rs`. The
false-negative audit is covered by `cilly/src/ir/typecheck.rs::tc_tests` (a float-into-int store, an
i64-where-f64 call arg, and a registered ill-typed method are all still **rejected**; the fatal-abort
path is unit-tested). `ALLOW_MISCOMPILATIONS` now **defaults to `false`** — the build aborts on the
first ill-typed method. The canonical Docker `::stable` gate stays green under the fatal checker
(428 pass / 12 known-baseline fail, **zero** new failures, **zero** fatal aborts).

5. **`ilverify` — DEFERRED (env-available, not yet a sound gate).** `dotnet-ilverify` 8.0.0 installs
   and runs, but over an emitted assembly it reports ~34k errors dominated by `UnmanagedPointer`,
   `InitLocals`, `StackByRef`, `StackUnexpected`, `Unverifiable` — i.e. the backend's *intentional*
   unsafe-IL idioms (raw pointers, non-zeroed locals, byref pointer arithmetic), the direct analogue
   of C# `/unsafe`. CoreCLR runs this unverifiable-but-correct IL fine (the gate proves correct
   execution). Using ilverify as a pass/fail oracle therefore needs a curated ignore-set for those
   structural classes (which risks masking real errors *within* a class) or a non-strict mode CoreCLR
   does not expose. Tracked as a follow-up; the cilly typechecker remains the sound fatal gate for I1.

## 4. Phase P2 — Exhaustive behavioral equivalence (delivers I2)

- **Full library suites to zero diffs.** Run core/alloc/std test suites (via `setup_rustc_fork.sh`) under
  the backend, differential vs native; drive the real tail to zero: float-formatting ULP (`flt2dec`),
  `f32/f64::min/max` NaN/signed-zero, `i128/u128` `pow`/`isqrt`, `bignum` overflow, iterator codegen,
  the sub-word/RMW atomics. Update `BROKEN_TESTS.md` → empty.
- **Crank the fuzzer.** `bin/fuzz.rs` already finds cases (the skipped `fuzz47/86/87/96`); run it
  continuously as a differential oracle, fix every divergence, un-skip them. Add structure-aware
  generation (types, generics, closures, FP, overflow, slices/DSTs).
- **Crate corpus.** Expand the soak set (75 → top-N crates.io) under the differential harness.
- **Systematic edge probes** for what fixed tests under-cover: FP bit-exactness, multithreaded
  invariants (build on the new `pal_threads`), panic/unwind across boundaries, atomics under contention.

## 5. Phase P3 — Totality census + loud failure (delivers I3)

- **Enumerate every construct:** each MIR `Rvalue`/`StatementKind`/`TerminatorKind`, each rustc
  intrinsic, each `TyKind`, each `CILNode`/`CILRoot` — into a tracked matrix: supported+tested / wall /
  must-fix. (Seed from the 433 `todo!` markers + the `TyKind`/intrinsic enums.)
- **Invariant sweep:** convert every *reachable* `todo!`/`unsupported`/silent-fallthrough into either a
  real implementation or a clear `fatal!("X is unsupported on .NET because …")`. CI test: grep proves
  no bare `todo!()` survives on a reachable codegen path.
- **Close the tractable coverage gaps** (also in GAPS.md): the remaining sync primitives
  (`Condvar`/`RwLock`/`Once`/`Parker` — same hook pattern as the just-landed Mutex/TLS), TLS-drop
  destructors, the SIMD walls that are polyfillable, arrays-of-structs/strings interop.

## 6. Phase P4 — The frontier ("everything *possible*", with cost)

- **`f128` via softfloat.** No native .NET quad float, but it is *translatable* via a softfloat
  library (correct though slow) — bring it under the differential oracle rather than leaving a wall.
- **Generic-interop bridge (WF-9):** generic Rust types ↔ generic .NET.
- **Multi-platform:** verify the whole correctness gate on Windows x64 and under Native AOT.
- **Optimizer correctness:** any optimizer pass must be proven behavior-preserving under P1+P2 (the
  typecheck-fatal + differential gates make a wrong optimization fail loud, not silently miscompile).

## 7. The irreducible walls (the boundary of "possible")

These have **no sound mapping** to stock CoreCLR; under I3 they **fail loud**, never miscompile:
`fork`/`vfork`/`execve`; arbitrary novel inline/`global_asm`; `mmap MAP_FIXED`/shared, `mprotect`,
`brk`; real signal delivery beyond the 4 .NET exposes; true inode/dev/nlink identity, abstract-ns
`AF_UNIX`, `SCM_RIGHTS`; zero-cost open generics overlapping managed refs; static borrow-safety
*guarantees* across the managed boundary. (f128 moves OUT of this list via P4 softfloat.)

## 8. Sequencing + honest framing

**Order: P0 → P1 → P2/P3 (interleaved) → P4.** Correctness-first is non-negotiable: P1 (fatal type
gate + ilverify) must precede broad coverage, or every new feature is again "probably right." P1 will
likely surface real bugs the moment it's fatal — that's the point.

**The honest ceiling:** completing P0–P4 yields a backend where *every emitted method is machine-checked
type-safe, the full std test surface + a fuzzer + a large corpus are differentially identical to native
Rust, and nothing untranslatable is silently wrong.* That is the strongest correctness attainable
without a formal verified-compiler proof — which remains a separate, multi-year research undertaking and
is explicitly not promised here. Each phase is gated by the same discipline used all campaign
(adversarial review + differential proof + commit-only-on-green), now with the verifier itself as the
arbiter rather than a narrow test subset.
