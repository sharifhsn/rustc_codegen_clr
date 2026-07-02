# pal_litmus results

`pal_litmus` is the campaign's canonical weak-memory litmus harness: MP (message passing), SB
(store buffering), LB (load buffering), and IRIW (4-thread independent reads), each parameterized
by `std::sync::atomic::Ordering`, plus a `Relaxed` sensitivity-control variant of MP/SB/LB used to
confirm the harness can actually observe hardware reordering on this machine before trusting a
clean (zero-violation) result under stronger orderings.

Methodology: real `std::thread` parallelism, persistent worker threads (not spawned per iteration)
synchronized every round via a reusable `std::sync::Barrier`, ≥1,000,000 iterations per test per
run, ≥3 process-level runs per side, machine-parseable `RESULT ...` lines.

**Interpretation rule**: on the backend, outcomes Rust forbids under the tested ordering (MP
Release/Acquire stale-data read, SB SeqCst both-zero, LB SeqCst both-one, IRIW SeqCst disagreeing
orders) must be zero across all runs. `Relaxed`-control reorderings are allowed on the backend
(stronger-than-required is fine) and are reported, not flagged.

## Native (oracle) — 3 runs × 1,000,000 iterations, this machine (macOS ARM64 / Apple Silicon)

| Run | MP Relaxed viol (control) | MP Rel/Acq viol (forbidden) | SB Relaxed both-zero (control) | SB SeqCst both-zero (forbidden) | LB Relaxed both-one (control) | LB SeqCst both-one (forbidden) | IRIW SeqCst disagree (forbidden) |
|---|---|---|---|---|---|---|---|
| 1 | 0 / 498,767 observed | 0 / 499,683 observed | 55 | 0 | 0 | 0 | 0 / 249,564 observed |
| 2 | 0 / 500,144 observed | 0 / 499,117 observed | 40 | 0 | 0 | 0 | 0 / 250,365 observed |
| 3 | 1 / 500,383 observed | 0 / 497,818 observed | 43 | 0 | 0 | 0 | 0 / 251,752 observed |

**Calibration gate: PASSED.** Every run's SB-Relaxed control showed real StoreLoad reorderings
(40–55 out of 1,000,000, consistent order of magnitude across all 3 runs), and run 3's MP-Relaxed
control additionally showed a real stale-data reordering (1/500,383 races where the flag was
observed set). This confirms the harness's race window is tight enough to observe genuine ARM64
weak-memory behavior, and that native Rust correctly forbids these outcomes once the ordering is
strengthened to Release/Acquire or SeqCst (0 violations for every forbidden column, every run, as
required by the Rust memory model). LB's `Relaxed` control did not show a reordering in these 3
runs at this iteration count — LB is the hardest classic shape to trigger even on real weak-memory
hardware (it requires a same-cycle load-then-store dependency to race just right); this is reported
honestly as a non-trigger rather than treated as a calibration failure, since MP and SB already
independently establish the harness is sensitive.

Raw `RESULT` lines (run 1):
```
RESULT name=MP ordering=Relaxed forbidden="flag=1,data=0 allowed control" iterations=1000000 violations=0 observed=498767 elapsed_ms=12210
RESULT name=MP ordering=Release/Acquire forbidden="flag=1,data=0" iterations=1000000 violations=0 observed=499683 elapsed_ms=11918
RESULT name=SB ordering=Relaxed forbidden="r1=0,r2=0 allowed control" iterations=1000000 violations=55 observed=1000000 elapsed_ms=12687
RESULT name=SB ordering=SeqCst forbidden="r1=0,r2=0" iterations=1000000 violations=0 observed=1000000 elapsed_ms=14179
RESULT name=LB ordering=Relaxed forbidden="r1=1,r2=1 allowed" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12942
RESULT name=LB ordering=SeqCst forbidden="r1=1,r2=1" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12192
RESULT name=IRIW ordering=SeqCst forbidden="xy=1,0 and yx=1,0" iterations=1000000 violations=0 observed=249564 elapsed_ms=27484
```
Raw `RESULT` lines (run 2):
```
RESULT name=MP ordering=Relaxed forbidden="flag=1,data=0 allowed control" iterations=1000000 violations=0 observed=500144 elapsed_ms=11871
RESULT name=MP ordering=Release/Acquire forbidden="flag=1,data=0" iterations=1000000 violations=0 observed=499117 elapsed_ms=11945
RESULT name=SB ordering=Relaxed forbidden="r1=0,r2=0 allowed control" iterations=1000000 violations=40 observed=1000000 elapsed_ms=15939
RESULT name=SB ordering=SeqCst forbidden="r1=0,r2=0" iterations=1000000 violations=0 observed=1000000 elapsed_ms=18187
RESULT name=LB ordering=Relaxed forbidden="r1=1,r2=1 allowed" iterations=1000000 violations=0 observed=1000000 elapsed_ms=17692
RESULT name=LB ordering=SeqCst forbidden="r1=1,r2=1" iterations=1000000 violations=0 observed=1000000 elapsed_ms=18233
RESULT name=IRIW ordering=SeqCst forbidden="xy=1,0 and yx=1,0" iterations=1000000 violations=0 observed=250365 elapsed_ms=36810
```
Raw `RESULT` lines (run 3):
```
RESULT name=MP ordering=Relaxed forbidden="flag=1,data=0 allowed control" iterations=1000000 violations=1 observed=500383 elapsed_ms=13060
RESULT name=MP ordering=Release/Acquire forbidden="flag=1,data=0" iterations=1000000 violations=0 observed=497818 elapsed_ms=12268
RESULT name=SB ordering=Relaxed forbidden="r1=0,r2=0 allowed control" iterations=1000000 violations=43 observed=1000000 elapsed_ms=14840
RESULT name=SB ordering=SeqCst forbidden="r1=0,r2=0" iterations=1000000 violations=0 observed=1000000 elapsed_ms=17666
RESULT name=LB ordering=Relaxed forbidden="r1=1,r2=1 allowed" iterations=1000000 violations=0 observed=1000000 elapsed_ms=17442
RESULT name=LB ordering=SeqCst forbidden="r1=1,r2=1" iterations=1000000 violations=0 observed=1000000 elapsed_ms=17514
RESULT name=IRIW ordering=SeqCst forbidden="xy=1,0 and yx=1,0" iterations=1000000 violations=0 observed=251752 elapsed_ms=33968
```

## Backend — status: NOT COMPLETED for this harness

The backend (`.NET`/CoreCLR via `cargo-dotnet`) run of `pal_litmus` itself did not finish inside
this task's time budget. The process building/running it hit a sandbox-specific obstacle (its
execution sandbox had `~/.rustup` mounted read-only, which `cargo-dotnet`'s dotnet-PAL injection
step needs to write to) and spent its budget building a writable toolchain-copy workaround rather
than completing the actual litmus runs. This is a tooling/sandboxing issue in that specific
execution environment, not a finding about the backend's atomics lowering, and no backend numbers
for `pal_litmus`'s MP/SB/LB/IRIW shapes are fabricated or estimated here.

**What is empirically confirmed instead**: a second, independently-built litmus probe covering the
two shapes most directly implicated by the atomic-lowering analysis (MP Release/Acquire, which
targets the `atomic_load` fix, and SB SeqCst, which targets the `atomic_store` fix) — designed the
same way (persistent threads, reusable `Barrier`, native-calibrated) — was run directly with full
filesystem access (no sandbox obstacle) and produced a real, clean, 3-run backend result. See
`docs/MEMORY_MODEL.md` §5.2–5.3 for that harness's design and full data:

- Native calibration: MP-Relaxed control 1 violation / 151,065 observed; SB-Relaxed control 1
  both-zero — harness confirmed sensitive.
- Backend, 3 runs × 300,000 iterations: **MP Release/Acquire violations = 0, SB SeqCst
  both-zero = 0, all 3 runs** — matching the native oracle's zero-violation result for those
  forbidden outcomes.

`pal_litmus`'s LB and IRIW shapes were **not** empirically run on the backend by either harness.
Per the code-level ECMA-335 soundness argument in `docs/MEMORY_MODEL.md` §3–4, both are expected to
inherit the same fix (LB's SeqCst shape depends on the same load-not-hoisted-above-local-store
property the `atomic_load` fix provides; IRIW built from plain SeqCst loads depends on the same
property, and IRIW built from RMW-observing reads was already sound via `Interlocked`), but this
expectation is explicitly flagged as **unconfirmed by direct measurement**, not claimed as tested.

This harness (`cargo_tests/pal_litmus`) is committed as-is so a future session with unrestricted
filesystem access to `~/.rustup` can complete its backend run (`CARGO_DOTNET_BACKEND=native
DOTNET_ROOT=$HOME/.dotnet PATH="$HOME/.rustup/toolchains/nightly-2026-06-17-aarch64-apple-darwin/bin:$PATH"
/Users/sharif/Code/rustc_codegen_clr/tools/cargo-dotnet/target/release/cargo-dotnet dotnet run --
--iterations 1000000`, 3×) to close LB/IRIW empirically and cross-check MP/SB against this fuller
harness's own numbers.
