# Differential project validation

Passing the unit test suite does **not** mean a program is compiled correctly —
FractalFir's own warning. A unit test only proves the *one* path it exercises; a
real project hits far more of the codegen, and that's where latent miscompilations
surface. This harness validates a whole project end-to-end.

## The idea: differential testing

Build and run the project **twice with identical inputs** — once with the normal
(native) toolchain, once through `rustc_codegen_clr` on .NET — and **diff the output**.
The native run is ground truth, so any difference is a miscompilation. No hand-written
"expected output" is needed, which is what makes it generalize across project types.

```
feasibility/validate/run.sh                 # validate every project
feasibility/validate/run.sh kitchen_sink    # validate one
# on Apple Silicon, force the on-path arch for a faithful run:
PLATFORM=linux/amd64 feasibility/validate/run.sh kitchen_sink
```

A run prints `✅ PASS` (outputs identical) or `❌ FAIL` with the diff (`- native`,
`+ .NET`) — the diff *is* the miscompilation report.

## Adding a project

Copy [`projects/_template/`](projects/_template/) to `projects/<name>/`, edit
`project.env`, and put the crate in `projects/<name>/crate/` (or point `SRC` at a git
URL). The descriptor says how to build (`BIN`, `BUILD_STD`), run (`RUN_ARGS`,
`STDIN_FILE`), and compare (`PROFILE`, `NORMALIZE`). See the template for the fields
and how to pick a good candidate (deterministic, no proc-macros, computes-and-prints).

[`projects/kitchen_sink/`](projects/kitchen_sink/) is a worked example: a self-contained,
dependency-free program that exercises a broad swath of std (int/float arithmetic, bit
ops, iterators, closures, generics, BTree collections, strings/formatting, enums,
Option/Result, recursion, slices) — a good smoke test for the codegen as a whole.

## "Full testing" means different things — the `PROFILE` knob

Different projects need different notions of "works":

| profile | how it validates | good for |
|---------|------------------|----------|
| `diff` *(implemented)* | run native vs .NET, diff stdout + exit code | CLIs, computations, anything deterministic |
| `expected` *(future)* | diff .NET output against a checked-in `expected.txt` | when native isn't available/relevant |
| `testsuite` *(future)* | build the crate's own `#[test]`s through the backend, run them on .NET, compare pass/fail vs native | libraries |
| `examples` *(future)* | build+run each `examples/*.rs` both ways, diff | example-driven crates |

Only `diff` is wired today; the others are the natural extensions (the runner is
structured so adding a profile is a new branch in `validate.sh`).

## What FractalFir already validated (and the gap this fills)

The crates under [`cargo_tests/`](../../cargo_tests/) — **glam, rapier, fastrand,
criterion, guessing_game**, plus hello-world variants and building core/alloc/std —
are the projects he set up. But the existing `cargo_test!` harness only checks that
they **compile** (`"Finished"` in cargo's output); it never runs the result or checks
behaviour. And several of the mains are stubbed (e.g. `glam_test` has its glam calls
commented out). So there was no end-to-end *behavioural* validation — which is exactly
the miscompilation blind spot. This harness adds the run-and-diff half.

## What the first run surfaced (the gateway blocker)

Running `kitchen_sink` (a normal std program) immediately exposed that **`build-std` of
`alloc` currently fails through the backend** on nightly-2026-06-17 — the backend's own
typechecker rejects its output for std internals:
- `FieldOwnerMismatch` building `Rc`/`Arc`/`Cell` (`RcInner`, `ArcInner`, `Atomic<usize>`),
- `CallArgTypeWrong` in `String::push_str_slice` (a fat-pointer-of-fat-pointer mismatch:
  got `FatPtru8`, expected `FatPtr<FatPtru8>`).

This is **not** specific to `kitchen_sink` or to overflow-checks — it's why the
`hello_world` / `std_hello_world` / `fastrand` *cargo* tests fail too. Note that the ~207
passing unit tests are mostly `#![no_std]` programs that hand-reimplement std pieces (e.g.
`test/alloc/abox.rs` reimplements `Box`/`Layout`/`Alloc`) and print via the .NET interop —
they do **not** compile real `alloc`/`std`. So this blocker is invisible to the unit suite
but blocks essentially every real-world project. **Fixing it is the prerequisite for using
this on real code** (and is the natural next target — the harness here is how you'd drive it:
fix a bug, re-run, watch `alloc` get further).

## Limitations / notes

- **Determinism is required** for `diff`. RNG (seed it), time, threads, and HashMap
  iteration order will produce spurious diffs — seed/avoid them or strip with `NORMALIZE`.
- **Panic strategy**: the .NET pass uses `panic_abort` build-std by default; programs that
  rely on unwinding for normal control flow need `BUILD_STD=...panic_unwind` and care.
- **proc-macros are unsupported** by the backend, so crates that need them won't build.
- Run on `linux/amd64` for a faithful result — aarch64 is viable for the build but is
  still off the project's primary tested path.
