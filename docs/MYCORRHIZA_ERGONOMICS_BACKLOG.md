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

**Current-truth update:** both small return-type wins subsequently shipped. `marshal_return`
accepts `Task`, `TaskT<T>`, and primitive `Vec<T>`; `cd_export` and the product-shaped
`cd_efcore_async` C# host prove ordinary and real multi-await `Task<int>` consumption. The open
part of the richer-return item is true incremental `IAsyncEnumerable<T>`, not Task or materialized
collection return.

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
- ~~**Richer `#[dotnet_export]` return types: `Task<T>` and materialized `IEnumerable<T>`**~~ —
  **DONE.** `Task`/`TaskT<T>` pass managed handles through the export seam, and primitive `Vec<T>`
  becomes the existing C# `RustVec<T>`/`IEnumerable<T>` wrapper. True incremental
  `IAsyncEnumerable<T>` remains a separate blocked stream-production item; see the findings below.
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
- ~~**`Span<T>`/`Memory<T>` deeper API**~~ — **DONE, with one documented gap.** `Span<T>` and
  `ReadOnlySpan<T>` provide zero-copy borrowed views with slicing, `CopyTo`, and Rust-side
  `contains`/`index_of`. `Memory<T>` and `ReadOnlyMemory<T>` now copy into GC-owned arrays for
  retained/async-safe handoff, with managed slicing, mutation, and `CopyTo` (`cd_span` 68/68). This
  also closes the guarded `!0[]` generic-constructor signature path. Did **not** wire `IndexOf`/
  `Contains` to the real `MemoryExtensions` static generic methods — that needs a static generic
  call whose argument is itself a generic-struct type constrained on `T: IEquatable<T>`, not
  reachable through the existing generic-interop intrinsics without new `cilly/src` support;
  documented in `span.rs`'s module docs as a real, scoped gap rather than silently worked around.
  `cd_span` 45/45.
- **`IAsyncEnumerable<T>` bridge** — the consumer direction now ships as
  `mycorrhiza::enumerate_async`: Rust incrementally drives `GetAsyncEnumerator` / `MoveNextAsync` /
  `Current`, including pending `ValueTask<bool>` operations (`cd_async_stream`, debug + release).
  The producer direction (Rust `async fn` → managed async stream) remains open. **Research first** (interacts
  with the same coroutine-layout wall documented in `mycorrhiza::task`). **Researched 2026-07-06,
  see findings below — blocked for the sugared `async fn` case; a hand-rolled non-overlapping
  Stream-state struct is a viable Tier A follow-up. The blocking generic-interface-instantiation
  gap noted here (2026-07-07) is now FIXED** (`rustc_codegen_clr_add_generic_interface_impl` +
  `implements = "…<[Asm]Ns.Ty>"` / `"…<valuetype [Asm]Ns.Ty>"` syntax) — see the new Tier C
  finding §8 below for the full story, including a real Roslyn bug isolated and worked around along
  the way. The Stream-state-struct producer spike itself is still open (this only unblocked the interface
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
  `<RustCrate>` — the same shape as `cargo_tests/cd_*/csharp`, or `cargo dotnet new --lib` / `--plugin`).
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
- ~~**Incremental-build feedback**~~ — **DONE.** The typed `cargo-dotnet` driver now reads the inner
  Cargo process incrementally instead of waiting on `Command::output()`, mirrors ordinary
  `Compiling ...` lines plus the linker's existing `==> ...` stages as they happen, and still writes
  the complete output to `_lastbuild.log`. `--verbose` remains unfiltered and failures still replay
  the complete log. A unit regression keeps consumer-crate and linker progress in the default view.

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

> **UPDATE (2026-07-07) — the `pe_exporter` / `DIRECT_PE=1` gaps in findings #1, #2 and #5 are now
> CLOSED.** All three features (virtual-method overrides, interface export, `.NET` events) work on
> the **default** hand-rolled PE-writer path — the individual "IL-exporter-only / `DIRECT_PE=1` not
> supported / loud assert guard" notes below are HISTORICAL. `pe_exporter/tables.rs` gained the
> `MethodImpl` (§II.22.27), `EventMap`/`Event`/`MethodSemantics` (§II.22.12/13/28) tables plus
> `Interface`/`Abstract` `TypeDef` flags and abstract-method RVA=0 handling; `pe_exporter/export.rs`
> wires them from the IR (`ClassDef::with_interface`/`add_event`, `MethodDef::with_override`/
> `with_abstract`), and the loud asserts were removed. **This was the right call per the project's
> own direction** (`docs/PE_EMISSION_PLAN.md`: the PE writer is the default precisely to get `ilasm`
> out of the loop) — and it also **dissolved the Mono-`ilasm`-at-scale event blocker** described in
> finding #5's earlier update: under `DIRECT_PE=1` no `ilasm` runs at all, so the ~500K-line `.il`
> that Mono choked on is never produced. Proven end-to-end under the default path:
> `cargo_tests/cd_override` 5/5 (incl. the decisive `((object)g).ToString()` base-slot dispatch) and
> `cargo_tests/cd_event` 4/4 (`+=`/`-=` + `GetEvent` reflection), both built with zero `ilasm`; plus
> a `pe_exporter` structural-readback unit test for the interface `TypeDef`
> (`interface_type_def_and_abstract_method_are_emitted_by_pe_writer`).
>
> **UPDATE (2026-07-07, follow-up) — interface-export now has its macro surface too, so ALL THREE
> features are fully Rust-facing and fully PE-writer-emitted.** `#[dotnet_interface]` on a Rust
> `trait` (each method `fn Foo(&self, …) -> …`, no body) synthesizes a genuine C# interface via two
> new comptime intrinsics (`rustc_codegen_clr_mark_interface` + `rustc_codegen_clr_add_abstract_
> method_def`, whose signature-carrier fns are never codegen'd — abstract members have no body) →
> `PendingClass.is_interface`/`abstract_methods` → `ClassDef::with_interface()` + abstract
> `MethodDef::with_abstract()`. Proven end-to-end on the default `DIRECT_PE=1` path by
> `cargo_tests/cd_interface` 4/4: a Rust `trait ISpeaker { fn Speak(&self); fn Volume(&self) -> i32; }`
> is implemented by a C# `class Parrot : ISpeaker` (which only COMPILES against a real interface),
> used polymorphically through the interface, and reflects as `typeof(ISpeaker).IsInterface == true`
> with `Parrot.GetInterfaces()` listing it — zero `ilasm`. Nothing remains for these three features
> at the shipped scope (deeper items — base-ctor-chaining for overrides, thread-safe event bodies,
> default interface methods — stay explicitly out of scope, see each finding).
>
> **UPDATE (2026-07-07, ref/out parameters)** — a `#[dotnet_interface]` member's `&mut T`
> (thin, sized `T`) parameter now maps to a managed byref (`ELEMENT_TYPE_BYREF` → C# `ref T`)
> instead of the frontend's uniform `T*` lowering, and `#[dotnet_out]` on such a parameter stamps
> `ParamAttributes.Out` (0x0002) on its `Param` row (→ C# `out T`). Implemented as a TARGETED
> comptime-layer rewrite of the sig-carrier's lowered signature (`byref_interface_sig` in
> `src/comptime.rs`, driven by the carrier's Rust-level `TyKind::Ref(_,_,Mut)` so it works through
> type aliases), NOT a `get_type` change; `MethodDef` gained `out_params: Vec<u16>` (serialized-IR
> format change — fingerprint trap applies) re-applied across `asm_link` and consumed by
> `pe_exporter`'s Param-row loop (+ il_exporter `[out]` parity). Applies to instance AND
> `static abstract` members. Proven by `cargo_tests/cd_interface` 15/15 (C# `Cell : IRefCell`
> implements `void Fill(ref int)`, `void FillOut(out int)`, mixed params, static-abstract
> `Reset(ref int)` via `T.Reset`, plus `IsByRef`/`IsOut` reflection through the linker).
> Loud rejects: shared `&T` (C# `in` needs `modreq(InAttribute)`), reference returns,
> `#[dotnet_out]` on non-`&mut`/static/event params (macro, `syn::Error`), `&mut str`/`&mut [T]`/
> `&mut dyn` even alias-hidden (comptime panic). **Known wall (documented escape hatch respected):
> Rust-side IMPLEMENTORS are out of scope** — `#[dotnet_methods]` class virtuals still lower
> `&mut T` to `T*`, so a Rust `#[dotnet_class]` naming a byref-parameter interface in
> `implements=` fails LOUDLY at CLR type load (`TypeLoadException` naming the member), never
> silently wrong; the natural follow-up is applying the same rewrite to class-virtual DECLARED
> sigs while the `AliasFor` target keeps `Ptr` (byref/ptr agree at `ldind`/`stind` level), but
> that changes every existing `#[dotnet_methods]` `&mut` surface, so it is explicitly deferred.
> `*mut T`/`*const T` still emit `T*` everywhere (unchanged escape hatch).
>
> **UPDATE (2026-07-07, default interface methods)** — an *instance* `#[dotnet_interface]` trait
> fn WITH a default body is now a genuine .NET **default interface method** (DIM, CoreCLR 3.0+):
> a virtual, NON-abstract `MethodDef` with a real IL body (RVA != 0) on the interface `TypeDef`,
> inherited by a C# class that omits the member and beaten by one that defines it. The PE writer
> needed ZERO changes (Pass 3 already stamps `Virtual|NewSlot` without `Abstract`; Pass 4
> assembles the `AliasFor` body — same pipeline as `#[dotnet_class]` virtuals, just interface-
> owned). The macro LIFTS the default body into a free fn (`__iface_dim_<M>` + `#[used]` KEEP
> anchor): `self` becomes the explicit interface handle and every `self.<trait_method>(…)` call is
> rewritten to `this.instanceN::<"M", …>(…)` — a `callvirt` through the handle, so an inner
> self-call dispatches to the implementing CLASS's definition (proven). New intrinsic
> `rustc_codegen_clr_add_default_method_def` → `PendingClass.default_methods` → a
> `MethodKind::Virtual + MethodImpl::AliasFor` member with no `.with_abstract()`. Proven by
> `cargo_tests/cd_dim` 10/10 (DIM runs; inner self-call hits `Minimal.Base`; args; self-free;
> DIM-calls-DIM; class override wins; inherited DIM dispatches into `Overrider`'s members;
> `!IsAbstract && IsVirtual` reflection; abstract sibling unharmed) + a `pe_exporter` readback
> unit test (`default_interface_method_gets_body_and_stays_nonabstract`). Loud rejects (all
> macro-level `syn::Error`, spot-verified): `self`/`Self` in any un-lowerable shape (macro bodies
> like `println!("{}", self.x())` — a token-scan BACKSTOP behind the AST rewriter), self-calls to
> non-trait/supertrait members, >2-argument self-calls (`instanceN` ladder), turbofish, default
> bodies on `static` (receiver-less) members (non-abstract static virtuals aren't emitted) or
> `#[dotnet_event]` members, and reference (`&T`/`&mut T`) parameters on a defaulted method
> (byref declared surface vs raw-pointer lifted body would mismatch) — including self-CALLS to
> byref-parameter members from inside a DIM. `HideBySig` is still omitted (0x146 vs Roslyn's
> 0x1C6) — proven tolerated, same as abstract members/events.
>
> **Hardening pass (2026-07-07, post-adversarial-review).** The macro-level rejects above are
> SYNTACTIC, so a type alias (`type Slot<'a> = &'a mut i32;`) used to slip past them and ship a
> silently-wrong assembly (verified: a DIM self-call to an alias-hidden byref member exported a
> phantom non-abstract `Fill(int32*)` next to the real `Fill(ref int32)` — `AmbiguousMatchException`
> on reflection, runtime throw on call). Now closed with three fail-loudly backstops, none of which
> touch the typechecker: (1) **linker** (`asm.rs` `patch_missing_methods`): an in-assembly
> `MethodRef` that matches no `MethodDef` on an *interface* TypeDef panics at link time (naming the
> member and the likely alias/generic cause) instead of materializing a `Missing` stub — this
> catches the whole dangling-interface-ref defect class; (2) **comptime** (`src/comptime.rs`):
> the `default_methods` loop rejects reference params/returns from the target's RUST-level types
> (authoritative through aliases), and `byref_interface_sig` now also panics on alias-hidden shared
> `&T` params and reference returns (literal spellings were already macro-rejected); (3) **macro**:
> DIM self-calls to GENERIC members are rejected loudly (a shadowing concrete type used to make
> `self.Conv(Meters(3))` compile into a phantom member; a non-shadowed `T` produced a confusing
> E0425), and `async fn`/`const fn` interface members are rejected (an `async fn` used to silently
> emit a synchronous member instead of anything `Task`-shaped).

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

### 2. Export Rust traits as C# interfaces (reverse of `implements=`) — SPIKE DONE (2026-07-07)

**Smallest-safe-first-step spike shipped, exactly as scoped below — `cilly`-only, IL-exporter-only,
no macro/comptime/typechecker/PE-writer work.** `ClassDef::with_interface()` (new bool flag,
default `false`, additive) marks a `ClassDef` as a genuine ECMA-335 `interface` `TypeDef`;
`MethodDef::with_abstract()` (same pattern) marks a member as `Abstract`/`NewSlot`/no-body
(RVA=0). `il_exporter` emits `.class {vis} interface abstract ansi '{name}'{implements}{{` with
**no** `extends` clause at all (even the usual implicit `[System.Runtime]System.Object` is illegal
for an `Interface`-flagged `TypeDef` — CoreCLR rejects it at load time), and
`newslot abstract virtual instance` + an empty `{}` body (no `.maxstack`, no `.entrypoint` — both
are body-only directives `ilasm` rejects on an abstract method) for each abstract member.

Proven with `cilly/src/ir/asm::export_interface` (a `#[test]`, part of the persisted `cargo test -p
cilly` suite) hand-building an `Assembly` with one interface `ISpeaker` and one abstract member
`Speak()`, exported through the real `il_exporter` + `ilasm` — `ilasm` accepts the emitted IL
without error. **Also hand-verified the actual ask from this finding** (CoreCLR genuinely treating
it as an implementable interface, not just something `ilasm` tolerates): a scratch C# console app
implementing `ISpeaker` on a `Parrot` class, called through the interface type, reports
`is ISpeaker: True` and `typeof(Parrot).GetInterfaces()` correctly lists `ISpeaker` — a real,
consumable interface, verified via reflection and a virtual dispatch, not just a load-time check.

**Same lesson as the virtual-override work applies and was pre-empted this time:** both new fields
are threaded through `asm_link.rs`'s `translate_method_def`/`translate_class_def` (the linker's
cross-rlib merge pass) up front, plus a `merge_defs` assert requiring `is_interface()` to agree
across re-opened class definitions — this was the exact class of bug (a field silently dropped by
that merge pass) found and fixed for the override feature, so it's handled proactively here instead
of needing a second debugging pass.

**Update (2026-07-07, generic interfaces SHIPPED, PE-writer-first):** `#[dotnet_interface]` now
accepts a plain type-parameter list — `trait IBox<T>` emits a genuine GENERIC interface
DEFINITION: the TypeDef is named with the CLS backtick-arity suffix (`IBox`1`), one ECMA-335
`GenericParam` row (§II.22.20, new table 0x2A in `pe_exporter/tables.rs`, sorted by
Owner/Number) per parameter, and a bare `T` in a member's parameter/return position lowers via
the pre-existing `RustcCLRInteropTypeGeneric<N>` marker to `ELEMENT_TYPE_VAR N` (no
`SIG_GENERIC` convention bit — that is generic-METHOD-only). The macro also emits a
PARAMETERIZED `IBoxHandle<T>` alias (over `RustcCLRInteropManagedGeneric`), and the PE writer
gained in-assembly open-generic resolution (`find_open_generic_def`): an instantiated reference
to the assembly's OWN generic type (e.g. an exported fn taking `IBoxHandle<i32>`, or
`#[dotnet_class(implements = "IBox<…>")]` against the local definition) resolves to a `TypeSpec`
over the local TypeDef instead of a dangling external `TypeRef` with a doubled arity postfix.
Proven by `cargo_tests/cd_iface_generic` (9/9: open-definition reflection incl. the `T` name,
`IBox<int>` AND `IBox<string>` implementors — two instantiations = genuine genericity — generic
C# helper dispatch, and C# calling an exported Rust fn typed `IBox<int>`). Loudly rejected
(macro errors, all negative-tested): lifetime/const params, defaults, bounds/`where` clauses (no
`GenericParamConstraint` emission — a bound would be silently-dropped metadata), `T` anywhere
but a bare param/return position, default bodies on generic traits (the lifted DIM free fn can't
scope `T`), and a generic param as an event delegate. Generic CLASS definitions remain walled
(comptime assert — the no-explicit-layout-on-generics ban applies to classes, not interfaces),
and Rust-side CALLING through `IBox<int>` (constrained dispatch on the local generic interface)
is untested stretch surface. NOTE: `ClassDef` gained a serialized `generic_names` field —
postcard `.bc` format change (the documented fingerprint trap: rebuild dylib+linker together,
clean consumers).

**Remaining scope, not started (this was intentionally just the spike):** wiring this into
`#[dotnet_class]`/`dotnet_macros`/`src/comptime.rs` so an actual Rust `trait` synthesizes one of
these (a genuine comptime-intrepreter + macro-attribute addition, comparable in size to the
`implements=` machinery itself); `pe_exporter` support (no `Interface`/`Abstract` `TypeDef` flags
or abstract-method RVA=0 handling exist there yet — same `DIRECT_PE=1` gap class as the
virtual-override work, needs its own loud guard before this is wired into macros); default
interface methods, static interface members, and interface-to-interface `extends` (an interface
CAN implement other interfaces via the ordinary `implements` clause — untested here). The original
Tier C verdict below still describes the honest size of the FULL feature.

**Original Tier C verdict, for context:**

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
`cilly/src` + root `src/type` work requiring the full Docker gate and new
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
only — the general MIR-codegen path has no such signal without new root `type`/`call_info`
plumbing) down through a new field on `cilly::ir::method::Method`/`FnSig`
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

### 5. .NET events (`add_*`/`remove_*`) on exported classes — SHIPPED, DIRECT-PE GATED

**Current status:** class and interface events are emitted by both the IL exporter and the default
direct-PE writer. `feasibility/event_acceptance.sh` continuously compiles real C# `+=`/`-=` syntax,
checks reflected event/accessor metadata, exercises interface dispatch, and proves the direct path
does not execute ILAsm. Accessor-body synchronization remains application-owned.

**Historical implementation record follows.** The IL-only and missing-PE statements below describe
the intermediate 2026-07-07 state, before EventMap/Event/MethodSemantics landed in the PE writer.

**Both the hand-written-ilasm spike AND a real `cilly` capability shipped, exactly as scoped
below.** Step 1 (hand-written `.il`, zero `cilly` involvement): a `.event`/`.addon`/`.removeon`
block for a `Notifier.Changed` event with genuine `Delegate.Combine`/`Delegate.Remove` bodies,
assembled with `ilasm` and hand-verified against a real C# consumer — `+=`/`-=`, multi-subscriber
fan-out via `Delegate.Combine`, and `typeof(Notifier).GetEvent("Changed")` reflection are all
correct, confirming CoreCLR treats hand-rolled event metadata as a genuine `event`, not just
something `ilasm` tolerates.

Step 2 (real `cilly` capability, `il_exporter`-only): `ClassDef::add_event` + a new `EventDef`
struct (name, delegate `Type`, `add`/`remove` `MethodRef`s — the `add_`/`remove_` bodies are
*ordinary* `MethodDef`s already emitted by the normal per-method loop; `EventDef` only links their
names into the Event-shaped IL block, exactly as predicted — no new invocation semantics). New
`il_exporter::method_ref_operand_text` helper (factored out of the existing `CILNode::Call` operand
builder, which needs the identical `<class>::'<name>'(<params>)` full-signature text) builds the
`.addon`/`.removeon` operands. Proven by `cilly::ir::asm::export_event` (persisted unit test,
hand-builds one `Notifier`/`Changed` event with trivial `ret`-only bodies — its job is to prove the
*metadata linking* is well-formed IL `ilasm` accepts, not to re-prove the runtime semantics already
covered by Step 1's C# consumer).

**At that intermediate point, `pe_exporter`/`c_exporter` support did not exist** — the same
`DIRECT_PE=1` gap class as virtual
overrides and interface export. No EventMap/Event/MethodSemantics tables were added to
`pe_exporter/tables.rs` (would need the exact row-sorting care this finding's "what could go wrong"
section originally called out). `EventDef` currently has no assert-guard in either exporter (there
is no event-emission code path there AT ALL yet to guard — `ClassDef::events()` is simply never
read outside `il_exporter`), but this is a live gap: once events are wired past `cilly` (macro/
comptime work), both `pe_exporter::export_pe` and `c_exporter::export_class` need the same loud
`assert!(class_def.events().is_empty(), ...)` guard pattern used for `is_interface`/`overrides`
before this can ship past the spike stage.

**Adversarial review caught a real asymmetry** (see [[mycorrhiza-ergonomics-backlog-campaign]] for
the reviewing session's full findings): the virtual-override commit added a loud
`pe_exporter`/`c_exporter` guard the moment `MethodDef::with_override` was added, but the
interface-export commit (`63804ee`) did NOT add the equivalent guard for `is_interface`/
`is_abstract` — a real gap since `pe_exporter` is the DEFAULT export path (`DIRECT_PE=1`). Fixed
retroactively (same commit as the events work): `pe_exporter::export_pe` now asserts
`!class_def.is_interface()` and `!method.is_abstract()`; `c_exporter::export_class`/
`export_method_def` assert the same plus `def.overrides().is_none()` (C mode has no override/
interface concept either — this exporter had NO guards for `overrides` before this fix, a gap
missed in the original override commit). Also added: an `il_exporter` guard rejecting instance
fields on an interface `ClassDef` (ECMA-335 forbids them, nothing enforced it before), and a guard
rejecting the nonsensical `is_abstract() && overrides().is_some()` combination (an abstract member
has no body for a `.override`'s `MethodImpl` row to attach to). None of these were live bugs (no
code path reaches them yet outside hand-built test assemblies) but all three close latent traps for
whoever wires the next layer.

**What could go wrong** (unchanged from the original research, still applies to the un-shipped
`pe_exporter` work): MethodSemantics/EventMap row-ordering bugs in the PE writer; the `add_`/
`remove_` bodies need genuine thread-safety (`Interlocked.CompareExchange`-based) for real
concurrent-subscription correctness — the shipped spike's bodies are deliberately trivial/
non-thread-safe, matching the "smallest safe first step" scope, not a production-ready event; and a
naming-scheme decision for the backing delegate field once this reaches production use.

**UPDATE (2026-07-07): macro/comptime wiring done, TWO real bugs found and fixed, one real
end-to-end blocker discovered and NOT yet resolved.** `#[dotnet_event("Name")]` on a pair of
`add_*`/`remove_*` methods in a `#[dotnet_methods]` impl now drives `ClassDef::add_event` through
new `mycorrhiza::comptime::rustc_codegen_clr_mark_last_method_event_add`/`_remove` intrinsics — the
delegate type is inferred from the method's own second parameter (the subscriber value), never a
separately-spelled string. Two real `il_exporter` bugs were found and fixed while wiring this up
(both were invisible to the earlier hand-built-`ClassDef` spike, which happened to construct
self-consistent metadata by hand):
1. The `.event`/`.addon`/`.removeon` block used the wrong type-formatting helper (the body-position
   `non_void_type_il`, not the declaration-position `non_void_type_il_signature` the `.method`
   header itself uses) — produced a real assembly-qualifier mismatch (`[System.Private.CoreLib]`
   vs `[System.Runtime]` for the identical delegate type) that `ilasm` correctly rejected with
   "Invalid Add method of event".
2. The `add_`/`remove_` methods themselves weren't marked `specialname` — `ilasm` requires this on
   event accessors (confirmed by a hand-written `.il` file that worked WITH `specialname` and one
   that failed without it). Fixed via a class-events lookup in `il_exporter::emit_one_method`
   (checking whether this `MethodDefIdx` is referenced as an `add`/`remove` in its own class's
   `EventDef`s) rather than a new `MethodDef` flag, since `MethodDefIdx` already IS the method's
   own `Interned<MethodRef>` — no new state needs threading through the linker's merge pass.

Both fixes are proven correct via `cilly::ir::asm::export_event` (still passes) AND two independent
hand-assembled `.il` repros (a minimal `Notifier` class with the exact emitted shape, PLUS the same
class prefixed with the real generated assembly's full 26-entry `.assembly extern` list) — both
assemble cleanly with `ilasm`.

**However, the actual end-to-end proof crate (`cargo_tests/cd_event`, since removed) still fails
to build under `DIRECT_PE=0`** with the SAME "Invalid Add method of event" error, even after both
fixes — despite the emitted `Notifier` class being byte-for-byte structurally identical (modulo
irrelevant `.line` debug directives) to the two isolated repros that DO assemble cleanly. The
difference is scale: the real crate's generated `.il` is ~500K lines (the entire monomorphized
std library gets baked in even for a ~15-line Rust program, a known pre-existing characteristic of
this backend, not something the events work caused) versus a ~30-line hand-built repro. This
strongly suggests a genuine Mono `ilasm` limitation/bug at extreme file scale (tested against both
Mono ilasm 6.14.1.0 locally on macOS and 6.8.0.105 inside the project's own Docker dev image — same
symptom on both), analogous in kind (though not mechanism) to the Roslyn CS0648 bug found earlier
in this campaign for generic interface instantiation — i.e., a third-party tool limitation, not
necessarily a defect in this project's emitted metadata. **Not yet resolved** — candidate next
steps: try CoreCLR's `ilasm` instead of Mono's (this project already has version-gated CoreCLR
ilasm plumbing for other reasons, per `cargo-dotnet --dotnet` version selection); try shrinking the
demo crate's std footprint (a `#![no_std]`-adjacent minimal build, if this backend's PAL supports
one); or investigate whether `DIRECT_PE=1` (the default `pe_exporter` path) sidesteps `ilasm`
entirely once it gains real Event/EventMap/MethodSemantics table support (see the `pe_exporter` gap
noted below — it currently has NO code path that reads `ClassDef::events()` at all, so it silently
drops any event's metadata rather than emitting it, wrong OR right).

**Original Tier C verdict, for context:**

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

> **Current status:** Cases A and B below shipped after this research snapshot. The macro still
> correctly rejects `async fn` sugar, but ordinary functions can return `Task`/`TaskT<T>` and
> primitive `Vec<T>` directly. Case C remains open/blocked.

**Original verdict: three cases of wildly different size.** At the time of this snapshot,
`#[dotnet_export]` hard-rejected `async fn` and lacked container/Task return arms.

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

**Sequencing result:** Cases A and B shipped as independent `dotnet_macros` changes. Case C remains
blocked pending the Stream-state-struct work; generic interface instantiation itself has since
shipped and is no longer the blocker.

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

### 9. Generic METHODS on `#[dotnet_interface]` (`fn Echo<T>(&self, value: T) -> T`) — SHIPPED (2026-07-07, PE-writer default path)

**What shipped:** a plain type-parameter list on an *instance* trait method inside
`#[dotnet_interface]` now emits a genuine ECMA-335 generic method DEFINITION on the interface's
TypeDef: the member's `MethodDefSig` blob carries `SIG_GENERIC` (0x10) + a compressed
`GenParamCount` (§II.23.2.1), one METHOD-owned `GenericParam` row (§II.22.20, coded
`TypeOrMethodDef` owner tag 1) is emitted per declared parameter (reusing the table
infrastructure the generic-interfaces feature landed for TYPE-owned rows), and a bare `T` in a
parameter/return position lowers to `ELEMENT_TYPE_MVAR N` (`!!N`) via the pre-existing
`RustcCLRInteropMethodGeneric<N>` marker (`GenericKind::CallGeneric`). C# implements and calls
it as an ordinary generic interface method — proven at value AND reference instantiations, two
parameters (`fn First<K, V>`), reflection round-trip (`IsGenericMethodDefinition`,
parameter-name readback, `MakeGenericMethod(typeof(int)).Invoke(...)` — full loader validation),
and MIXED with a generic owning interface (`trait IPicker<T> { fn Pick<U>(&self, a: T, b: U) ->
U; }` — `!0` and `!!0` in one signature). Plumbing: `MethodDef::generic_params`
(serialized-IR change — fingerprint trap applies), linker field re-application in
`asm_link.rs`, `export.rs` Pass 3 (SIG_GENERIC + rows + loud in-range asserts for every
`!!N`/`!N` marker), intrinsic `rustc_codegen_clr_add_generic_abstract_method_def`
(`;`-separated name list, substring-dispatch audited), il_exporter `<T, U>` header parity.
Proof crate: `cargo_tests/cd_iface_genmethod` (13/13).

**Walls kept loud (macro-level `syn::Error`, all verified firing):** bounds (`fn f<T: Clone>`)
and `where` clauses (no `GenericParamConstraint` emission — a constraint would be silently
dropped metadata), lifetime/const parameters, parameter defaults, composite uses of a parameter
(`&T`, `Vec<T>`, tuples — only bare `x: T` / `-> T` positions lower), generic parameters on a
`#[dotnet_event]` member (accessors have a fixed `void (Delegate)` shape), on a default-body
(DIM) member (the lifted body would need a generic IL body — a generic MethodBody the backend
doesn't emit), and on a static (receiver-less) member (generic `static abstract` needs the
static-virtual flag combination plus generic-def support in one member — deferred). Like
`static abstract`s, a generic member is declaration-only surface from Rust: calling it through
the handle would need the `!!N` call-site infra pointed at an interface `callvirt` +
`constrained.`-free dispatch — the existing `call_gmethod` path can already bind a `MethodSpec`
to an in-assembly MethodDef, so this is feasible follow-up work, not a wall.
