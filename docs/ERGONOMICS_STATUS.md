# Ergonomics campaign — completion report

> **⚠️ Superseded snapshot (2026-06-30).** The 🟡/⬜ items below were subsequently closed almost in
> full — value-type generics, dict iteration, `Span<T>`, `Nullable<T>`, generic methods `!!N`,
> capturing closures, delegate-as-generic-arg, interface impls, enum interop, LINQ + EF expression
> trees all **shipped** after this was written. Current truth:
> [STATE_OF_THE_PROJECT.md](STATE_OF_THE_PROJECT.md) (incl. a corrections table for this doc).

Concise, verified status of the interop-ergonomics campaign against
[ERGONOMICS_ROADMAP.md](ERGONOMICS_ROADMAP.md). The *how-to-operate + start-here* is
[ERGONOMICS_HANDOFF.md](ERGONOMICS_HANDOFF.md); this is *what-landed / what-didn't / why*.

**Headline (2026-06-30):** the campaign is essentially complete. All six themes shipped their keystones
and most breadth. Everything claimed here is verified by a runnable `cargo_tests/cd_*` proof running on
the **real .NET backend** (`CARGO_DOTNET_BACKEND=native`) with an equal pass/total tally (the `chk!`
convention). Backend-touching commits also passed `./feasibility/dev.sh gate` (baseline 426 pass / 14
fail, "no real regressions"). Branch `gaps-campaign`, ~84 commits ahead of `mine/gaps-campaign`, **NOT
pushed**.

**Counts:** ✅ done **17** · 🟡 partial **6** · ⬜ not-started **8**. (Per-item status is the marked table
in the roadmap.) None of the 🟡/⬜ items are on the critical path; they are backend-blocked tails or
optional breadth.

---

## What landed & is verified (✅), by theme

**Theme 1 — collections & iteration**
- `mycorrhiza::prelude`; collection conveniences (`first`/`last`/`pop`/`sort`/`reverse`/`to_vec`/
  `from_slice`/deep `clone`); `Vec`↔`List` (`From<Vec>`, `to_vec`, `from_slice`); `FromIterator`/`Extend`/
  `IntoIterator for &_`. — `24055af`, `fb35b88`. Proof: `cd_collections` 128/128.
- **Enumerator bridge ⚑** — `Enumerator<T>` wraps `IEnumerator<T>` as `impl Iterator`; `for x in
  &List/&HashSet/&Stack/&Queue`. — `fb35b88`. Proof: `cd_enumerate` 22/22.
- More collections — Sorted{Dictionary,Set}, LinkedList, PriorityQueue, Concurrent{Dictionary,Queue,Bag},
  all in the prelude. — `57e01c1`. Proof: `cd_collections` grew to 128/128.

**Theme 2 — idiomatic traits & type wrappers**
- First-class managed `DotNetString` (Display/Debug/Eq/Hash, real UTF-16 round-trip incl. non-ASCII). —
  `24055af`, `eb78316`.
- `Option`/`Result` bridges — `mycorrhiza::error`: `null→Option` (`Nullable`/`from_nullable`),
  `throw→Result<T,ManagedException>` (`try_managed`/`.try_()`). — `eb78316`. Proof: `cd_idiomatic` 45/45.
- Common BCL wrappers — DateTime/TimeSpan/Guid/Uri/Regex/Random/Stopwatch/StringBuilder/Environment/Math
  under `mycorrhiza/src/bcl/`, all in the prelude. — `957ca95`. Proof: `cd_bcl` 313/313.
- `System.Text.Json` bridge — `mycorrhiza::bcl::json` on the `JsonNode` reference model. — `3d6217d`.
  Proof: `cd_json` 47/47.

**Theme 3 — big capabilities**
- **Task/async bridge ⚑** — `await_task` / `future_to_task`. — `94d8e59`. Proof: `cd_async` 7/7.

**Theme 4 — Rust-from-C#**
- **`#[dotnet_export]` auto-marshal ⚑** — `#[dotnet_export] fn` → `MainModule.method(...)`, managed-string
  marshalling, zero C# glue, no backend change. — `d08aba3`. Proof: `cd_export` 11/11.
- `RustHashMap<K,V>` + `RustString` reusable C#-consumable containers. — `8ce47bd`. Proof: `cd_containers2`
  30/30. (Plus the pre-existing `RustVec<T>`/`RustBoxVec<T>`: `cd_containers` 13/13.)

**Theme 5 — tooling & onboarding**
- **`cargo dotnet new` ⚑** (`--app`/`--lib`/`--plugin`) + `doctor` + `test`. — `8f7eb61`. Verified:
  scaffolds build+run (myapp 6/6, mylib 3/3, plugin 2/2); `test` runs a `#[test]` on .NET.
- `mycorrhiza::prelude` ⚑ (`24055af`); `cargo dotnet pack` → `.nupkg` (`tools/cargo-dotnet/src/pack.rs`).

**Theme 6 — docs**
- `INTEROP_COOKBOOK.md` (recipe-per-task, grounded in the `cd_*` crates) — `8616f24`.
- `BCL_COVERAGE.md` (idiomatic/raw/unsupported matrix) — `8e1fa66`.

---

## Partial (🟡) — core landed, a documented tail deferred

- **Delegates & callbacks ⚑** (`cd_delegates`, `cd_vtgen`, `cd_event_subscription`). Concrete
  signatures, capturing closures, delegates whose parameter uses an enclosing class generic,
  exported class/interface event metadata, and deterministic Rust-side event subscription are done.
  **Tail:** generated high-level adapters for uncommon concrete event-delegate signatures; event
  backing semantics remain user-owned.
- **Std-trait impls for wrappers** (`24055af`). `DotNetString` has Display/Debug/Eq/Hash; List has
  element-wise Clone/Eq/Hash. **Deliberately NOT done:** a blanket `Display`-via-`ToString` (would print
  type names for collections and ref-identity equality — dishonest); `Ord` via `String.CompareTo`
  (culture-sensitive, takes `Object`). The honest call was per-type, not a blanket.
- **`Span<T>`/`Memory<T>`** — `Span<T>` and `ReadOnlySpan<T>` are zero-copy borrowed views;
  `Memory<T>` and `ReadOnlyMemory<T>` copy into GC-owned arrays for storage/async-safe handoff.
  Slicing, mutation, and `ReadOnlyMemory<T>.CopyTo` are proven by `cd_span` 68/68.
- **More C#-consumable containers** — `RustHashMap`/`RustString` and `IEnumerable<T>` over
  `RustVec<T>`/`RustBoxVec<T>` and managed async-stream consumption shipped; async-stream production
  remains optional backend breadth.

---

## Subsequent closures and remaining breadth

After this original snapshot, the generic-value-type instance-method path shipped without weakening
the checker. `cd_vtgen`, `cd_collections`, and `cd_span` now prove `KeyValuePair<K,V>`, Dictionary
iteration, `Nullable<T>`, and zero-copy spans. `dotnet_enum!`, LINQ/IQueryable expression builders,
capturing delegates, interface implementations, `cargo dotnet publish`, and the flagship
`examples/issue-dashboard` application also shipped.

Remaining breadth includes generated adapters for uncommon event-delegate signatures,
managed-reference/non-primitive `Option<T>`, delegate arities above three, automatic owned-value
callback marshalling and an `impl Fn` adapter, async-stream production,
nullable signature annotations, and hosted API documentation. Primitive
`Option<T>` ↔ `Nullable<T>`, `Result<T,E>` → exception, Rust traits exported as C# interfaces,
`IEnumerable<T>` over `RustVec`, and managed `Memory<T>` are already shipped.

Rust-defined enums are now exported as genuine CLR enums via `#[dotnet_enum]`; typed
`#[dotnet_export(enums(...))]` parameters/returns, reflection, literal fields, and C# `switch`
syntax are proven by `cd_export_ergonomics` (37/37), alongside three-argument and managed-string
C# callback imports.

---

## Honesty ledger (constraints held)

- `cilly/src/ir/typecheck.rs` was **never weakened**. Where a value-type ctor+`transmute_copy` or a
  managed-ref `transmute` was ill-typed CIL, the codegen was changed to be honest (method-based
  construction via `Parse`; real `castclass` upcasts) rather than relaxing the verifier. The one place a
  checker change was warranted (WF-9 marker guard) was a *sound* binding-consistency guard, added earlier
  (`f73bca7`), not a relaxation.
- No test was deleted or weakened to pass. Every `cd_*` count above is a real equal pass/total on the
  native backend.
- Value-type instance calls in the BCL wrappers required a genuine, correct helper (`vt_*` in
  `intrinsics.rs`, IS_VALUETYPE=true → `call instance` on `valuetype`); the pre-existing
  IS_VALUETYPE=false path was left for the GCHandle case.

---

## Next steps for a fresh session (current leverage order)

1. Keep the supported build/onboarding/NativeAOT/release evidence continuously green.
2. Add the remaining interop breadth only with a product-shaped consumer fixture.
3. Publish hosted API documentation once the external hosting/release identity is selected.

Build/verify loop, footguns, and the copy-these patterns are in
[ERGONOMICS_HANDOFF.md](ERGONOMICS_HANDOFF.md) §2 + §4.
