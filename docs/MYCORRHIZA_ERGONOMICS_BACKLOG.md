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
  unclear size until scoped).
- ~~**`IEnumerable<T>` over `RustVec<T>`**~~ — **DONE.** `RustVec<T>` and `RustBoxVec<T>`
  (`msbuild/RustDotnet.Containers.cs`, plus the inline copy in `cargo_tests/cd_rustvec/csharp/Program.cs`)
  now implement `IEnumerable<T>` via an allocation-free struct `Enumerator`, so both `foreach` and LINQ
  work directly. Verified with real `foreach`/`Sum`/`Where`/`Select` checks in `cd_rustvec`
  (44/44, up from 37/37).
- **Richer `#[dotnet_export]` return types** — `Task<T>` and `IEnumerable<T>`/`IAsyncEnumerable<T>`
  as direct return types from an exported fn, to close the loop with async/enumerator work already
  done in the other direction. **Research first** (interacts with the Task<T>-production ceiling
  documented in `mycorrhiza::task`'s own module docs).
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
  Stream-state struct is a viable Tier A follow-up.**
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
- **Source-generator-driven C# boilerplate** (e.g. auto-generating a typed wrapper class around a
  raw `RustVec<T>` handle) — currently hand-written by the consumer. **Research first** (bigger
  scope, needs a design for what a C#-side source generator would own).

## 4. Tooling / CLI (`cargo dotnet`)

- **`cargo dotnet publish --aot`** — AOT is codegen-proven end-to-end but not wired as a
  subcommand; today it's a manual recipe. **Pure library/tooling.**
- ~~**Proc-macro error message quality**~~ — **DONE, with one residual gap** (`fe8b28e`). Added
  span-accurate `compile_error!`s for unknown/malformed `#[dotnet_class]`/`#[dotnet_entity]`
  attribute keys, verified via a new `trybuild`-based `negative_ui` test suite (5/5). **Residual,
  not fixed:** `#[dotnet_methods]` applied to an impl with no matching `#[dotnet_class]` elsewhere
  in the crate currently compiles silently — detecting that needs whole-crate visibility a single
  attribute-macro invocation can't see without a larger registry-based design; flagged as a future
  item rather than attempted here to avoid scope creep into "exhaustive macro rewrite."
- **`cargo dotnet doctor` breadth** — currently checks toolchain/.NET presence; could also lint
  missing `RustCrate` csproj wiring, TFM/`--dotnet`-flag mismatches, stale generated bindings.
  **Pure library/tooling.**
- **Incremental-build feedback** — no progress/timing signal during a large crate's MIR→CIL→PE
  pipeline. **Deferred** — needs profiling against a genuinely large crate first to know if it's
  even a real problem before investing in UX for it.

## 5. Documentation & discoverability

- ~~**`INTEROP_COOKBOOK.md`/`BCL_COVERAGE.md` freshness pass**~~ — **DONE** (`a81cba0`, `0d33055`).
  Both cross-checked against `STATE_OF_THE_PROJECT.md`'s verified-capability ledger and current
  code; stale ⛔/gap markers closed, new recipes added for shipped-but-undocumented surface.
- **A flagship end-to-end example app** combining exported async endpoints + EF-queried LINQ +
  background thread pool in one real ASP.NET Core service — nothing currently shows the
  *combination* of shipped features together. **Deferred** — significant standalone effort, do
  after the above lands so the flagship reflects the improved ergonomics rather than the current
  ones.
- **`mycorrhiza` rustdoc landing-page pass** — confirm `cargo doc -p mycorrhiza` has a top-level
  narrative pointing a newcomer at `prelude`, not just a flat module list. **Pure docs.**

## 6. Testing ergonomics (for external consumers, not just us)

- **`cargo dotnet test` `#[should_panic]`/`#[ignore]`/filtering support** — confirm parity with
  what `cargo test` normally supports, not just the happy path. **Pure library/tooling.**
- **A documented, reusable differential-testing pattern** for external consumers (we use "native
  Rust as oracle vs `CARGO_DOTNET_BACKEND=native`" internally; not written up for outside use).
  **Pure docs.**

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

### 1. `#[dotnet_class]` virtual-method overrides

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
emitted metadata name, not just "the C# compiles."
