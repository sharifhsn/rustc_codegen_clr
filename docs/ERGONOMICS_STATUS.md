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

- **Delegates & callbacks ⚑** (`5277560`, `cd_delegates` 14/14). Core done for concrete signatures. **Tail:**
  closure *captures* (boxed-env trampoline); delegate as a **generic-method** argument
  (`List<T>.Sort(Comparison<T>)` — needs a nested-`!N`-binding *extension* of the CIL typechecker, sound,
  NOT a relaxation); .NET **events** (`add_*`).
- **Extend `#[dotnet_class]`** (`c1c90ce`, `cd_typedef` 16/16). Static methods, multiple ctors, field
  setters, managed-type fields done. **Tail:** virtual methods (needs "re-open a class" comptime) and
  implementing a .NET interface.
- **Std-trait impls for wrappers** (`24055af`). `DotNetString` has Display/Debug/Eq/Hash; List has
  element-wise Clone/Eq/Hash. **Deliberately NOT done:** a blanket `Display`-via-`ToString` (would print
  type names for collections and ref-identity equality — dishonest); `Ord` via `String.CompareTo`
  (culture-sensitive, takes `Object`). The honest call was per-type, not a blanket.
- **Dictionary iteration** — backend-blocked (see below). Documented at `collections.rs:413`.
- **`Nullable<T>` (valuetype)** — reference-type `null→Option` shipped; the valuetype wrapper is the same
  backend gap.
- **`Span<T>`/`Memory<T>`** — marker type exists (`intrinsics.rs:329`), no wrapper; same backend gap.
- **More C#-consumable containers** — `RustHashMap`/`RustString` shipped; `IEnumerable<T>` over `RustVec`
  not done.

---

## Did NOT land (⬜) — and why

- **Dictionary iteration** (`for (k,v) in &dict`, `.keys()`, `.values()`), **`Span<T>`/`Memory<T>`**,
  valuetype **`Nullable<T>`**: all blocked by ONE backend gap — **generic value-type instance methods**.
  Enumerating dict entries yields `KeyValuePair<K,V>` (a generic *value type*); extracting `.Key`/`.Value`
  needs `get_Key`/`get_Value`, which `src/terminator/call.rs` asserts unsupported (`!is_valuetype` for
  KIND=1). The `get_Keys`/`get_Values` route returns a *nested* generic `KeyCollection<!0,!1>` the CIL
  typechecker soundly rejects (and must not be relaxed). Not weakenable at the library level — needs the
  deferred WF-9 Stage-1 tail (pass the by-value valuetype receiver by managed-pointer for `call instance`).
- **LINQ-style adapters**, **C# delegates → Rust**, **export Rust enum/Result/Option**, **export Rust
  traits as C# interfaces**, **enum interop**: not started. All now *unblocked* by the shipped enumerator
  bridge + delegates, so these are pure-library follow-ups (except the trait/interface ones, which pair
  with `#[dotnet_class]` interface support).
- **`cargo dotnet publish --aot`**: not wired as a subcommand (AOT is codegen-proven per the gaps-campaign
  memory, but no first-class command).
- **Hosted API docs**, **flagship end-to-end app**: not done.

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

## Next steps for a fresh session (leverage order)

1. **WF-9 generic value-type instance methods** (backend: `src/terminator/call.rs`; rebuild + install +
   `dev.sh gate` required). ONE change unblocks Dictionary iteration + `Span<T>` + valuetype `Nullable<T>`.
   Pass the by-value valuetype receiver by managed-pointer (address-of) for `call instance`. Do NOT touch
   `cilly/src/ir/typecheck.rs`.
2. **Delegate tail** — captures / delegate-as-generic-method-arg (nested-`!N`-binding typecheck extension)
   / events. Unblocks LINQ predicate adapters.
3. **Pure-library breadth** (mycorrhiza-only): LINQ adapters, enum interop, `IEnumerable<T>` over
   `RustVec`, C#→Rust delegates, `#[dotnet_class]` virtual/interface.
4. **Tooling/docs polish:** `cargo dotnet publish --aot`; hosted rustdoc + C# XML docs; a flagship app.

Build/verify loop, footguns, and the copy-these patterns are in
[ERGONOMICS_HANDOFF.md](ERGONOMICS_HANDOFF.md) §2 + §4.
