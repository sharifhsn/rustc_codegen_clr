# Debugging the backend: a runtime observability framework

`rustc_codegen_clr` compiles Rust → .NET CIL. The hardest bugs are **runtime divergences**
(an `AccessViolationException`, or wrong output) where the emitted `.il` looks internally
consistent — reading it cannot tell you *which branch ran*, *what value a niche held*, or
*where a pointer went bad*. This page documents the tools that bridge that static→runtime gap.

The motivating case: `regex_automata::meta::Regex::new("[a-o]").is_match(...)` faulted in the
backend. A full session of `.il` reading concluded (wrongly) it was a fat-pointer/vtable bug in
`Arc<dyn PrefilterI>::drop`. Runtime observability corrected that in minutes: the real fault is a
**niche-byte corruption of `Core.pre: Option<Prefilter>`** during the meta strategy build.

## 1. `rcc-debug` — one entrypoint (`feasibility/rcc-debug`)

| command | what you get |
|---|---|
| `rcc-debug stack  <crate>` | run; on crash print the **demangled managed call stack** (the single most useful artifact — the .NET runtime prints it for free on an unhandled AV) |
| `rcc-debug trace  <crate> <fn_substr>` | **`TRACE_FN`**: a `Console.WriteLine` at every basic-block entry of matching fns. The last line before the crash *is* the crashing block. Reveals which branch actually ran. |
| `rcc-debug val    <crate> <fn_substr>` | **`TRACE_VAL`**: prints the runtime `SwitchInt` discriminant — "what value does the miscompiled branch see?" |
| `rcc-debug diff   <crate>` | native-rustc vs backend output differential (the oracle) |
| `rcc-debug vendor <crate> <ver> [dst]` | vendor a registry crate for source instrumentation (below) |

### The backend hooks (env vars)
Defined in `src/assembly.rs` (`add_fn`, block tracer) and `src/terminator/mod.rs` (`handle_switch`,
value tracer), backed by `Assembly::debug_msg` / `Assembly::debug_val` in `cilly/src/ir/asm.rs`.
Both are **codegen-time env reads** that inject **runtime** prints, and are **off unless set**
(zero behavior change otherwise). Because the print fires at runtime, it is immune to
cargo-dotnet's discarded-warm-pass / bin-CU codegen placement that defeats the *codegen-time*
`DUMP_MIR` (see `feasibility/mirtool`). Keep the filter narrow (one type/fn) to avoid flooding hot loops.

## 2. Differential source instrumentation (the decisive technique)

When a struct's bytes are wrong but you can't see where, **observe the real values**: vendor the
crate, add `eprintln!`s, and diff native vs backend.

```sh
rcc-debug vendor regex-automata 0.4.9 /tmp/ra-dbg     # copies registry src, strips checksum
# in the repro Cargo.toml:
#   [patch.crates-io]
#   regex-automata = { path = "/tmp/ra-dbg" }
# edit /tmp/ra-dbg, then run native (truth) and CARGO_DOTNET_BACKEND=native (backend); diff the prints.
```

Read a niche byte from a `&T`: `unsafe { *(&v as *const T as *const u8).add(NICHE_OFFSET) }`.
Bracket a corruption by printing the same byte at successive points (construction → each move →
use); the step where native stays correct but the backend flips is the miscompiled op.

### Two hard-won gotchas
- **PIN THE PATCH VERSION** (`=0.4.9`). A bare `0.4` re-resolves to a different patch release with a
  *different code path*. This caused a multi-hour misdiagnosis: 0.4.9 builds **no** prefilter for
  `[a-o]` (`Core.pre = None`) while 0.4.14 **does** (`Core.pre = Some`) — entirely different crash
  mechanisms. Always confirm native and backend exercise the same version + path first.
- **In-place registry edits are unreliable** — cargo re-extracts the source from the cached `.crate`
  when `Cargo.lock` is deleted, silently reverting your edits. Use a `[patch]` path dep instead.

## 3. The static tools (complementary)

- **`DUMP_LAYOUT=<substr>`** (`rcc-debug layout <crate> <type>`) — dump the backend's computed enum
  layout: tag encoding (Direct/Niche), tag type + byte offset, per-variant field offsets, and rustc's
  `untagged_variant`/`niche_variants`/`niche_start`. This pinpointed the regex root cause (a U128 niche
  with `niche_start = 2^128-2` exposed an index-vs-value compare in `get_discr`). **The
  discriminant/niche/layout family — Direct vs Niche tags, 128-bit tags, shifted/nested niches, tag
  offsets — is the canonical "passes the type-checker, fails silently" class**; reach for `layout` on any
  enum/`match`/discriminant misbehavior. Minimal regression net: `cargo_tests/probe_enum_discr`.
- `feasibility/mirtool cil <crate> <method_substr>` — slice a method's CIL out of the linked `.il`
  (the reliable source of emitted CIL; works where `DUMP_MIR` misses bin-CU/strategy fns).
- `DUMP_MIR=<substr>` (`src/assembly.rs`) — dump the optimized MIR the backend received, for MIR↔CIL
  alignment. Blind to fns codegen'd in passes that don't carry the env; prefer `cil` + runtime traces.
- `OPT_FUEL=0` fully disables the optimizer (note: `OPTIMIZE_CIL=0` does NOT — the linker always opts).

## Method (what worked on regex)
1. `rcc-debug stack` → the crash is in `create_cache`, **not** `drop` as assumed. (Corrected the whole diagnosis.)
2. `rcc-debug trace`/`val` → `Core.pre` discriminant read as `Some` when it should be `None`.
3. `vendor` + niche-byte prints, bracketed across the strategy build → niche is correct `2` (None) at
   `Core::new`, **garbage** by `create_cache`: corrupted during the by-value `Core` move inside the
   reverse-suffix strategy build. (Native preserves `2` throughout → a real backend miscompile.)
