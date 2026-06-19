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

**Fundamental ceilings:** generics-with-explicit-layout (see §7), ZSTs, GC can't disambiguate an
overlapping managed-ref vs raw-pointer field.

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
- **Atomics partial + a deliberate bug:** sub-word `xchg` is `todo!` (`atomic.rs:66`); **1-byte
  `cxchg` intentionally returns the wrong value** (`atomic.rs:127`, stale "remove after .NET 9"
  comment); `atomic_store` is a plain store.
- **`float → int` `as` cast is wrapping, not saturating** (`casts.rs:176` via `rvalue.rs:106`) — a
  latent **correctness miscompile** for out-of-range/NaN (Rust `as` saturates).
- f16/f128 float-to-float `as` casts `panic!` (`rvalue.rs:238`); `int → f128` cast `todo!`.

**Fundamental:** SIMD layout/semantics don't line up with .NET vectors; sub-word/weak atomics (no CLI
primitive pre-.NET 9); `type_id` is approximated via `GetHashCode` (collision-prone vs Rust's 128-bit
id).

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

## 7. Fundamental limits (cannot fully preserve semantics)

These are architectural, not "unfinished" — more code won't erase them:

1. **Rust generics can't be instantiated from .NET.** The GC bans `LayoutKind.Explicit` on generics
   (can't disambiguate overlapping ref/ptr fields), but Rust enums/unions require explicit layout. So
   each monomorphization becomes a distinct mangled class; you cannot call `Vec3<T>` with a new `T`
   from C#. The author: *"my problem is not strictly a technical one… I can't do anything about it."*
2. **ZSTs** — no .NET type is < 1 byte; collapsed to `Void`, with documented clobber hazards.
3. **Ownership/lifetimes vs GC** — a managed ref must be GC-pinned (`GCHandle`) to be held; the type
   system can't enforce this (`StackOnly` is advisory).
4. **async/coroutines, inline asm, foreign/cross-language unwinding, `TailCall`** — no clean CIL
   mapping.
5. **proc-macros** — deliberately unsupported (cost > value, per `QUICKSTART.md`).

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
- **The fundamental limits (§7) do NOT block the PAL.** Generics-not-instantiable-from-.NET is a
  *.NET→Rust* (direction-2) problem; the PAL is Rust→.NET only and exposes no Rust generics to managed
  callers. ZSTs/ownership-vs-GC are codegen concerns already handled. The PAL sits squarely in the
  "feasible" zone.
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
fundamental-limit features. Empirically the gap is small: `core`+`alloc` compile clean for
`os=dotnet`; `std` needs `dotnet` arms on ~5 `sys/*` cascades (alloc, stdio, random, thread_local,
pal-error) that lack an `unsupported` fallback.

## 9. Feasibility backlog (prioritized)

**Feasible & high-value (mostly mechanical):**
1. **The `dotnet` PAL** (§8) — bounded; replaces the surrogate; hardens the interop substrate. *Next.*
2. **Extend the binding generator to emit *methods*** — turns 1,075 methodless types into a real
   callable BCL API. The single biggest lever for Rust→.NET breadth (and unlocks `std::fs`/`net`).
3. **Implement marshalling traits both ways** (String↔&str, slice↔array, Option/Result, structs);
   lift the 0–3 arg cap to N.
4. **Close codegen pockets:** real SIMD (→ `Vector128`/intrinsics), f16/f128 casts, atomics edges,
   **float→int saturation**, coroutines.

**Hard but buildable:** revive the comptime interpreter + wire `AssemblyUtilis` (Cecil) → define .NET
classes in Rust and reverse-export Rust functions; the real unwinding throw-bridge.

**Out of scope / non-goals (treat as such):** Java/JS exporters, AOT, comptime-as-shipped,
proc-macros, and the §7 fundamental limits.

## 10. Concrete bugs surfaced by the audit
(Independent of feature work — worth fixing/flagging.)
- `float → int` `as` is non-saturating — a real miscompile (`casts.rs:176`).
- 1-byte `cxchg` returns the comparand instead of exchanging (`atomic.rs:127`); stale ".NET 9" comment.
- Two divergent ABI checkers — `function_sig.rs` (strict) vs `CallInfo` (permissive); latent `panic!`
  on drop/interop paths.
- `#[track_caller]` location assembled from uninitialized memory (`call.rs:576`).
- `"System.Objetc"` typo in `mycorrhiza/src/class.rs:37`.
- `_Unwind_DeleteException` registered twice in the linker.
- The typechecker is off-by-default and non-fatal → miscompilations aren't caught at link time.
