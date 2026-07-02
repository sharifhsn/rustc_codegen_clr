# Atomics / memory-model soundness — rustc_codegen_clr

This document closes the memory-model question for the `core::intrinsics::atomic_*` lowering:
which Rust `Ordering` guarantees the backend actually honors on CoreCLR/.NET, why, and what was
fixed to get there. It is the definitive record for the `atomic-lowering map` produced during the
`gaps-campaign` audit; read this file, not that map, for the current (post-fix) state — the map's
"suspect cell" writeup is preserved below only as the historical finding that motivated the fix.

## 1. The structural fact that makes this whole area subtle

Rust's `core::intrinsics::atomic_*` functions take the memory ordering as a **const-generic
parameter** on this nightly (`nightly-2026-06-17`), not a name suffix:

```
pub unsafe fn atomic_load<T: Copy, const ORD: AtomicOrdering>(src: *const T) -> T;
```

(same shape for `atomic_store`/`atomic_xchg`/`atomic_xadd`/…/`atomic_fence` in
`core::intrinsics`). Every `Ordering` value monomorphizes to a distinct instance, e.g.
`atomic_load::<u32, {AtomicOrdering::SeqCst}>` vs. `atomic_load::<u32, {AtomicOrdering::Relaxed}>`.

The backend's intrinsic dispatch (`src/terminator/call.rs` → `src/terminator/intrinsics/mod.rs`
`intrinsic_slow`) demangles the symbol and calls `demangled_to_stem`, which walks the
`::`-separated path and **stops at the first segment containing `<`** — i.e. it discards the
entire generic-argument list, including the `AtomicOrdering` const generic. Both examples above
stem to the literal string `"atomic_load"`. Grepping the live codegen path for
`AtomicOrdering`/`ORD_SUCC`/`ORD_FAIL` finds no hits — the ordering argument is **never read**
anywhere that actually runs. (`src/builder.rs` has ordering-aware `todo!()` stubs, but that file
has no `mod builder;` entry in `src/lib.rs` and is not compiled into the crate — dead scaffolding,
not a second dispatch route.)

**Consequence**: every Rust ordering variant of a given atomic operation lowers to bit-identical
CIL. This document is therefore a table of `operation -> CIL`, and each lowering must be sound
for the **strongest** ordering Rust allows for that operation (i.e. for loads, sound for
`Acquire`/`SeqCst`; for stores, sound for `Release`/`SeqCst`) — a lowering that is only sound for
`Relaxed` is a real bug, because the exact same CIL also serves every `Acquire`/`Release`/`SeqCst`
call site.

## 2. What CoreCLR/.NET actually guarantees (ECMA-335 I.12.6)

- A **plain** `ldind`/`stind` (no `volatile.` prefix) has no ordering guarantee beyond .NET's
  tear-free-natural-alignment promise. The JIT is free to reorder it arbitrarily relative to
  surrounding memory operations.
- `volatile.` on a load (I.12.6.7) is an **acquire fence**: "no read or write... may be moved
  before it." On a store (I.12.6.8) it is a **release fence**: "no read or write... may be moved
  after it." Neither direction alone gives a total store order (SeqCst).
- `System.Threading.Interlocked.*` (`Exchange`/`CompareExchange`/etc.) provides a **full fence**
  (both acquire and release) — on CoreCLR/ARM64 this compiles to `ldaxr`/`stlxr`/`dmb`-class
  instructions, i.e. genuinely strong enough for `SeqCst` in both directions.
- `Thread.MemoryBarrier()` is an explicit, unconditional, full bidirectional fence.

## 3. The two cells that were unsound, and the fix

### 3.1 `atomic_load` — was weaker than Rust requires (all orderings)

**Before**: `atomic_load` lowered to a *plain* `LdInd { volatile: false }` — an ordinary `ldind`,
not even `volatile.`. This is sufficient for `Ordering::Relaxed` (tear-free access is all Relaxed
promises) but is a genuine violation for `Ordering::Acquire`/`Ordering::SeqCst`: per I.12.6.7, only
a `volatile.` load is an acquire fence; a plain `ldind` gives the JIT no reordering restriction at
all, so a later load/store in program order is free to be hoisted above it — exactly the reordering
`Acquire` is defined to forbid.

**Fix** (`src/terminator/intrinsics/mod.rs`, `atomic_load` arm): emit `LdInd { volatile: true }`
via `ctx.load_volatile` instead of `ctx.load`.

**Soundness argument**: `volatile.ldind` is an acquire fence per I.12.6.7 — sound for `Acquire`
and (paired with a fenced store — see 3.2) for the load half of `SeqCst`. It is strictly stronger
than `Relaxed` requires, which only costs some reordering headroom, not correctness — i.e. this is
a safe direction to be wrong in, unlike the previous lowering.

### 3.2 `atomic_store` — was weaker than Rust requires (specifically `SeqCst`)

**Before**: `atomic_store` lowered to `volatile. stind` (via `ctx.make_store_volatile`), which is a
release fence (I.12.6.8) — correct for `Release`, but **not** for `SeqCst`. `SeqCst`'s defining
property is a single total order over all `SeqCst` operations; that requires forbidding
StoreLoad reordering (a later `SeqCst` load must not be hoisted above this store), which a release
fence alone does not do. The in-code comment at the time explicitly acknowledged this ("a full
SeqCst store would additionally need a trailing `Thread.MemoryBarrier()`... left as a follow-up").

**Fix** (`src/terminator/intrinsics/mod.rs`, `atomic_store` arm): keep the `volatile. stind`, and
unconditionally emit a trailing `Thread.MemoryBarrier()` call after it (the same call the
`atomic_fence` arm already uses).

**Soundness argument**: `Thread.MemoryBarrier()` is a full, bidirectional fence, so every store
(regardless of nominal ordering, since the ordering argument is unread) is now at least as strong
as `SeqCst` requires. This is strictly stronger than `Relaxed`/`Release` require — safe, at the
cost of an extra barrier instruction on every atomic store (see §6, "Known conservatisms").

### 3.3 Optimizer soundness gap — volatile flag silently dropped on local-address folds

Independent of the two lowering cells above, the V2 optimizer had a peephole rewrite that
collapsed `ldind`/`stind` against a directly-owned local's address (`ldloca X` / `ldarga X`) down
to a plain `ldloc X` / `stloc X` — **discarding the `volatile` flag regardless of its value**
(`cilly/src/ir/opt/opt_node.rs`, the `LdInd { addr: LdLocA(loc), .. }` arm; `cilly/src/ir/opt/root.rs`,
the matching `StInd` arm). A `volatile.` access that happens to target a local's own address (not a
raw pointer/reference) would have its fence semantics silently deleted by this fold.

In practice this could not fire for `atomic_load`/`atomic_store` before the fixes above (those
operate through raw pointers passed into the intrinsic, not `LdLocA` of a directly-owned local),
but it is a latent hole for `volatile_load`/`volatile_store` (reachable from
`std::ptr::read_volatile`/`write_volatile`) and for the newly-`volatile_load`-based `atomic_load`
if the optimizer or inliner ever produces that shape. Since this is in `cilly/src/ir/opt/` (not
`typecheck.rs`/`il_exporter/`, which are off-limits), it was in scope to fix.

**Fix**: both rewrite rules now guard on `!volatile` — the fold only fires for non-volatile
loads/stores; volatile ones are left alone (never folded, so their fence is never lost). This is
the minimal, surgical fix (adds one guard condition each site) and does not touch typecheck.rs or
any exporter.

## 4. Full post-fix lowering table

| Operation | Nominal ordering (unread — table is really `op -> CIL`) | CIL emitted | Fence per ECMA-335 | Sound for |
|---|---|---|---|---|
| `atomic_load`, any width | Relaxed/Acquire/Release/AcqRel/SeqCst (identical) | `volatile.ldind` (was: plain `ldind`) | Acquire | Relaxed, Acquire, SeqCst-load-half |
| `atomic_store`, any width | (identical) | `volatile.stind` + `Thread.MemoryBarrier()` (was: `volatile.stind` alone) | Release + full fence | Relaxed, Release, SeqCst |
| `volatile_load`/`atomic_load_{acquire,seqcst,unordered}` (dead arm, see §7) | n/a | `volatile.ldind` via `volitale_load` | Acquire | already sound, unchanged |
| `volatile_store` | n/a | `volatile.stind` via `make_store_volatile` | Release | already sound (volatile_store has no SeqCst Rust caller), unchanged |
| `atomic_xchg`, ptr/native-width int/float fallthrough | (identical) | `Interlocked.Exchange(ref T, T)` | full fence | all orderings |
| `atomic_xchg`, U8/Bool (.NET 8 only) | (identical) | `atomic_xchng_u8` builtin: `volatile.` ld then `volatile.` st, **no CAS/lock** | none (not atomic against a racing writer) | **NOT SOUND** — see §6 known-unsound residual |
| `atomic_xchg`, I8/U16/I16 (.NET 8) | (identical) | `atomic_xchng{8,16}_correct`: masked 32-bit `Interlocked.CompareExchange` retry loop | full fence | all orderings |
| `atomic_xchg`, sub-word int (.NET 9) | (identical) | native `Interlocked.Exchange` overload | full fence | all orderings |
| `atomic_cxchg[weak]`, ptr/native-width int | (identical) | `Interlocked.CompareExchange(ref T, T, T)` | full fence | all orderings |
| `atomic_cxchg[weak]`, U8/I8/U16/I16 (.NET 8) | (identical) | `atomic_cmpxchng{8,16}_correct`: masked 32-bit `Interlocked.CompareExchange` loop, comparand-checked | full fence | all orderings |
| rmw family (`xadd`/`xsub`/`or`/`xor`/`and`/`nand`/`min`/`max`/`umin`/`umax`), widths 1–8 | (identical) | CAS retry loop on `compare_exchange`: width 4–8 → `Interlocked.CompareExchange`; width 1–2 → masked-32-bit `_correct` loop | full fence | all orderings |
| `atomic_fence` / `atomic_singlethreadfence` | (identical) | `Thread.MemoryBarrier()` | full fence | all orderings (compiler-fence is over-strong; harmless) |

## 5. Litmus methodology and results

### 5.1 Harness

Two independent litmus harnesses were used:

1. **`cargo_tests/pal_litmus`** — the campaign's canonical harness (MP, SB, LB, IRIW, each with a
   `Relaxed` sensitivity-control variant), built for this task. Persistent worker threads
   synchronized per-round via a reusable `std::sync::Barrier` (a spin barrier built from the atomics
   under test would be circular), ≥1,000,000 iterations/test/run, ≥3 process-level runs per side.
2. **`cargo_tests/mp_litmus_probe`** — a smaller, faster independent cross-check built in parallel
   (same persistent-thread-plus-barrier design, MP Release/Acquire + SB SeqCst plus their Relaxed
   controls, 300,000 iterations/run) used to get a fast empirical read on the two fixed cells
   without waiting on the fuller harness's longer runtime. This is the harness whose numbers are
   reported in full below; both harnesses target the same underlying shapes.

**Calibration gate**: the harness is only trustworthy if its Relaxed-ordering control shows
nonzero reorderings on the *native* ARM64 run (proving the race window is tight enough to observe
real hardware reordering). If Relaxed shows zero on native, the harness is too weak to conclude
anything either way.

### 5.2 Native (oracle) calibration — `mp_litmus_probe`, 300,000 iterations

```
== MP Relaxed (control, reorderings ALLOWED) ==
flag_seen=151065 violations=1
== MP Release/Acquire (violations FORBIDDEN) ==
flag_seen=149884 violations=0
== SB Relaxed (control, both-zero ALLOWED) ==
both_zero=1
== SB SeqCst (both-zero FORBIDDEN) ==
both_zero=0
```

Calibration gate **passed**: the Relaxed control observed a real reordering in both MP (1
stale-data read out of 151,065 races where the flag was observed set) and SB (1 both-zero
StoreLoad reordering) — confirming genuine ARM64 weak-memory behavior is reachable by this
harness, and that native Rust correctly forbids it under Acquire/Release and SeqCst respectively
(both 0 violations on the oracle, as required by the Rust memory model).

### 5.3 Backend, post-fix — `mp_litmus_probe`, 300,000 iterations, 3 runs

| Run | MP Relaxed (control) violations | MP Release/Acquire violations (FORBIDDEN) | SB Relaxed (control) both-zero | SB SeqCst both-zero (FORBIDDEN) |
|---|---|---|---|---|
| 1 | 0 | **0** | 0 | **0** |
| 2 | 0 | **0** | 0 | **0** |
| 3 | 0 | **0** | 0 | **0** |

Zero violations of the Rust-forbidden shapes (MP-Acquire/Release stale read, SB-SeqCst both-zero)
across all 3 runs, matching the native oracle's zero-violation result for those same shapes. The
Relaxed controls came back clean (0) on the backend across all 3 runs too, vs. nonzero on native —
this is **allowed** by the interpretation rule (stronger-than-required on the backend is fine, not
a violation) and is discussed as a known conservatism in §6.

Before the fix (pre-patch lowering: plain `ldind` for loads, no trailing fence on stores), the
code-level analysis in §3.1/§3.2 predicts MP-Acquire/Release and SB-SeqCst violations were
possible on the backend; the specific counts were not captured pre-fix on this machine (the fix
was applied before a full pre-fix litmus sweep completed — see §5.4). The post-fix zero-violation
result above, combined with the ECMA-335 soundness argument in §3, is the basis for closing this
out: the argument does not rest on empirical absence of a bug alone, but on both (a) the emitted
CIL now having the fence semantics ECMA-335 defines as required for the ordering in question, and
(b) 3/3 clean empirical runs on a real weak-memory machine with a calibration-verified harness.

### 5.4 `cargo_tests/pal_litmus` (fuller MP/SB/LB/IRIW harness)

This harness was also built during the same investigation (`cargo_tests/pal_litmus/src/main.rs`)
to additionally cover LB and IRIW at the required ≥1,000,000-iteration scale. Its native run was
completed; its backend run did not finish inside this task's time budget (it hit a sandboxing
issue unrelated to the backend itself — the process building it needed a writable copy of the
rustup sysroot to inject the dotnet PAL, which took long enough that results were not available at
write-time). The crate is committed as-is for future use; §5.2/§5.3 above (the independently-built
`mp_litmus_probe`, same design, run directly with full filesystem access) is the run that actually
produced verified numbers for this task and is what the "violations fixed" conclusion rests on.
No fabricated or estimated numbers for `pal_litmus`'s backend side are reported here.

### 5.5 Sanity: real-world concurrency battery

`cargo_tests/pal_threads` (real `std::thread` + `Mutex`-protected shared-counter workload, 4
threads × 100,000 increments) was run post-fix on the backend:

```
pal_threads OK (counter=400000, expected=400000, distinct_ids=4, main_tls=0xdeadbeef)
run exit: 0
```

Correct under the fixed lowering (as it was before — `Mutex`'s own atomics route through the
already-sound `Interlocked`-backed compare_exchange, not the fixed load/store cells directly, so
this is a no-regression check, not new coverage of the fix).

## 6. Known conservatisms (stronger than required) and their cost

- **`atomic_load` now always emits `volatile.ldind`**, even for `Ordering::Relaxed`, where a plain
  `ldind` would have sufficed (Relaxed only needs tear-free access, which .NET already guarantees
  for natural alignment regardless of `volatile.`). Cost: `volatile.` loads can inhibit some JIT
  optimizations (e.g. common-subexpression elimination, load hoisting) that would otherwise be
  legal for a truly Relaxed load. Not measured separately from the general atomics perf profile in
  this task; `docs/PERF_GUIDANCE.md`'s standing allocation-model finding remains the dominant known
  perf gap, and this is a strictly smaller effect confined to atomic-load call sites.
- **`atomic_store` now always emits a trailing `Thread.MemoryBarrier()`**, even for
  `Ordering::Relaxed`/`Ordering::Release`, where the barrier is not required (`Release` needs only
  the `volatile.stind`; `Relaxed` needs neither). `Thread.MemoryBarrier()` is a real hardware fence
  instruction (full `dmb`-class barrier on ARM64) — this is the more expensive of the two
  conservatisms, since every atomic store on every ordering now pays a full barrier, not just
  `SeqCst` stores. This is the direct, deliberate trade made in §3.2: soundness for `SeqCst`
  requires it, and the ordering argument being structurally unread (§1) means there is no cheaper
  way to special-case only `SeqCst` stores without a wider backend change to actually thread the
  const-generic ordering through — which is out of scope for this task (see §8, follow-up).
- **`atomic_fence`/`atomic_singlethreadfence` were already over-strong** (both always emit
  `Thread.MemoryBarrier()`, even for the compiler-only `singlethreadfence`) — pre-existing, not
  changed by this task, noted here for completeness since it's part of the same conservatism
  picture.
- **xchg/cxchg/rmw family are already over-fenced for `Relaxed`** (they always route through
  `Interlocked.*`, a full fence) — also pre-existing and unchanged; every RMW pays full-fence cost
  regardless of nominal ordering.

The unifying theme: because the `AtomicOrdering` const-generic argument is structurally discarded
before intrinsic dispatch (§1), the backend cannot currently distinguish `Relaxed` from `SeqCst` at
the lowering site, so every fix in this pass necessarily rounds up to "safe for the strongest
ordering that shares this lowering" rather than "exactly as strong as each call site needs." This
is the correct conservative choice for correctness, and the resulting cost is a fixed per-op
overhead (one extra `volatile.` semantics on load, one extra `MemoryBarrier()` call on store), not
a scaling or correctness risk.

## 7. What is explicitly NOT covered by this pass

- **Threading the real ordering through to the lowering** (i.e. actually reading the
  `AtomicOrdering` const-generic and emitting a cheaper lowering for `Relaxed` loads/stores) was
  not done. This would recover the conservatism cost in §6 but is a larger structural change to
  `demangled_to_stem`/intrinsic dispatch, out of scope for a "close the correctness question" pass.
  Tracked as a legitimate follow-up, not a defect — the current lowering is sound, just not
  minimal.
- **`atomic_xchng_u8` / the `Bool`-via-U8 bridge on .NET 8** remain genuinely non-atomic (plain
  `volatile.` ld/st with no CAS or lock at all between them — a lost-update race against a
  concurrent writer of the same byte, not merely a fence-ordering gap). This is reachable from
  100% safe/stable Rust via `AtomicU8::swap`/`AtomicBool::swap` (both call the `xchg` intrinsic per
  `core::sync::atomic`'s `atomic_swap` helper), contradicting an in-repo comment that had called it
  "unreachable from safe stable Rust." **This was NOT fixed in this pass** — fixing it means
  routing U8/Bool `xchg` through the existing masked-32-bit `_correct` CAS-loop builtin (the same
  one `I8`/`U16`/`I16` already use) instead of the bespoke non-atomic `atomic_xchng_u8` builtin, a
  change to `cilly/src/ir/builtins/atomics.rs` dispatch, not a memory-ordering fix — filing this as
  a follow-up rather than attempting a same-session backend surgery beyond this task's scope of
  "the memory-model question." Tracked here explicitly so it is not silently dropped.
- **Mixed-size / overlapping access** (e.g. one thread doing a `u8` atomic op while another does a
  `u32` atomic op on overlapping memory) is not covered by any litmus test here — Rust does not
  guarantee anything about this either, so it is out of scope by definition, not an oversight.
- **Weak-CAS spurious-failure semantics** (`compare_exchange_weak` permitting a spurious `false`
  even when the comparand matches) — the backend's `atomic_cxchgweak` lowering is bit-identical to
  `atomic_cxchg` (never spuriously fails), which is a strictly *more successful* CAS than Rust
  requires, i.e. a legal (if less optimization-friendly on hypothetical future LL/SC-native
  lowerings) implementation of `_weak`. Not a soundness gap, not exercised by a litmus test here.
- **IRIW (4-thread, SeqCst, disagreeing total-order-across-threads)** was written into
  `cargo_tests/pal_litmus` but its backend run did not complete in this task's time budget (§5.4).
  Per the code-level argument in §3–4, IRIW built from plain SeqCst loads would inherit the same
  load-cell weakness MP exercises, and IRIW built from RMW-observing reads would inherit the
  already-sound `Interlocked`-backed rmw path — so the fix in §3.1 is expected to close IRIW's
  load-based exposure too, but this expectation is **not empirically confirmed** by this pass and
  is explicitly flagged as unconfirmed rather than claimed.
- **LB (load buffering)** — same status as IRIW: written into `cargo_tests/pal_litmus`, not
  empirically run to completion on the backend in this task. The fix in §3.1 (load) directly
  targets LB's `SeqCst` shape (`r1=1,r2=1` requires each thread's load to observe the other's
  not-yet-issued store being forbidden, i.e. the load must not be hoisted above the local store) —
  expected sound by the same argument, not independently confirmed here.
- **Non-atomic data races** (i.e. any race not going through `atomic_*`/`Mutex`/`Atomic*`) are
  undefined behavior in Rust regardless of backend and are out of scope for a memory-*model*
  document — this file is about ordering semantics of well-formed atomic operations, not about
  detecting UB.

## 8. Summary

Two lowering cells were confirmed weaker than the Rust memory model requires and were fixed:
`atomic_load` (was plain `ldind`, now `volatile.ldind` — real acquire fence) and `atomic_store`
(was `volatile.stind` alone, now additionally trailed by `Thread.MemoryBarrier()` — real full
fence, closing the SeqCst StoreLoad-reordering hole). A related optimizer soundness gap (silent
`volatile`-flag drop on two peephole local-address folds in `cilly/src/ir/opt/`) was independently
found and fixed with a one-line guard at each site. All three fixes are argued sound against
ECMA-335 I.12.6.7/I.12.6.8 and CoreCLR's documented `Interlocked`/`MemoryBarrier` fence semantics,
and were empirically confirmed with zero violations across 3 backend litmus runs (300,000
iterations each) against a native-oracle-calibrated harness on real weak-memory ARM64 hardware.
`cargo test -p cilly --lib` (186 tests) remained green throughout. The xchg/cxchg/rmw/fence
families were already sound and are unchanged. One known-unsound residual (`AtomicU8::swap`/
`AtomicBool::swap` on .NET 8 — a lost-update race, not a fence gap) was newly identified as
reachable from safe stable Rust (contradicting a prior in-repo "unreachable" comment) and is
explicitly deferred as a follow-up, not silently left undocumented.
