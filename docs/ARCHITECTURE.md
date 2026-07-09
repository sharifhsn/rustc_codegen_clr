# Architecture & background notes

A codebase-oriented digest of the design decisions behind `rustc_codegen_clr`, distilled
from the author's (FractalFir's) blog series. Local copies of the articles are in
[`fractalfir_articles/`](fractalfir_articles/) (see its [README](fractalfir_articles/README.md)
for the index); citations like *(v0.2.1)* point at the article that explains a point in depth.

The articles are the authoritative source for **why** things are the way they are. This file
maps those explanations onto the actual code so you can navigate faster. Where the blog and the
code disagree, the **code wins** — the project moves fast and some articles predate later rewrites.

---

## 1. The big picture

- This is a **rustc codegen backend plugin** loaded via `-Z codegen-backend=librustc_codegen_clr.so`.
  rustc hands it **MIR** (mid-level IR); the backend translates MIR → **CIL** (Common Intermediate
  Language, the stack-based IR that .NET/CoreCLR and Mono execute) or, in `C_MODE`, → C source. *(v0.0.1, v0.1.0)*
- To the .NET runtime, compiled Rust looks like **unsafe C#** — it can call .NET APIs and hold
  managed objects. The end goal is near-seamless Rust ↔ C#/F# interop (see the `mycorrhiza` crate
  and the WIP `dotnet_typedef!` macro for defining .NET classes in Rust). *(v0.2.0, v0.2.1)*
- The IR (`cilly`) is deliberately **backend-agnostic**: the same IR is lowered to .NET CIL, C, and
  (experimentally) JVM/JS. This is why C support cost only ~1–2K LOC — "pretend C is a very weird
  .NET runtime." *(v0.1.2)* See the exporters under `cilly/src/ir/{pe_exporter,il_exporter,c_exporter,java_exporter}`.
  For .NET output the linker's **default** path is the hand-rolled `pe_exporter` (writes a PE directly,
  no external tools); `il_exporter` (ilasm-based) is only the `DIRECT_PE=0` fallback. *(see docs/PE_EMISSION_PLAN.md)*

## 2. Two guiding principles (these explain most of the code's shape)

1. **Faithful-to-MIR, then optimize.** Each MIR statement is first lowered to a *literal, exact*
   (often redundant) CIL sequence kept isolated from other statements, so any malformed CIL traces
   back to exactly one MIR statement. A separate optimization pass then applies many small
   behavior-preserving micro-ops that together roughly halve instruction count. *(v0.0.1, v0.0.3)*
   → When debugging a miscompilation, set `OPTIMIZE_CIL=0` to keep the 1:1 mapping.
2. **Pure / functional translation.** Each MIR element is handled by a pure function over immutable
   inputs, which makes panic-recovery trivial (the backend *expects* to hit unsupported code). The
   notable mutable exception is the per-codegen-unit `TyCache`, resettable after a panic. *(see `src/lib.rs` rustdoc)*

> Why optimize MIR at all? Not for faster output — to make *compilation* faster: optimizing a
> generic function once (pre-monomorphization) saves re-optimizing every monomorphized instance.
> MIR opts are limited because they must hold for all `T`. *(v0.2.1)*

## 3. The CIL-trees IR

- **CIL "trees" (`CILNode` / `CILRoot`).** Early CIL was a flat array of stack ops; the relationships
  between ops were implicit, making optimization/validation hard. It was rewritten into a **tree**
  where each node references its inputs. *(v0.1.1)* This is the `cilly/src/ir/cilnode.rs` /
  `cilroot.rs` IR.
  - **`CILNode`** = a pure, value-producing node (one output). Every node except `Call` has a *fixed*
    arity, validated at construction — you structurally *cannot* build a malformed `mul` with 3 inputs.
  - **`CILRoot`** = a side-effecting statement; **only a root may write** to a local/address
    (`stloc`, `stind.*`). This root-vs-node invariant is what makes reordering safe to check. *(v0.1.1)*
  - Tree limitations, by design: **no `dup`** (1 input / 2 outputs can't be a tree — re-introduced
    only after optimization, "flattening"); **no branch-join stack values**. Both sacrificed as minor
    micro-opts. *(v0.1.1)*
- **Single interned IR.** The tree IR lives directly under `cilly/src/ir/` — addressed by
  `Interned<T>` handles into a `BiMap`. There is no separate V1/tree-vs-interned split anymore (an
  earlier V1→V2 two-generation design was collapsed into this one IR); optimization (`ir/opt/`), the
  typechecker (`ir/typecheck.rs`) and all exporters operate on it directly, and it's what gets
  serialized (postcard) into the `.bc`/`.rlib`. (See `join_codegen` in `src/lib.rs`: build → `opt` →
  `typecheck`.)

## 4. The custom linker does the heavy lifting

Set via `-C linker=` (binary in `cilly/src/bin/linker/`). It loads the serialized assemblies from
rlibs, merges them, patches in libc / intrinsic implementations, and emits the final .NET executable
(`pe_exporter`, the default, or `il_exporter`+ilasm under `DIRECT_PE=0`) or C output (AOT path in
`aot.rs`). Things that live here rather than in the compiler:

- **Cross-crate dead-code elimination** (a copying-GC-style reachability pass) — rustc's frontend DCE
  can't see across crates and must keep all public `std` functions. DCE roughly halved assembly size;
  the remainder is mostly *types* and *static data*. *(v0.1.1)*
- **Command-line arguments** — the single hardest GSoC task; Rust uses the GNU `.init` section to grab
  argv, emulated via .NET static constructors (`.cctor`) on the `RustModule` class. *(v0.1.2, v0.2.0)*
- **Native-library P/Invoke** generation (`native_passtrough.rs`, gated by `NATIVE_PASSTROUGH`). *(v0.1.1)*

## 5. How Rust constructs map to .NET (and the gotchas)

- **Functions** → static .NET methods; **Rust name mangling is preserved** in symbols
  (`_ZN…E`, with `$u7b$`/`$u7d$` escapes). `ASCI_IDENT` forces ASCII-only names for stricter compilers. *(v0.0.1, v0.2.1)*
- **Generics are monomorphized.** rustc gives a `subst` (concrete type args, indexed `G0,G1,…` — MIR
  stores them by index, not name) + a `DefID` recipe. Mapping Rust generics onto *real* .NET generics
  was **tried and abandoned**: .NET forbids `LayoutKind.Explicit` on generic types (the GC can't tell
  an overlapping field is a managed ref vs raw pointer), but explicit layout is *required* for Rust
  enums/unions. → fell back to **name mangling**. You cannot instantiate new generic variants from C#. *(v0.0.3, v0.1.0)*
- **Enums** → tagged union: a discriminant field + variant payloads overlaid with `[FieldOffset]`
  (`LayoutKind.Explicit`). Layout uses `_tag`, `v_<Variant>` fields, `m_<n>` members. `discriminant` +
  `switchInt` drive dispatch; the `otherwise`/`unreachable` arm currently lowers to a **`throw`**
  ("Unreachable reached…"), so a corrupt tag surfaces as a runtime exception. *(v0.0.3, v0.1.3)*
- **Fat pointers / DSTs** = pointer + metadata (slice length, or `dyn Trait` vtable). Watch the
  **three-way** distinction, which was a real bug: (1) sized, (2) DST → fat pointer, (3) **foreign /
  `extern` types (`TyKind::Foreign`)** → unsized-but-no-metadata → must use a **thin** pointer.
  Deciding "fat?" by `!is_sized()` alone wrongly fattens foreign pointees and corrupts the ABI; the
  check must also exclude `TyKind::Foreign(_)`. *(v0.1.3)* Relevant constants are exported from `cilly`:
  `DATA_PTR`, `METADATA`, `ENUM_TAG` (re-exported in `src/lib.rs`). `dyn Trait` reuses slice/fat-pointer
  code paths almost for free. *(v0.2.0)*
- **`#[track_caller]`** injects a hidden `&'static Location` argument invisible in MIR — which is why
  **`FnSig` ≠ `FnAbi`** and their argument *counts* can differ. Must be threaded through everywhere
  (especially fn pointers). *(v0.1.0, v0.2.2)* This is `rustc_codegen_clr_call`'s `CallInfo` territory.
- **ZSTs:** .NET has no zero-sized types (every type ≥ 1 byte), a recurring bug source — a size-0
  trailing field can become size-1 and clobber an adjacent byte on copy. *(v0.1.1)* (`Type::Void` is special-cased throughout.)
- **Atomics** → `System.Threading.Interlocked` + inserted memory fences; **8/16-bit atomics**
  (no .NET < 9 support) are **emulated with locks**. *(v0.1.4, v0.2.0)*
- **Threads** → emulate the pthreads POSIX API *inside* .NET, keeping changes in the backend rather
  than patching Rust `std`. `std` itself is a POSIX "surrogate" built via P/Invoke (no .NET-native
  `std` target yet — see `target.md` for the upstreaming discussion). *(v0.2.0, v0.1.3)*

### CIL emission footguns (worth knowing before editing exporters)
- The eval stack is **sign-agnostic**; widening is counterintuitive: `conv.i8` sign-extends,
  `conv.u8` zero-extends (to widen `u32`→`i64` emit `conv.u8`). *(v0.1.2)*
- **Float→int casts: Rust saturates, .NET wraps** (and constant-folds differently again). The backend
  emits explicit range-checking cast helpers. Found via fuzzing. *(v0.1.1)*
- `conv.r.un` yields an unspecified-width float ("F" type) → must follow with `conv.r8`. *(v0.1.2)*
- `calli` on a null pointer crashes the runtime silently (no exception) — motivates the per-op
  trace/console-logging debug modes (`TRACE_CIL_OPS`, and historically `TRACE_STATEMENTS`). *(v0.1.2)*
- Two **ILASM flavours** (Mono vs CoreCLR) differ in `.line` debug-info syntax and quoting of nested
  type paths (`'A'/'B'` vs `'A/B'`); `IlasmFlavour` handles this. *(v0.1.3)*

## 6. Panics & unwinding (directly relevant to recent commits)

- **Panicking** (the language feature) is currently implemented via **unwinding** (the mechanism), but
  the two are distinct. *(v0.2.1)* Only MIR **terminators** can panic, so cleanup handling is per-terminator.
- Rust **cleanup blocks** → **.NET exception handlers** (`try`/`catch`/`leave`). Central mismatch:
  MIR cleanup blocks can jump into one another, but .NET handlers cannot → **cleanup blocks are
  duplicated into each handler**, the main source of CIL bloat. You exit a protected region only via
  the **`leave`** instruction (branch to an inside label, then `leave`). *(v0.1.1, v0.2.1)*
- Empty drop glue (`InstanceKind::DropGlue(_, None)`, e.g. dropping an `i64`) lowers to just a
  `CILRoot::GoTo` — hence decompiled handlers that look like empty `catch { … throw; }` ("ghost drops"). *(v0.2.1)*
- **Performance:** Rust-on-.NET is typically 1.5–2× native (≤5× common; pathological iterators up to
  70×). Exception handlers are a big cost: RyuJIT refuses to inline callees over `MAX_BASIC_BLOCKS`
  (=5), and duplicated multi-block handlers blow past it. Stripping all handlers alone gave ~2× on a
  bad benchmark — which is what **`NO_UNWIND`** does (emit no try/catch). An optimizer pass also deletes
  handlers that contain only local assignments/jumps/`rethrow` (no observable side effects). *(v0.2.1)*
- For the **std side** of panics (which symbols/intrinsics the backend must support): the key one is the
  `catch_unwind` **intrinsic** (→ .NET try/catch), plus the `panic_impl`/`panic_handler` lang items,
  `__rust_start_panic` (→ throw a .NET exception carrying the payload), and `__rust_panic_cleanup`
  (→ extract the payload). On native Linux these ride libunwind with class `b"MOZ\0RUST"` and a per-`std`
  "canary"; the .NET backend substitutes .NET exceptions for that machinery. *(v0.2.2)*

## 7. Status / where the project is going

- ~95% of the `core` and `std` test suites compile and run (GSoC 2024 result); C mode ~95% too. *(v0.2.0)*
  Still expect miscompilations — not for production use. Fuzzing uses a modified
  [rustlantis](https://github.com/cbeuw/rustlantis) MIR fuzzer.
- No proper `.NET` target triple exists upstream yet, so `std` is a Linux-x86_64 "surrogate"; getting a
  real target + `std` patches upstreamed is the path to becoming an official Rust target (`target.md`). *(v0.1.3, v0.2.0)*

---

### Terminology cheat-sheet (appears in the code)
`CILNode` / `CILRoot` (pure node vs side-effecting root); `Interned`/`BiMap` (hash-consing);
`TyCache`; `subst` + `DefID`, `Gn` (generics by index); `FnSig` vs `FnAbi`; `_tag`/`v_<Variant>`/`m_<n>`
(enum layout); `DATA_PTR`/`METADATA`/`ENUM_TAG`; `TyKind::Foreign` (thin-ptr unsized); ZST / `Type::Void`;
`RustModule` + `.cctor`; `leave` / cleanup-block duplication; `MAX_BASIC_BLOCKS` (JIT inline limit);
`IlasmFlavour`; config flags `OPTIMIZE_CIL`, `NO_UNWIND`, `C_MODE`, `ASCI_IDENT`, `TRACE_CIL_OPS`, `NATIVE_PASSTROUGH`.
