# Performance harness

Tooling to **measure, compare, and profile** `rustc_codegen_clr` performance so bottlenecks can be
investigated systematically. Two complementary tools:

## 1. `run.sh` — 3-way microbenchmark + allocation profiling

Builds and runs the same workloads three ways and prints one comparison table:

- **native Rust** (`cargo build --release`, host target) — the upper bound.
- **Rust via `rustc_codegen_clr`** (`cargo dotnet`, `x86_64-unknown-dotnet`) — what we measure.
- **C#** (idiomatic-fast .NET) — the peer ceiling on the same runtime.

```bash
feasibility/perf/run.sh            # the table
feasibility/perf/run.sh --knobs    # + OPTIMIZE_CIL=0 / NO_UNWIND=1 deltas on the backend
```

Each workload is **self-timed** (warmup + best-of-K around a fixed amount of work) and prints
`RESULT <name> <ns> <m1> <m2>`. The Rust side carries a **counting global allocator** so the
`rs-allocs`/bytes columns are identical native vs backend — they isolate *how much* we allocate
(codegen/algorithm) from *what each allocation costs* on .NET (the memory-model axis). The C# side
reports `GC.GetTotalAllocatedBytes` + gen-0 collections.

Columns: `native(ms) | backend(ms) | C#(ms) | bk/nat | bk/C# | rs-allocs | cs-gen0`.

**Add a workload:** add an `#[inline(never)] fn` + one `bench!(...)` line in `rust/src/main.rs`, and
the mirror in `csharp/Program.cs`. Keep the logic byte-identical so the toolchain is the only
variable.

Workloads cover the bottleneck dimensions: `int_arith`, `float_arith`, `iter_sum` vs `iter_indexed`
(zero-cost-iterator check), `iter_zip`, `vec_churn`, `box_churn`, `hashmap`, `string_build`,
`slice_fill` (memset hotspot), `sort_ints`, `fib_rec` (call overhead).

## 2. `rank_corpus.sh` — bottleneck map from the existing `#[bench]` corpus

No new runs. Joins `latest_benchmarks.txt` (backend) against `native_benchmark.txt` (native) — the
core/alloc library `#[bench]` suites already run both ways — and ranks by slowdown ratio, plus a
per-category aggregate.

```bash
feasibility/perf/rank_corpus.sh [TOP_N]
```

This is the fastest way to see *which kinds of code* are slow today (the iterator-adapter cluster,
`slice::fill`, string patterns, …) before drilling in with `run.sh` + a focused workload.

## Profiling deeper

- **CIL inspection** for a single workload: shrink `rust/src/main.rs` to one `bench!`, build via
  `cargo dotnet`, and read the emitted `.il` (or set `OPTIMIZE_CIL=0` to see the 1:1 MIR→CIL map).
- **GC / allocation**: the `rs-allocs` column + the C# `cs-gen0` column. For managed-heap detail,
  run the C# build under `dotnet-trace`/`dotnet-counters`.
- **Knob deltas**: `--knobs` quantifies the CIL optimizer and `NO_UNWIND` contributions per workload.
