# mycorrhiza ergonomics backlog

Snapshot from a 2026-07-06 broad ergonomics survey, taken *after* the original
[ERGONOMICS_ROADMAP.md](ERGONOMICS_ROADMAP.md) campaign closed almost in full (see
[STATE_OF_THE_PROJECT.md](STATE_OF_THE_PROJECT.md) for current verified-capability truth — most
🟡/⬜ items in the older `ERGONOMICS_STATUS.md` have since shipped: dict iteration, `Span<T>`,
`Nullable<T>`, generic methods `!!N`, capturing closures, delegate-as-generic-arg,
`#[dotnet_class(implements=…)]` interfaces, and LINQ/EF expression trees are all done).

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
  touches the export codegen's signature-emission path.

## 2. Rust consuming .NET (BCL surface)

- **`Ord` for wrapped BCL types where a real, non-culture-sensitive comparison exists** (`Guid`,
  `TimeSpan`, numeric wrappers) — currently skipped as a blanket policy because `String.CompareTo`
  is culture-sensitive, but that reasoning doesn't generalize to every wrapper. **Pure library**,
  audit + fill per-type.
- **`?`-operator ergonomics for `ManagedException`** — `try_managed`/`.try_()` exist, but there's
  no reusable `From<ManagedException> for YourError` helper, so every consumer hand-rolls the
  conversion. **Pure library.**
- **`Span<T>`/`Memory<T>` deeper API** — the wrapper exists post-WF-9 unlock; audit against what
  real `Span<T>` callers reach for (slicing, subspan, `CopyTo`, `IndexOf`) and fill gaps. **Pure
  library.**
- **`IAsyncEnumerable<T>` bridge** — `Task`/`Task<T>` are bridged both ways; async *streams*
  (common in modern .NET APIs — EF Core, gRPC streaming) aren't. **Research first** (interacts
  with the same coroutine-layout wall documented in `mycorrhiza::task`).
- **A safe `Once`/lazy-init wrapper**, cross-language-shareable, natural sibling to
  `SharedMutex<T>`/`SharedRwLock<T>` shipped this session. **Pure library.**
- **A channel-style primitive** over `System.Threading.Channels` — idiomatic MPSC/MPMC channel
  backed by a real .NET implementation, letting C# be a producer/consumer (unlike
  `std::sync::mpsc`). **Pure library.**
- **`DotNetDecimal` operator/trait completeness audit** — confirm `+`/`-`/`*`/`/`,
  `Display`, `TryFrom` are as complete as the primitive-numeric wrappers; fill gaps. **Pure
  library.**

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
- **Proc-macro error message quality** — when `#[dotnet_export]`/`#[dotnet_class]` is misused
  (wrong signature shape, unsupported type in an exported position), does it emit a clear
  span-accurate `compile_error!`, or does misuse surface as a confusing downstream type error or
  ICE? **Pure library** (audit + fix, `dotnet_macros` only, no `cilly/src`).
- **`cargo dotnet doctor` breadth** — currently checks toolchain/.NET presence; could also lint
  missing `RustCrate` csproj wiring, TFM/`--dotnet`-flag mismatches, stale generated bindings.
  **Pure library/tooling.**
- **Incremental-build feedback** — no progress/timing signal during a large crate's MIR→CIL→PE
  pipeline. **Deferred** — needs profiling against a genuinely large crate first to know if it's
  even a real problem before investing in UX for it.

## 5. Documentation & discoverability

- **`INTEROP_COOKBOOK.md`/`BCL_COVERAGE.md` freshness pass** — both predate the WF-9 unlock,
  generics work, and interfaces/LINQ shipping; likely stale the same way `ERGONOMICS_STATUS.md`
  was before this survey. **Pure docs.**
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
