# Lessons from cranelift & gcc on getting `std` working

Both backends are cloned locally for reference:
`~/Code/rustc_codegen_cranelift` (~14k LOC src, **5 patches**) and
`~/Code/rustc_codegen_gcc` (~25k LOC src, **5 patches, 144 lines**).

## The headline: patch count is an inverse maturity metric

Both mature backends compile the **entire real std** with ~5 patches, and those patches are
almost all *test-only* (disable slow tests, trim f16/128-bit-atomic asserts). **They do not
fork std.** The reason is the single most important fact for us:

> **Cranelift and GCC target real OS triples (`x86_64-unknown-linux-gnu`, …) and reuse the
> existing `std::sys` platform layer + the host libc.** They never reimplement threads,
> files, TLS, env, or unwinding — std already has those for real OSes; the new backend just
> has to lower the IR.

cg_clr **cannot** take this shortcut. It runs std on the .NET runtime: no host libc, no
native `.eh_frame`, no native TLS relocations, no system linker. So our std burden is
*categorically larger* — the right mental model is **`wasm32` / SGX / UEFI**: targets that
define a **bespoke `std::sys` module**. cranelift's tiny `patches/` is not a reachable goal
for us; a few-thousand-LOC platform layer is the realistic shape.

## How they build std (mechanism — worth copying)

- Neither uses prebuilt std. Both compile the **real upstream library tree** from `rust-src`
  via their own build tool (`build_system/` + `y.sh`), i.e. a managed `-Zbuild-std`.
  cg_clr already does this (`cargo … -Zbuild-std`), so the mechanism is in place.
- **git-commit-per-patch sysroot** (gcc `build_system/src/prepare.rs`): `git init` the copied
  std source and apply each patch as a commit, so patches rebase cleanly across std bumps.
  Adopt this — it directly addresses the bit-rot pain we just lived through.
- **`SysrootKind::Llvm` vs `Clif` toggle** (cranelift `build_system/build_sysroot.rs`): build
  std with the *real* LLVM backend OR with yourself, swappable per crate. This is the
  bisection tool for "which std crate do I miscompile" — high value, cheap to add.
- **`mini_core` staging** (gcc `example/mini_core.rs`): prove the codegen on a hand-rolled
  minimal core before real core/alloc/std, to separate codegen bugs from std-feature gaps.
  cg_clr's `#![no_std]` test corpus already plays this role.

## How they handle the hard subsystems (and what transfers)

| Subsystem | cranelift / gcc approach | Transfer to cg_clr |
|---|---|---|
| **Allocator** (`__rust_alloc`) | thin shim → host `malloc`/`free` | **Repoint to the managed heap** (`Marshal.Alloc*` / GC), not libc. Trivial wiring. |
| **i128** | software low/high pairs + compiler-rt-style helpers (gcc `int.rs`, 1066 LOC) | .NET has `Int128` (since .NET 7) — *easier* than gcc; cg_clr already uses it. |
| **Atomics / TLS** | map to `__atomic_*` builtins / OS TLS models | Map to `Interlocked` / `[ThreadStatic]` — likely **cheap wins**. (8/16-bit atomics: gate out via target, don't lock-emulate.) |
| **Panic / unwind** | the **multi-year hard part** for cranelift (Cranelift regalloc bug); both ship **`panic=abort` baseline**, unwinding behind a feature | **.NET exceptions are native — this is where the managed runtime is *easier*.** Lean in; still keep abort as the safe baseline. |
| **Intrinsics + SIMD** | the **bulk** of both backends (~60% of gcc) and the *last* std blocker | Same long tail awaits. **Funnel un-lowerable ops to named runtime helpers** and `span_fatal` the rest. |
| **inline `asm!` / `global_asm!`** | shell out to an external assembler (cranelift) / big translation layer (gcc) | **No managed analogue — hard-error with the offending span.** Don't try to emulate. |
| **f16/f128** | libcalls (`__extendhfsf2`, …); a recurring sore spot | `System.Half` exists; f128 needs emulation/libcall. |

## The five transferable engineering patterns

1. **Own a `std::sys` platform layer** — budget for it; model on wasm/SGX, not on cranelift's
   "reuse the host OS."
2. **Keep the codegen dumb; push platform complexity into a runtime support library** (our
   `mycorrhiza`/`RustModule`) — the analogue of their libcall-to-compiler-builtins funnel.
3. **Fail loud and specific** (`span_fatal("<thing> not yet supported")`). This converts
   "std doesn't work" into a *finite, prioritizable list of named gaps* — the single most
   useful pattern for making progress measurable.
4. **`panic=abort` is the supported baseline; unwinding is a feature** (and on .NET it's an
   advantage, not a multi-year slog).
5. **Build the LLVM-vs-self sysroot toggle + commit-per-patch sysroot** for debuggability and
   bit-rot resistance.

Sources: cranelift devlogs (bjorn3.github.io, Oct 2023 / Jun 2025), gcc progress reports
(blog.antoyo.xyz #8, #11), and the cloned repos' `build_system/`, `patches/`, `src/`.
