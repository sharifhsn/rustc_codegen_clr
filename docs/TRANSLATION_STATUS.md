# Rust ↔ .NET translation layer — completeness & feasibility map

> Status snapshot as of 2026-06 (nightly-2026-06-17, post the V1→V2 IR collapse, the
> allocator-ABI fix, the **`dotnet` PAL**, the **complete-std** pass, the **"go big"** full-BCL
> binding generation, the **WF-6 unwinding throw-bridge**, the **libc/POSIX shim + `target-family=["unix"]`
> flip**, **full `std::os::unix`**, **async/tokio on the PAL**, and the **one-command `cargo dotnet` DX**).
> Compiled from a code audit of the whole backend cross-checked against
> FractalFir's design articles (`docs/ARCHITECTURE.md`, `docs/fractalfir_articles/`). Intended as a
> planning reference: what exists, what's partial, what's missing-but-feasible, and what is
> fundamentally limited. File:function pointers are given so each claim is checkable.
>
> **New-user entry point:** [docs/CARGO_DOTNET.md](CARGO_DOTNET.md) — compile arbitrary Rust to .NET in
> one command (`cargo dotnet build|run`), and consume a Rust library from C#. This status doc is the
> deep map; CARGO_DOTNET.md is the how-to.

## The one-sentence picture

The **codegen** (Rust → CIL) is mature and largely faithful — ~90–96% of `core`/`alloc`/`std` test
suites pass. The **Rust→.NET half is complete end-to-end**: a real `dotnet` PAL runs `std` on .NET with
no surrogate (files, net, threads, time, **`panic=unwind`**, **async/await + tokio**, **full
`std::os::unix`** under the `target-family=["unix"]` flip + a libc/POSIX shim), the full BCL call surface
is generated (4,256 method/ctor wrappers), and an arbitrary crate compiles + runs with **zero hand-config**
via the one-command **`cargo dotnet`** DX (with auto-applied crate overlays for syscall-using deps). The
**`.NET→Rust` direction works at its core** (WF-7): C# imports a Rust *library* and calls its functions
on the **real dotnet PAL** through `cargo dotnet` (primitives, UTF-8 strings, de-mangled `#[repr(C)]`
structs, slices), and a Rust `dotnet_typedef!` declaration emits a real managed class. **Soak: ~74 real
crates, 73/74 pass** on the PAL (the one non-pass, `regex`, is a deep allocator issue). What remains is
the **ergonomic tail** of `.NET→Rust` (managed-`String`/`Result` through the real-PAL flow, NuGet
packaging) — not new capability walls. This matches the author's own framing: *the translation is the
validated, even elegant part; the interop ergonomics and platform integration are where the hard work
remains* (`docs/fractalfir_articles/`, `v0_1_0`/`v0_2_0`).

Mental model of the layers (each builds on the one above):

```
  codegen core      Rust MIR → CIL: types, ops, calls, dispatch         ~90% — mature
  interop substrate Rust → .NET BCL call mechanism (magic fns / extern) mechanism done
  Rust → .NET API   generated wrappers over the BCL (all bindable types)  done — 4256 methods
  runtime/std       alloc, stdio, threads, fs, net, async, os::unix      real dotnet PAL (no surrogate); +libc/POSIX shim
  build DX          arbitrary crate → .NET, zero config                 `cargo dotnet build|run` + auto overlays
  ── consumers ──
  .NET → Rust       call Rust fns + define managed classes (WF-7)       core works; ergonomic tail left
  packaged library  class-lib output + de-mangled API + marshalling      J3: real-PAL .dll via `cargo dotnet`, C#-called (Tier-1 marshalling)
```

The Rust→.NET stack is now exercised end-to-end by **four worked journeys** (`cargo_tests/cd_*` + the
north-star), documented for the average user in [docs/CARGO_DOTNET.md](CARGO_DOTNET.md):
- **J1** pure Rust → .NET (`cargo_tests/cd_pure`);
- **J2** syscall-deps → .NET via auto-applied overlays (`cargo_tests/cd_tokio`, a tokio echo);
- **J3** a Rust `cdylib` consumed from C# (`cargo_tests/cd_interop`, all four marshalling categories);
- **J4** the north-star: a **real, dependency-using production library** imported by C# and running its
  logic as .NET CIL, returning the correct result (via a transient FFI wrapper + a read-only cross-repo
  mount, leak-safe).

---

## 1. Type lowering — ~90% complete, faithful
`rustc_codegen_clr_type/src/type.rs` (`get_type`), `…/src/adt.rs`.

**Solid:** all scalars incl. **i128/u128** (→ .NET `Int128`/`UInt128`), `f32`/`f64`, Bool/Char,
tuples/structs/unions, thin & **fat pointers/DSTs** (`{DATA_PTR@0, METADATA@8}`), **ZSTs** (→ `Void`,
intercepted before the match), Never, **Foreign** (kept thin), FnPtr/FnDef, **pattern types**,
closures, and **monomorphized generics via name-mangling**. Enums → `[FieldOffset]` tagged unions is
faithful (Single/Direct/Niche encodings, real `tag_field`/offsets from rustc layout).

**Partial:** **f16/f128** (types lower; f16 casts done, f128 is a genuine .NET platform gap — no quad
float in the BCL, §4; an f128 enum-tag panics at `adt.rs:132`); **SIMD** (64/128/256/512-bit; >512-bit
and odd-width degrade to a fixed array); `dyn` pointee is a single contentless `"Dyn"` class (fine —
identity is in the vtable). Latent: layout offsets > `u16::MAX` are silently clamped (`adt.rs:55`).

**Coroutines/async — work fully for stable async** (`adt.rs:198-228` `coroutine_field_descriptor` is
implemented; the earlier "field access `todo!` at adt.rs:216" claim was stale). Suspend/resume AND
dropping an incomplete `Future` both work, because rustc's `coroutine::StateTransform` pass pre-lowers
`Yield`→(discriminant write + `Return`) and routes coroutine drop through the ordinary `Drop`
terminator before the backend sees MIR — so the backend's `Yield`/`CoroutineDrop` arms are
*unreachable* (now accurate `unreachable!()` assertions, not `todo!`). `CoroutineWitness`/
`CoroutineClosure`/async-closures (unstable) still hit the catch-all.

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
- **SIMD — now a high-coverage subset (was "the single biggest hole").** SIMD types are first-class
  (`Type::SIMDVector` → real BCL `Vector{64,128,256,512}<T>`); the generic-static-call machinery
  was already solved. The **dispatched + builtin-backed** op set now covers core::simd's hot path:
  elementwise `add/sub/mul/div` (div was missing entirely → ICE; now per-lane), bitwise `and/or/xor`,
  shifts `shl/shr` (sign-aware), all comparisons `eq/ne/lt/gt/le/ge` (BCL → correct all-ones masks),
  `splat`, `extract`/`insert`, `cast`/`as`, `neg`/`abs`/`fabs`, `bitmask`, **`select`** (per-lane,
  float-safe via address-select), and **horizontal reductions** `reduce_{add,mul}` (ordered+unordered),
  `reduce_{and,or,xor,min,max}`. Value ops with no clean BCL static (xor/shl/shr/div/cast/select/reduce)
  use a target-agnostic per-lane loop (also serves C mode). Verified by `test/intrinsics/simd.rs`
  (differential vs native). **Still deferred (documented `todo!`/array-fallback):** general
  `simd_shuffle` (const-index immediate; the `x==y` scalar fast-path works), `gather`/`scatter`/
  masked, float transcendentals (`fsqrt`/`floor`/`ceil`/`round`/`trunc`/`fma`), float
  `reduce_min/max` (portable-simd routes these through a *scalar* `f32::max` fold, not the intrinsic),
  `ctlz`/`cttz`/`ctpop`/`bswap` in SIMD form, and >512-bit/odd-width vectors.
- **Atomics:** sub-word `cxchg`/`xchg` for `u8`/`i8`/`u16`/`i16` correct via masked-CAS-loop builtins
  (WF-5 — see §10); **`atomic_store` now emits a *volatile* store** (release semantics; was a plain
  store). Residual: a sub-word-atomic page-boundary hazard (`emulate_subword_cmp_xchng` aligns the
  byte address down to its 32-bit word, which can touch up-to-3 unowned bytes when the atomic is at
  the start of an allocation/static-storage region → AccessViolation; only bites the *default* panic
  hook's `get_backtrace_style` static `Atomic<u8>`). The safe fix (lay sub-word atomic statics
  4-byte-aligned/wide) is **deferred**: it changes static-field layout and the root cause has an
  unresolved managed-vs-unmanaged-pointer question — high risk for a narrow edge case (custom panic
  hooks already work, `pal_panic2`).
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
for perf. No DWARF personality fn is *invoked* at runtime (the CLR runs the handlers) — but one must
**exist** so `std` compiles under `panic=unwind`: rustc's front-end `eh_personality` weak-lang-item
check (`rustc_passes`) fires *before* codegen, so a link-time builtin can't satisfy it. **Phase 3
(real dotnet target, `panic-strategy: unwind`) provides it the no-DWARF way** (mirroring
wasm/msvc/uefi): a trivial aborting-stub `rust_eh_personality` lang item is injected into
`std::sys::personality` for `os = "dotnet"`, and the `panic_unwind` + `unwind` crates get
`target_os = "dotnet"` arms (gcc flavour for `imp::panic` → `_Unwind_RaiseException`; `libunwind` for
the `_Unwind_*` declarations). With those three rust-src arms (injected by `feasibility/dev.sh
pal-build`) std builds under unwind and a Rust `panic!` propagates to `catch_unwind`'s managed
try/catch on the real dotnet target — validated by `cargo_tests/pal_panic2` (catch_unwind returns
`is_err=true` then `Some(42)`). `cargo_tests/pal_panic` (default panic hook) still crashes, but only on
the **orthogonal pre-existing sub-word-atomic bug** (WF-5): `get_backtrace_style`'s
`static Atomic<u8>::compare_exchange` faults in `emulate_subword_cmp_xchng` (aligns the 1-byte static
DOWN to a 32-bit word it doesn't own → AccessViolation). That is a panic-hook atomic bug, not the
unwind machinery. C-mode unwinding is still a non-compiling sketch (gated on an undefined
`UNWIND_SUPPORTED`, with `setjump`/`longjump` typos).

**Missing/hard terminators:** `InlineAsm` (throwing stub), `TailCall`/`Yield`/`CoroutineDrop`
(`todo!`). `UnwindTerminate` now hard-aborts via `Environment.FailFast` (WF-6; was incorrectly
re-throwing/continuing the unwind). Residual: `UnwindAction::Terminate` routing in `basic_block.rs`
still returns `None` (needs a synthesized abort handler block).

## 4. Runtime / `std` support
`cilly/src/libc_fns.rs`, `cilly/src/bin/linker/main.rs`, `cilly/src/ir/builtins/`.

**Solid (.NET path), via the linker's `MissingMethodPatcher`:** heap → `NativeMemory.AlignedAlloc`
(malloc/realloc/free → `Marshal.*HGlobal`), EH → `RustException` try/catch, atomics → `Interlocked`,
math, and a **full pthread mapping** (`builtins/thread.rs`, 701 LOC).

**The "surrogate libc" was host-libc *delegation*** — 610 functions in `LIBC_FNS` PInvoke'd to the
host's real libc/libm/libgcc (Linux/GNU only), so that artifact was **not self-contained** and
x86_64-Linux-bound. **The `dotnet` PAL replaces it — see §8 — and now covers the full vertical:** WF-2
(`9d042ef`) landed alloc/stdio/RNG/time/thread; subsequent work added **fs** (`System.IO`), **net**
(TCP/UDP over `System.Net.Sockets` + I/O-driven async/tokio via the mio reactor), **`panic=unwind`**,
and — under the `target-family=["unix"]` flip plus a **libc/POSIX shim** (fd-table + thread-local errno
+ ~20 bare-C-ABI POSIX symbols backed by the existing BCL hooks) — **full `std::os::unix`** (AF_UNIX,
`MetadataExt`, symlinks, pread/pwrite, the fd onion). `std` runs on .NET with **no surrogate**; the
remaining host-libc `LIBC_FNS` entries are unreachable on the dotnet target. The detailed libc map is
[docs/LIBC_SHIM_SCOPE.md](LIBC_SHIM_SCOPE.md); the os::unix plan + leaky-bits ledger is
[docs/STD_OS_UNIX_PLAN.md](STD_OS_UNIX_PLAN.md).

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
`rust_add`/`rust_mul`/`rust_fib`/`rust_add_f64` on .NET, all correct.

**J3 (the real-PAL `cargo dotnet` flow): WORKING for Tier-1 marshalling.** The interop above was first
proven on the **surrogate** target (`x86_64-unknown-linux-gnu`, no `panic_unwind`). J3 brings it onto
the **real dotnet PAL** through the one-command `cargo dotnet` library flow: `cargo dotnet build` detects
a `cdylib`, emits `lib<crate>.so` (a managed PE), and copies it to `<crate>.dll`; a real C# console app
references it and asserts results match Rust. Worked example: `cargo_tests/cd_interop/` (`rustlib/` +
`csharp/`). **Verified on the real PAL (Tier 1):** primitives (`rust_add`), UTF-8 `(ptr, len)` strings
(`greet` out-buffer round-trip), a de-mangled `#[repr(C)]` struct value-type (`cd_interop.Point` with
synthesized ctor/getters, `point_sum`/`make_point`), and an inbound slice/`Vec` sum (`sum_slice`). The
make-or-break assumption held: an I/O-free, panic-free `cdylib` has no entrypoint, so `lang_start` /
weak-static runtime tail is unreachable and DCE'd — the lib emits cleanly pulling only CoreLib extern
refs. **Tier 2 (surrogate-only, not yet through this real-PAL flow):** returning a managed
`System.String` directly (`mycorrhiza::system::MString`/`greet_managed`) and a Rust-raises-a-.NET-
exception `Result` (`rustc_clr_interop_throw`/`try_div`) — both pull `mycorrhiza` + the throw intrinsic.
**Finding (real-PAL Tier-2 blocker):** pushing these through the real `cargo dotnet` flow showed the
*rustlib emits cleanly* (an I/O-free `cdylib` with `MString`/throw builds + DCE-clean on the dotnet PAL
— the feared runtime-tail drag does NOT happen), **but the produced `.dll` is not C#-consumable**: the
`mycorrhiza` intrinsics (`MString` → `System.Private.CoreLib`'s `String`; the throw → `System.Exception`)
attribute the assembly's base types to `System.Private.CoreLib`, so a standard C# project fails with
`CS0012` ("`Object`/`ValueType` defined in `System.Private.CoreLib`, not referenced") on *every* call —
and referencing `System.Private.CoreLib` directly cascades into `CS0433`/`CS0518` (duplicate predefined
types vs the `System.Runtime` ref assembly). The fix is backend-level **reference-assembly attribution**
(emit public base/exception/string identities as the C#-resolvable `System.Runtime`, not the
implementation `System.Private.CoreLib`) — a focused follow-up, not the pure reuse first assumed. Tier-2
therefore remains **surrogate-proven only**; the real-PAL path is blocked on this attribution work.
Consumer guide: `docs/INTEROP_CSHARP.md`. Zero backend code changed for J3 — only `feasibility/` shell
(the `cargo dotnet` lib-artifact branch) + the new probe crate. The enabling backend changes (from WF-7):
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
unwired (an alternate, unneeded emission path). **C# can now `new` the class** (WF-8b): the interpreter
emits a default `.ctor` (a real CIL body chaining to the base ctor), so `new RustObj().get_value()`
returns 42 from C# (`cargo_tests/rust_typedef_cs`). **Remaining follow-ups:** a virtual method returning
a managed `System.String` hits the P2 managed-return codegen bug; fields/ctors with non-primitive types;
generic Rust types → §7 limits.
**\.NET → Rust (string marshalling): WORKING (WF-7 P2).** Strings cross as UTF-8 `(ptr, len)` pairs
(thin pointers → directly-C#-usable `byte*`/`nuint`). `rust_strlen(*const u8, usize)` proves inbound
(C# `string` → Rust `&str`); `greet(*const u8, usize, *mut u8, usize)` proves outbound — it builds an
owned Rust `String` and copies its UTF-8 into a caller-provided out-buffer (nothing crosses ownership →
no cross-boundary free). Proven in `cargo_tests/rust_export` + `_cs`. A Rust fn can also return a
managed `System.String` **directly** (`greet_managed`, WF-8c) — C# gets a `string`; the bug that typed
such returns `void` (a 0-arg managed getter's methodref hardcoded a Void return in
`src/terminator/call.rs`) is fixed. (Direct *typed* C# calls — `MainModule.rust_add(2,3)`, vs
reflection — work since WF-8a emits real BCL-ref versions; see §11.) Still open:
`Vec`/slice/struct marshalling (the `(ptr,len)` convention generalizes).

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
- **Unwinding interaction — `panic=unwind` now works on the real dotnet target (Phase 3).** Flipping
  `x86_64-unknown-dotnet.json` to `panic-strategy: unwind` compiles std and propagates panics to `catch_unwind` end-to-end
  (validated by `cargo_tests/pal_panic2`). The pieces: the §3 throw-bridge (gcc `imp::panic` →
  linker-overridden `_Unwind_RaiseException` → `RustException` throw → managed try/catch), an
  aborting-stub `eh_personality` lang item for `os = "dotnet"` in `std::sys::personality` (no-DWARF
  pattern; the front-end weak-lang-item check needs it to *exist*, the CLR never *calls* it), and
  `target_os = "dotnet"` arms in the `panic_unwind` (→ gcc) and `unwind` (→ libunwind decls) crates —
  all injected into rust-src by `feasibility/dev.sh pal-build`. Residual blocker for the *default* panic
  hook only: `get_backtrace_style`'s `static Atomic<u8>::compare_exchange` hits the orthogonal WF-5
  sub-word-atomic page-hazard (`emulate_subword_cmp_xchng` aligns a 1-byte static down to a word it
  doesn't own → AccessViolation); a custom hook (as in `pal_panic2`) avoids it entirely.

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

**Benchmark (the capability yardstick):** a real, dependency-using Rust production module living *in* a
.NET solution that C# **imports and calls like a normal library** — correct answers, `std` working
inside, a clean public API. This one test exercises *every* capability layer at once, so **"the
benchmark passes" ≈ "the translation layer works."** It is fundamentally the `.NET→Rust` direction
sitting on the whole stack. **This has now been met (J4):** a real production library (built on
serde/chrono/uuid) was imported by C# and ran its pagination logic as .NET CIL, returning the correct
result, via a transient FFI wrapper over a read-only cross-repo mount (leak-safe). The four end-to-end
journeys (J1–J4) are walked in [docs/CARGO_DOTNET.md](CARGO_DOTNET.md) §7.

The benchmark decomposes into 6 layers → workflows:

| Layer | Workflow(s) | Status |
|---|---|---|
| 1 correct codegen | WF-1, WF-5 | ✅ **DONE** |
| 2 `std` runs on .NET | WF-2, WF-4 | ✅ **DONE** (full PAL — files/net/threads/time/panic/**async-tokio**/**`os::unix`**; no surrogate; 73/74 soak) |
| 3 Rust→.NET calls | WF-3 | ✅ **DONE** (full BCL, 4256 methods) |
| 4 errors/panics cross cleanly | WF-6 | ✅ **DONE** (throw-bridge; catch_unwind works) |
| 5 .NET→Rust export (call Rust from C#) | **WF-7** | ✅ **DONE** (C# calls a Rust *library*; string marshalling; Rust *defines* managed classes) |
| 6 ergonomic packaged library | **WF-8** | 🟢 **a–e DONE** (direct typed calls; ctors; managed-`String` return; **de-mangled names + struct marshalling**; **slices + `Result`→exception both ways**); **MSBuild auto-build (`RustDotnet.targets`) + NuGet (`cargo dotnet pack`) DONE (G2)**; `Option`→nullable + managed-`T[]` return remain |

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
- **WF-8** Library packaging & ergonomic surface. **WF-8a DONE** (`3b6915f`): a produced **library**
  emits real `.assembly extern` BCL identities (`.ver`/`.publickeytoken`), so C# references it and uses
  **direct typed calls** (`MainModule.rust_add(2,3)`) instead of reflection — `cargo_tests/rust_export_cs`
  now does this, 6/6 on .NET. **WF-8b DONE** (`fb5fa71`): the comptime interpreter emits a default
  `.ctor`, so C# can `new RustObj()` and call its virtual method (`get_value()`→42,
  `cargo_tests/rust_typedef_cs`). **WF-8c DONE** (`b133a35`): a Rust fn returns a managed `System.String`
  directly (the 0-arg-managed-getter Void-return codegen bug fixed). **WF-8d DONE** (`613dd4b` +
  struct): **de-mangled type names** (`stable_adt_name` gives a library's exported, non-generic, local
  types a clean build-stable `Crate.Type` name instead of `rust_export[<hash>].Type`, while keeping
  foreign/generic types mangled for cross-crate coherence — `::stable` stays byte-identical) **+ struct
  marshalling** (an exported `#[repr(C)]` struct lowers to a value-type with a backend-synthesized public
  all-fields `.ctor` + per-field `get_<field>` accessors, so C# does `new Point(2,3)` / `p.get_x()` —
  `cargo_tests/rust_export_cs` round-trips `point_sum`/`make_point`, 10/10 on .NET). **WF-8e DONE**
  (`8ff3be9` + throw): **richer marshalling** — **slices/collections** cross both ways via the `(ptr,len)`
  convention (inbound `sum_slice`: C# `int[]`→`&[i32]`; outbound `fill_squares`: Rust fills a C# buffer),
  a `Result`'s `Ok` value crosses unwrapped (`checked_div`), and the **error direction now works**: a new
  `rustc_clr_interop_throw` intrinsic raises a managed `System.Exception` via a real `throw` IL op (not a
  Rust `panic!`, which faults reaching a managed frame), so `try_div(1,0)` is caught by C# `try`/`catch`
  (`cargo_tests/rust_export_cs`, 15/15 on .NET). **Remaining:** `Option`→nullable; managed-`T[]`/`Span`
  *return* (needs a `newarr`/`stelem` IR op) + self `.assembly .ver`. The **cargo↔MSBuild build glue is
  DONE (G2)**: `msbuild/RustDotnet.targets` makes `dotnet build`/`dotnet run` on a C# project auto-build a
  declared `<RustCrate>` via the installed `cargo dotnet` and reference its assembly (incremental, zero
  manual steps), and `cargo dotnet pack` emits a NuGet `.nupkg` a C# project `<PackageReference>`s from a
  local feed. Worked: `cargo_tests/cd_interop/csharp` (auto-build) + `…/csharp_nupkg` (NuGet), both 6/6.
- **WF-9** Generic-interop bridge — `RustGeneric<T>` ↔ C# (size-parameterized for `T: unmanaged`,
  boxed/`GCHandle` for managed T; §7). Needed iff the module's public API uses generic containers.
- **WF-10 (open-ended)** Real-crate soak / hardening — **DONE for breadth**: ~74 real crates driven
  through `cargo dotnet` on the dotnet PAL under the flip, **73/74 pass** (the one non-pass, `regex`, is
  a deep allocator issue, not a class-level gap); 11+ class-level codegen fixes landed over the campaign.
  This is where "experimental" became "usable" for the covered surface.
- **The build DX (the consolidation):** the whole stack is now driven by the one-command **`cargo dotnet
  build|run`** (`feasibility/cargo-dotnet` over the shared `_cargo_dotnet_core.sh`), with the
  [`dotnet_overlays`](../dotnet_overlays/README.md) registry auto-applied for syscall-using deps and
  `cdylib` library output for C# consumption. New-user guide: [docs/CARGO_DOTNET.md](CARGO_DOTNET.md).

**Status vs. the benchmark:** the capability work is **~95% done** — layers 1–6 all have a working core,
the north-star J4 has been met (a real production library run from C#), and the soak set is 73/74. The
platform is complete (files/net/threads/time/panic/async/`os::unix` on the real PAL), and the one-command
`cargo dotnet` DX wraps it all with zero hand-config — including the **C#-consumes-Rust seam (G2)**:
`dotnet build` auto-compiles a declared Rust crate + references it, and `cargo dotnet pack` ships it as a
NuGet package. Remaining is the **ergonomic tail**: the `.NET→Rust` Tier-2 surface through the real-PAL
flow (managed-`String`/`Result` return), WF-9 (only if the consumed module's API is generic), and the
`regex` allocator fix. The hard, ceiling-adjacent pieces are behind us.
