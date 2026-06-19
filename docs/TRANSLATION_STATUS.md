# Rust ↔ .NET translation layer — completeness & feasibility map

> Status snapshot as of 2026-06 (nightly-2026-06-17, post the V1→V2 IR collapse and the
> allocator-ABI fix). Compiled from a code audit of the whole backend cross-checked against
> FractalFir's design articles (`docs/ARCHITECTURE.md`, `docs/fractalfir_articles/`). Intended as a
> planning reference: what exists, what's partial, what's missing-but-feasible, and what is
> fundamentally limited. File:function pointers are given so each claim is checkable.

## The one-sentence picture

The **codegen** (Rust → CIL) is mature and largely faithful — ~90–96% of `core`/`alloc`/`std` test
suites pass. The unfinished frontier is **interop ergonomics** (especially calling Rust *from* .NET)
and **platform integration** (a real .NET `std`). This matches the author's own framing: *the
translation is the validated, even elegant part; the interop ergonomics and platform integration are
where the hard work remains* (`docs/fractalfir_articles/`, `v0_1_0`/`v0_2_0`).

Mental model of the layers (each builds on the one above):

```
  codegen core      Rust MIR → CIL: types, ops, calls, dispatch         ~90% — mature
  interop substrate Rust → .NET BCL call mechanism (magic fns / extern) mechanism done, API thin
  runtime/std       allocator, stdio, threads, fs … on .NET             surrogate today; PAL = the fix
  ── consumers ──
  std::sys::pal::dotnet (the PAL)   a bounded vertical over the substrate   scoped, not built
  full bidirectional layer          all BCL methods + .NET→Rust + marshalling   ~25% overall
```

---

## 1. Type lowering — ~90% complete, faithful
`rustc_codegen_clr_type/src/type.rs` (`get_type`), `…/src/adt.rs`.

**Solid:** all scalars incl. **i128/u128** (→ .NET `Int128`/`UInt128`), `f32`/`f64`, Bool/Char,
tuples/structs/unions, thin & **fat pointers/DSTs** (`{DATA_PTR@0, METADATA@8}`), **ZSTs** (→ `Void`,
intercepted before the match), Never, **Foreign** (kept thin), FnPtr/FnDef, **pattern types**,
closures, and **monomorphized generics via name-mangling**. Enums → `[FieldOffset]` tagged unions is
faithful (Single/Direct/Niche encodings, real `tag_field`/offsets from rustc layout).

**Partial:** **f16/f128** (types lower; some constants/ops `todo!`, an f128 enum-tag panics at
`adt.rs:132`); **coroutines/async** (type forms but field access is `todo!` at `adt.rs:216`;
`CoroutineWitness`/`CoroutineClosure` hit the catch-all); **SIMD** (only 64/128/256/512-bit; others
`panic!` in `tpe/simd.rs`); `dyn` pointee is a single contentless `"Dyn"` class (fine — identity is
in the vtable). Latent: layout offsets > `u16::MAX` are silently clamped (`adt.rs:55`).

**Ceilings (interrogated in §7):** the *one* true wall here is a transparent zero-cost open generic
whose overlapping layout holds a managed ref (CLI §9.5 + the GC ref-map). Internally Rust generics
work fully (monomorphized → concrete mangled classes with legal overlapping layout, since Rust values
are unmanaged); only C#-instantiating-a-Rust-generic-with-a-new-T is blocked, and that has a bridge
(§7). ZSTs are special-cased, not a true wall.

## 2. Operations & intrinsics — broad, with sharp pockets
`src/terminator/intrinsics/`, `src/binop/`, `src/casts.rs`, `src/rvalue.rs`.

**Solid:** integer arith/cmp/bitwise/shifts (full i128/u128; sign-agnostic stack handled), float math
(verified against an IEEE min/max truth table), mem (copy/write_bytes/raw_eq), ptr offset, type_info
(size/align/type_name/variant_count), aggregate construction, transmute, int↔int & int↔float casts.

**Real gaps / bugs:**
- **SIMD is effectively unimplemented — the single biggest hole.** `intrinsics/simd.rs` is empty; the
  ~14 "handled" ops defer to runtime managed helpers that don't exist; `simd_splat` missing; most
  `simd_*` ICE via `must_be_overridden`. Builtin side (`ir/builtins/simd/`) has a fallback but
  `simd_shuffle` is an unregistered `todo!`.
- **Atomics:** sub-word `cxchg`/`xchg` for `u8`/`i8`/`u16`/`i16` now correct via masked-CAS-loop
  builtins (WF-5 — see §10); `atomic_store` is still a plain store.
- **`float → int` `as` cast is wrapping, not saturating** (`casts.rs:176` via `rvalue.rs:106`) — a
  latent **correctness miscompile** for out-of-range/NaN (Rust `as` saturates).
- f16/f128 float-to-float `as` casts `panic!` (`rvalue.rs:238`); `int → f128` cast `todo!`.

**Surmountable, not fundamental (see §7):** SIMD maps to `System.Runtime.Intrinsics`
(`Vector128/256/512` + `Sse`/`Avx`/`AdvSimd`; the immediate-operand caveat lines up with Rust's
const-index `simd_shuffle`) — it's the biggest *implementation* hole, not a wall. `type_id` can emit
the real 128-bit constant instead of the `GetHashCode` shortcut. Sub-word/weak atomics need lock/wide-
word emulation (the 1-byte `cxchg` is a real bug — §10).

## 3. Calls / dispatch / unwinding
`rustc_codegen_clr_call/`, `src/terminator/`, `cilly/src/ir/basic_block.rs` (EH), `…/builtins/`.

**Solid:** static, indirect/fn-pointer (`calli`), **virtual dispatch via vtables**, closures/
`rust_call` tuple-splitting, drop glue. ABI handling is permissive and sound (CIL is
calling-convention-agnostic): `CallInfo` accepts Rust-family/C/Custom/X86.

**Fragile:** **two divergent ABI checkers** — `src/function_sig.rs` (stale, `Rust|C` only) is still on
the drop-glue + interop paths and can `panic!` where the modern `CallInfo` succeeds; `#[track_caller]`
panic-location is filled from **uninitialized memory** (`call.rs:576`; benign under abort, a bug once
unwinding lands); vtable layout `[drop,size,align,…]` is baked in.

**Unwinding — half-built:** the hard part (handler regions → real .NET `try/catch`,
`resolve_exception_handlers`, `block_gc`, `RustException`) **is done**, but the **throw side is not
wired** (`ir/builtins/unwind.rs` is empty, `raise_exception` commented out in the linker, no
personality fn) → in practice it's **panic=abort with dormant plumbing**. `NO_UNWIND` strips handlers
for perf. C-mode unwinding is a non-compiling sketch (gated on an undefined `UNWIND_SUPPORTED`, with
`setjump`/`longjump` typos).

**Missing/hard terminators:** `InlineAsm` (throwing stub), `TailCall`/`Yield`/`CoroutineDrop`
(`todo!`). `UnwindTerminate` rethrows instead of aborting.

## 4. Runtime / `std` support
`cilly/src/libc_fns.rs`, `cilly/src/bin/linker/main.rs`, `cilly/src/ir/builtins/`.

**Solid (.NET path), via the linker's `MissingMethodPatcher`:** heap → `NativeMemory.AlignedAlloc`
(malloc/realloc/free → `Marshal.*HGlobal`), EH → `RustException` try/catch, atomics → `Interlocked`,
math, and a **full pthread mapping** (`builtins/thread.rs`, 701 LOC).

**The "surrogate libc" is host-libc *delegation*** — 610 functions in `LIBC_FNS` are PInvoke'd to the
host's real libc/libm/libgcc (f128 support "not portable at all", Linux/GNU only). So the artifact is
**not self-contained** and is x86_64-Linux-bound. **This is exactly what the `dotnet` PAL replaces —
see §8.**

**Dead/WIP:** comptime interpreter, AOT (`aot.rs`/`native_passtrough.rs`: `#![allow(dead_code)]`, zero
callers), softfloat ("Sample code", unused).

## 5. Multi-target exporters
`cilly/src/ir/{il,c,java,cillyir}_exporter/`.

- **IL (.NET CIL): production — the only one.** ~90% op coverage; narrow F16/F128/I128 edges. Emits
  `.il` → `ilasm` (CoreCLR or Mono).
- **C: functional prototype, ~80%.** No atomics; 33 `todo!` in cold type-case paths; WIP-marked. A
  credible second target — bug-fixes are shared with IL because both consume the same V2 assembly.
- **Java: skeleton** (panics on refs/generics/SIMD). **JS: orphaned flag**, no exporter. **cillyir:**
  a debug tool (dumps IR back to Rust), not a target.
- The **optimizer** runs by default (conservative, fuel-throttled). The **typechecker** is
  comprehensive but **off-by-default and non-fatal** — it logs `FieldAssignWrongType`/
  `FieldOwnerMismatch` and continues, so it is **not a release safety net**.

## 6. Bidirectional interop — the user's core interest
`mycorrhiza/`, `src/terminator/call.rs`, `src/utilis/mod.rs`, `AssemblyUtilis/`, `src/comptime.rs`.

**Rust → .NET (calling the BCL): elegant mechanism, thin API.** Const-generic "magic functions"
encode .NET call metadata; 9 backend handlers (static/instance/virtual/ctor/cast/is_inst/null/ld_len/
try_catch) are **complete and symmetric**. `GCHandle`-based ref-holding (`mycorrhiza/src/class.rs`) is
sound. **But the API surface is ~0% generated:** `bindings.rs` binds **1,075 BCL types with ZERO
methods** (just a type-identity + cast graph); the only callable methods are ~12 hand-written ones
(Console, StringBuilder, Stopwatch, Marshal). Marshalling traits are *declared with zero impls* —
only `&str→String`, `char→DotNetChar`, and primitives actually work. Arg count capped at 0–3 in the
wrappers (the backend itself handles N).

**\.NET → Rust (calling Rust, exposing Rust types): ~5%, essentially dead.** `dotnet_typedef!` → a
comptime interpreter (`src/comptime.rs`) that is **commented out and aborts the build**; the
Cecil-based `AssemblyUtilis` backend is **unwired**; no reverse-pinvoke export of Rust functions
exists. Only the program `entrypoint` is exposed to managed callers.

---

## 7. The "hard ceilings", interrogated — true walls vs. polyfillable-with-cost

Stress-tested against the CLR spec + GC physics + prior art (C++/CLI, IKVM, Mono). The earlier
framing over-counted "fundamental" limits: **only one is a true wall** (and even it has a working
bridge at the interop seam). The rest are surmountable engineering or functional-with-a-cost.

### True walls (irreducible on stock CoreCLR)
1. **A *transparent, zero-cost* open generic whose overlapping layout holds a managed reference.**
   - Spec-level: CLI Partition I §9.5 — *"Generic types shall not be marked explicitlayout"* (blanket;
     `TypeLoadException` at load even if `T` is unused). Overlapping a managed ref with a non-ref is
     forbidden even in *non-generic* structs (the GC needs an unambiguous ref/non-ref map per offset;
     an open `T` can't be classified). dotnet keeps this deliberately.
   - **But it barely bites.** Rust generics are a *compile-time* construct; the backend monomorphizes
     them to concrete mangled classes that *may* use explicit/overlapping layout — legal, because Rust
     values are unmanaged memory (no overlapped slot is ever a managed ref). So Rust generics work
     **fully** internally (Rust-on-.NET). The *only* blocked case is **C# instantiating a Rust generic
     with a brand-new C# type at runtime**, and even that has a bridge (below). The irreducible residue
     is just the *transparent + zero-cost + managed-ref-overlapping* combination. Author:
     *"my problem is not strictly a technical one… I can't do anything about it"* (forked runtimes lift
     §9.5 — it's policy, not physics).
2. **Static borrow-safety across the managed boundary.** The CLR has no borrow checker; once a value
   crosses the seam, Rust's compile-time ownership guarantee can't be enforced (`StackOnly`/
   `ManagedSafe` are *advisory* markers the backend can't verify). Functional correctness is
   achievable (below); the *guarantee* is not.
3. **Arbitrary, novel inline asm.** No general asm→CIL lowering exists (common cases are coverable —
   below — but a hand-rolled novel asm block is genuinely unmappable).

### Polyfillable-with-cost (capability yes, at a price)
- **Generic interop across the seam (the bridge for wall #1).** Expose a *normal* C# generic wrapper
  `RustGeneric<T>` (legal — a thin handle-holder, no explicit layout) over either:
  - **size-parameterized sharing for `T: unmanaged`** — one Rust monomorphization keyed by `sizeof(T)`,
    operating via `memcpy` (like C's `void* + size_t`): **near-zero-cost, layout-preserving** (open
    dotnet proposal #97526 would even allow explicit layout when `T: unmanaged`); or
  - **boxing/`GCHandle` for managed `T`** — one Rust monomorphization over a universal managed-handle
    type, each element boxed: works for **any** T, ~10–20× in hot loops + GC pressure.
  A two-mode wrapper gives functional generic interop for any T; you forfeit only zero-cost
  transparency for the managed-ref case.
- **Holding managed refs from Rust (functional half of wall #2).** `GCHandle` (Pinned) + the Pinned
  Object Heap (.NET 5+, avoids fragmentation) lets Rust hold/deref managed objects safely;
  `[UnmanagedCallersOnly]` exposes Rust fns to managed callers (reverse P/Invoke).
- **Cross-language exceptions.** On **Unix (the project's target) native↔managed exception crossing is
  UB/unsupported by design** — so map Rust panics to *managed* exceptions caught entirely within
  managed frames, never across a P/Invoke boundary (exactly what `RustException` + .NET try/catch
  already do). Solvable *by construction*; a hard design rule for the unwinding work (§3).
- **Inline asm (common cases).** A pattern-library of known intrinsics/syscalls → hand-written
  CIL/BCL covers most real inline asm (the `mem*` ones already are); only novel asm stays at wall #3.

### Not actually ceilings (surmountable engineering; currently unbuilt/approximated)
- **async / coroutines.** The state-machine *struct* already lowers (just data). `Yield`/resume is a
  *designable* codegen — a coroutine is a resumable state machine and MIR already is a switch on a
  state discriminant; lower `Yield` to "save state + return to driver", resume to "jump to saved
  state". Bridging to .NET `Task`/`await` is a separate adapter. Hard, **not** impossible.
- **`type_id`.** Currently a 32-bit `GetHashCode` shortcut. The real 128-bit `TypeId` is a
  *compile-time constant* — just emit it (the `GlobalAlloc::TypeId` `todo!`).
- **SIMD.** `System.Runtime.Intrinsics` (`Vector128/256/512`, `Sse`/`Avx`/`AdvSimd`) covers it; the
  immediate-operand caveat (`[ConstantExpected]`) lines up with Rust's const-index `simd_shuffle`. The
  biggest *implementation* hole (§2), not a fundamental limit.
- **ZSTs.** Collapsed to `Void` + skipped in layout + special-cased in the ops; residual risk is a
  missed call-site (discipline), not representational impossibility — effectively solved.
- **proc-macros.** A *non-issue*: they run at host compile time on normal rustc, independent of the
  codegen backend; a crate *using* them compiles fine. ("Unsupported" only means you wouldn't compile
  a proc-macro crate *itself* to .NET — which you'd never want.)

**Net:** a near-"perfect" translation layer is mostly engineering. The only irreducible losses are
(1) zero-cost transparency for managed-ref-overlapping open generics, (2) static borrow-safety across
the seam, and (3) arbitrary novel inline asm — each with a functional workaround for everything but
the specific guarantee/zero-cost it gives up. Prior art agrees: C++/CLI segregates the two worlds,
IKVM erases+boxes, Mono shares the same §9.5 constraint — all land on **monomorphize + mangle**, which
is exactly what this backend does.

## 8. How this relates to the PAL / `std` work

The PAL (`std::sys::pal::dotnet`, scoped in the H2 effort) is **not a separate track — it is the
first real, bounded *vertical* of this translation layer applied to `std`.** Concretely:

- **The PAL replaces the §4 surrogate runtime.** Today `std` is built for `x86_64-unknown-linux-gnu`
  and the libc symbols are delegated to the host (not self-contained, Linux-only). The PAL is the
  clean replacement: build `std` for a custom `os="dotnet"` target and implement its platform
  primitives directly against the BCL. It is the answer to the "#4 runtime/std" gap.
- **The PAL is a *consumer* of the §6 Rust→.NET interop substrate.** Its arms call the BCL:
  `alloc` → `NativeMemory` (already done — see the allocator-ABI fix), `stdio` → `Console`,
  `thread` → `System.Threading` (or the existing pthread mapping), `fs` → `System.IO`, `env`/`args`/
  `abort` → `System.Environment`, `time` → `DateTime`/`Stopwatch`. So the PAL's reach is gated by how
  complete that substrate is **for those specific paths**.
- **It uses the *cleanest* slice of the substrate, which is also the most proven.** The recommended
  PAL binding style is `extern "C"` hooks in pure-Rust PAL code, mapped to BCL calls by the linker's
  `MissingMethodPatcher` — the *same* mechanism that already backs the surrogate's malloc→`Marshal`
  and the allocator→`NativeMemory`. So the PAL does **not** depend on the thin, half-built mycorrhiza
  managed-object/`GCHandle` API or on completing the binding generator; a handful of extern→BCL maps
  suffice for the early milestones.
- **The §7 ceilings do NOT block the PAL.** The one true generics wall is a *.NET→Rust* (direction-2)
  problem; the PAL is Rust→.NET only and exposes no Rust generics to managed callers. ZSTs/ownership-
  vs-GC are codegen concerns already handled. The PAL sits squarely in the "feasible" zone.
- **Where the PAL meets the interop-completeness gap:** `fs`/networking. `System.IO` types are *bound*
  (47 of them) but **methodless** (§6), so a real `std::fs` is where the PAL would benefit from the
  binding generator emitting methods — or from hand-written IO bindings. Until then, `fs`/`net` fall
  back to `unsupported`. `alloc`/`stdio`/`abort` need none of that.
- **Unwinding interaction:** the PAL with `panic=abort` is fine for early milestones. `panic=unwind`
  needs the §3 throw-bridge wired — a *shared* prerequisite for both the PAL and general correctness,
  not PAL-specific.

**Why the PAL is the right next vertical:** it is bounded and high-value (delivers the "real
shippable .NET `std`" / H2 goal and kills the brittle surrogate), and it is a **forcing function**
that exercises and hardens exactly the interop paths (alloc/stdio/thread/fs/env/time) a broader
BCL-binding effort would also need — while steering clear of the hard `.NET→Rust` direction and the
§7 true-wall features. Empirically the gap is small: `core`+`alloc` compile clean for
`os=dotnet`; `std` needs `dotnet` arms on ~5 `sys/*` cascades (alloc, stdio, random, thread_local,
pal-error) that lack an `unsupported` fallback.

## 9. Feasibility backlog (prioritized)

**Feasible & high-value (mostly mechanical):**
1. **The `dotnet` PAL** (§8) — bounded; replaces the surrogate; hardens the interop substrate. *Next.*
2. **Extend the binding generator to emit *methods*** — turns 1,075 methodless types into a real
   callable BCL API. The single biggest lever for Rust→.NET breadth (and unlocks `std::fs`/`net`).
3. **Implement marshalling traits both ways** (String↔&str, slice↔array, Option/Result, structs);
   lift the 0–3 arg cap to N.
4. **Close codegen pockets:** real SIMD (→ `Vector128`/intrinsics), f16/f128 casts, atomics edges
   (incl. the real 1-byte CAS — §10), the real 128-bit `type_id`. (float→int saturation: done, WF-1.)
5. **Generic-interop bridge** (new — from the §7 interrogation): a `RustGeneric<T>` ↔ Rust wrapper,
   *size-parameterized* for `T: unmanaged` (near-zero-cost) + *boxed/`GCHandle`* for managed `T`, so C#
   can use Rust generic containers with C# types. The key lever for pushing direction-2 (.NET→Rust)
   toward seamless across the one true generics wall.

**Hard but buildable:** async/coroutines (`Yield`/resume as an explicit state-machine driver + a .NET
`Task` adapter — §7, *not* a fundamental limit); revive the comptime interpreter + wire
`AssemblyUtilis` (Cecil) to define .NET classes in Rust and reverse-export Rust functions; the real
unwinding throw-bridge (managed-frames-only on Unix — §7).

**Out of scope / non-goals:** Java/JS exporters, AOT, comptime-as-shipped, and the three *true walls*
in §7 (zero-cost managed-ref-overlapping open generics, static cross-seam borrow-safety, arbitrary
novel inline asm). proc-macros are a non-issue (host-time), not a non-goal.

## 10. Concrete bugs surfaced by the audit (WF-1 status)
**Fixed (WF-1, gated no-regression):**
- ✅ `float → int` `as`: `NaN` mapped to int `MAX` instead of `0` — the saturating builtin's
  overflow branch used `bge.un`, which NaN satisfies. (Finite saturation was already correct; the
  original "non-saturating" framing was imprecise.) Fixed with a `Ne(arg,arg)` NaN→0 guard in
  `cilly/src/ir/builtins/casts.rs`.
- ✅ Two divergent ABI checkers: `src/function_sig.rs::sig_from_instance_` now delegates to
  `CallInfo::sig_from_instance_` (single source of truth) — kills the latent `panic!` on the
  fn-ptr-reify / drop / interop paths.
- ✅ `"System.Objetc"` typo → `"System.Object"` (`mycorrhiza/src/class.rs`); the IL exporter
  special-cases the exact name `System.Object`, so the typo broke that path.
- ✅ Duplicate `_Unwind_DeleteException` registration removed in the linker.

**Still open (deferred to later workflows, with sharpened specs):**
- ✅ **1-byte (and `i8`/`u16`/`i16`) atomic `cxchg` — FIXED (WF-5).** The old `Type::Int(Int::U8) =>
  comparand` shortcut (always-success, no write) is replaced by dedicated comparand-checked builtins
  `atomic_cmpxchng{8,16}_correct` (`cilly/src/ir/builtins/atomics.rs::emulate_subword_cmp_xchng`):
  a masked 32-bit `Interlocked.CompareExchange` loop that reads the containing word, extracts the
  target sub-word, and **bails without writing if it != comparand**, splicing+CASing only on a match
  and retrying solely on other-byte contention; it returns the genuine old sub-word so
  `cxchng_res_val`'s `old == expected` is exact. The loop-internal `cmpxchng{8,16}_i32` builtins (the
  *unconditional* splice that WF-1 wrongly tried to reuse) are left untouched — they remain correct
  inside the re-reading RMW loop in `generate_atomic`. Sub-word `atomic_xchg` (`i8`/`u16`/`i16`),
  previously `todo!`, now uses `atomic_xchng{8,16}_correct` (an unconditional-splice CAS loop —
  genuinely atomic, unlike the plain volatile load/store the `u8` path still uses). LE-only +
  page-boundary caveats documented on the builtins; retire for .NET 9's native sub-word overload.
- ⛔ `#[track_caller]` location assembled from uninitialized memory (`call.rs:576`) — benign under
  panic=abort; **fix as part of WF-6 (unwinding)**, where it actually gets read.
- 🔶 The typechecker is off-by-default and non-fatal. Triage verdict: it **can** become a staged hard
  gate via reviving `TYPECHECK_CIL`; most errors are real (`FieldOwnerMismatch`, `CallArgTypeWrong`),
  with benign ref-vs-ptr-store `FieldAssignWrongType` noise to clear first; ~days of effort to reach
  a clean `::stable`. Candidate for its own workflow.

## 11. North-star benchmark & full workflow roadmap

**Benchmark (the capability yardstick):** a real, dependency-using Rust module living *in* a .NET
solution (concretely: `monark/primary-offerings`) that C# **imports and calls like a normal library** —
correct answers, `std` working inside, a clean public API with IntelliSense. This one test exercises
*every* capability layer at once, so **"the benchmark passes" ≈ "the translation layer works."** It is
fundamentally the `.NET→Rust` direction sitting on the whole stack.

The benchmark decomposes into 6 layers → workflows:

| Layer | Workflow(s) |
|---|---|
| 1 correct codegen | WF-1 (done), WF-5 |
| 2 `std` runs on .NET | WF-2, WF-4 |
| 3 Rust→.NET calls | WF-3 |
| 4 errors/panics cross cleanly | WF-6 |
| 5 .NET→Rust export (call Rust from C#) | **WF-7 — the linchpin** (the ~5%-dead direction) |
| 6 ergonomic packaged library | **WF-8** (new) |

**Roadmap** (run order; critical path to the benchmark is 1→2→3→7→8):
- **WF-1** Correctness foundation — **DONE** (`50a6b39`).
- **WF-2** dotnet PAL core — *in progress.*
- **WF-3** BCL binding generator (emit methods) + marshalling.
- **WF-4** PAL flesh-out + retire surrogate.
- **WF-5** codegen pockets — SIMD, f16/f128, atomics (incl. the real 1-byte CAS), `type_id`, coroutines.
- **WF-6** unwinding throw-bridge (managed-frames-only on Unix — §7).
- **WF-7** `.NET→Rust` direction — reverse-export (`[UnmanagedCallersOnly]`-style) + `dotnet_typedef!`.
  *The linchpin: the benchmark is impossible without it, and it is the hardest, ceiling-adjacent piece.*
- **WF-8 (new)** Library packaging & ergonomic surface — emit a .NET **class library** (not an
  exe-entrypoint) with **de-mangled** public types/methods/namespaces + **bidirectional marshalling for
  real API signatures** (`Result`→exception/`out`, `Option`→nullable, `Vec`/slice↔array/`Span`,
  struct↔record, `String`↔`string`) + NuGet/`.csproj` packaging. This is what makes a Rust crate
  *importable*. (The cargo↔MSBuild build glue is separable tooling, not codegen.)
- **WF-9 (new)** Generic-interop bridge — `RustGeneric<T>` ↔ C# (size-parameterized for `T: unmanaged`,
  boxed/`GCHandle` for managed T; §7). Needed iff the module's public API uses generic containers.
- **WF-10 (new, open-ended)** Real-crate soak / hardening — drive an actual dependency-using crate
  end-to-end and close the long tail. Where "experimental" becomes "usable."

**Status vs. the benchmark:** WF-1…7 are the deep capability work (**~80%**, WF-7 the hardest). +WF-8
reaches a *trivial* importable module; +WF-9 (if generic) + WF-10 (soak) reach a *real* one. **≈10
workflows total, ~3 beyond the original 7** — and those 3 are mostly ergonomics/packaging/hardening,
except WF-7 which is both linchpin and hardest.
