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
