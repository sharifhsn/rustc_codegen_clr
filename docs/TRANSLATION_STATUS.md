# Rust ↔ .NET translation layer — completeness & feasibility map

> Status snapshot as of 2026-06 (nightly-2026-06-17, post the V1→V2 IR collapse, the
> allocator-ABI fix, the **`dotnet` PAL**, the **complete-std** pass, the **"go big"** full-BCL
> binding generation, and the **WF-6 unwinding throw-bridge**). Compiled from a code audit of the whole backend cross-checked against
> FractalFir's design articles (`docs/ARCHITECTURE.md`, `docs/fractalfir_articles/`). Intended as a
> planning reference: what exists, what's partial, what's missing-but-feasible, and what is
> fundamentally limited. File:function pointers are given so each claim is checkable.

## The one-sentence picture

The **codegen** (Rust → CIL) is mature and largely faithful — ~90–96% of `core`/`alloc`/`std` test
suites pass. As of the latest milestones the **Rust→.NET half is substantially complete**: a real
`dotnet` PAL runs `std` on .NET with no surrogate (for the vertical it covers), and the full BCL call
surface is generated (4,256 method/ctor wrappers). And the **`.NET→Rust` direction now works at its
core** (WF-7): C# imports a Rust *library* and calls its functions (incl. string marshalling), and a
Rust `dotnet_typedef!` declaration emits a real managed class. What remains is the **ergonomic tail**
(constructors, richer marshalling, de-mangled/typed API, NuGet packaging) — not new capability walls.
This matches the author's own framing: *the translation is the validated, even elegant part; the interop
ergonomics and platform integration are where the hard work remains* (`docs/fractalfir_articles/`,
`v0_1_0`/`v0_2_0`).

Mental model of the layers (each builds on the one above):

```
  codegen core      Rust MIR → CIL: types, ops, calls, dispatch         ~90% — mature
  interop substrate Rust → .NET BCL call mechanism (magic fns / extern) mechanism done
  Rust → .NET API   generated wrappers over the BCL (all bindable types)  done — 4256 methods
  runtime/std       allocator, stdio, threads, fs … on .NET             real dotnet PAL (vertical); surrogate retiring
  ── consumers ──
  .NET → Rust       call Rust fns + define managed classes (WF-7)       core works; ergonomic tail left
  packaged library  class-lib output + de-mangled API + marshalling      partial (WF-8): lib .dll emits; naming/NuGet left
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
- **`float → int` `as` — FIXED (WF-1).** Finite saturation was already correct; the one real bug was
  `NaN → MAX` (overflow branch used `bge.un`, which NaN satisfies). Now guarded `NaN → 0` in
  `cilly/src/ir/builtins/casts.rs`. See §10.
- **f16 float-to-float `as` — FIXED (WF-5)** via `System.Half::op_Explicit`
  (`cilly/src/ir/builtins/f16/mod.rs`). **f128** float-to-float still `panic!`s (`rvalue.rs:238`) and
  `int → f128` is still `todo!`.

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
the drop-glue + interop paths and can `panic!` where the modern `CallInfo` succeeds; vtable layout
`[drop,size,align,…]` is baked in. (`#[track_caller]` panic-location was filled from uninitialized
memory; **fixed in WF-6** — now the real caller file/line/col, see §10.)

**Unwinding — DONE (WF-6, managed-frames model):** the catch side (handler regions → real .NET
`try/catch`, `resolve_exception_handlers`, `block_gc`, `RustException`) was already built; **WF-6 wired
the throw side** (`ir/builtins/unwind.rs::raise_exception` overrides `_Unwind_RaiseException` to throw a
`RustException` carrying the panic payload). A Rust `panic!` now propagates as a managed exception and
`std::panic::catch_unwind` catches it end-to-end on .NET (validated by `cargo_tests/catch_panic`). This
is the §7 managed-frames-only design: never crossing a P/Invoke boundary. `NO_UNWIND` strips handlers
for perf. No DWARF personality fn is needed (the CLR runs the handlers). C-mode unwinding is still a
non-compiling sketch (gated on an undefined `UNWIND_SUPPORTED`, with `setjump`/`longjump` typos).

**Missing/hard terminators:** `InlineAsm` (throwing stub), `TailCall`/`Yield`/`CoroutineDrop`
(`todo!`). `UnwindTerminate` now hard-aborts via `Environment.FailFast` (WF-6; was incorrectly
re-throwing/continuing the unwind). Residual: `UnwindAction::Terminate` routing in `basic_block.rs`
still returns `None` (needs a synthesized abort handler block).

## 4. Runtime / `std` support
`cilly/src/libc_fns.rs`, `cilly/src/bin/linker/main.rs`, `cilly/src/ir/builtins/`.

**Solid (.NET path), via the linker's `MissingMethodPatcher`:** heap → `NativeMemory.AlignedAlloc`
(malloc/realloc/free → `Marshal.*HGlobal`), EH → `RustException` try/catch, atomics → `Interlocked`,
math, and a **full pthread mapping** (`builtins/thread.rs`, 701 LOC).

**The "surrogate libc" is host-libc *delegation*** — 610 functions in `LIBC_FNS` are PInvoke'd to the
host's real libc/libm/libgcc (f128 support "not portable at all", Linux/GNU only). So the artifact is
**not self-contained** and is x86_64-Linux-bound. **This is exactly what the `dotnet` PAL replaces —
see §8.** WF-2 (`9d042ef`) landed the real PAL for the alloc/stdio/RNG/time/thread vertical (`std` runs
on .NET with no surrogate there); fs/net and full surrogate retirement remain (WF-4).

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

**Rust → .NET (calling the BCL): substantially done.** Const-generic "magic functions" encode .NET
call metadata; 9 backend handlers (static/instance/virtual/ctor/cast/is_inst/null/ld_len/try_catch)
are **complete and symmetric**. `GCHandle`-based ref-holding (`mycorrhiza/src/class.rs`) is sound.
**The API surface is now generated at full BCL scale ("go big", `67157cd`):** `bindings.rs` is the
self-hosted output of `spinacz` (compiled with this backend, run on .NET, reflecting the BCL via the
magic-fns) — **4,256 method/ctor wrappers (528 ctors) across 869 inherent-impl blocks + 988 type
aliases + 881 `From`-casts (11,626 lines)**, forwarding to the `staticN`/`instanceN`/`virtN`/`ctorN`
helpers. Marshalling is *solved by codegen* (the emitted CIL sig is the monomorphized Rust generic
sig), so the generator only needs name + static/instance/virtual flag + a mappable Rust type per
param/return; genuinely-unmappable signatures (generic methods, `ref`/`out`, raw pointers, varargs)
are dropped pending the WF-9 generic bridge. Verified: `cargo_tests/interop_method_sample` calls 13
real BCL methods from Rust on .NET matching native. (Backend handles N args; the old 0–3 wrapper cap
is gone for generated methods.)

**\.NET → Rust (calling Rust functions): WORKING (WF-7 P1).** The key reframe: because Rust compiles
to *managed* CIL, calling Rust from C# is **not** native FFI — a Rust fn is already a `public static`
method on the `MainModule` class. WF-7 made a Rust **library** crate (`crate-type=["cdylib"]`) emit a
real **.NET class-library assembly** (named after the crate, no entrypoint), so C# references it and
calls its `#[no_mangle]` functions as ordinary managed methods. Proven end-to-end:
`cargo_tests/rust_export` (a Rust lib) + `cargo_tests/rust_export_cs` (a C# program) — C# calls
`rust_add`/`rust_mul`/`rust_fib`/`rust_add_f64` on .NET, all correct. The enabling backend changes:
- `#[no_mangle]` → `Access::Extern` (`src/assembly.rs`), making exports **dead-code-elimination roots**
  — essential, since a library has no entrypoint to root the call graph (without it the whole API is
  eliminated).
- Library output (`cilly/src/ir/il_exporter/mod.rs` + `bin/linker/main.rs`): for `is_lib`, the .NET
  assembly is written to the requested `-o` path (was hard-coded `<stem>.exe`) and **no native launcher**
  is built (a library isn't launched). Previously a `dylib`/`cdylib` produced a native ELF, not a .NET
  assembly.
- Assembly naming (`il_exporter` `.assembly` directive): named after the crate (was the placeholder `_`)
  so C# can reference it by identity. (A library having no `main`/`lang_start` also sidesteps the std
  runtime weak-static tail — see §10.)

**\.NET → Rust (exposing Rust *types* as managed classes): core WORKING (WF-7 P3).** The comptime
interpreter (`src/comptime.rs`) is **revived** (was a dead `todo!` over ~200 lines of drifted code). A
`dotnet_typedef!` declaration now produces a real managed class: `cargo_tests/rust_typedef` emits
`.class public RustObj extends [System.Runtime]System.Object { .field int32 value; .method public
virtual int32 get_value() }` (verified by `ikdasm`), where the virtual method **aliases** an ordinary,
separately-codegen'd Rust fn (`MethodImpl::AliasFor`). Mechanism: the interpreter reads the MIR of the
macro-generated `…_comptime_entrypoint` (whose four magic intrinsic calls carry the class metadata as
const-generics) and registers a `ClassDef` as a side effect. Two backend fixes enabled it — the
method-body fn (`…_not_magic`, declared *inside* the entrypoint) now falls through to normal codegen
(`src/assembly.rs`), and the dead-code pass follows `AliasFor` edges (`cilly/src/ir/asm.rs`); the
emitted methods are `Access::Extern` (DCE roots). The Cecil-based `AssemblyUtilis` backend remains
unwired (an alternate, unneeded emission path). **Follow-ups for full C# *use*:** `dotnet_typedef!`
emits no constructor (the type loads + reflects, but C# can't `new` it yet); a virtual method returning
a managed `System.String` hits the P2 managed-return codegen bug; generic Rust types → §7 limits.
**\.NET → Rust (string marshalling): WORKING (WF-7 P2).** Strings cross as UTF-8 `(ptr, len)` pairs
(thin pointers → directly-C#-usable `byte*`/`nuint`). `rust_strlen(*const u8, usize)` proves inbound
(C# `string` → Rust `&str`); `greet(*const u8, usize, *mut u8, usize)` proves outbound — it builds an
owned Rust `String` and copies its UTF-8 into a caller-provided out-buffer (nothing crosses ownership →
no cross-boundary free). Proven in `cargo_tests/rust_export` + `_cs` (no backend changes). Two deferred
idiomaticity follow-ups: (a) returning a managed `System.String` directly hits a codegen mismatch — the
interop-call result is typed `void` when returned from an exported fn (`LocalAssigementWrong got "v"`),
so the out-buffer convention is used instead; (b) direct *typed* C# calls (vs reflection) hit `CS0012`
because the assembly's BCL references carry version `0.0.0.0` — emitting proper reference versions is
WF-8 packaging. Still open: `Vec`/slice/struct marshalling (the `(ptr,len)` convention generalizes).

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

The PAL (`std::sys::pal::dotnet`, the H2 effort) is **not a separate track — it is the first real,
bounded *vertical* of this translation layer applied to `std`.** **WF-2 (`9d042ef`) built it** for the
alloc/stdio/RNG/time/thread arms; the framing below is how it fits, with that vertical now landed.
Concretely:

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
- **Where the PAL meets the interop-completeness gap:** `fs`/networking. `System.IO` types are now
  *method-bearing* (go-big, §6) — so the binding-generator prerequisite for a real `std::fs` is met;
  what remains is wiring the PAL `fs`/`net` arms to those bindings (WF-4). Until then, `fs`/`net` fall
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

**Fixed (complete-std `dad047d` + go-big `67157cd`) — the `cast_ptr` over-pointering family.** One
root cause produced three separate "frontier" miscompiles: `Asm::cast_ptr(addr, tpe)` builds
`Ptr(tpe)` (it treats `tpe` as the *pointee*), so passing an already-pointer type yields `Ptr(Ptr(..))`.
The sibling `cast_ptr_to` (takes the full pointer type) is the correct helper. Sites fixed:
- ✅ **Fat-ptr `DATA_PTR` double-indirection** (`src/aggregate.rs`, `AggregateKind::RawPtr`): stored
  `**void` into the `*void` `DATA_PTR` field of *every* slice/str fat pointer → **2,513
  `FieldAssignWrongType` → 0**. This was the long-standing std-compile blocker.
- ✅ **`CantCompareTypes` std-glue** (`get_environ`/argv/`mstring_to_utf8ptr` in `cilly/src/utilis.rs`)
  — over-pointering on the environment/argv path; blocked `lang_start`/panic glue.
- ✅ **`calli` fn-ptr typing** (`src/terminator/call.rs`+`mod.rs`): the virtual-dispatch + drop-glue
  fn-ptr load passed `Ptr(FnPtr)` to `cast_ptr` → `Ptr(Ptr(FnPtr))` → `LdInd{tpe:FnPtr}` off a data
  `Ptr` → `DerfWrongPtr` → `BadImageFormatException`. This was the one bug gating `spinacz` method
  emission; fixing it (pass the bare `FnPtr(sig)`) unlocked "go big".
- ✅ **`callvirt` for ref-type instance receivers** (`call_managed`): abstract slots like
  `GetParameters` need `callvirt`, not `call instance`; value-type receivers still use `call`.

**Fixed (WF-7 P1 — Rust library → .NET assembly).**
- ✅ **Library crate-type emits a .NET assembly** (was native ELF). For `is_lib`, `ILExporter::export`
  writes the assembly to the requested `-o` path (not `<stem>.exe`) and the linker builds **no native
  launcher** (`cilly/src/ir/il_exporter/mod.rs`, `bin/linker/main.rs`). Assembly named after the crate
  (was `_`). `#[no_mangle]` → `Access::Extern` (`src/assembly.rs`) so exports are DCE **roots** (a
  library has no entrypoint root). See §6.
- ✅ **`gettid` weak static** (`rustc_codgen_clr_operand/src/constant.rs::get_fn_from_static_name`): was
  an unsupported `todo!`; added an arm + `LIBC_FNS` entry (PInvoke to host libc), like `pidfd_getpid`.
  NOTE: a tail of sibling `import_linkage` weak statics remains (`posix_spawn_file_actions_addchdir`,
  …); for a **library** these are unreachable from the export roots, so the per-method panic-recovery
  skips + DCE removes them (the lib still builds). For a **bin**, `lang_start` makes them reachable →
  fatal. General fix (derive sig from `def_id` + libc resolution) is a future std-codegen hardening.

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
- ✅ **`#[track_caller]` location — FIXED (WF-6).** Was assembled from uninitialized memory
  (`call.rs`); now materializes the real caller file/line/col via `span_as_caller_location` +
  `load_const_value` (the `caller_location` intrinsic's own path). Needed once unwinding lands, since
  the location is read when a track_caller callee panics.
- ✅ **Throw-bridge + `UnwindTerminate` — FIXED (WF-6).** `ir/builtins/unwind.rs::raise_exception`
  overrides `_Unwind_RaiseException` to throw a `RustException` wrapping the panic payload, so
  `catch_unwind` catches Rust panics end-to-end (`cargo_tests/catch_panic`). `UnwindTerminate` now
  hard-aborts via `Environment.FailFast` instead of re-throwing. Residual: `UnwindAction::Terminate`
  routing (`basic_block.rs`) still `None`; a non-fatal `CallArgTypeWrong` typecheck warning on the
  `catch_unwind` glue (try-fn data ptr typed as the concrete closure `Data*` vs `u8*`).
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

| Layer | Workflow(s) | Status |
|---|---|---|
| 1 correct codegen | WF-1, WF-5 | ✅ **DONE** |
| 2 `std` runs on .NET | WF-2, WF-4 | ✅ **DONE** (PAL vertical; surrogate retiring) |
| 3 Rust→.NET calls | WF-3 | ✅ **DONE** (full BCL, 4256 methods) |
| 4 errors/panics cross cleanly | WF-6 | ✅ **DONE** (throw-bridge; catch_unwind works) |
| 5 .NET→Rust export (call Rust from C#) | **WF-7** | 🟢 **P1+P2+P3 core DONE** (C# calls a Rust *library*; string marshalling; Rust *defines* managed classes); ergonomic tail remains |
| 6 ergonomic packaged library | **WF-8** | ⬜ |

**Layers 1–4 done + WF-7 P1** — the entire Rust→.NET half, error-crossing, AND the core of the reverse
direction (C# imports a Rust library and calls its functions, §6). What remains: WF-7 P2 (marshalling
`&str`/`String`/`Vec`/struct) + P3 (Rust *types* as managed classes, the comptime revival) + packaging
(WF-8).

**Roadmap** (run order; critical path to the benchmark is 1→2→3→7→8):
- **WF-1** Correctness foundation — **DONE** (`50a6b39`).
- **WF-2** dotnet PAL core — **DONE** (`9d042ef`): real `std::sys::pal::dotnet` (alloc→`NativeMemory`,
  stdio→`Console`, RNG/time/thread arms) via `rcl_dotnet_*` extern hooks; `std` runs on .NET, no
  surrogate for the covered vertical.
- **WF-3** BCL binding generator — **DONE** (complete-std `dad047d` + go-big `67157cd`): `spinacz`
  self-hosts and emits the full method-bearing `bindings.rs` (4256 wrappers); marshalling solved by
  codegen. See §6.
- **WF-4** PAL flesh-out + retire surrogate — *partial* (RNG/time/thread arms landed in WF-2; fs/net
  and full surrogate retirement remain).
- **WF-5** codegen pockets — **DONE** (`f3cd172`) for atomics (sub-word CAS) + f16 casts; SIMD, f128,
  `type_id` 128-bit, coroutines still open (see §2).
- **WF-6** unwinding throw-bridge — **DONE** (`22b0a00`): `_Unwind_RaiseException` throws a
  `RustException`, `catch_unwind` catches Rust panics end-to-end on .NET (`cargo_tests/catch_panic`);
  `UnwindTerminate`→`FailFast`; real `#[track_caller]` location. Managed-frames-only per §7.
- **WF-7** `.NET→Rust` direction — *the linchpin.* **P1+P2+P3-core DONE.** P1 (`rust_export[_cs]`): a
  Rust **library** crate compiles to a referenceable .NET class-library assembly, C# calls its
  `#[no_mangle]` functions as managed methods. P2: **string marshalling** both ways (UTF-8 `(ptr,len)`).
  **P3 (`rust_typedef`): `dotnet_typedef!` + the revived `src/comptime.rs` now make a Rust declaration
  emit a real managed class** (field + inheritance + virtual method aliasing a Rust fn — verified by
  `ikdasm`, §6). The reframe — Rust→managed-CIL means this is *not* native FFI — collapsed most of the
  expected difficulty; even the "ceiling-adjacent" type-export works for the core case. **Remaining
  (ergonomic tail):** P2 `Vec`/slice/struct marshalling; P3 constructors (so C# can `new` a Rust class)
  + managed-`String`-return; managed `System.String` return codegen bug; direct typed C# refs (BCL-ref
  versions). Generic Rust types → §7 limits.
- **WF-8** Library packaging & ergonomic surface — emit a .NET **class library** (not an
  exe-entrypoint) with **de-mangled** public types/methods/namespaces + **bidirectional marshalling for
  real API signatures** (`Result`→exception/`out`, `Option`→nullable, `Vec`/slice↔array/`Span`,
  struct↔record, `String`↔`string`) + NuGet/`.csproj` packaging. This is what makes a Rust crate
  *importable*. (The cargo↔MSBuild build glue is separable tooling, not codegen.)
- **WF-9** Generic-interop bridge — `RustGeneric<T>` ↔ C# (size-parameterized for `T: unmanaged`,
  boxed/`GCHandle` for managed T; §7). Needed iff the module's public API uses generic containers.
- **WF-10 (open-ended)** Real-crate soak / hardening — drive an actual dependency-using crate
  end-to-end and close the long tail. Where "experimental" becomes "usable."

**Status vs. the benchmark:** the capability work is **~85% done** — layers 1–4 (WF-1/2/3/5/6) complete
*and* WF-7 P1+P2+P3-core (C# calls a Rust library; string marshalling; Rust defines managed classes).
**All five capability layers now have a working core.** Remaining is mostly the **ergonomic tail**:
WF-7 finish (constructors, `Vec`/struct marshalling, managed-`String` return, direct typed C# refs) +
WF-8 (packaging — de-mangled API, BCL-ref versions, NuGet), then WF-9 (if the module's API is generic)
+ WF-10 (soak) for a *real* module. The hard, ceiling-adjacent pieces are behind us.
