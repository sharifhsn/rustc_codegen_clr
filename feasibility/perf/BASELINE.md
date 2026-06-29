# Performance baseline & bottleneck diagnosis

First run of the harness (arm64 macOS, .NET 8 CoreCLR, native backend). Regenerate with
`run.sh` / `rank_corpus.sh`. Numbers are machine-relative; the **ratios** are what matter.

## 3-way microbenchmark (`run.sh`)

```
workload          native   backend        C#    bk/nat    bk/C#    rs-allocs  cs-gen0
--------            (ms)      (ms)      (ms)         x        x      (count)  (count)
int_arith           30.5     120.3      61.8      3.9x     1.9x            0        0
float_arith         57.1     226.0      67.9      4.0x     3.3x            0        0
iter_sum            30.6    1763.9      55.8     57.7x    31.6x            0        0   <-- iterator
iter_indexed        30.4     114.4      59.3      3.8x     1.9x            0        0   <-- same math, manual loop
iter_zip            35.6    2765.0      55.9     77.8x    49.5x            0        0   <-- iterator
vec_churn           34.1    1217.8      72.5     35.7x    16.8x       200000       99
box_churn           32.2    1818.7      70.0     56.5x    26.0x     20000000       58
hashmap             80.6    1459.1      20.0     18.1x    73.0x           21        0
string_build         7.9     120.1       9.3     15.3x    13.0x           22        3
slice_fill          58.9      84.2      62.7      1.4x     1.3x            1        0
sort_ints           14.5     346.0      82.3     23.8x     4.2x            1        0
fib_rec             12.7      35.2      21.9      2.8x     1.6x            0        0
```

## Diagnosis (attack order)

1. **Iterators are NOT zero-cost on the backend — #1 priority.** `iter_sum` is **15.4× slower than
   `iter_indexed`** (the identical math as a manual `while` loop) *on the backend* — yet **identical
   natively** (30.6 vs 30.4 ms: LLVM makes the abstraction free). `iter_zip` is even worse. The
   `map`/`fold`/`zip`/`Range` adapter chains are not collapsing to tight loops; they leave per-element
   closure/Option/Iterator machinery the CIL optimizer + RyuJIT don't eliminate. This is the highest-
   leverage target and is corroborated by the corpus ranker: the `iter` category averages **31.6×**
   over **78 benches** (`step_by`, `zip`, `next_chunk`, `flat_map`, …).

2. **Allocation cost — the memory-model axis.** `box_churn` 26× / `vec_churn` 17× vs C#. The
   `rs-allocs` column shows the volume is identical native-vs-backend (same Rust code); the cost is
   per-allocation: the libc-shim `malloc`/`free` path is far slower than .NET's gen-0 bump allocator
   (which C# rides). Candidates: a faster shim allocator, or mapping `Box`/`Vec` backing onto the
   managed heap where it's sound.

3. **Raw scalar codegen ~2× vs C# — acceptable / RyuJIT-bound.** `int_arith`/`iter_indexed`/`fib`
   land at ~1.9× C# and ~3–4× native (native gets LLVM autovectorization). This matches the prior
   "numeric 1.8×" finding; it's the runtime JIT ceiling, not low-hanging fruit.

4. **Not actually bottlenecks:** `slice_fill` (1.3×) is fine — the corpus's 272× was a measurement
   artifact (native constant-folded it). `hashmap` 73× vs C# is mostly **SipHash** (Rust's
   DoS-resistant default hasher) vs .NET's fast hash — algorithmic, not codegen.

## Method notes
- Inputs are `black_box`'d so native LLVM can't constant-fold whole pure-compute loops (the trap
  that made an earlier run show native = 0 ms / bogus ratios).
- `bk/nat` overstates the "real" gap for scalar code because native = LLVM -O3 (autovectorized);
  `bk/C#` is the fairer peer comparison (both on RyuJIT).
- Concurrency caveat: don't run two `cargo dotnet` builds at once (shared target dir / config).

---

## Run 2 — MIR-level inlining (the zero-cost-abstraction fix)

The #1 bottleneck above (iterators not collapsing) is fixed at the **MIR layer**, not the CIL layer.
rustc's own MIR inliner already runs at `mir-opt-level>=2` (release) but is tuned conservatively
because the native pipeline lets LLVM finish inlining `#[inline]` adapter chains. Our backend hands
MIR to RyuJIT, which will **not** inline struct-returning adapter chains — so `(0..n).map(f).sum()`
survived as a per-element `Range::fold` CALL. Raising **only** the `#[inline]` budget,
`-Z inline-mir-hint-threshold=500` (in the cargo-dotnet harness RUSTFLAGS), makes rustc collapse the
whole chain into one flat loop *before* the backend sees it — correct by construction (typed MIR +
real borrow info), exactly the MIR LLVM gets for native.

| workload     | native | backend (Run 1) | **backend (Run 2)** | speedup | bk/nat | bk/C# |
|--------------|-------:|----------------:|--------------------:|--------:|-------:|------:|
| iter_sum     |  29.8  |          1763.9 |           **343.7** | **5.1×**|  11.5× |  6.1× |
| iter_zip     |  35.7  |          2765.0 |           **575.3** | **4.8×**|  16.1× | 10.4× |
| iter_indexed |  30.2  |           114.4 |               112.2 |   flat  |   3.7× |  2.0× |

`iter_indexed` (the identical math as a manual `while` loop, no `#[inline]` chain) is the control: it
stays flat, confirming the win is specifically from collapsing the iterator abstraction. The residual
`iter_sum` gap vs the manual loop (~3×) is the RyuJIT scalar ceiling — the collapsed loop still packs/
unpacks `Option<u64>` per iteration (a niche struct RyuJIT doesn't scalarize like LLVM does), the same
floor `iter_indexed`/`int_arith` hit (~3.7× native). MIR inlining can't close that; the JIT is the wall.

**This replaced a CIL-level single-block inliner** (an earlier attempt, commits af6ab62/de23c11/
4384107). That inliner re-derived at the untyped CIL level the type/borrow/aliasing safety MIR already
guarantees, which bred subtle miscompiles (a niche/alloc-path bug) and forced a type-only verify/revert
net. Moving inlining to the MIR layer made all of that disappear: the CIL inliner + the net were
deleted (−317 lines net), the optimizer is purely local/intra-method again, and correctness is rustc's
battle-tested responsibility. Validated by two native-vs-backend differential checksums (iterator +
alloc + enum-niche + Option/Result + dyn-trait + generic patterns, byte-identical) and the `::stable`
gate (426/14, no regressions).

Remaining perf axes are unchanged by this work: allocation cost (`box_churn`/`vec_churn`, the
gen-0-vs-malloc memory-model axis) and the RyuJIT scalar ceiling.

---

## Run 3 — SROA (scalar replacement of non-escaping aggregates) + checked-arith de-call

A second RyuJIT-friendliness pass, `cilly/src/ir/opt/scalarize.rs` (default ON; `SROA=0` disables).
Even after MIR inlining collapses an iterator chain, the body still builds a per-element `Option<T>`
and — for `.sum()`/`.product()` — an overflow-check `(T,bool)` tuple, both via `ldloca; stfld`. That
address-taken form makes RyuJIT mark the local **address-exposed** and refuse to enregister it. The
pass splits such a non-escaping aggregate local into per-field **scalar** locals (escape-checked +
field-overlap-guarded for soundness), after which copy-prop + dead-store-elim forward the live field
and delete the dead ones. A small companion step **de-calls** the `ovf_check_tuple` helper into field
stores first, so the same scalarizer dissolves the checked-arithmetic tuple — and since the overflow
`assert` is already elided in release, the whole overflow apparatus (the redundant add, the carry
compare, the tuple) falls out as dead code, leaving a plain wrapping add (exactly what native gets).

Isolation on a focused probe (`v_iter = (0..n).map.filter.sum`, normalized to an in-run manual-loop
control to cancel machine load): the dead **overflow check was ~40% of `v_iter`**; removing it took
`v_iter` from **2.0–2.55× → 1.30–1.46×** the manual loop.

Full harness, on top of Run 2 (MIR inlining):

| workload     | native | backend (Run 2) | **backend (Run 3)** | further | bk/nat | bk/C# |
|--------------|-------:|----------------:|--------------------:|--------:|-------:|------:|
| iter_sum     |  30.5  |           343.7 |           **155.8** | **2.2×**|  5.1×  |  2.7× |
| iter_indexed |  30.5  |           112.2 |               110.3 |   flat  |  3.6×  |  1.9× |
| sort_ints    |  14.7  |           263.2 |               224.9 |  better |  15.3× |  3.0× |
| vec_churn    |  35.1  |          1125.2 |              1016.3 |  better |  29.0× | 14.7× |

`iter_sum` is now **1.4× the hand-written manual loop** (down from 15× at the original baseline) and
**5.1× native** (down from 57.7×). Cumulative across Run 2 + Run 3: **1764 → 156 ms = 11.3×**. The win
generalizes — sort/vec/box all improved — because it removes address-exposed value-type locals across
all struct-heavy code, not just iterators. The remaining `iter_sum`-vs-manual gap and the
`iter_indexed`/`int_arith` ~3.6× vs native are the RyuJIT scalar/autovectorization ceiling.

Correctness: three native-vs-backend differential checksums byte-identical, including a probe that
exercises the **live** overflow path (`checked_mul`/`checked_add` → `Some`/`None`, which must keep the
flag) alongside the dead path (`sum`/`product`) and sub-word (`i8`) checked+saturating arithmetic;
plus the `::stable` gate (426/14, no regressions) under the fatal CIL type-checker.

---

## Run 4 — cumulative state (MIR-inline + SROA + nested-SROA + transmute-ldfld) + loose-ends audit

Full harness with everything landed:

| workload     | native | backend | C# | bk/nat | bk/C# | vs Run 1 backend |
|--------------|-------:|--------:|----:|-------:|------:|-----------------:|
| int_arith    | 30.8   | 111.0   | 60.7| 3.6×   | 1.8×  | — |
| iter_sum     | 30.6   | 156.0   | 56.2| 5.1×   | 2.8×  | 1764 → 156 (**11.3×**) |
| iter_zip     | 35.8   | **215.6** | 55.5| 6.0× | 3.9×  | 2765 → 216 (**12.8×**) |
| iter_indexed | 30.8   | 118.7   | 57.7| 3.9×   | 2.1×  | — |
| vec_churn    | 35.1   | 817.6   | 71.5| 23.3×  | 11.4× | 1125 → 818 |
| box_churn    | 32.1   | 1547.5  | 68.4| 48.3×  | 22.6× | 1701 → 1548 |
| hashmap      | 84.2   | 1258.6  | 22.1| 14.9×  | 57.0× | (mostly SipHash — FxHash 2.35×, §1) |
| string_build | 7.3    | 83.3    | 8.6 | 11.5×  | 9.7×  | (alloc-bound) |
| sort_ints    | 14.8   | 224.9   | 77.4| 15.2×  | 2.9×  | — |

`iter_zip` joined `iter_sum` in the "essentially solved" column (12.8× cumulative; the transmute-`ldfld`
+ nested-SROA collapse it). The remaining cluster is **allocation** (box/vec/string) and **hashmap**.

### box_churn / allocation — audited: AOT territory + intrinsic floor, not a clean JIT fix

box_churn's per-iteration residual (from its IL) is: an un-inlined `box_new_uninit` + `drop_glue` (Rust
std method calls), a 2-field `transmute(UInt128 → Layout)`, and a nested `transmute(*u8 → Box<u64>)` —
**none of which are the single-field extracts the `ldfld` rewrite handles** (`Layout` is 2-field, `Box`
is a nested wrapper), and `box_new_uninit`/`drop_glue` are methods RyuJIT won't inline. Measured under
**NativeAOT**: box_churn **38.8 → 21.6 ns/op (~1.8×)**, landing on the NativeMemory alloc+free floor
(~17 ns) — i.e. ILC inlines exactly that wrapper layer RyuJIT leaves. So the JIT-path residual is *not*
a clean codegen bug to fix; it's (a) the un-inlinable std/transmute wrappers → **NativeAOT** closes them,
and (b) the gen-0-vs-malloc floor → intrinsic (a custom managed allocator was explicitly out of scope).
The construct-direction transmute inline (field→struct) was considered but the hot cases here are
multi-field/nested, so it would not move box_churn; deferred as a general newtype-construct cleanup.

Net: the tractable JIT-codegen wins (iterators, the extract transmutes) are banked; the allocation
cluster's real lever is NativeAOT (proven) and its floor is intrinsic. hashmap is the hasher algorithm
(use FxHash/ahash, §1). The small scalar gaps (int/iter_indexed ~2× C#) are RyuJIT-on-our-IL quality —
also collapsed by NativeAOT (int_arith 14.3 → 6.5 ms, §5).
