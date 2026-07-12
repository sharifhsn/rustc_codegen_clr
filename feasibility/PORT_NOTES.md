# Porting notes — surviving rustc nightly drift

`rustc_codegen_clr` links against unstable `rustc_private` APIs, so every nightly bump can
break the build. This file records the changes made to compile on **`nightly-2026-06-17`**
(from a last-working nightly ~Oct 2025), as a template for the next bump.

## How to re-port (the loop)

1. Bump `NIGHTLY` in [`Dockerfile`](Dockerfile) and the exact channel in
   [`rust-toolchain.toml`](../rust-toolchain.toml) together.
2. `cargo check 2>/tmp/cc.txt; grep -E '^error' /tmp/cc.txt | sort | uniq -c`
3. For each error, **read the real signature** in the local rustc sources rather than guessing:
   `$(rustc --print sysroot)/lib/rustlib/rustc-src/rust/compiler/…`
   (the `rustc-dev` component ships these — `rustc_middle`, `rustc_abi`, `rustc_codegen_ssa`).
4. Fix in dependency order: standalone `cilly` first, then the root `rustc_codegen_clr` crate.
   The former `ctx → type → place/call → operand` helper packages were consolidated into root
   modules in July 2026, so nightly drift is now surfaced in one rustc-facing compiler pass.
5. Keep fixes **faithful** — adapt to renamed/moved/reshaped APIs, don't change codegen semantics.

Encouragingly, the `cilly` IR crate (the bulk of the codebase) needed **no** changes — only the
root rustc-facing modules rot. Most fixes are mechanical renames.

## Changes applied for nightly-2026-06-17

### Type / layout APIs (`rustc_abi`, `rustc_middle::ty`)
- `FieldDef::ty(tcx, args)` now returns `Unnormalized<'tcx, Ty>` → append `.skip_normalization()`.
- `tcx.type_of(def).instantiate_identity()` likewise returns `Unnormalized<…>` → `.skip_normalization()`.
- `FieldsShape::Arbitrary { offsets, memory_index }` → field renamed to `in_memory_order`
  (`IndexVec<u32, FieldIdx>`, the inverse map; existing code only used it as a counter, so behavior is unchanged).
- `Variants::Multiple.variants` now holds reduced `VariantLayout`s, not full `LayoutData`s →
  reconstruct a variant's layout with `LayoutData::for_variant(&layout, idx)`.
- `ValTree`: `valtree.try_to_scalar_int()` removed → use `Value::try_to_leaf() -> Option<ScalarInt>`
  (call it on the `Value` from `ConstKind::Value`, not on `.valtree`). `ScalarInt::to_u64()` now
  returns `u64` directly (no `Result`).

### MIR (`rustc_middle::mir`)
- `Operand` gained `RuntimeChecks(RuntimeChecks)` — a compile-time bool (`checks.value(sess)`), e.g.
  UB/overflow-check flags. Emit it directly as a bool const node: `ctx.alloc_node(value)`.
- `Rvalue::Use(Operand)` → `Use(Operand, WithRetag)` (add `, _` to patterns).
- `Rvalue::NullaryOp` **removed** — `size_of`/`align_of`/`offset_of` are now const intrinsics
  (arrive as `Operand::Constant`); `UbChecks`/`ContractChecks` are now `Operand::RuntimeChecks`.
- `Rvalue::ShallowInitBox` **removed** — box construction lowers via ordinary alloc + assignment.
- `Rvalue::Reborrow(Ty, Mutability, Place)` **added** — lower as a place read (`place_get`), matching
  `rustc_codegen_ssa`.
- `TerminatorKind::Drop` lost the `async_fut` field (the async-drop continuation is `drop: Option<BasicBlock>`).
- `StatementKind::Retag` removed; `PointerCoercion::ReifyFnPointer` is now a tuple variant `ReifyFnPointer(Safety)`.
- `Instance::resolve_drop_in_place` → `Instance::resolve_drop_glue`.

### Codegen interface (`rustc_codegen_ssa`) — the structural change
The `CodegenBackend` trait was reworked; `CodegenResults` is split into `CompiledModules` + `CrateInfo`:
- `locale_resource` removed; `target_cpu(&self, sess) -> String` now **required**.
- The driver owns `CrateInfo` now (it calls `target_cpu` to build it), so `codegen_crate` no longer
  bundles it; `join_codegen` gains a `&CrateInfo` param and returns `(CompiledModules, WorkProductMap)`;
  `link` gains a `crate_info: CrateInfo` param. `link_binary(…)` takes `crate_info` + the backend name.
- `CompiledModule` gained `global_asm_object: Option<PathBuf>` (set `None`).
- `OutputFilenames::temp_path_for_cgu` dropped its trailing arg.

### Misc renames / moved items
- `tcx.profiler()` → field `tcx.prof`.
- `rustc_span::source_map::Spanned` → `rustc_span::Spanned` (now private under `source_map`).
- `rustc_span::FileNameDisplayPreference` private → use `FileName::prefer_local_unconditionally()`.
- `rustc_middle::mir::mono` → `rustc_middle::mono`; `dep_graph::{WorkProduct,WorkProductId}` use → `WorkProductMap`.
- `sess.target.arch` / `sess.target.os` are now the `Arch` / `Os` enums (compare to `Arch::X86_64`,
  `Os::None`, `Os::MacOs`, …) instead of strings.
- `FnSig::c_variadic` is a method again (`.c_variadic()`).
- `std::range::RangeInclusive<_>` (not `core::ops`): use the `.start` / `.last` **fields**, not `.start()`/`.end()`.

## Not yet runtime-verified
The build is green and the smoke test runs, but these semantic changes deserve targeted runtime checks
(the project's full `cargo test ::stable` suite is the real gate):
- `NullaryOp` removal — programs using `size_of`/`align_of`/`offset_of`.
- `ShallowInitBox` removal — box-heavy code.
- `Rvalue::Reborrow` lowering.
- The `join_codegen`/`link` rewrite — confirm produced `.rlib`s link end-to-end.

## Running on aarch64 (non-x86_64) Linux

The project was historically "x86_64 Linux only", but that turned out to be a *toolchain*
assumption, not a codegen one. The MIR→CIL lowering is arch-neutral, type layouts come from
rustc's target spec, and CIL output is portable across .NET runtimes (incl. arm64). The only
x86_64-specific code was **three hardcoded library paths**:
- `cilly/src/bin/linker/main.rs` — `get_libc_`/`get_libm_` scanned `/lib64` and `.unwrap()`'d it
- `cilly/src/libc_fns.rs` — `f128_support_lib` read `/usr/lib64` and `.expect()`'d it

On aarch64 Debian `/lib64` doesn't exist (libc/libm/libgcc_s live under `/usr/lib/<triple>/`),
so these panicked. The fix (commit on branch `arm64-linux-support`) makes discovery multiarch-aware:
auto-detect `*-linux-gnu` subdirs and skip missing dirs instead of panicking. With it, the codegen
runs **end-to-end on aarch64-linux** (.NET 8 in an arm64 container): **136/228 (~60%) of the
`::stable` subset** (excluding f128/num_test/simd/fuzz) pass, identical in serial and parallel runs.

### Pattern types (implemented)

The biggest single cause of failures on this nightly was `type.rs`’s catch-all `todo!()` for
**pattern types** (`pattern_type!(*const u8 is !null)`), which the updated std uses for non-null
pointers in `NonNull`/`Vec`/`RawVec`. `get_type` now looks through `TyKind::Pat(base, _)` to the
base type — correct because rustc’s `ty::Pat` layout clones the base layout and only tightens the
scalar valid-range/niche (applied to *enclosing* layouts). This is target-independent and lifted
the aarch64 `::stable` subset from **136/228 to 181/228 (+45)** with zero regressions; `vec`/`raw_vec`
pass end-to-end.

Subsequent work lifted the subset further: the reworked float min/max/abs intrinsics
(see [semantics_mapping.md](../docs/semantics_mapping.md)) plus clearing test-source bitrot —
removed-intrinsic imports, and the big one: C-string pointers hardcoded as `*const i8`, which
break on aarch64 where `c_char = u8` (the portable form is `*const core::ffi::c_char`). Net so
far: **136 → 207 / 230 (~90%)**. The ~23 still-failing are now genuine codegen `todo!()` gaps
(new language/std features the backend doesn't yet lower) plus the full-cargo-crate tests — real
feature work, not bitrot or architecture issues.

> Note: this covers aarch64 **Linux** (e.g. a Docker container on Apple Silicon). aarch64 **macOS**
> native is a much larger effort — no ELF `.so`/`/lib` layout, `libSystem.dylib`, and a different
> libc/syscall interop story the upstream never built.
