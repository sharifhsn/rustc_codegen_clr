# .NET 8 vs .NET 10 performance snapshot

Measured on the same Apple-silicon MacBook on 2026-07-12 with
`feasibility/perf/run.sh`, the native backend, release builds, and matching C# target frameworks.
Each workload is self-timed after warmup and records the best repeated sample. Commands:

```bash
DOTNET_VERSION=8 PERF_OUT=feasibility/perf/results/net8 feasibility/perf/run.sh
DOTNET_VERSION=10 PERF_OUT=feasibility/perf/results/net10 feasibility/perf/run.sh
```

| workload | .NET 8 backend (ms) | .NET 10 backend (ms) | 10 speedup |
|---|---:|---:|---:|
| int_arith | 161.6 | 69.2 | 2.34x |
| float_arith | 120.0 | 90.4 | 1.33x |
| iter_sum | 158.7 | 80.5 | 1.97x |
| iter_indexed | 141.6 | 78.6 | 1.80x |
| iter_zip | 182.8 | 87.3 | 2.09x |
| vec_churn | 418.0 | 159.8 | 2.62x |
| box_churn | 2319.9 | 1491.6 | 1.56x |
| hashmap | 1119.5 | 743.8 | 1.51x |
| string_build | 47.4 | 26.6 | 1.78x |
| slice_fill | 144.0 | 85.2 | 1.69x |
| sort_ints | 173.5 | 118.3 | 1.47x |
| fib_rec | 44.8 | 36.2 | 1.24x |

All 12 backend workloads were faster in this snapshot. The geometric shape is plausible for a newer
JIT/runtime, but this is workstation evidence rather than a controlled lab result: the .NET 8 run
preceded .NET 10, and thermal state and background load were not randomized. Treat the figures as
directional, not as a promise that .NET 10 makes every application 1.2-2.6x faster.

The matched C# controls also became substantially faster between the two runs. Relative to C#, the
backend remains close for scalar/iterator kernels (roughly 1.1-1.5x in the .NET 10 run) and much
slower for allocation- and hashing-heavy kernels. That says the version-to-version gain is mostly a
runtime effect; it does not erase the backend's known allocation model and Rust `HashMap` costs.
