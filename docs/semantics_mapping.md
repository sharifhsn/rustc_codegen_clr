# Mapping Rust semantics onto .NET — a research framework

`rustc_codegen_clr` constantly has to answer one question: *"Rust operation X — what's
the .NET equivalent?"* For most things the mapping is obvious. For others (float min/max,
integer overflow, casts, atomics) the operations look similar but differ in edge cases
(NaN, signed zero, wrapping, ordering), and a wrong guess is a **silent miscompile** — the
program runs and produces subtly wrong numbers.

This is a repeatable method for getting those mappings right, plus a worked example
(float min/max/abs). Use it whenever you add an intrinsic or operation whose semantics
aren't trivially obvious.

## The method

1. **Pin the Rust semantics — from the source, not memory.** Find the exact intrinsic /
   operation in the local rustc/core sources (`$(rustc --print sysroot)/lib/rustlib/src`
   and `.../rustc-src`). Read its doc and the spec it cites (often IEEE 754). Write down the
   behaviour on the *edge cases that distinguish near-identical operations* — for floats:
   NaN (propagate vs ignore), signed zero (±0 ordered?), ±inf. Note the intrinsic's exact
   name and signature — names change across nightlies (see [PORT_NOTES](../feasibility/PORT_NOTES.md)).

2. **Establish Rust ground truth empirically.** Don't trust the docs alone — write a tiny
   native probe that runs the operation over the edge-case vector and prints **raw bits**
   (`to_bits()`), so +0/-0 and NaN payloads are unambiguous. Run it with the project's
   nightly (`rustc +nightly`). This is the oracle everything else is checked against.

3. **Survey the .NET candidates.** Find the BCL methods / IL opcodes that could implement it.
   For floats the relevant surface is the generic-math statics on `System.Single`/`Double`/`Half`
   (`Max`, `Min`, `MaxNumber`, `MinNumber`, `Abs`, …) — these are what `Float::class()` targets
   in [cilly/src/ir/tpe/float.rs](../cilly/src/ir/tpe/float.rs). Read their docs for the *same*
   edge cases. .NET's generic-math methods are specified against IEEE 754:2019, which is exactly
   the spec Rust's float intrinsics cite — so they usually line up, but verify.

4. **Decide: direct map or emulate.** Pick the .NET method whose edge-case behaviour matches the
   Rust oracle. If none matches exactly, emulate (e.g. an explicit NaN check, or a helper method
   synthesised by the linker's `MissingMethodPatcher` — see `cilly/src/ir/builtins/`).

5. **Verify on .NET, against the oracle.** The chosen mapping is a *hypothesis* until the codegen
   output is diffed against the Rust ground truth. Add a test that runs the operation over the
   same edge-case vector on .NET and asserts the bit-exact results from step 2. The project's
   test harness already compares program output, so a passing edge-case test *is* the proof.

> The crux is steps 2 and 5: **two empirical anchors** (native Rust, and codegen-on-.NET) around
> a documentation-derived hypothesis. Docs disagree with reality often enough that an
> operation-level mapping should never ship on docs alone.

## Worked example: float min / max / abs

### 1–2. Rust semantics + ground truth
Rust reworked these intrinsics (`f32::max` etc.); the current names and behaviours
(confirmed with a native bits-probe over `{NaN, ±0, ±inf, normals}`):

| Rust method | intrinsic | NaN | signed zero |
|---|---|---|---|
| `f32::max` | `maximum_number_nsz_f32` | **ignored** (returns the number) | unspecified (nsz) |
| `f32::min` | `minimum_number_nsz_f32` | ignored | unspecified |
| `f32::maximum` | `maximumf32` | **propagated** | ordered, −0 < +0 |
| `f32::minimum` | `minimumf32` | propagated | ordered, −0 < +0 |
| `f32::abs` | `fabs` (generic) | NaN→NaN | −0 → +0, −inf → +inf |

The `nsz`/`number` family vs the IEEE `maximum`/`minimum` family is the trap: same name in
casual speech, opposite NaN behaviour.

### 3–4. .NET candidates + mapping
`System.Single`/`Double`/`Half` implement `INumber<T>`/`IFloatingPointIeee754<T>`, whose statics
are specified against IEEE 754:2019:

| Rust intrinsic | .NET static method | why it matches |
|---|---|---|
| `maximumf32/64` | `Max` | IEEE `maximum`: NaN-propagating, −0 < +0 |
| `minimumf32/64` | `Min` | IEEE `minimum` |
| `maximum_number_nsz_f32/64` | `MaxNumber` | IEEE `maximumNumber`: NaN-ignoring (nsz freedom on zero sign is satisfied) |
| `minimum_number_nsz_f32/64` | `MinNumber` | IEEE `minimumNumber` |
| `fabs` (generic) | `Abs` | absolute value; dispatch on the argument's float width |

Implemented in [src/terminator/intrinsics/mod.rs](../src/terminator/intrinsics/mod.rs) via
`float_binop`/`float_unop`, which emit a call to `<FloatClass>::<Method>`.

### 5. Verification
Native ground truth: the probe in step 2. .NET verification:
[test/intrinsics/float_minmax.rs](../test/intrinsics/float_minmax.rs) calls each intrinsic
directly over the edge-case vector and asserts the bit-exact results (NaN-result cases via
`.is_nan()`; nsz signed-zero left sign-agnostic since Rust leaves it unspecified). It passes on
.NET 8, confirming the mapping end-to-end — not just that it compiles.
