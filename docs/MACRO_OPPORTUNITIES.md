# Macro & Helper Opportunities

A survey of where `rustc_codegen_clr` benefits from declarative macros / plain
helpers, where it explicitly should **not**, and what a future contributor
should (and should not) reach for. This is a durable backlog: the ranked DO list
below has mostly **already been implemented** (see the "gaps-campaign" history,
commits `c86e699`..`4d4c023`); the remaining unchecked items and the DON'Ts are
the standing guidance.

> Style note: every macro here is a `macro_rules!` table macro or a plain
> `impl`/free-fn helper. They follow the in-tree precedent of
> `const_impl!` ([cilly/src/ir/cst.rs](../cilly/src/ir/cst.rs)) and `gen_binop!`
> ([cilly/src/ir/macros.rs](../cilly/src/ir/macros.rs)) — a compact "table" of
> rows expanding to near-identical items. **No proc-macros, no new crate, no new
> dependency.** That is a deliberate constraint, not an accident — see the
> verdict.

---

## Headline verdict

**Declarative macros and plain helpers — yes. Proc-macros — essentially no.**

The win in this codebase is *table-shaped boilerplate*: dozens to hundreds of
call-sites that differ only in a string and a flag (BCL class constructors,
static-method calls, interop magic-fn arities). `macro_rules!` and small
`Assembly`/`MethodCompileCtx` helpers collapse those perfectly and the
expansions are trivially checkable as byte-identical. A proc-macro would buy
nothing the table macros don't already, while costing a great deal. Three
project-specific reasons it is the wrong tool here:

1. **mycorrhiza's zero-dep posture.** The interop layer
   ([mycorrhiza/](../mycorrhiza/)) is compiled *through the backend itself* and
   is meant to stay a thin, auditable, dependency-free surface. Adding a
   `proc-macro2`/`syn`/`quote` build-dependency (and a separate `proc-macro =
   true` crate that must build on the host with a *normal* compiler while the
   rest of the crate is compiled for the .NET target) is a large tax on a layer
   whose whole value is being small and obvious. The interop magic-fn names are
   matched by the backend *by substring* (`argc_from_fn_name` /
   `is_magic_fn`), so they must be spelled **literally** — a proc-macro
   that built them with `concat!`/`paste!` would actively break the contract.
   The declarative `interop_magic_fn!` arity-ladder
   ([mycorrhiza/src/intrinsics.rs](../mycorrhiza/src/intrinsics.rs)) keeps every
   name a literal ident and still removes ~150 lines.

2. **The IR exhaustiveness safety net.** The IR is a closed set of `enum`s
   (`Type`, `CILNode`, `CILRoot`, `Const`) and the entire backend is built on
   *exhaustive* `match` over them. That non-exhaustiveness is a *compile error*
   is the project's primary miscompilation guard: add a variant and the compiler
   marches you through every site that must learn about it. A proc-macro (or even
   an over-eager declarative macro) that "folds" those matches would convert a
   compiler-enforced obligation into a silent fallthrough — exactly the class of
   bug this codebase is most afraid of. See the DON'Ts.

3. **spinacz's irreplaceable .NET reflection.** The bindings generator
   ([cargo_tests/spinacz](../cargo_tests/spinacz)) emits Rust wrappers by
   *reflecting over real .NET metadata at build time*. That work is inherently
   imperative and data-driven (walk types, resolve overloads, pick arities) — it
   is not table-shaped boilerplate and there is no macro, declarative or
   procedural, that expresses it more clearly than the code already does. The
   right long-term automation for "generate interop wrappers" is **spinacz**, not
   a `syn`-based derive. Keep it explicit.

The one place a proc-macro *would* genuinely pull its weight — a
`#[dotnet_class]` attribute that turns an annotated Rust struct directly into a
managed .NET class — is now **implemented** (the project's first proc-macro crate,
`dotnet_macros`), together with the backend "comptime" capability it was gated on
(a field-initializing parameterized constructor). See "Implemented vs deferred".

---

## DO — ranked (highest leverage first)

Repetition counts are approximate and reflect the pre-refactor tree.

| # | Item | Where | Reps folded | Status |
|---|------|-------|-------------|--------|
| 1 | **`Assembly::call_static` / `call_static_root` / `static_mref` helpers** | [cilly/src/ir/asm.rs](../cilly/src/ir/asm.rs) (def); call-sites across `src/binop/**`, `src/casts.rs`, `src/terminator/intrinsics/**`, `src/terminator/mod.rs`, `src/utilis/adt.rs` | ~95 | ✅ done (`84ed5fc`) |
| 2 | **`dotnet_generic_method`** (`gen!`, `dotnet_generic!`, `dotnet_generic_impl!`) — WF-9 generic-interop wrappers | [mycorrhiza/src/generic_bridge.rs](../mycorrhiza/src/generic_bridge.rs); used by [cargo_tests/cd_generic](../cargo_tests/cd_generic) | ~10 turbofish wrappers | ✅ done (`7209baa`) |
| 3 | **`bcl_class!`** table macro — the ClassRef BCL constructors | [cilly/src/ir/macros.rs](../cilly/src/ir/macros.rs) (def), [cilly/src/ir/class.rs](../cilly/src/ir/class.rs) (table) | ~79–81 | ✅ done (`c86e699`) |
| 4 | **`dotnet_hook!`** — the `MissingMethodPatcher` builtin envelope | [cilly/src/ir/builtins/dotnet.rs](../cilly/src/ir/builtins/dotnet.rs) | 64 (62 generic + 2 `static_getter`) | ✅ done (`1c2256f`) |
| 5 | **`managed_call_family`** (`interop_magic_fn!`) — interop magic-fn arity ladder | [mycorrhiza/src/intrinsics.rs](../mycorrhiza/src/intrinsics.rs) | 26 magic fns | ✅ done (`a3b2870`) |
| 6 | **`simd_table` + `cmp_op!`** — SIMD passthrough table & lt/gt ordering table | [src/terminator/intrinsics/mod.rs](../src/terminator/intrinsics/mod.rs) (simd), [src/binop/cmp.rs](../src/binop/cmp.rs) (cmp) | SIMD passthrough arms; lt/gt pair | ✅ done (`7c26a97`, `9d2fd8b`) |
| 7 | **`InteropHeader`** — typed decode of the `(asm, class_name, is_vt)` interop prefix | [src/terminator/call.rs](../src/terminator/call.rs) | 5 decode sites | ✅ done (`55a4549`) |
| 8 | **Tidy `config!` / test macros** — share one body across doc/no-doc `config!` arms; factor `test_lib!` release/debug | [src/config.rs](../src/config.rs), [src/compile_test.rs](../src/compile_test.rs) | doc/no-doc pair; release/debug pair | ✅ done (`4d4c023`) |

### Notes per item

- **#1 `call_static`** — encapsulates the recurring
  `MethodRef::new(*main_module(), alloc_string(NAME), sig(INS,OUT), Static, []) →
  alloc_methodref → call(.., IsPure::NOT)` idiom in one line. Placed on
  `Assembly` (not only `MethodCompileCtx`) so both `ctx.*` and raw `asm.*`
  call-sites use it; `MethodCompileCtx` reaches them via `DerefMut`.
  Behavior-preserving because the helper interns the same string/sig/methodref in
  the same order. **Deliberately left explicit:** sites taking an
  already-interned sig (fn-ptr reify / `AliasFor` / entry wrappers), the
  `interop_try_catch` magic path, and one hoisted reused mref — none match the
  inputs-then-immediate-call shape.

- **#2 `dotnet_generic_method`** — `dotnet_generic!(RustList<T> = [ASM] CLASS
  <(T,)>)` declares the handle alias; `dotnet_generic_impl!` munches concise
  per-method lines into the right `generic_call{1,2,3}`/`generic_ctor0` arity and
  fixes `KIND=2` (callvirt) for ref-type receivers. `gen!(N)` expands to the
  **def-shape `!N` marker** (`RustcCLRInteropTypeGeneric<N>`) in the `Sig` slot
  while the runtime value keeps its concrete Rust type — the `!N` crux spelled
  once per position. **Its real validation is the native interop tests**
  (`cd_generic`), not a host build; flagged for Phase 3.

- **#3 `bcl_class!`** — defaults the assembly to `"System.Runtime"`, supports an
  optional doc-comment arm so load-bearing comments survive verbatim (the
  `System.Private.CoreLib`-vs-`System.Runtime` subtlety on
  `semaphore_slim`/`thread_local`; the value-type `TypeLoadException` notes on
  `double`/`single`; the net assembly-resolution notes on `socket`). **Kept
  hand-written:** `fixed_array` (builds its name via `format!`) and
  `dictionary`/`concurent_dictionary` (their signatures take `asm` *last*, unlike
  the span/thread_local family — hundreds of call-sites depend on that order).

- **#4 `dotnet_hook!`** — wraps `alloc_string(symbol)` + `box move |_, asm| ->
  MethodImpl` generator + `patcher.insert`. A `static_getter` archetype arm
  collapses the two no-arg static-getter hooks (`instant_ticks`, `getpid`). The
  bespoke multi-step builtins are **left untouched**: `insert_dotnet_process`,
  `insert_dotnet_write`, `insert_dotnet_thread_spawn`, `insert_dotnet_fs_stat`.

- **#5 `managed_call_family`** — each invocation spells the **literal** fn-name
  ident plus its const/type-param prefix and an arg ladder; the macro folds only
  the shared `#[allow]`/`#[inline(never)]` attrs, the generic header, and the
  `abort()` body. Names are written literally (never `concat!`/`paste!`) because
  the backend matches them by substring and parses the arity digit. **Real
  validation is the native interop tests**; flagged for Phase 3.

- **#6 `cmp_op!`** — per-op behavior stays explicit at the invocation site: the
  signed/unsigned `BinOp` split (Lt/LtUn, Gt/GtUn), the operator/helper names,
  and the exact pointer pattern (lt includes `FnPtr`; gt matches only `RawPtr`)
  are all macro *parameters*, never inferred — the sign-agnostic-stack /
  unordered-float subtleties live at the call. `eq_unchecked`/`ne_unchecked` are
  **left untouched** (eq routes its 128-bit path through main-module helpers and
  carries the extra fat-pointer arm).

---

## DON'T — and why

These are not "not yet"; they are "no, on purpose." Each one trades a real safety
property for a cosmetic line-count win.

- **Do NOT macro the exhaustive `match` over `Type` / `CILNode` / `CILRoot` /
  `Const`.** The IR is a closed set of enums and exhaustiveness is the project's
  primary **miscompilation guard**: adding a variant *must* fail to compile at
  every site that has to handle it. The big matches live in the type lowering
  ([src/type/mod.rs](../src/type/mod.rs)),
  the typechecker ([cilly/src/ir/typecheck.rs](../cilly/src/ir/typecheck.rs)),
  and the exporters. A macro that generated those arms would let a new variant
  slip through as a silent fallthrough — converting a compiler error into a
  runtime miscompile, which is the single worst outcome for this project. Leave
  them spelled out; the verbosity *is* the feature.

- **Do NOT add an exporter trait or a "one macro to emit all targets" abstraction.**
  The IL ([il_exporter](../cilly/src/ir/il_exporter/mod.rs), ~1.9k lines), C
  ([c_exporter](../cilly/src/ir/c_exporter/mod.rs), ~1.8k lines), and JVM
  ([java_exporter](../cilly/src/ir/java_exporter/mod.rs)) exporters look
  superficially parallel but the duplication is **dispatch-only**: the bodies
  produce *target-specific output* (CIL opcodes vs C expressions vs JVM
  bytecode) with genuinely different structure, escaping, and type mapping. A
  shared trait/macro would force a lowest-common-denominator shape and make every
  target-specific quirk a special-case override — net more complexity, not less,
  and it would obscure exactly the per-target detail a contributor needs to see.
  Keep the three exporters independent.

- **Keep `spinacz` explicit.** The bindings generator
  ([cargo_tests/spinacz](../cargo_tests/spinacz)) reflects over real .NET
  metadata; it is imperative, data-driven, and irreplaceable. No
  declarative-or-procedural macro models it better than the code does.

- **Keep `get_type` / ADT layout explicit.** Rust `Ty` → cilly `Type` lowering
  in [src/type/mod.rs](../src/type/mod.rs)
  is a dense exhaustive `match` on `TyKind` carrying the project's hardest-won
  knowledge (fat-ptr/DST layout, `TyKind::Foreign` thin pointers, ZSTs, enum
  `[FieldOffset]` tagging). It is exactly the exhaustiveness-guarded surface
  above — do not table-ize it.

- **Keep `binop` dispatch explicit at the per-op level.** The `call_static`
  helper (#1) and `cmp_op!` (#6) removed the *mechanical* repetition, but the
  signed/unsigned split, saturating-vs-wrapping casts, and 128-bit/soft-float
  routing must stay visible per operator. Do not push them behind a macro that
  infers them.

- **Keep intrinsic dispatch explicit.** The big `match` on intrinsic name in
  [src/terminator/intrinsics/mod.rs](../src/terminator/intrinsics/mod.rs) is the
  authoritative list of what the backend supports; a macro that hid the arm list
  would make "is this intrinsic handled?" unanswerable by reading the code. The
  `simd_table` (#6) folds only the *verbatim passthrough* arms — the rest stay
  spelled out.

---

## Implemented vs deferred (this workflow)

**Implemented** (commits `c86e699`..`4d4c023` on `gaps-campaign`): all eight DO
items above. Equivalence-preserving in every case — verified per item by
`-Zunpretty=expanded` byte-diff (mycorrhiza macros), 1:1 mapping audits (bcl /
hook / call_static), or release-build green (cmp / simd). Host-buildable crates
(`cilly`, `mycorrhiza`) were compiled; the `.NET`-target interop crates
(`cd_generic` etc.) are validated on the real target in **Phase 3**, not here.

**Implemented (later) — `#[dotnet_class]` proc-macro + the comptime capability it needed:**

- A new `proc-macro = true` crate `dotnet_macros` (the workspace's first; host-compiled, so it never
  reaches the codegen backend — a target crate only ever sees its ordinary-Rust expansion). Its
  `#[dotnet_class]` attribute parses a real `syn::ItemStruct` and emits the same
  `rustc_codegen_clr_comptime_entrypoint` shape `dotnet_typedef!` produces — real field syntax + real
  diagnostics instead of the `tt`-muncher DSL. The magic intrinsics moved into a `mycorrhiza::comptime`
  module (still zero external deps — they are bare `#[inline(never)]` fn declarations).
- The backend capability it was gated on: a **field-initializing parameterized primary constructor**.
  `src/comptime.rs` now synthesizes `.ctor(field0, field1, …)` (chain to base + `stfld` each arg into
  its field) plus a public `read_<field>()` accessor per field — so a Rust `struct Counter { value: i32,
  step: i64 }` becomes a .NET class C# can `new Counter(5, 100)` and read back. Two latent backend gaps
  the fatal checker had never seen on this path were also fixed: the base-ctor `this` upcast typing, and
  `SetField` parity with `LdField` (a managed reference type accepts `stfld`/`ldfld` on the objref
  directly — valid CIL the exporter already emits). Verified on real CoreCLR: `cargo_tests/cd_typedef`
  (Rust lib + C# consumer) 4/4; `::stable` gate + the WF-9 interop tests (cd_generic 18/18, cd_rustvec
  37/37) green, no regression. Later work closed the original follow-ups too: managed-type fields,
  interface implementation, inheritance, and explicit base-slot virtual overrides now ship; see
  `cd_typedef`, `cd_iface`, `cd_override`, and `cd_bgservice_bgtest`.

**Deferred / non-goal:**

- **Optional `IrChildren` derive — confirmed NON-GOAL.** A derive to generate the IR child-traversal
  (currently hand-written exhaustive matches) was reconsidered and explicitly **declined** (owner's call):
  it would trade a compiler-enforced exhaustiveness guarantee (a new variant = compile error at every
  traversal today) for a proc-macro with a silent-miscompile failure mode, on the correctness-critical IR
  core, for marginal boilerplate savings — and it would add `syn`/`quote` to `cilly`. The exhaustive
  matches stay. If ever pursued, it must be paired with a compile-time exhaustiveness assertion.
