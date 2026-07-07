# mycorrhiza ergonomics backlog

Snapshot from a 2026-07-06 broad ergonomics survey, taken *after* the original
[ERGONOMICS_ROADMAP.md](ERGONOMICS_ROADMAP.md) campaign closed almost in full (see
[STATE_OF_THE_PROJECT.md](STATE_OF_THE_PROJECT.md) for current verified-capability truth — most
🟡/⬜ items in the older `ERGONOMICS_STATUS.md` have since shipped: dict iteration, `Span<T>`,
`Nullable<T>`, generic methods `!!N`, capturing closures, delegate-as-generic-arg,
`#[dotnet_class(implements=…)]` interfaces, and LINQ/EF expression trees are all done).

**2026-07-06 update:** all Tier A (pure-library) and Tier B (pure-docs) items below shipped via a
10-agent parallel orchestration, each independently built + verified + committed on green (10
commits, `fa5ccbd`..`b9e683f`; no `cilly/src` touched; full workspace rebuild + every affected
`cargo_tests/cd_*` probe re-run clean post-merge: `cd_sync` 43/43, `cd_channel` 34/34 (new probe),
`cd_error` 6/6 (new probe), `cd_bcl` 324/324, `cd_decimal` 26/26, `cd_span` 45/45, `cd_export`
13/13, `cd_typedef` 16/16, `cd_rustvec` 44/44, `dotnet_macros` negative-UI suite green). The 4
Tier C items were researched read-only in parallel (no code written) — findings appended at the
end of this doc.

**2026-07-07 round 2 update:** 7 of 8 remaining Tier A/B items shipped (`cargo dotnet publish
--aot`, `doctor` workspace-wiring lints, `cargo dotnet test` libtest-parity probe, real NuGet
metadata in `pack`, sidecar XML docs for `#[dotnet_export]`, the rustdoc landing-page pass, the
differential-testing writeup) — commits `b16eab5`..`8d8de7a`, no `cilly/src` touched, consolidated
re-verification post-merge all green (`cd_test_harness` new probe 6/1-ignored, `cargo-dotnet`'s own
suite 34/34, `cd_export`/`cd_typedef` unaffected 13/13 + 16/16, `publish --aot` end-to-end on
`cd_interop` byte-identical to the JIT path). The 8th item (hand-rolled `IAsyncEnumerable<T>`
Tier A slice) hit a genuine, correctly-reported blocker: `#[dotnet_class(implements=...)]` cannot
express a *generic* interface instantiation today (`rustc_codegen_clr_add_interface_impl` in root
`src/comptime.rs` hardcodes empty generics on the interface `ClassRef`) — this is now folded into
the Tier C findings below rather than attempted as a workaround. The 4 newly-researched items
(`.NET events`, richer `#[dotnet_export]` return types, C# source generators, incremental-build
feedback) turned up two more small Tier A wins (`Task<T>` export returns, `Vec<T>`→`RustVec<T>`
export returns) and one closed non-issue (the source-generator premise is already solved by
generics) — findings appended below.

This doc is the **next layer** of ergonomics work, organized by interface/surface rather than by
theme, so it's easy to tell which items are pure-library (safe, mycorrhiza-only, cheap to verify)
vs. which need backend research first (touch `cilly/src`, need the full `::stable` gate, or need a
design decision before any code is written).

## 1. Rust writing code *for* .NET to consume (export surface)

- **`#[dotnet_class]` virtual-method overrides.** Interfaces (`implements=…`) work; overriding a
  *base class's* virtual method does not. Needed for anyone wrapping WPF/ASP.NET base classes.
  **Research first** — needs a "re-open a class" comptime design, may touch codegen.
- **Export Rust traits as C# interfaces** (the reverse of `implements=`) — let a C# consumer
  program against a Rust-defined contract, not just a concrete exported type. **Research first.**
- **.NET events (`add_*`/`remove_*`)** on exported classes — delegates work as fields/params, but
  idiomatic `event EventHandler Foo` isn't wired. **Research first** (small backend surface,
  unclear size until scoped). **Researched 2026-07-07, see findings below — feasible without
  weakening the typechecker, comparable in size to the interface-export item; genuinely new
  `cilly/src` metadata tables (EventMap/Event/MethodSemantics), not an extension of existing ones.**
- ~~**`IEnumerable<T>` over `RustVec<T>`**~~ — **DONE.** `RustVec<T>` and `RustBoxVec<T>`
  (`msbuild/RustDotnet.Containers.cs`, plus the inline copy in `cargo_tests/cd_rustvec/csharp/Program.cs`)
  now implement `IEnumerable<T>` via an allocation-free struct `Enumerator`, so both `foreach` and LINQ
  work directly. Verified with real `foreach`/`Sum`/`Where`/`Select` checks in `cd_rustvec`
  (44/44, up from 37/37).
- **Richer `#[dotnet_export]` return types** — `Task<T>` and `IEnumerable<T>`/`IAsyncEnumerable<T>`
  as direct return types from an exported fn, to close the loop with async/enumerator work already
  done in the other direction. **Research first** (interacts with the Task<T>-production ceiling
  documented in `mycorrhiza::task`'s own module docs). **Researched 2026-07-07 — three cases of
  wildly different size, see findings below: `Task<T>` return and `Vec<T>`→`RustVec<T>` return are
  both small Tier A follow-ons riding on already-shipped backend capability; true incremental
  `IAsyncEnumerable<T>` production is blocked on the sibling item's coroutine-layout wall.**
- **C# nullable/XML-doc annotation emission** on generated signatures — exported methods carry no
  `?` nullable-reference annotations and don't forward Rust doc comments as C# `///` XML docs, so
  IntelliSense on the C# side is blind to both. **Research first** — design proposal, since it
  touches the export codegen's signature-emission path. **Researched 2026-07-06, see findings
  below — split into two differently-sized problems; XML docs via sidecar file is actually Tier
  B-sized, nullability is genuinely Tier C.**

## 2. Rust consuming .NET (BCL surface)

- ~~**`Ord` for wrapped BCL types where a real, non-culture-sensitive comparison exists**~~ —
  **DONE** (`1441d47`). Audited `Guid`/`TimeSpan`/`DotNetDecimal`; added `Ord`/`PartialOrd` only
  where the comparison is confirmed culture-invariant. `cd_bcl` grew to 324/324 with sort-based
  proofs. String-like/culture-sensitive types deliberately still excluded.
- ~~**`?`-operator ergonomics for `ManagedException`**~~ — **DONE** (`1a09398`). Added
  `impl_from_managed_exception!` macro in `mycorrhiza::error` (a blanket `From` impl isn't legal
  Rust across the orphan rules, so a macro is the right shape). New `cd_error` probe (6/6) proves a
  custom error enum using `?` against a fallible managed call end-to-end.
- ~~**`Span<T>`/`Memory<T>` deeper API**~~ — **DONE, with one documented gap** (`5a3baaa`). Filled
  slicing, `CopyTo`, `contains`/`index_of` (as Rust-side scans). Did **not** wire `IndexOf`/
  `Contains` to the real `MemoryExtensions` static generic methods — that needs a static generic
  call whose argument is itself a generic-struct type constrained on `T: IEquatable<T>`, not
  reachable through the existing generic-interop intrinsics without new `cilly/src` support;
  documented in `span.rs`'s module docs as a real, scoped gap rather than silently worked around.
  `cd_span` 45/45.
- **`IAsyncEnumerable<T>` bridge** — `Task`/`Task<T>` are bridged both ways; async *streams*
  (common in modern .NET APIs — EF Core, gRPC streaming) aren't. **Research first** (interacts
  with the same coroutine-layout wall documented in `mycorrhiza::task`). **Researched 2026-07-06,
  see findings below — blocked for the sugared `async fn` case; a hand-rolled non-overlapping
  Stream-state struct is a viable Tier A follow-up. The blocking generic-interface-instantiation
  gap noted here (2026-07-07) is now FIXED** (`rustc_codegen_clr_add_generic_interface_impl` +
  `implements = "…<[Asm]Ns.Ty>"` / `"…<valuetype [Asm]Ns.Ty>"` syntax) — see the new Tier C
  finding §8 below for the full story, including a real Roslyn bug isolated and worked around along
  the way. The Stream-state-struct spike itself is still open (this only unblocked the interface
  instantiation it needs).
- ~~**A safe `Once`/lazy-init wrapper**~~ — **DONE** (`fa5ccbd`). `mycorrhiza::sync::SharedOnce<T>`,
  built on the existing `SharedLock` (double-checked locking over an `UnsafeCell<Option<T>>`), not
  `System.Lazy<T>` (which is CLR-generic over `T` with no way to bind an arbitrary Rust `T` through
  the current interop surface — documented as a design-rationale comment on the type). `cd_sync`
  grew to 43/43, including a 16-thread simultaneous-race proof.
- ~~**A channel-style primitive** over `System.Threading.Channels`~~ — **DONE** (`b9e683f`).
  `mycorrhiza::sync::{channel, bounded_channel}` + `Sender<T>`/`Receiver<T>`, genuinely MPMC
  (unlike `std::sync::mpsc`), with `try_*`/blocking/async variants and a `raw()`/`from_raw()`
  escape hatch for handing the same channel object to C#. New `cd_channel` probe, 34/34, covering
  multi-producer/multi-consumer ordering, backpressure, and mixed blocking↔async access.
- ~~**`DotNetDecimal` operator/trait completeness audit**~~ — **DONE** (`4868c96`). Filled gaps
  against the primitive-numeric wrappers' trait surface, using real `System.Decimal` arithmetic
  (not `f64` approximation) throughout. `cd_decimal` new probe, 26/26, including an exact
  `0.1m + 0.2m == 0.3m` proof.

## 3. The C# side of the interop boundary

- **Generated C# stub quality** (XML docs, nullable annotations, `[EditorBrowsable]`/naming
  polish) — the single highest-leverage item for a C# developer's first impression, and the least
  design attention has gone into it so far. **Research first** — needs a concrete proposal before
  changing generated-signature codegen.
- **NuGet package metadata audit** — does `cargo dotnet pack`'s `.nupkg` carry a proper README,
  license, repo URL, version, and TFM set fit for publishing (not just in-repo consumption)?
  **Pure library/tooling**, mostly a `pack.rs` metadata fill-in.
- ~~**Source-generator-driven C# boilerplate**~~ — **CLOSED, non-issue.** Researched 2026-07-07: the
  motivating example (a typed wrapper per `RustVec<T>` instantiation) is already solved by real CLR
  generics — `RustVec<T>`/`RustHashMap<K,V>` are single, size-erased, fully generic C# types, so
  there is no per-`T` boilerplate for a source generator to eliminate. No `Microsoft.CodeAnalysis`
  dependency exists anywhere in the tree; a real Roslyn generator would be a new, heavier mechanism
  (netstandard2.0 analyzer package, separate versioning axis) with no current concrete payoff. Full
  finding below for the record, in case a genuinely per-consumer generation need surfaces later.

## 4. Tooling / CLI (`cargo dotnet`)

- ~~**`cargo dotnet publish --aot`**~~ — **DONE.** `cargo dotnet publish <csproj-dir>` runs
  `dotnet publish -c Release -r <host-rid> --self-contained -p:PublishAot=true` against an
  existing C# host project (one that imports `RustDotnet.targets` and declares its
  `<RustCrate>` — the same shape as `cargo_tests/cd_*/csharp`, or `cargo dotnet new --app`).
  The Rust crate build happens as part of the SAME `dotnet publish` invocation via that
  existing MSBuild import, so this genuinely wraps the proven manual recipe rather than
  reimplementing it. Verified end-to-end against `cargo_tests/cd_interop/csharp`: the
  produced standalone native binary (no .NET runtime needed) reproduces the JIT path's
  6/6 output byte-for-byte.
- ~~**Proc-macro error message quality**~~ — **DONE, with one residual gap** (`fe8b28e`). Added
  span-accurate `compile_error!`s for unknown/malformed `#[dotnet_class]`/`#[dotnet_entity]`
  attribute keys, verified via a new `trybuild`-based `negative_ui` test suite (5/5). **Residual,
  not fixed:** `#[dotnet_methods]` applied to an impl with no matching `#[dotnet_class]` elsewhere
  in the crate currently compiles silently — detecting that needs whole-crate visibility a single
  attribute-macro invocation can't see without a larger registry-based design; flagged as a future
  item rather than attempted here to avoid scope creep into "exhaustive macro rewrite."
- ~~**`cargo dotnet doctor` breadth**~~ — **DONE, with one explicitly-skipped check** (`6d4a7be`).
  Added workspace-wiring lints: missing `<RustCrate>` references for sibling Rust crates, and
  `TargetFramework`/`RustDotnetVersion` TFM mismatches. **Skipped:** "stale generated bindings" —
  `mycorrhiza/src/bindings.rs` and `cargo_tests/spinacz/out.rs` are one-time hand-committed
  `spinacz` output with no existing hash/timestamp staleness signal to check against; adding one
  would be inventing new infrastructure, out of scope for a lint pass.
- ~~**Incremental-build feedback**~~ — **CONFIRMED REAL, fix scoped, not yet implemented.**
  Researched 2026-07-07 by measuring a real regex-scale crate: 10-12 seconds of complete terminal
  silence during the MIR→CIL→link→PE pipeline in default (non-verbose) mode, because
  `feasibility/_cargo_dotnet_core.sh`'s log-filtering grep allow-lists `Compiling std/core/alloc`
  but not the target crate's own `Compiling` line or any of the linker's existing stage
  `println!`s — the signal already exists and is silently thrown away, not missing. See findings
  below for the two small, additive, no-typechecker-risk edits that fix it (grep allow-list +
  timing instrumentation) — not yet implemented, next up.

## 5. Documentation & discoverability

- ~~**`INTEROP_COOKBOOK.md`/`BCL_COVERAGE.md` freshness pass**~~ — **DONE** (`a81cba0`, `0d33055`).
  Both cross-checked against `STATE_OF_THE_PROJECT.md`'s verified-capability ledger and current
  code; stale ⛔/gap markers closed, new recipes added for shipped-but-undocumented surface.
- **A flagship end-to-end example app** combining exported async endpoints + EF-queried LINQ +
  background thread pool in one real ASP.NET Core service — nothing currently shows the
  *combination* of shipped features together. **Deferred** — significant standalone effort, do
  after the above lands so the flagship reflects the improved ergonomics rather than the current
  ones.
- ~~**`mycorrhiza` rustdoc landing-page pass**~~ — **DONE** (`c812463`). `lib.rs`'s top-level doc
  comment now has a what-is-this intro, points to `mycorrhiza::prelude` as the recommended start,
  and a categorized module tour. Verified zero new rustdoc warnings vs. before the change.

## 6. Testing ergonomics (for external consumers, not just us)

- ~~**`cargo dotnet test` `#[should_panic]`/`#[ignore]`/filtering support**~~ — **DONE, already
  worked** (`8d8de7a`). Confirmed empirically (new permanent `cargo_tests/cd_test_harness` probe)
  that all three already work correctly via the standard libtest harness running through the
  backend unmodified — no code fix needed for the feature itself. Found and fixed one real,
  unrelated toolchain-drift bug while verifying: a `palinject.rs` manifest literal had drifted out
  of sync with the current nightly's reindentation of `os::unix::io::null_fd()`, hard-erroring PAL
  injection for every `cargo dotnet` invocation.
- ~~**A documented, reusable differential-testing pattern** for external consumers~~ — **DONE.**
  New §14 in `INTEROP_COOKBOOK.md` ("Catch a codegen bug in your own crate early") writes up the
  native-Rust-as-oracle vs. `CARGO_DOTNET_BACKEND=native` pattern for outside use: a minimal
  standalone example (no `mycorrhiza` dependency), the exact two shell invocations, how to strip
  `cargo dotnet run`'s build banner before diffing, a worked example of what a real divergence looks
  like, the `.cargo/config.toml`-clobber ordering footgun (run native first), and the `cargo test`
  vs. `cargo dotnet test` one-level-up analogue. Verified by actually running both legs on the
  example and confirming byte-identical output.

---

## Execution tiers (for scheduling future work)

- **Tier A — pure library/tooling, mycorrhiza-only or `dotnet_macros`-only, additive, no
  `cilly/src` changes.** Safe to parallelize across agents; verify via `cargo test -p mycorrhiza`
  + the relevant `cargo_tests/cd_*` proof; commit on green per the standing convention.
- **Tier B — pure docs.** Safe to parallelize; no build verification needed beyond confirming
  referenced facts against current code/tests.
- **Tier C — research first.** Touches `cilly/src`, an unresolved design question, or a documented
  wall in `mycorrhiza::task`/similar. Must produce a written feasibility finding (and, if a real
  backend change is warranted, a scoped follow-up plan) before any implementation — never silently
  attempt a `cilly/src` change without the full Docker `::stable` gate and a human go/no-go.

---

## Tier C research findings (2026-07-06, read-only, no code written)

### 1. `#[dotnet_class]` virtual-method overrides — SHIPPED (2026-07-07, IL-exporter-only)

**Done.** Implemented exactly the "smallest safe first step" scoped below: `MethodDef` gained an
`overrides: Option<Interned<MethodRef>>` field (`with_override`/`overrides`), `il_exporter` emits
the ECMA-335 `.override [Asm]Ns.BaseType::'Method'` clause (§II.15.4.2.3) right after the `.method`
header, and a new `#[dotnet_override("[Asm]Ns.BaseType")]` attribute on a `#[dotnet_methods]` fn
drives it end-to-end via a new comptime intrinsic
(`rustc_codegen_clr_mark_last_method_override<BASE_ASM, BASE_TYPE>`, must immediately follow the
overriding method's own registration in the entrypoint's MIR). Proven by `cargo_tests/cd_override`
(`Greeter` overrides `System.Object.ToString()`; the decisive check —
`((object)g).ToString()` returning the override's text, not the BCL default — passes 5/5) under
`DIRECT_PE=0` (`il_exporter`).

**`DIRECT_PE=1` (the default, `pe_exporter`) does NOT support `.override` yet** — there is no
`MethodImpl` metadata table in `pe_exporter/tables.rs` at all, comparable in size to the
`InterfaceImpl` work done for generic-interface instantiation (finding #8 below). Silently dropping
the override there would emit an ordinary new-slot virtual instead of a genuine override — a real
miscompilation, not a degradation — so `pe_exporter::export_pe` now has a loud
`assert!(method.overrides().is_none(), …)` guard that fails the build with a clear message pointing
at `DIRECT_PE=0` as the workaround, instead of silently emitting wrong code. Confirmed by testing
both paths: `DIRECT_PE=0` → 5/5; `DIRECT_PE=1` → build fails loudly (exit 101) instead of silently
regressing to 1/5 (which is what happened before the guard was correctly wired — see the bug below).

**Real bug found and fixed en route:** the guard above did not fire on the first attempt even though
the produced DLL clearly lacked the override — root cause was `Assembly::translate_method_def` in
`cilly/src/ir/asm_link.rs` (the cross-rlib merge pass the **linker** binary runs to combine every
crate's serialized `.bc` into one assembly), which rebuilds each `MethodDef` field-by-field and had
never been updated to carry the new `overrides` field through the merge — so it silently reset to
`None` for every method crossing that pass, including ones defined in the crate being linked
itself. Fixed by translating `def.overrides()` through `translate_method_ref` and re-attaching it
with `.with_override(...)`. This is a durable gotcha for future `MethodDef`/`ClassDef` field
additions: `asm_link.rs`'s `translate_*` functions are a second, easy-to-forget reconstruction site
that field-by-field `MethodDef::new(...)` call sites (there are ~30 across the codebase) can also
silently regress if a field is added but not threaded through — grep for `MethodDef::new(` before
trusting any newly-added field survives a real multi-crate `cargo build`, not just a single-file
comptime-derived one.

**Remaining scope, not started:** general base-constructor-chaining validation and
non-virtual/protected member forwarding (needed for real WPF/ASP.NET base-class wrapping) — the
Tier C verdict below on those specific points still stands.

**Original Tier C verdict, for context:**

**Verdict: feasible, but a genuinely different (larger) mechanism than `implements=`, not an
extension of it.** `implements=` works today because CLR *interface* binding matches virtual
methods to interface members by name+signature alone, with no pre-existing vtable slot to reuse —
`cilly`'s `MethodKind` enum has only `Static/Instance/Virtual/Constructor`, no `Override`/`newslot`
concept, and no `MethodImpl` (explicit-override) table support. Overriding a *base class* virtual
must land in the base's exact vtable slot, which needs an explicit `.override` clause (ECMA-335
§II.15.3.4) to be robust — a new, small IR concept, not existing machinery. Two harder problems
stack on top: base-constructor-chaining is currently a one-shot unverified guess (`extends=` picks
one ctor shape with zero validation against the real base), which overrides would make load-bearing
for any real framework base class; and non-virtual/protected member forwarding (needed for real
WPF/ASP.NET wrapping) is a separate, unaddressed problem. **Smallest safe first step:** a minimal
spike scoped to exactly one well-known, parameterless, unsealed base virtual (`System.Object.
ToString()`) — add an explicit `.override` MethodImpl-row capability, IL-exporter-only at first, no
`typecheck.rs` changes, proven by one `cd_override`-style test calling the override through both
the concrete type and an `Object`-typed reference. Do not attempt general ctor-chaining or
protected-member forwarding in that first step.

### 2. Export Rust traits as C# interfaces (reverse of `implements=`)

**Verdict: feasible without weakening the typechecker, but requires new `cilly/src` capability
that doesn't exist today — a genuine Tier C item, not a quick follow-on.** `implements=` only ever
*references* an interface assumed to already exist as metadata somewhere; it never defines one.
Synthesizing a real interface `TypeDef` from a Rust trait needs the backend to emit a `TypeDef`
marked `Interface`+`Abstract` with no `extends`, and members with **no body** (RVA=0) marked
`Abstract`+`Virtual`+`NewSlot` — none of which exists in `cilly`'s IR: `ClassDef` has no
"is-interface" flag and both exporters unconditionally emit an `extends` clause; `MethodImpl::
Missing` looks like a candidate for "no body" but is NOT — it gives a real body that throws at
runtime, so reusing it would silently produce a concrete throwing method instead of an abstract
slot (a real miscompilation class, not a clean failure). This is additive to the type system's
legal-shape set (a currently-unimplemented shape, not a currently-rejected-for-soundness one), so
no typechecker weakening is required — but the real work (interface-`TypeDef` support + a genuine
abstract/no-body `MethodImpl` variant threaded through both exporters + typechecker validation of
the new shape) is comparable in size to the WF-9 value-type-generic unlock, not a pure-library
item. **Smallest safe first step:** a throwaway `cilly`-only spike — hand-build one `ClassDef`
manually flagged as an interface with one abstract/no-body method, run it through the IL-text
exporter, and hand-verify with `ilasm` + a trivial C# class that CoreCLR accepts it as a genuine
interface — before touching the typechecker, the PE writer, or any macro/comptime work.

### 3. `IAsyncEnumerable<T>` bridge

**Verdict: blocked for the sugared case, but a working non-sugared alternative exists today.** The
same wall `mycorrhiza::task` already documents by name — Rust coroutines are laid out with
`has_nonoverlapping_layout = false` (variants' saved locals physically overlap, since only one
suspend point's state is live at a time), and `ClassDef::layout_check` correctly rejects **any**
managed/GC-ref field on such a class, because the CLR's GC cannot safely trace a reference inside
overlapping union storage — weakening that check would risk silent heap corruption, not a
reasonable trade. `Task<T>` production (`future_to_task`) does **not** actually cross this wall —
it dodges it, by driving the future to completion synchronously via `block_on` before ever handing
a value back to .NET, so no coroutine state is ever suspended-and-observable-as-incomplete by .NET.
`IAsyncEnumerator<T>.MoveNextAsync()` is contractually a *real* suspend point (.NET may observe an
incomplete `ValueTask<bool>` and poll again later) — collapsing that to `block_on`-then-return
would defeat the entire point of async streaming (no backpressure, fully buffered). **A real Rust
`async fn`/`.await`-sugared generator can never back a genuine `IAsyncEnumerator<T>` that holds any
managed handle across a suspend, until the coroutine-layout wall is lifted — this is a flat block
for the sugared case, not a workaround-away situation.** The sound fix (hoisting GC-ref-typed
saved-locals out of overlapping union storage into their own always-present fields) is a multi-week
`cilly/src` + `rustc_codegen_clr_type` project requiring the full Docker gate and new
coroutine/gcref test coverage, given the GC-soundness stakes. **Smallest safe first step (ships
value without touching `cilly/src`):** a `mycorrhiza`-only pattern — a hand-authored, non-
overlapping Stream-state struct via `#[dotnet_class]`/`#[dotnet_methods]` (`implements =
IAsyncEnumerator<T>`, with `MoveNextAsync()` as an ordinary non-async fn, not `async fn` sugar) as
the first working producer bridge. This is Tier A once scoped, and previews the shape a future
coroutine-layout fix might target.

### 4. Generated C# stub quality: nullable annotations + XML docs

**Verdict: two differently-sized problems bundled under one backlog line — XML docs is much
smaller than the title implies.** Both would hook the same `type_il_signature`/
`SignatureOnlyResolver` split added for the recent CS0012 ref-assembly fix, but that split does
nothing today related to nullability or docs (it only normalizes assembly *names*) — this is fresh
design, not an extension of existing plumbing. **Nullability (genuinely Tier C):** by the time a
Rust `Option<T>` reaches `cilly::ir::Type` it has already been monomorphized via niche/layout
rules — the "this came from an `Option`" fact is erased before `cilly` ever sees the value, and
`#[dotnet_export]`'s marshalling doesn't handle `Option<T>` at all today. A real fix needs a new,
non-erased nullability signal threaded from the one place that still has the pre-erasure Rust type
available (`dotnet_macros`'s `marshal_param`/`marshal_return`, for the `#[dotnet_export]` surface
only — the general MIR-codegen path has no such signal without new `rustc_codegen_clr_type`/
`rustc_codegen_clr_call` plumbing) down through a new field on `cilly::ir::method::Method`/`FnSig`
and into both exporters' signature emission. Realistically 1-2 focused PRs, and note the real scope
is narrower than the backlog title suggests: "nullable annotations for the `#[dotnet_export]`/
`#[dotnet_class]` proc-macro surface," not "for all Rust-to-.NET exported functions." Nothing here
requires weakening the typechecker — NRT metadata is pure C#-compiler-surfaced custom-attribute
bytes, invisible to and unchecked by the CLR/JIT and by `cilly`'s own typecheck. **XML docs (this
half is actually Tier B-sized, not Tier C):** the raw material (`#[doc = "..."]` attrs) is already
trivially reachable in `dotnet_macros`'s proc-macro input and just isn't read today. C# has no
native in-assembly doc-comment metadata slot; the standard mechanism is a sidecar `<AssemblyName>
.xml` file. Recommended first step: scrape `#[doc]` attrs during `#[dotnet_export]` expansion and
generate that sidecar XML via `cargo-dotnet`'s existing MSBuild packaging step — zero `cilly/src`
changes, fully reversible, validates real IntelliSense pickup before investing in the larger
nullable-annotation work. **Watch for:** XML docs are keyed by an exact ECMA-334 member-ID string
that must match `dotnet_class_name`'s existing >1023-char FNV-hash-shortened output exactly, or
Visual Studio silently shows no docs with no error — needs a differential check against the actual
emitted metadata name, not just "the C# compiles." (Update 2026-07-07: this half shipped, see §4's
DONE entry — `ca74a46`.)

### 5. .NET events (`add_*`/`remove_*`) on exported classes

**Verdict: feasible without weakening the typechecker, and smaller than the interface-export or
virtual-override items — but genuinely new `cilly/src` metadata capability, not a pure-library
follow-on.** Confirmed there is currently zero Event/EventMap support anywhere in the codebase:
`ClassDef` (`cilly/src/ir/class.rs`) models only `fields`/`static_fields`/`methods`/`implements` —
no event *or* property concept at all, so this would be the first semantic (non-field/method) CLR
member kind added to the IR. What ECMA-335 needs: an **EventMap** table row (§II.22.12, one per
type declaring events), an **Event** table row (§II.22.13, name + delegate type), and
**MethodSemantics** rows (§II.22.28) tagging ordinary `MethodDef`s as `AddOn`/`RemoveOn` for an
Event row. Crucially, an event's `add_`/`remove_` bodies are *ordinary* instance methods calling
`Delegate.Combine`/`Delegate.Remove` on a backing delegate field — exactly the pattern
`mycorrhiza::delegate` already proves works as a plain method call — so this is a pure
metadata-linking problem, not new invocation semantics; nothing here touches IL-verification rules.
The IL-exporter (ilasm text) side is easy (a templated `.event`/`.addon`/`.removeon` directive,
ilasm computes the tables itself); the hand-rolled PE writer is the harder half, needing to emit
correctly-sorted EventMap/Event/MethodSemantics rows itself — this exact class of "silent-until-
CoreCLR-loads-garbage" table-ordering bug is a previously-seen hazard in this codebase's PE-emission
work. **Smallest safe first step:** skip the PE writer for the first spike — hand-build one
`ClassDef` with a delegate-typed field and `add_Changed`/`remove_Changed` methods, hand-write the
`.event`/`.addon`/`.removeon` ilasm text directly (bypassing macro/exporter plumbing entirely) to
confirm CoreCLR accepts hand-rolled event metadata and a C# consumer's `obj.Changed += handler;`
actually fires it, before touching `il_exporter/mod.rs` templating, `dotnet_macros` attribute
wiring, or the PE writer's new tables. **What could go wrong:** MethodSemantics/EventMap row-
ordering bugs in the PE writer (a real, previously-documented hazard class here); the synthesized
`add_`/`remove_` bodies need genuine thread-safety (`Interlocked.CompareExchange`-based, matching
what the real C# `event` keyword compiles to) or the feature would be subtly wrong under concurrent
subscription while looking correct; and a naming-scheme decision is needed so a delegate-typed
backing field doesn't collide with the existing field-accessor-generation machinery.

### 6. Richer `#[dotnet_export]` return types: `Task<T>` and `IEnumerable<T>`/`IAsyncEnumerable<T>`

**Verdict: three cases of wildly different size.** `#[dotnet_export]` currently hard-rejects
`async fn` unconditionally and `marshal_return` only recognizes `&str`/`String`/`()`/primitives —
no container or `Task` arm exists at all.

- **Case A — `Task<T>` return (small, Tier A-adjacent):** the hard part this item worried about —
  producing a result-bearing `Task<T>` at all — is *not actually a wall anymore*: `future_to_task`
  in `mycorrhiza::task` already works end-to-end via the WF-9 nested-generic-binding unlock (its own
  comments call this "the former wall — now unblocked"). What's missing is purely a
  `marshal_return` arm recognizing `Task`/`TaskT<T>` as pass-through FFI-safe handle types (they
  already are, same shape as any other managed handle the macro threads through) plus scoping
  `async fn` rejection to keep rejecting the *sugar* while letting a plain non-async fn that
  explicitly constructs and returns a `Task`/`TaskT<T>` pass straight through. No `cilly/src`
  touch, rides entirely on already-shipped capability.
- **Case B — `IEnumerable<T>` return, materialized collection (small, Tier A):** achievable today
  by direct analogy with the already-DONE `RustVec<T>`/`IEnumerable<T>` item, which was implemented
  entirely C#-side (a hand-written wrapper around a concrete exported handle, not a synthesized
  bare interface value). The same shape applies: teach `marshal_return` a `Vec<T>` → `RustVec<T>`
  arm (for primitive `T` first), and the *existing* C#-side `IEnumerable<T>` implementation already
  makes the result `foreach`/LINQ-able. Zero `cilly/src` involvement.
- **Case C — `IAsyncEnumerable<T>`, true incremental producer (blocked):** identical wall to the
  sibling `IAsyncEnumerable<T>` bridge finding above — fully tied to that item's fate, not an
  independent problem. Do not attempt separately; it would either re-derive the same coroutine-
  layout wall or tempt a fully-buffered shortcut that silently breaks the backpressure semantics a
  .NET consumer would reasonably assume from the type.

**Recommended sequencing:** ship Case A and Case B as two small, independent `dotnet_macros`
changes riding on already-shipped backend capability; leave Case C blocked until the sibling
Stream-state-struct spike lands (and, per that finding above, until the `implements=`
generic-interface-instantiation gap is fixed first).

### 7. Source-generator-driven C# boilerplate — CLOSED, non-issue

**Verdict: the motivating premise no longer exists — real generics already solved it.** The item's
example (a typed wrapper per `RustVec<T>` instantiation) described a per-`T` codegen problem that
doesn't exist in this codebase: `RustVec<T>`/`RustHashMap<K,V>` are single, size-erased Rust cores
with a single hand-written, fully generic C# wrapper (already `IEnumerable<T>`-capable) — a
consumer never hand-writes anything per their own `T`, the CLR's real generics handle every
instantiation. There is no `Microsoft.CodeAnalysis`/Roslyn-generator dependency anywhere in the
tree, and every existing "C#-facing codegen" mechanism (the `#[dotnet_export]`/`#[dotnet_class]`
proc-macros, the `xmldoc.rs` sidecar-file post-processor) works at the Rust-compile or MSBuild
layer, never as a C# compiler plugin. A real Roslyn Source Generator would be justified only for a
genuinely *per-consumer-project* generation need (not per-Rust-type) — no concrete example of that
exists today, and building the machinery speculatively would add a second, heavier C#-generation
mechanism (netstandard2.0 analyzer packaging, a new debugging surface, a new versioning axis)
alongside the existing simple one with no net capability gain. **Recommendation:** treat this
backlog line as closed/re-scoped rather than pursued; if a genuine per-consumer need surfaces later
(plausibly from the flagship end-to-end example app item), the first real step would be a
throwaway standalone `IIncrementalGenerator` proof-of-concept validating the packaging mechanics,
not an integration design done in the abstract.

### 8. Incremental-build feedback — confirmed real, fix scoped

**Verdict: a genuine, now-measured UX problem with a small, additive, zero-typechecker-risk fix —
not a hypothetical concern.** Measured on a real regex-scale crate (`cargo_tests/soak_regex`, 17MB
serialized IR): default (non-verbose) `cargo dotnet build` goes **completely silent for 10-12
seconds** between the config-regeneration line and `Finished`, with no indication anything is
happening. Root cause, read from source: `feasibility/_cargo_dotnet_core.sh`'s default-mode log
filter allow-lists `Compiling std/core/alloc` but not the target crate's own `Compiling` line, and
allow-lists *no* linker output at all — even though the linker already has stage `println!`s
(`Preparing to load assmeblies`/`Loaded assmeblies`/`Eliminating dead code` in
`cilly/src/bin/linker/{load.rs,main.rs}`) that are silently thrown away by the grep in the exact
mode most users run in. The signal already exists; it's being filtered out, not missing. Separately,
even in verbose mode there is *no timing instrumentation anywhere* in the linker (`.opt()`,
`.typecheck()`, the three `ilasm` `Command::new` invocations each currently run with zero markers),
so which sub-stage actually dominates the 10s window is unknown. **Smallest safe first step (not
yet implemented):** (1) a one-line grep-allowlist edit in `_cargo_dotnet_core.sh` to pass through
the target crate's own `Compiling` line and the linker's existing stage text; (2) wrap `.opt()`,
`.typecheck()`, and the `ilasm` invocations with `Instant::now()`/`elapsed()` timing `println!`s
matching the linker's existing style. Explicitly **do not** build a progress bar yet — there's no
cheap per-item counter available (opt/typecheck/ilasm are each one opaque call over the whole
assembly), so a real progress bar needs materially bigger internal-instrumentation work than coarse
per-stage timing; that's a possible follow-up only once the timing breakdown shows one stage
dominating badly enough to justify it. **What could go wrong:** the broadened `Compiling ` grep
term could unmask dependency-compile spam that was likely filtered deliberately (verify or scope
the addition to just the target crate's own name); the linker's unmasked `println!`s have no
`==>`-style prefix matching `cargo-dotnet`'s own banner convention, so a small formatting pass is
worth doing alongside to keep the UX coherent.

### 8. Generic-interface-instantiation gap — FIXED (2026-07-07)

**What shipped:** `#[dotnet_class(implements = "…")]` can now reference a *generic* managed
interface bound to a concrete external type argument, e.g.
`implements = "[System.Runtime]System.IEquatable<valuetype [System.Runtime]System.Int32>"`. New
intrinsic `rustc_codegen_clr_add_generic_interface_impl` (mirrors the existing non-generic
`rustc_codegen_clr_add_interface_impl`, arity-1 only — a multi-argument interface like
`IDictionary<K,V>` still isn't expressible). The generic argument is a plain external-type
reference, never derived from a Rust type, exactly like `extends=`'s own superclass reference — a
`valuetype ` prefix marks a value-type argument (mirroring IL's own keyword) since there's no Rust
type to infer it from.

**A real, independent Roslyn bug was found and worked around along the way.** The first
implementation (generic argument built as a plain `ClassRef`, `VALUETYPE <TypeRef>` encoding)
produced metadata verified byte-correct via `System.Reflection.Metadata` (assembly refs, type
refs, the `TypeSpec`'s `GENERICINST` blob, the `InterfaceImpl` row — all matched ECMA-335
§II.23.2.12 exactly) under BOTH exporters, yet `csc` rejected it with `CS0648: '' is a type not
supported by the language`. Isolated via a hand-assembled `ilasm` repro (bypassing this project's
exporters entirely) that `System.IEquatable<int>` fails identically, while the *identical* shape
with a reference-type argument (`System.IEquatable<string>`) compiles cleanly — pinning the failure
to value-type PRIMITIVE arguments encoded via the generic `VALUETYPE <TypeRef>` form specifically.
**Fix:** well-known CLR primitives (`Int32`, `Boolean`, `Double`, …) are now mapped to cilly's
native `Type::Int`/`Type::Float`/`Type::Bool` variants (their dedicated ECMA-335 element-type byte,
e.g. `ELEMENT_TYPE_I4`) instead of a generic `ClassRef`, in `well_known_primitive_type`
(`src/comptime.rs`). Non-primitive value types (a user struct, `DateTime`, `Guid`, …) still use the
`ClassRef`+`VALUETYPE` form, untested against this specific Roslyn behavior — flagged as an open
question if one is ever used as a generic-interface argument this way.

**Also fixed as a genuine prerequisite** (both exporters previously only ever built extends/
implements references with empty generics, since nothing needed otherwise): `il_exporter::
simple_class_ref` now emits the arity suffix + `<…>` instantiation list for a generic `extends`/
`implements` reference (was dead code, correctly, until this feature existed); `pe_exporter::
export_pe`'s extends/implements construction now routes through `MetadataBuilder::class_ref_token`
(widened from `pub(super)`) instead of the coded-index-only `type_def_or_ref`, so a generic
reference gets a real `TypeSpec`+`GENERICINST` row instead of a bare `TypeRef` to the unbound open
generic (which the CLR rejects at load time as a `TypeLoadException` — a loud, not silent, failure
mode, but wrong either way).

**Verified:** new proof crate `cargo_tests/cd_generic_iface` (`IntBox` implements
`System.IEquatable<int>`, upcast/polymorphic-call/`is`/`as` checks) — 7/7 under both `DIRECT_PE=1`
(default) and `DIRECT_PE=0` (ilasm). Existing non-generic `implements=`/`extends=` consumers
(`cd_iface` 9/9, `cd_export` 20/20, `cd_typedef` 16/16) unaffected under both exporter paths.
`cilly` unit tests 199/199. Full Docker `::stable` gate run before commit (see commit message for
the exact pass/fail tally).

**Unblocks:** the `IAsyncEnumerable<T>` Stream-state-struct spike (finding §3) and Case C of the
richer-export-return-types item (finding §6) — both were waiting on exactly this.
