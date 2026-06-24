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

### P2 status (in progress — iterative slices, I2 NOT complete)

**P2-S1 (commit 69b8e0e).** Stood up the differential oracle (native rustc vs `cargo dotnet`,
byte-identical stdout+exit) and fixed 3 real codegen miscompiles: float-valuetype `TypeLoadException`
(`class.rs`), `u128/i128` `ctlz/cttz` garbage (`ints.rs`), and the sub-word atomic
`InvalidProgramException` that crashed all 8/16-bit atomics + `catch_unwind` (`atomics.rs`). Each has a
permanent build-std regression crate (`float_class_methods`, `wideint_ctlz`, `cd_subword_atomics`).
Proved flt2dec ULP a non-divergence; classified C-variadic `printf` (fuzz47/86/87/96) + half
`f16/bf16` as fundamental walls.

**P2-S2 (this slice).** Extended the oracle to also diff **stderr** (the panic family is a
stderr-shape question). Fixes + findings:

- **FIXED — `std::process::exit(code)` aborted instead of exiting with the code.** The injected
  `target_os="dotnet"` arm of `sys::exit::exit` was `let _ = code; crate::intrinsics::abort()` — it
  dropped the requested code and threw "Called abort!" (SIGABRT, exit 134) where native rustc exits
  with `code`. Fixed: the arm now declares + calls `rcl_dotnet_exit(code)`, a PAL symbol the cilly
  linker (`insert_dotnet_exit`, `cilly/src/ir/builtins/dotnet.rs`) maps to
  `System.Environment.Exit((int)code)` — a clean managed process-exit carrying the code. (`libc::exit`
  is NOT usable here: std's in-tree libc shim does not declare `exit`.) Injected by both
  `feasibility/_cargo_dotnet_core.sh` and `tools/cargo-dotnet/src/palinject.rs`. Verified
  byte-identical vs native (stdout + exit 7) by **`cargo_tests/pal_exit_code`** (new permanent
  regression crate). Canonical Docker `::stable` gate stays **428 pass / 12 fail** (baseline, zero
  regressions).

- **Panic note STREAM routing is already CORRECT.** Contrary to the S1 deferral note, the dotnet PAL
  panic note already lands on **stderr** (fd 2 → `Console.Error`): `rcl_dotnet_write(fd=2,…)` maps to
  `Console.Error.Write` and `dotnet_pal/sys/stdio/dotnet.rs` routes fd 2 → `Stderr` →
  `panic_output()`. Re-verified by splitting the produced `.dll`'s streams directly (`panic!` +
  `catch_unwind`, index-OOB, `expect`, `assert_eq!`): all panic notes + `note:` + `eprintln!` lines are
  on stderr, stdout matches native exactly. No fix needed.

- **OPEN (real, deferred) — caught/uncaught panic note reports the WRONG caller-location text.**
  Every panic note prints `panicked at <WORKSPACE>/src/panic/location.rs:181:9` (the body of
  `core::panic::Location::caller`) instead of the user call site (e.g. `src/main.rs:4:9`). Reproduces
  minimally with a bare `Location::caller()` and with `panic!`/`v[i]`/`expect`/`assert_eq!`. **Root
  cause is NOT in the call/intrinsic/Assert terminator codegen** — instrumentation proved
  `caller_location` never reaches those paths for these programs; the wrong `Location` is materialized
  upstream as a **`ConstOperand`** (the const-`Location` value baked by rustc's MIR/const-eval), so the
  divergence is in const-`Location` materialization (`rustc_codgen_clr_operand`), not the
  `#[track_caller]` threading. Program *logic* (catch → `is_err()`, `Err`, exit code) is fully correct;
  only the diagnostic `file:line:col` diverges. Tractable but a distinct, deeper fix — left for a
  follow-up slice. (A unifying `materialize_caller_location` helper mirroring rustc's
  `Body::caller_location_span` was prototyped and reverted: it is correct for the live track_caller-call
  path but does NOT touch the const path that actually produces these notes, so it added hot-path risk
  without delivering the fix.)

- **FOUND (build-time, deferred) — `overflow-checks = true` in the build-std profile ICEs the backend
  while compiling `std`.** A backend panic (swallowed by cargo) during the overflow-assert-heavy std
  build; **pre-existing** (confirmed independent of any S2 edit by bisect). Default profiles are
  unaffected (the `::stable` gate and all default-profile probes are green), so this is an
  overflow-checked-build limitation, not a default-path miscompile.

**Differential census (this slice), all default-profile, byte-identical stdout+stderr+exit vs native:**
edge probes `ep_float / ep_int / ep_str / ep_coll / ep_iter / ep_enum / ep_dyn / ep_overflow_rt`
(FP bit patterns, 128-bit ints, UTF-8/formatting, BTree/Hash/VecDeque/BinaryHeap, iterator adaptors,
enum/Result/Option, trait objects + closures, wrapping/checked/saturating/euclid) and soak crates
`itertools / hex / base64 / arrayvec / bitflags / byteorder / fxhash / smallvec / tinyvec / memchr /
indexmap / libm / euclid / approx / data-encoding / compact_str / bstr` — **all FULL MATCH**. The only
recurring real runtime divergence is the caller-location TEXT above; `soak_half` is the known f16/bf16
wall. **I2 remains open** (caller-location const path + overflow-checked-build ICE are the next
codegen targets; full upstream library suites still not routed in this env).

**CI-surface gap (honest).** The P2-S1 + S2 regression crates (`float_class_methods`, `wideint_ctlz`,
`cd_subword_atomics`, `pal_exit_code`) are `cargo dotnet` **build-std PAL** crates and do **not** fit
the `::stable` harness (`src/compile_test.rs`), which compiles against the host/surrogate target
without build-std and cannot exercise the dotnet PAL. They need a **separate CI surface**: a
`cargo dotnet`-driven differential runner (akin to `feasibility/dev.sh pal-build`, extended to run the
artifact and diff stdout/stderr/exit vs native). Not force-fit into `::stable`. Tracked as a P2 CI
follow-up.

**P2-S3 (this slice) — the two OPEN codegen targets above, both fixed + differential-verified.**

1. **Caller-location text — FIXED (the S2 "ConstOperand" diagnosis was WRONG).** The S2 note above
   blamed a const-`Location` `ConstOperand`; that was a *measurement artifact* of build-std core
   caching. The native `cargo dotnet` harness reuses compiled `core`/`std` artifacts across crates
   (cargo fingerprints the RUSTFLAGS *string*, which holds the backend dylib **path**, not its
   content), so the instrumentation never saw `Location::caller`/`panic_bounds_check` recompiled and
   wrongly concluded the intrinsic path was untouched. The real cause is exactly the `#[track_caller]`
   threading: the backend materialized `span_as_caller_location(span)` with the *local* statement span
   at all three sites (the `caller_location` intrinsic, the track_caller call-site append, the
   `Assert`→panic-lang-item append), so `Location::caller()` reported its own body
   (`location.rs:181:9`). FIX: a single `get_caller_location(ctx, source_info)` helper
   (`src/terminator/mod.rs`) that mirrors rustc's `FunctionCx::get_caller_location` — it forwards the
   enclosing fn's implicit trailing `&Location` arg when the fn is `#[track_caller]` (`LdArg(arg_count)`)
   and delegates the MIR-inlining scope walk to rustc's own `Body::caller_location_span` (release builds
   inline the track_caller chain, so the span must climb the inlined source scopes to the real user
   site). Threaded `SourceInfo` (not bare `Span`) through `call`/`call_inner`/`handle_intrinsic`/
   `intrinsic_slow`/`call_panic_lang_item`. Verified byte-identical vs native via
   `cargo_tests/caller_location` (depth-1/2 `Location::caller` chains, a 2-arg `#[track_caller]`
   forwarder = the `panic_bounds_check` shape, a non-track_caller `caller()`), plus a forced-clean-core
   bounds-check probe: native `line=11` == backend `line=11` (was `181`). **Harness footgun now FIXED**:
   `_cargo_dotnet_core.sh` + `tools/cargo-dotnet/src/rustflags.rs` fold the backend dylib's content hash
   into RUSTFLAGS as an inert, `--check-cfg`-declared `--cfg cd_backend_<hash>`, so cargo's build-std
   fingerprint busts EXACTLY when the backend changes (and only then — unchanged backend keeps the key,
   caching preserved, installed users pay nothing). This removes the silent-stale-`core` trap that
   produced the wrong S2 root cause; investigations can no longer be fooled by reused `core` artifacts.

2. **`fn main() -> T: Termination` ICE — FIXED.** `cilly::entrypoint::wrapper` handled only `() -> ()`
   and the C-main ABI, so `fn main() -> Result<_,_>` / `-> ExitCode` (non-`Void` return, no args) hit
   its `panic!`. FIX mirrors rustc's `create_entry_fn`: `src/lib.rs` detects the non-`Void`/no-arg entry,
   resolves `std::rt::lang_start::<main_ret_ty>` (`LangItem::Start`), and a new
   `entrypoint::wrapper_lang_start` loads a fn-ptr to user `main`, calls
   `lang_start(main_ptr, 0, null, sigpipe) -> isize` (which runs `main`, maps `T` via
   `Termination::report`, printing `Error: <e>` to stderr on `Err`), and propagates the returned code
   via `System.Environment.Exit`. The **fatal type-checker (I1) caught a first cut** (a `*const*const u8`
   argv built one indirection too deep — `pppu8` vs `ppu8`) at build time instead of miscompiling —
   exactly its job. Verified: `cargo_tests/term_main` (`Ok`-returning `Result`, exit 0) FULL MATCH;
   `Err`-returning `main` → `Error: "boom"` on stderr + exit **1**, and `-> ExitCode::from(3)` → exit
   **3**, both byte-identical to native via `dotnet <dll>` directly. (The single-file `cargo dotnet run`
   apphost still drops a non-zero managed exit code — the known P2-S2 harness limitation, orthogonal to
   this codegen; the `Ok`/exit-0 path is unaffected.) The plain `fn main() -> ()` path is unchanged.

Both fixes keep the `::stable` gate green (428/12) under the fatal checker, with **no** checker
relaxation (the type-verifier is the proof, not a bypass). Regression crates `caller_location` +
`term_main` join the P2 build-std differential set (same CI-surface gap as above).

**P2-S4 (this slice) — seam-audit Slice A: the `UnwindAction::Terminate` abort path.** The first
finding driven by the `docs/SEAM_AUDIT.md` reimplementation-drift map. `handler_for_block` mapped a
`Terminate` unwind *edge* to `None` (no handler), so a panic that must abort uncatchably (escaping a
`nounwind`/`extern "C"` boundary) instead propagated as a managed exception an outer `catch_unwind`
**absorbed** — a real silent-wrong (`cargo_tests/term_abort`: backend printed `REACHED`/exit 0; native
aborts). FIX (`src/basic_block.rs` + `src/terminator/mod.rs` + `src/assembly.rs`): route a `Terminate`
edge to a **synthetic `FailFast` handler** (id placed past the dense MIR block range, materialized in
`add_fn`), via a shared `emit_terminate` helper now also used by the `UnwindTerminate` *terminator* —
dispatching the message on `Abi` vs `InCleanup` (closes the audit's "reason ignored" gap #6). Verified:
`term_abort` aborts uncatchably (stdout `start`, never `REACHED`; `dotnet <dll>` exit **134** = native).
**Honest partial close:** this closes the med-impact headline (gap #3, a panic *directly* in a nounwind
context, the common case) + #6. It does **not** cover a `Terminate` edge on a MIR *cleanup* block — a
destructor panicking *during* unwinding (`InCleanup` double panic) or a `Drop` unwinding inside a
nounwind fn — because cleanup blocks are the catch *body* of a normal block and the exporter renders a
single try/catch layer per block (no nested regions). That tail is a documented low-impact residual
(needs nested exception regions; `BROKEN_TESTS.md` §P2-S4), not a regression. Gate stays 428/12 under
the fatal checker; `typecheck.rs` unchanged.

**P2-S5 (this slice) — seam-audit Slices B + C + D (the tractable remainder), all FIXED + FULL MATCH.**
Driven by the `seam-close` workflow (it produced verified designs + rustc-API checks in parallel; the
parent implemented on the live tree + did the execution-differential verification, since the workflow's
isolated worktrees landed on a stale pre-V1→V2-flip base whose diffs don't apply). **B** —
`src/terminator/mod.rs` Assert arm elides an optional overflow assert when `!sess.overflow_checks() &&
msg.is_optional_overflow_check()` (mirrors `codegen_assert_terminator`), so a `#[rustc_inherit_overflow_checks]`
helper inlined into a release crate WRAPS (300→44) instead of panicking. **C** —
`rustc_codgen_clr_operand/src/constant.rs` `create_const_from_data` stops discarding `offset_bytes`
(gates the whole-alloc scalar path on `offset==0`; adds the byte offset to the pointer in the by-ref
paths like `load_scalar_ptr`), so a const into the middle of a larger alloc (`ARR[2]`) reads the right
sub-object. **D** — three loud-ICE arms: signed `atomic_max`/`atomic_min` (wired to the sign-aware
helpers), `atomic_singlethreadfence` (folded into `atomic_fence`), and narrow `PointerExposeProvenance`
(`ptr as u32`, generalized to `Type::Int(_)`). Regression crates `overflow_elision` / `indirect_offset`
/ `atomic_cast_arms`, all byte-identical to native; gate 428/12 under the fatal checker, `typecheck.rs`
unchanged. **Net across the seam audit (before P2-S6):** all 4 LOUD gaps + 5 of the 6 SILENT_WRONG closed; the lone
remainder was the Slice A cleanup-block `Terminate(InCleanup)` double-panic — closed next.

**P2-S6 (this slice) — Slice A CLOSED: cleanup-block `Terminate` via nested exception regions.** The last
open seam-audit gap, and the one architectural EH change. A destructor that panics *while a first panic
is unwinding* (a double panic, `Terminate(InCleanup)`) sits on a `Drop` call in a MIR *cleanup* block;
P2-S4's synthetic FailFast handler only covered NORMAL-block edges (cleanup blocks are never run through
`resolve_exception_handlers`, and the il_exporter renders one try/catch layer per block). Produced by the
`eh-nested-regions` workflow (5-facet investigate → 3-design judge panel that disqualified full-nesting as
high-blast-radius and a naive promotion as producing dangling `leave` IL → a single writer agent), then
integrated + re-verified by the parent. FIX: a new leaf IR root `CILRoot::TerminateRegion { protected,
reason }` that the frontend `Drop` arm wraps around ONLY the cleanup-block drop call; it exports as an
inner `.try{ <drop>; leave done } catch System.Object { pop; ldstr msg; Environment.FailFast(msg);
rethrow } done: nop` — an **uncatchable** abort — WITHOUT any `BasicBlock` carrying a nested handler, so
the single-layer handler model / `resolve_exception_handlers` / `block_gc` / the 2-layer assert are all
untouched and IL is byte-identical for every method without such an edge. Threaded through the IR core
(iter recurse, `realloc_roots` re-intern of the off-block-list `protected` child, `asm_link` translate,
`typecheck` delegate); C mode delegates unguarded (no managed EH — a mid-cleanup panic already aborts).
Verified: `cargo_tests/double_panic` aborts (`dotnet <dll>` exit 134 with the "panic in a destructor
during cleanup" FailFast message; stdout `start`, never `REACHED`); a NORMAL `catch_unwind` still catches
(`caught=true`, exit 0 — the load-bearing no-regression check); the P2-S4 extern-C abort still aborts.
Docker `::stable` gate: no real regressions (no drop/panic/unwind/terminate test failed), fatal checker
accepts `TerminateRegion`. **With this, every confirmed seam-audit gap is closed** except the deliberate
`#[link_section]` loud wall. (Residual: a cleanup-block `Call`-with-`Terminate` — not `Drop` — stays on
the old never-wired route; does not occur in double-panic MIR, no regression, low impact.)

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

### P3 status (census done; first slice landed)

The **census is complete** — `docs/P3_TOTALITY.md` (the `p3-totality-census` workflow): 242
failure/fallthrough sites across 8 areas, classified reachable + loud-vs-silent, the silent-wrong subset
adversarially verified. **VERDICT: substantially I3-clean** — the faithful-to-MIR discipline means nearly
every catch-all is a LOUD `todo!`/`panic!` over a structurally-impossible MIR shape, not a silent wrong
fallback; the cast machinery and `place_projections` came up with ZERO silent-wrong; the fatal CIL
typechecker is a real loud backstop. ~180 sites are genuinely-exotic/unreachable loud walls (I3-acceptable,
documented). 6 work slices proposed (silent-wrong first).

- **P3-S1 (this slice) — the one reachable silent miscompile, FIXED.** `static OBJ: &dyn T = &S;`
  (const/static trait object) dispatched through a **null vtable** → `NullReferenceException`, zero
  diagnostic. Root cause = the campaign's recurring *reimplementation-drift* class, internal to the operand
  crate: two implementations of "resolve a `GlobalAlloc` relocation to a pointer" — `load_scalar_ptr`
  (correct) and `add_allocation`/`create_const_from_data` (stubbed `VTable`/`Function` to a null static).
  FIX (`p3-const-ptr-unify` workflow → integrated + re-verified by the parent): the `add_allocation`
  `VTable` arm now delegates to the shared `crate::constant::get_vtable` (and `Function` → `LdFtn`),
  recovered via `unwrap_vtable()`. Because `add_allocation` is the **single chokepoint both paths funnel
  through** (path A's `get_vtable` calls it; path B's reloc loop calls it), fixing the one arm makes the
  resolver **total for every caller** — true unification, not a re-duplication. `get_vtable` →
  `tcx.vtable_allocation` → a `GlobalAlloc::Memory` alloc the Memory arm already materializes correctly, so
  no self-recursion. Verified: `cargo_tests/cd_static_dyn` FULL MATCH vs native (static `&dyn` / array-`&dyn`
  / fn-ptr static → 7/7/42, was a null-NRE crash); fatal type-checker accepts the output; Docker `::stable`
  gate 426/14 = baseline, no regressions, no const/static/vtable test failed. One small tracked change
  (`static_data.rs`, +38/-19). The remaining P3 slices (S2 trivial loud-ICE one-liners, S3 fn-ptr store +
  variadic wall, S4 size-clamp→fatal, S5 ABI two-enum + signed static defaults, S6 typechecker totality)
  are loud/exotic or hardening — none is another reachable silent miscompile.

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
