# Performance guidance for Rust on .NET

Most Rust code runs correctly and acceptably fast through this backend, but a few ecosystem defaults
are tuned for native targets and leave throughput on the table on .NET. This is the short list of
high-leverage, *source-level* choices a performance-sensitive program can make today — separate from
the backend's own codegen work (tracked in [feasibility/perf/BASELINE.md](../feasibility/perf/BASELINE.md)).

All numbers below are measured on this backend (arm64 macOS, .NET 8, native-backend harness).

## 1. Hashing — pick a fast hasher (≈2.35× on hash-heavy code)

Rust's default `HashMap` uses **SipHash-1-3**, a DoS-resistant keyed hash that is deliberately ~3–5×
slower than a non-cryptographic hash. .NET's `Dictionary` uses a fast non-crypto hash, so the gap on a
naive `HashMap` benchmark looks enormous (≈60× vs C#) — but **most of that is the hasher, not codegen**.

Measured (500k insert + lookup, `u64→u64`): `HashMap<_,_,SipHash>` **264.7 ms** vs the identical map
with a hand-rolled **FxHash** `BuildHasher` **112.8 ms** — **2.35× faster**, byte-identical results,
*zero interop*. Use [`rustc-hash`](https://crates.io/crates/rustc-hash) (`FxHashMap`) or
[`ahash`](https://crates.io/crates/ahash) when keys aren't attacker-controlled:

```rust
use rustc_hash::FxHashMap;
let mut m: FxHashMap<u64, u64> = FxHashMap::default();
```

This is the single biggest transparent win for collection-heavy code and needs no `.NET` knowledge.

## 2. The .NET BCL is reachable from Rust (`mycorrhiza`)

When the data naturally lives on the managed side, or you want .NET's gen-0-allocated collections
directly, the interop layer ([mycorrhiza](../mycorrhiza)) exposes BCL types via
`RustcCLRInteropManagedClass<ASSEMBLY, CLASS_PATH>` (see [docs/INTEROP_CSHARP.md](INTEROP_CSHARP.md)).
`System.Text.StringBuilder`, `Console`, `Math`, etc. are bound; generic containers like
`System.Collections.Generic.Dictionary<K,V>` are reachable but need marshalling bindings written per
instantiation. Prefer option 1 for pure-Rust hot paths; reach for the BCL when crossing the boundary is
already in the design (e.g. handing a collection to/from C#). Either way the managed allocator (gen-0
bump) is faster than `malloc` — see below.

## 3. Allocation — what's intrinsic vs what we're fixing

Decomposed with a C# microbenchmark (5M alloc+free):

| path | ns/op |
|------|------:|
| managed `new byte[8]` (gen-0 bump) | 3.0 |
| `NativeMemory.Alloc`/`Free` (what `Box`/`Vec` use) | 14–17 |
| this backend's `box_churn` today | ~39 |

* The **~3 ns gen-0 vs ~14 ns malloc** gap is intrinsic to a manual-memory language on .NET: gen-0 is a
  bump-allocator reclaimed by a *moving* collector, and we cannot relocate Rust's untracked raw
  pointers. Using managed collections (option 2) is the way to ride gen-0 where it fits.
* The **~22 ns above the malloc floor is ours and is being fixed** — it's the un-inlined
  `transmute`/`box_new_uninit`/`drop_glue` wrapper call chain around each allocation, not the allocator
  itself. See BASELINE.md.

## 4. Avoid where the backend is weakest (today)

* **Tight scalar/float loops** run ~1.9–3× a hand-written C# equivalent (RyuJIT codegen on our IL) and
  ~3.6× native (LLVM autovectorizes; RyuJIT doesn't). If a kernel is hot and vectorizable, calling a
  `System.Numerics`/`System.Runtime.Intrinsics` routine via the interop will beat scalar Rust today.
* **Deep iterator-adapter chains** (esp. `zip`) still leave per-element `Option`/tuple machinery; a
  manual indexed loop is faster on the backend until the nested-aggregate codegen lands.

These are backend-codegen targets, not advice to write un-idiomatic Rust — they're listed so a hot path
can be restructured if the profiler points here.

## 5. NativeAOT — the big lever for compute-bound code (~2.1× scalar, iterators fully collapse)

The JIT (RyuJIT) leaves real performance on the table: it won't autovectorize, and it won't inline
struct-returning helpers (the transparent-newtype reinterprets that thread through iterator/pointer
code). **`ILC`, the NativeAOT compiler, does both** — and it accepts this backend's IL unmodified.

Measured (controlled experiment, .NET 8, arm64): a pure-compute Rust `cdylib` built with this backend,
referenced from a `PublishAot` C# host and AOT-compiled by ILC:

| 20M-iteration kernel | JIT (RyuJIT) | NativeAOT (ILC) |
|----------------------|-------------:|----------------:|
| `int_arith` (scalar) | 14.3 ms | **6.5 ms (~2.1×)** |
| `iter_sum` (iterator)| —        | **4.9 ms** — *below* the manual scalar loop |

`iter_sum` under AOT runs *faster than the hand-written integer loop* because ILC inlined the whole
adapter chain — the `Option`, the transmute reinterprets, the closures — exactly the residue RyuJIT
leaves behind. So NativeAOT is the single highest-leverage option for compute-heavy Rust on .NET, and
it closes the scalar "JIT ceiling" that the JIT path cannot.

### Whole-program AOT works through the C#-consumes-Rust interop path

A `PublishAot` C# host that references a Rust crate (directly or via `msbuild/RustDotnet.targets`)
publishes to a **standalone native binary** (no .NET runtime) with the Rust compiled in. Recipe:

```bash
cargo dotnet build mylib --release          # Rust cdylib -> mylib.dll (managed assembly)
# host.csproj: <PublishAot>true</PublishAot> <RuntimeIdentifier>osx-arm64</RuntimeIdentifier>
#              <Reference Include="mylib"><HintPath>.../mylib.dll</HintPath></Reference>
dotnet publish -c Release                   # ILC compiles the Rust IL + C# host -> native Mach-O/ELF
```

Validated end-to-end on the full `cd_interop` marshalling sample (arm64 native binary): **primitives,
de-mangled struct value-types, struct methods, inbound slices, collections (`Vec`/`RawVec` growth),
`String`, heap allocation, `format!` (including interpolated args), and `str::parse` all work** — the
full `cd_interop` marshalling sample is 6/6 under AOT. There are no known whole-program-AOT correctness
gaps.

> Caveat: the lib build needs a *current* installed toolchain — re-run `cargo dotnet setup` if
> `~/.cargo-dotnet` predates a backend change (a stale install builds with the old linker/dylib; the
> il_exporter + optimizer run in BOTH the backend dylib and the linker, so refresh both).

### Const data under AOT (the bug that hid here)

What first looked like a `core::fmt` fn-pointer bug was actually a **FieldRVA sizing** bug (fixed): the
backend emitted every const-data blob (string literals, the integer-formatting `DEC_DIGITS_LUT`, const
`&[T]`) as a FieldRVA static typed `uint8` regardless of the blob length. The JIT loads the whole
contiguous `.data` section, so reading N bytes from `&c_X` worked and masked it — but NativeAOT/ILC
preserves only `sizeof(field-type)` = 1 byte of FieldRVA data and zeros the rest. So under AOT every
const blob was "first byte correct, then zeros": broken string literals, integer formatting (LUT →
garbage digits), `parse`, and all `format!`. Fixed by sizing each FieldRVA field to its blob (the
Roslyn `__StaticArrayInitTypeSize` idiom). Lesson for AOT debugging: FieldRVA field types must match
their data size — the JIT is forgiving, ILC is not.

Tradeoffs: AOT changes the deployment model (self-contained native binary, faster startup, no JIT
warm-up, larger artifact, no runtime codegen). With the const-data gap closed, a first-class
`cargo dotnet publish --aot` is now a tooling task (wrap the `PublishAot` host generation); full I/O-PAL
AOT-compatibility for a standalone Rust *binary* (vs the C#-host-consumes-Rust-lib path proven here) is
the remaining frontier.
