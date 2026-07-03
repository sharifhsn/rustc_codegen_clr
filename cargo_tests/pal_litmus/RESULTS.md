# pal_litmus results

`pal_litmus` is a weak-memory litmus harness for MP, SB, LB, and IRIW using real
`std::thread` worker threads. Each test resets its atomics before every iteration and uses
`std::sync::Barrier` synchronization for the per-iteration start and finish rendezvous.

The first backend smoke found that using the same `Barrier` object for both start and finish could
hang on the backend after the first iteration. The committed harness uses two reusable barriers per
test, one for start and one for finish. This still uses `std::sync::Barrier` for synchronization and
keeps persistent worker threads instead of spawning per iteration.

Methodology for the table below:

- 1,000,000 iterations per test per process.
- 3 process-level runs per side.
- Native oracle command: `cargo +nightly-2026-06-17 run --release -- --iterations 1000000`.
- Backend command: `cargo-dotnet dotnet run -- --iterations 1000000` with
  `CARGO_DOTNET_BACKEND=native`, `DOTNET_ROOT=$HOME/.dotnet`, and the nightly toolchain on `PATH`.
- In this sandbox, backend runs used a writable copy-on-write sysroot under `/private/tmp` plus a
  writable `CARGO_HOME` overlay, because `cargo-dotnet` injects the dotnet PAL into rust-src and the
  real `~/.rustup` sysroot was read-only to the command sandbox.

## Summary

Calibration gate: **PASSED**. Native relaxed controls produced 77 reorderings total:

- MP Relaxed stale-data control: 0 total.
- SB Relaxed both-zero control: 14 + 51 + 12 = 77 total.
- LB Relaxed both-one control: 0 total.

Backend forbidden outcomes were zero in all runs:

- MP Release/Acquire stale data: 0 total.
- SB SeqCst both-zero: 0 total.
- LB SeqCst both-one: 0 total.
- IRIW SeqCst disagreement: 0 total.

Backend relaxed controls also reported zero reorderings. That is allowed by the interpretation rule;
it is a stronger-than-required/null observation, not a failure.

## Comparison table

Violation counts are shown as `violations / observed` for MP and IRIW, where the interesting
observation count is smaller than the iteration count. SB and LB are shown as violations out of
1,000,000 iterations.

| Side | Run | MP Relaxed control | MP Release/Acquire forbidden | SB Relaxed control | SB SeqCst forbidden | LB Relaxed control | LB SeqCst forbidden | IRIW SeqCst forbidden |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| native | 1 | 0 / 500,952 | 0 / 503,138 | 14 | 0 | 0 | 0 | 0 / 245,882 |
| native | 2 | 0 / 499,811 | 0 / 502,319 | 51 | 0 | 0 | 0 | 0 / 246,275 |
| native | 3 | 0 / 500,737 | 0 / 498,799 | 12 | 0 | 0 | 0 | 0 / 249,532 |
| backend | 1 | 0 / 497,462 | 0 / 519,078 | 0 | 0 | 0 | 0 | 0 / 250,859 |
| backend | 2 | 0 / 509,751 | 0 / 508,109 | 0 | 0 | 0 | 0 | 0 / 251,455 |
| backend | 3 | 0 / 497,687 | 0 / 497,272 | 0 | 0 | 0 | 0 | 0 / 251,740 |

## Native raw output

Run 1:

```text
RESULT name=MP ordering=Relaxed forbidden="flag=1,data=0 allowed control" iterations=1000000 violations=0 observed=500952 elapsed_ms=12160
RESULT name=MP ordering=Release/Acquire forbidden="flag=1,data=0" iterations=1000000 violations=0 observed=503138 elapsed_ms=12209
RESULT name=SB ordering=Relaxed forbidden="r1=0,r2=0 allowed control" iterations=1000000 violations=14 observed=1000000 elapsed_ms=12101
RESULT name=SB ordering=SeqCst forbidden="r1=0,r2=0" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12193
RESULT name=LB ordering=Relaxed forbidden="r1=1,r2=1 allowed" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12060
RESULT name=LB ordering=SeqCst forbidden="r1=1,r2=1" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12033
RESULT name=IRIW ordering=SeqCst forbidden="xy=1,0 and yx=1,0" iterations=1000000 violations=0 observed=245882 elapsed_ms=23739
```

Run 2:

```text
RESULT name=MP ordering=Relaxed forbidden="flag=1,data=0 allowed control" iterations=1000000 violations=0 observed=499811 elapsed_ms=12856
RESULT name=MP ordering=Release/Acquire forbidden="flag=1,data=0" iterations=1000000 violations=0 observed=502319 elapsed_ms=12528
RESULT name=SB ordering=Relaxed forbidden="r1=0,r2=0 allowed control" iterations=1000000 violations=51 observed=1000000 elapsed_ms=12416
RESULT name=SB ordering=SeqCst forbidden="r1=0,r2=0" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12413
RESULT name=LB ordering=Relaxed forbidden="r1=1,r2=1 allowed" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12009
RESULT name=LB ordering=SeqCst forbidden="r1=1,r2=1" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12123
RESULT name=IRIW ordering=SeqCst forbidden="xy=1,0 and yx=1,0" iterations=1000000 violations=0 observed=246275 elapsed_ms=24335
```

Run 3:

```text
RESULT name=MP ordering=Relaxed forbidden="flag=1,data=0 allowed control" iterations=1000000 violations=0 observed=500737 elapsed_ms=12122
RESULT name=MP ordering=Release/Acquire forbidden="flag=1,data=0" iterations=1000000 violations=0 observed=498799 elapsed_ms=12155
RESULT name=SB ordering=Relaxed forbidden="r1=0,r2=0 allowed control" iterations=1000000 violations=12 observed=1000000 elapsed_ms=12195
RESULT name=SB ordering=SeqCst forbidden="r1=0,r2=0" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12181
RESULT name=LB ordering=Relaxed forbidden="r1=1,r2=1 allowed" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12062
RESULT name=LB ordering=SeqCst forbidden="r1=1,r2=1" iterations=1000000 violations=0 observed=1000000 elapsed_ms=12149
RESULT name=IRIW ordering=SeqCst forbidden="xy=1,0 and yx=1,0" iterations=1000000 violations=0 observed=249532 elapsed_ms=23844
```

## Backend raw output

Run 1:

```text
RESULT name=MP ordering=Relaxed forbidden="flag=1,data=0 allowed control" iterations=1000000 violations=0 observed=497462 elapsed_ms=4907
RESULT name=MP ordering=Release/Acquire forbidden="flag=1,data=0" iterations=1000000 violations=0 observed=519078 elapsed_ms=5543
RESULT name=SB ordering=Relaxed forbidden="r1=0,r2=0 allowed control" iterations=1000000 violations=0 observed=1000000 elapsed_ms=5985
RESULT name=SB ordering=SeqCst forbidden="r1=0,r2=0" iterations=1000000 violations=0 observed=1000000 elapsed_ms=6342
RESULT name=LB ordering=Relaxed forbidden="r1=1,r2=1 allowed" iterations=1000000 violations=0 observed=1000000 elapsed_ms=4702
RESULT name=LB ordering=SeqCst forbidden="r1=1,r2=1" iterations=1000000 violations=0 observed=1000000 elapsed_ms=5528
RESULT name=IRIW ordering=SeqCst forbidden="xy=1,0 and yx=1,0" iterations=1000000 violations=0 observed=250859 elapsed_ms=15469
```

Run 2:

```text
RESULT name=MP ordering=Relaxed forbidden="flag=1,data=0 allowed control" iterations=1000000 violations=0 observed=509751 elapsed_ms=4328
RESULT name=MP ordering=Release/Acquire forbidden="flag=1,data=0" iterations=1000000 violations=0 observed=508109 elapsed_ms=4877
RESULT name=SB ordering=Relaxed forbidden="r1=0,r2=0 allowed control" iterations=1000000 violations=0 observed=1000000 elapsed_ms=4378
RESULT name=SB ordering=SeqCst forbidden="r1=0,r2=0" iterations=1000000 violations=0 observed=1000000 elapsed_ms=4804
RESULT name=LB ordering=Relaxed forbidden="r1=1,r2=1 allowed" iterations=1000000 violations=0 observed=1000000 elapsed_ms=3967
RESULT name=LB ordering=SeqCst forbidden="r1=1,r2=1" iterations=1000000 violations=0 observed=1000000 elapsed_ms=4723
RESULT name=IRIW ordering=SeqCst forbidden="xy=1,0 and yx=1,0" iterations=1000000 violations=0 observed=251455 elapsed_ms=14503
```

Run 3:

```text
RESULT name=MP ordering=Relaxed forbidden="flag=1,data=0 allowed control" iterations=1000000 violations=0 observed=497687 elapsed_ms=5055
RESULT name=MP ordering=Release/Acquire forbidden="flag=1,data=0" iterations=1000000 violations=0 observed=497272 elapsed_ms=5593
RESULT name=SB ordering=Relaxed forbidden="r1=0,r2=0 allowed control" iterations=1000000 violations=0 observed=1000000 elapsed_ms=2963
RESULT name=SB ordering=SeqCst forbidden="r1=0,r2=0" iterations=1000000 violations=0 observed=1000000 elapsed_ms=5087
RESULT name=LB ordering=Relaxed forbidden="r1=1,r2=1 allowed" iterations=1000000 violations=0 observed=1000000 elapsed_ms=2833
RESULT name=LB ordering=SeqCst forbidden="r1=1,r2=1" iterations=1000000 violations=0 observed=1000000 elapsed_ms=4316
RESULT name=IRIW ordering=SeqCst forbidden="xy=1,0 and yx=1,0" iterations=1000000 violations=0 observed=251740 elapsed_ms=15053
```
