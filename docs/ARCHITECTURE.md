# Architecture & background notes

This file is the code-oriented description of the current compiler architecture. The source code
and acceptance tests are authoritative when implementation and prose differ.

---

## 1. The big picture

- This is a **rustc codegen backend plugin** loaded via `-Z codegen-backend=librustc_codegen_clr.so`.
  rustc hands it **MIR** (mid-level IR); the backend translates MIR → **CIL** (Common Intermediate
  Language, the stack-based IR that .NET/CoreCLR and Mono execute). *(v0.0.1, v0.1.0)*
- To the .NET runtime, compiled Rust looks like **unsafe C#** — it can call .NET APIs and hold
  managed objects. The end goal is near-seamless Rust ↔ C#/F# interop (see the `mycorrhiza` crate
  and the WIP `dotnet_typedef!` macro for defining .NET classes in Rust). *(v0.2.0, v0.2.1)*
- The IR (`cilly`) is deliberately backend-agnostic: the same interned representation feeds
  the optimizer, verifier, and direct PE emitter.

## 2. Two guiding principles (these explain most of the code's shape)

1. **Faithful-to-MIR, then optimize.** Each MIR statement is first lowered to a *literal, exact*
   (often redundant) CIL sequence kept isolated from other statements, so any malformed CIL traces
   back to exactly one MIR statement. A separate optimization pass then applies many small
   behavior-preserving micro-ops that together roughly halve instruction count. *(v0.0.1, v0.0.3)*
   → When debugging a miscompilation, set `OPTIMIZE_CIL=0` to keep the 1:1 mapping.
2. **Isolated, transactional translation.** Lowering mutates a per-item `MethodCompileCtx` and its
   interned assembly, but never the parent CGU directly. Every mono item first builds in an isolated
   assembly shard; an unsupported item fails the
   compilation instead of leaving a throwing placeholder or a partially mutated parent CGU. Before
   a completed shard commits, the linker performs a read-only semantic-conflict preflight. Expected
   conflicts return a structured error without changing the parent, while broken internal relocation
   invariants remain fail-stop. CGUs commit in rustc's deterministic order. *(see
   `src/assembly_transaction.rs` and `cilly/src/ir/asm_link.rs`)*

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
  `typecheck`.) The current envelope is schema 11 with the distinct `CILLYAR11` prefix. Schema 10
  is rejected before positional root decoding because initializer-fragment boundaries became
  serialized; schema 9 is likewise rejected because `ClassDef` gained positional
  value-kind-authority metadata.
- **Effects are explicit and conservative.** The optimizer uses an `EffectSummary` lattice rather
  than treating “does not write” as “safe to delete”: a load, cast, division, static access, or call
  can throw or trigger type initialization even when it has no write effect. Only total, pure trees
  may be discarded. CIL-level call inlining is deliberately absent; rustc's typed MIR inliner is the
  supported inlining layer.
- **Verification is a fatal boundary.** Local/argument/result/call/calli/block/static-field shapes
  are checked exhaustively before serialization and again after final reachability and runtime
  resolution. `MethodImpl::Missing` is allowed as an intermediate link placeholder, never in the
  retained executable graph.

## 4. The custom linker does the heavy lifting

Set via `-C linker=` (binary in `cilly/src/bin/linker/`). It loads the serialized assemblies from
rlibs, merges them, patches in libc / intrinsic implementations, and emits the final .NET executable
through the direct PE emitter. Things that live here rather than in the compiler:

- **Cross-crate dead-code elimination** (a copying-GC-style reachability pass) — rustc's frontend DCE
  can't see across crates and must keep all public `std` functions. DCE roughly halved assembly size;
  the remainder is mostly *types* and *static data*. Reachability includes constructed generic types,
  external method signatures, accessors, overrides, and metadata ownership. Before call-graph DCE,
  mandatory CFG canonicalization removes roots after unconditional transfers and walks blocks from
  the real entry (including exception-region edges), so a const-pruned MIR branch cannot resurrect a
  missing mono item. *(v0.1.1)*
- **Indexed transactional shard linking.** The first preflight of an arbitrary or deserialized
  destination validates and indexes all class identities, class-kind authority, methods, and native
  imports. Successful mono-item commits carry that non-serialized index forward, so later disjoint
  shards probe only their own identities instead of repeatedly sorting the accumulated program.
  Identity overlaps still receive the complete structural field/base/member/method-body audit.
- **Command-line arguments** — the single hardest GSoC task; Rust uses the GNU `.init` section to grab
  argv, emulated via .NET static constructors (`.cctor`) on the `RustModule` class. *(v0.1.2, v0.2.0)*
- **Native-library P/Invoke** — the backend records ordinary Rust `#[link]` foreign functions as
  artifact metadata; missing-method resolution turns them into `MethodImpl::Extern`, and both PE
  and IL exporters emit the library, entry point, calling convention, and last-error policy.
  `rust-dotnet-bindgen` generates those declarations from C headers, while
  `rust-dotnet-pinvoke` supplies explicit marshalling, ownership, and callback helpers. When both
  sides are Rust, its `native_export` and `native_import!` macros generate matching private C ABI
  shims from safe scalar, borrowed string/slice, and owned string/vector signatures. Generated
  deallocators keep cross-library memory ownership correct. None of these layers changes the
  compiler contract. The older `native_pastrough.rs` GCC/`nm` experiment is separate and not the
  public path.
- **Runtime services are capabilities, not name-shaped stubs.** The linker recognizes a finite set
  of exact allocator, pinned-native-core panic, UB-precondition, and managed-unwind symbols. Real
  linked definitions always win; a known missing service must have a registered capability or
  linking fails. Managed CIL has no DWARF frame-description entries, so four exact pinned
  libunwind capabilities describe that absence: `_Unwind_FindEnclosingFunction(c_void*) ->
  c_void*` preserves its input program counter, `_Unwind_GetIP(*void) -> usize` and
  `_Unwind_GetCFA(*void) -> usize` report zero because no native unwind context exists, and
  `_Unwind_Backtrace` returns
  `_URC_END_OF_STACK`. The first matches Rust's own fallback where native symbol lookup is
  unavailable or unreliable; the latter services avoid inventing a native stack or returning
  uninitialized data. The exact pinned `llvm.x86.xgetbv(u32) -> i64` service also has a typed
  managed capability: because managed CIL cannot read native XCR0, it returns zero so `std_detect`
  conservatively reports no OS-enabled extended register state. Adjacent libunwind symbols and
  neighboring `llvm.x86.*` intrinsics are not guessed. Unknown dead symbols may disappear in DCE,
  but an unknown retained symbol is fatal. Direct-PE capability checks run on the compacted retained
  graph before emission.

## 5. How Rust constructs map to .NET (and the gotchas)

- **Functions** → static .NET methods; **Rust name mangling is preserved** in symbols
  (`_ZN…E`, with `$u7b$`/`$u7d$` escapes). Internal Rust `Instance` names use rustc's complete
  defining-crate mangling identity, so the same upstream monomorphization emitted by two downstream
  crates receives one program-wide name instead of two incidental instantiating-crate suffixes.
  Explicit export/no-mangle names remain unchanged. `ASCII_IDENTS` forces ASCII-only C identifiers
  for stricter compilers. *(v0.0.1, v0.2.1)*
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
  (especially fn pointers). *(v0.1.0, v0.2.2)* `src/abi.rs`'s `AbiPlan` is the single source of truth
  for definition, direct-call, indirect-call, closure-receiver, RustCall tuple, ignored-ZST, and
  caller-location slots. It asks rustc for the physical `FnAbi`; it does not infer hidden arguments
  from source arity or from a leading `Type::Void`.
- **ZSTs:** .NET has no zero-sized types (every type ≥ 1 byte), a recurring bug source — a size-0
  trailing field can become size-1 and clobber an adjacent byte on copy. *(v0.1.1)* (`Type::Void` is
  special-cased throughout.) `LoweredPlace` walks a MIR projection prefix once, while shared field,
  sequence, and subslice plans keep layout offsets, DST metadata, enum variants, and zero-stride ZST
  addresses identical across address/read/write operations.
- **Unsizing is layout-derived.** `CoerceUnsized` first asks rustc which field is the coercion field,
  then copies every other non-ZST field at its real source/destination offset and recursively coerces
  only that field. Custom smart pointers therefore do not need to put their pointer first, and no
  aggregate-wide `cpblk` may overwrite changed metadata or padding.
- **Managed references are not Rust bytes.** A naked CLR object/array reference or managed byref may
  live in a managed evaluation-stack/local/argument slot, but not in Rust-owned arrays, aggregates,
  statics, allocations, raw-pointer storage, or bulk-memory operations: those locations have neither
  a CLR GC map nor write barriers. `src/managed_storage.rs` recursively rejects such escapes. Only
  exact unsafe `ManagedInteropType` identities and audited, region-free `NativeStorageSafe` value
  wrappers cross the respective boundaries; `GCHandle` remains the explicit rooted token for native
  storage.
- **Atomics** → `System.Threading.Interlocked` + explicit memory fences. The .NET 10 public path uses
  native subword exchange/compare-exchange and one generated operation/type matrix covers every
  integer RMW width, including signed and unsigned 16-bit operations. Older Unity compatibility
  fallbacks are isolated from the public runtime path. *(v0.1.4, v0.2.0)*
- **Target contract:** direct PE currently supports only a 64-bit, little-endian Rust data layout.
  This matches the public Linux x64, macOS Apple Silicon, and Windows x64 SDK hosts and the AnyCPU
  native-width CIL process model. Unsupported pointer widths or endianness fail before lowering; the
  backend does not claim that CLR field layout can emulate a big-endian or forced-32-bit process.
- **Threads** → emulate the pthreads POSIX API *inside* .NET, keeping changes in the backend rather
  than patching Rust `std`. `std` itself is a POSIX "surrogate" built via P/Invoke (no .NET-native
  `std` target yet — see `target.md` for the upstreaming discussion). *(v0.2.0, v0.1.3)*

### CIL emission footguns (worth knowing before editing exporters)
- The eval stack is **sign-agnostic**; widening is counterintuitive: `conv.i8` sign-extends,
  `conv.u8` zero-extends (to widen `u32`→`i64` emit `conv.u8`). *(v0.1.2)*
- **Float→int casts: Rust saturates, .NET wraps** (and constant-folds differently again). The backend
  emits explicit range-checking cast helpers. Found via fuzzing. *(v0.1.1)*
- `conv.r.un` yields an unspecified-width float ("F" type) → must follow with `conv.r8`. *(v0.1.2)*
- `calli` on a null pointer crashes the runtime silently (no exception) — use the scoped
  `TRACE_FN`, `TRACE_VAL`, and IR dump controls to narrow the generated operation. *(v0.1.2)*
- Two **ILASM flavours** (Mono vs CoreCLR) differ in `.line` debug-info syntax and quoting of nested
  type paths (`'A'/'B'` vs `'A/B'`); `IlasmFlavour` handles this. *(v0.1.3)*

## 6. Panics & unwinding (directly relevant to recent commits)

- **Panicking** (the language feature) is currently implemented via **unwinding** (the mechanism), but
  the two are distinct. *(v0.2.1)* Only MIR **terminators** can panic, so cleanup handling is per-terminator.
- Rust **cleanup blocks** → **.NET exception handlers** (`try`/`catch`/`leave`). MIR cleanup blocks
  can jump into one another, but .NET handlers cannot. `MethodImpl::RegionBody` therefore stores the
  cleanup CFG once with explicit protected-block associations; one shared exporter compatibility
  materializer currently expands it into the legacy per-handler shape. The serialized/link-time IR
  avoids duplicated traversal and storage even though final CIL still duplicates physical handlers.
  You exit a protected region only via **`leave`** (branch to an inside label, then `leave`).
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
`TyCache`; `subst` + `DefID`, `Gn` (generics by index); `FnSig` vs `FnAbi`; `AbiPlan`;
`LoweredPlace`; `_tag`/`v_<Variant>`/`m_<n>` (enum layout); `DATA_PTR`/`METADATA`/`ENUM_TAG`;
`TyKind::Foreign` (thin-ptr unsized); ZST / `Type::Void`; `ManagedInteropType` /
`NativeStorageSafe`; `RuntimeService` / `RuntimeCapability`; `RustModule` + `.cctor`; `leave` /
cleanup-block duplication; `MAX_BASIC_BLOCKS` (JIT inline limit); serialized artifact ABI settings
`NO_UNWIND`, `DOTNET_VERSION`; linker-local output and policy settings; diagnostic controls
`OPTIMIZE_CIL`, `OPT_FUEL`, `ASCII_IDENTS`.
