# Rust/.NET parity roadmap

Where this stands: how close can Rust (via `rustc_codegen_clr`/`mycorrhiza`) get to doing what a real,
large production C# codebase does? Grounded against `~/Code/monark/primary-offerings` (1.86M lines,
16 projects, ASP.NET Core + EF Core, settlement/ATS/primary-offering fintech domains) — every claim
below was independently built and re-verified against real .NET (CoreCLR), not assumed.

## Tier 0 — bugs found this campaign, fix these first

These aren't missing features, they're defects in things that otherwise work. Ordered by severity.

1. **FIXED — `TypeLoadException` on complex async coroutines** (`cargo_tests/cd_efcore_async`). Root
   cause: `Type::is_gcref` (`cilly/src/ir/tpe/mod.rs`) was shallow — it never recursed into a
   value-type struct's own fields, so `mycorrhiza::task::TaskFuture<T>` (which nests a real `Task<T>`
   object reference inside an otherwise-plain struct) slipped past `cilly`'s compile-time
   `layout_check` undetected. When a coroutine's rustc-computed layout happened to reuse that exact
   byte offset for a DIFFERENT, non-gcref field in another suspend-point variant, CoreCLR's class
   loader (correctly) rejected the type at load time with *"contains an object field at offset 16
   that is incorrectly aligned or overlapped by a non-object field."* Fixed by (1) a new recursive
   `Type::contains_gcref` closing the detection gap, (2) `ClassDef::layout_check` now reasons about
   offset-consistency across overlapping variants (a gcref-shaped field reused identically across
   variants — the pattern `cargo_tests/cd_persisted_async` already relies on and is proven safe on
   real CoreCLR — is allowed; a colliding DIFFERENT type at the same offset is rejected), and (3)
   `coroutine_typedef` (`src/type/mod.rs`) now relocates a colliding field to a
   freshly-appended, non-overlapping offset instead of reusing the unsafe one, so the workflow
   actually runs (not just fails cleanly). Verified end-to-end on real CoreCLR: `run_investor_workflow()`
   returns `2001` as expected; `cd_persisted_async` 4/4, `cd_async` 9/9, `cd_delegates` 14/14,
   `cd_efcore` 16/16 all still pass (no regressions).
2. ~~**`#[dotnet_override]`/`#[dotnet_class(extends=...)]` segfaults (exit 139) subclassing a real
   framework base class**~~ **FIXED.** Root cause was NOT the "general base-class wrapping" gap
   `rustc_codegen_clr_mark_last_method_override`'s doc warned about (that concern — protected
   ctors/members, sealed methods — remains a real, separate limitation for MORE complex cases, but
   wasn't what crashed here). The actual bug: a class described by `#[dotnet_class(extends =
   "...")]` is ALSO re-opened by its `#[dotnet_methods]` `impl` block's own comptime entrypoint,
   which has no access to the struct's `extends=` attribute — it used to hardcode
   `rustc_codegen_clr_new_typedef::<NAME, false, "System.Runtime", "System.Object">()`. Comptime
   entrypoint order is NOT guaranteed, and `finish_type`'s idempotent re-open path only ever merged
   FIELDS, never `extends` — so whichever entrypoint's `new_typedef` call ran FIRST permanently
   decided the class's real base, and for `cd_bgservice_bgtest`'s codegen-unit ordering that was
   always the `#[dotnet_methods]` one. The emitted `TypeDef` ended up `extends
   [System.Runtime]System.Object` even though its `.ctor` correctly chained to
   `BackgroundService::.ctor()` and its `.override` clause correctly named
   `BackgroundService::ExecuteAsync` — an inconsistency invisible in IL text but fatal at CLR
   type-load time: an explicit `.override` naming a base method that isn't anywhere in the
   (wrongly Object-rooted) actual hierarchy makes CoreCLR's
   `MethodTableBuilder::FindDeclMethodOnClassInHierarchy` walk off the end and dereference a null
   `MethodTable*` (confirmed via `lldb`: `EXC_BAD_ACCESS address=0x20`, i.e. a null pointer + the
   0x20 field offset it tried to read). Reproduced identically with a MINIMAL isolate-probe (plain
   non-abstract override of a simple user-compiled base class, one private field, no interfaces,
   no abstractness) — proving the defect was general, not `BackgroundService`-specific. Fixed by
   (1) `#[dotnet_methods]`'s generated `new_typedef` call now passing empty `INHERITS`/
   `INHERITS_ASM` ("no opinion") instead of a false `"System.Object"` claim, and (2)
   `finish_type`'s reuse path now applying a later entrypoint's real `extends` via the new
   `ClassDef::set_extends`, asserting instead of silently picking one if two entrypoints ever
   genuinely disagree. Verified: `cd_bgservice_bgtest` (`BackgroundService` + `.override
   ExecuteAsync`) loads, `Activator.CreateInstance` succeeds, and a real
   `Host.CreateDefaultBuilder()` `StartAsync`/`StopAsync` lifecycle runs it end to end — plus no
   regression on `cd_override` (5/5, the `System.Object.ToString()` proof) or `cd_bgservice`'s main
   `implements=`-only proof (9/9). While auditing this, also found and fixed a REAL (if
   non-crashing) second bug in the same area: `is_bcl_assembly()`'s blanket
   `name.starts_with("Microsoft")` — see item 3 below, now also fixed, though it turned out NOT to
   be this segfault's cause (both bugs were independently real; the wrong-PKT `AssemblyRef` still
   resolved by name at runtime, so fixing it alone didn't change the crash — only the `extends` fix
   did).
3. **FIXED.** `is_bcl_assembly()` misclassified `Microsoft.*`-named NuGet packages as
   shared-framework-signed (`cilly`, both `il_exporter` and `pe_exporter/tables.rs`) — any
   assembly name starting with `"Microsoft"` got stamped with CoreLib's public-key token
   (`B03F5F7F11D50A3A`) regardless of its real signing key. Replaced with a small, verified
   `bcl_public_key_token()` table: `System.*`/`mscorlib`/`netstandard`/`WindowsBase` still get the
   ECMA token; the `Microsoft.Extensions.*`/`Microsoft.AspNetCore.*`/`Microsoft.EntityFrameworkCore*`
   family gets their REAL token (`ADB9793829DDAE60`, confirmed via `ikdasm` against the actual
   net8.0 `Microsoft.Extensions.Hosting.Abstractions.dll`'s own `AssemblyRef` rows — it references
   `Microsoft.Extensions.DependencyInjection.Abstractions` with that exact token); every other
   `Microsoft.*` name now correctly falls through to a name-only extern. Still a curated table, not
   full per-assembly detection (that remains future work if a NEW `Microsoft.*` family shows up
   needing its own token), but no longer a blanket prefix guess. NOTE: this turned out to be a
   real, independent bug from item 2's segfault, NOT its cause — confirmed by fixing it alone
   first and observing the identical crash (see item 2's writeup for the actual root cause and
   why the wrong token didn't matter here: CoreCLR still bound the `AssemblyRef` by simple name).
4. **FIXED.** PE exporter never emitted `Param`-table names for exported methods (`cargo_tests/cd_mvc`).
   Confirmed via ASP.NET Core's `RequestDelegateFactory`, which threw
   `ArgumentException: An item with the same key has already been added. Key: ""` the moment a Rust
   function/method with 2+ parameters was passed directly as a route-handler delegate — every param
   reflected with `Name == ""`. Root cause was NOT the exporter — it was `src/comptime.rs::finish_type`
   hardcoding `vec![None; ...]` for parameter names on every comptime-synthesized class-method alias
   (`#[dotnet_class]`/`#[dotnet_methods]`/`#[dotnet_interface]`), instead of reading the aliased Rust
   fn's own MIR debug info like the plain `#[dotnet_export]`/`add_fn` path already did. Fixed via
   `src/assembly.rs::carrier_arg_names`; `cd_mvc`'s previously-blocked route now works end to end over
   real HTTP. One residual, unrelated gap: comptime-synthesized field accessors
   (`#[dotnet_class(field_setters = true)]`) still emit an unnamed value parameter (no backing Rust
   `Instance` to source a name from) — not part of this bug, left as a separate minor item.
5. **`add-nuget`'s DLL-selection picks locale satellite resource assemblies over the real assembly**
   (fixed this campaign, `tools/cargo-dotnet/src/nuget.rs` — verify it stays fixed on the next
   `add-nuget` target that ships localized resources).
6. **`add-nuget` never fetched transitive NuGet dependencies** (fixed this campaign, same file — now
   parses the `.nuspec` and fetches one level of deps; deeper transitive chains are unverified).
7. **PARTIALLY FIXED.** spinacz used to drop every method with a `byte[]` parameter or return.
   Rank-1 managed arrays whose element type is already expressible now generate
   `RustcCLRInteropManagedArray<T, 1>`, and virtual/interface wrappers use `virt1`/`virt2` instead
   of dropping abstract methods with arguments. `NATS.Client.Publish(string, byte[])` and
   `Msg.Data` now round-trip two real payloads in `cd_nats` with no C# shim (debug + release).
   Multidimensional/non-SZ arrays and value-type structs such as PdfSharpCore's `XPoint`/`XRect`
   remain explicit omissions.
8. **spinacz keeps only the last-seen overload when two C# methods share a name** (confirmed via
   `HtmlNodeCollection`'s two `Item` indexer overloads — the wrong one won). Affects any reflected type
   with legitimate C# overloading, which is common.
9. **FIXED.** `mycorrhiza::linq`'s `&`/`|` predicate-combine operators referenced a "bundled" C# helper
   assembly that wasn't actually bundled anywhere — `rebind_param` (`mycorrhiza/src/linq.rs`) calls into
   `Mycorrhiza.Interop.Helpers`/`Mycorrhiza.Linq.ParameterRebinder`, and the only copy of that C# source
   lived in an unrelated sibling repo, with nothing in `tools/cargo-dotnet/` building or copying it.
   Fixed by adding a real bundled project (`mycorrhiza_interop_helpers/`, repo root) plus a delivery
   mechanism (`tools/cargo-dotnet/src/interop_helpers.rs`) that detects any crate depending on
   `mycorrhiza` and copies the built dll into its output automatically, wired into the build pipeline.
   **Second bug found while independently verifying this fix**: the delivery mechanism worked in Dev
   mode (running against an in-repo checkout) but silently no-op'd for every real `cargo dotnet` user —
   Installed mode resolves the helper project's location to `<CARGO_DOTNET_HOME>/mycorrhiza_interop_helpers`,
   and nothing provisioned it there; `cargo dotnet setup` never copied the new project into the install
   home. This is exactly the "someone who just wants to `cargo dotnet build`" scenario, not a dev-checkout
   edge case — caught by re-verifying through a freshly `cargo install`ed global binary rather than
   trusting the fixing agent's own dev-mode test run. Fixed in `tools/cargo-dotnet/src/setup.rs`:
   `setup` now copies `mycorrhiza_interop_helpers/` into the install home as part of provisioning.
   Verified end-to-end through the real installed binary, from a clean `target/`: `cd_linq_expr` 89/89,
   `cd_linq_groupby` 17/17 (regression), `cargo check --workspace` clean.
10. **RESOLVED (downgraded from soundness gap to feature gap) — the fat-pointer/raw-pointer-cast
    GC-erasure class.** Deepening `PtrCast`'s check (item above, `contains_gcref`) surfaced a real,
    previously-silent codegen bug, in four independent manifestations found across this campaign: (a)
    `write!`/`writeln!`'s default `dyn Write` coercion erasing a GC-tracked pointee to raw `void*`
    (`cargo_tests/cd_bcl`, fixed narrowly — `StringBuilder::write_fmt` now avoids the trait-object
    coercion entirely, library-level workaround, 324/324); (b) an analogous hazard hit independently
    while building the raw-dynamic-reflection-invoke feature, iterating a `&[T]` slice of managed
    handles (worked around with a fixed-arity call ladder instead of a loop); (c) `Box<CoroutineState>::drop`'s
    *compiler-generated* deallocation glue doing the same erasure via `NonNull::cast`
    (`src/rvalue.rs::ptr_to_ptr`); (d) `#[dotnet_export]`'s `MaybeUninit<RetTy>`-based return-value
    extraction inside `catch_unwind` hitting the identical pattern for async fns returning a managed
    handle (`cargo_tests/cd_efcore_async`, fixed by switching to `Option<RetTy>`, no raw-pointer
    round-trip needed).

    **Follow-up investigation (this session, no code change needed):** the original write-up flagged
    `ptr_to_ptr`'s unconditional `Type::Ptr`-typing of raw-pointer-cast destinations as backing
    "essentially every raw pointer cast in `core`/`alloc`/`std`" — a large, unscoped blast radius. Direct
    testing shows this concern doesn't hold: `PtrCast`'s `contains_gcref` check (added for item above)
    is architecturally a universal chokepoint, not a narrow one. Two fresh, independent repros beyond
    the four instances above were built specifically to stress-test this: (1) `Box::new` + `drop` of an
    ordinary struct wrapping a `mycorrhiza` managed handle field (exercises the exact
    `Box<T>::drop`-generic-glue pattern this item worried about, with a genuinely arbitrary user type,
    not just the coroutine-state case already fixed) — **rejected at compile time** with `CIL
    type-verifier rejected method ...: ManagedPtrCast { ... }. Refusing to emit ill-typed CIL
    (ALLOW_MISCOMPILATIONS=0). This is invariant I1 of the absolute-correctness plan.`; (2) a `[T; 20]`
    array-repeat of a `Copy` managed-handle type (exercises `src/rvalue.rs::repeat`'s `cp_blk`
    raw-byte-blit tail for arrays over `SIMPLE_REPEAT_CAP`, a code path `typecheck.rs` never directly
    inspects) — **also rejected**, because obtaining the raw pointer `cp_blk` operates on requires
    first crossing a `PtrCast`/`RefToPtr`-checked conversion out of managed storage. In both cases the
    failure is loud (a compile-time reject citing I1) and immediate, not a silent miscompile. `RefToPtr`
    itself (`cilly/src/ir/typecheck.rs`, the "take the address of a place" primitive underlying nearly
    all internal address computation) has no `contains_gcref` check of its own — but this is correct,
    not a gap: unlike `PtrCast`, `RefToPtr` never relabels a pointee's type, so it cannot itself erase
    GC-tracking information; the danger is specifically in a *subsequent* type-changing cast, which is
    exactly what `PtrCast` guards.

    **Conclusion**: there is no remaining *silent*-miscompile risk in this bug class — the checker is a
    sound, general backstop for it, verified beyond the four instances already fixed. What remains is a
    **feature gap**: a handful of legitimate Rust patterns (boxing/repeating a type that transitively
    holds a managed handle) fail to compile today rather than being lowered through a safe
    (`GCHandle`-mediated) codegen path, and the failure surfaces as a raw rustc ICE with a stack trace
    rather than a clean, actionable diagnostic — poor UX for something that is actually an intentional
    safety rejection, not a real compiler bug. A future pass could either (a) teach the relevant
    generic-glue/array-fill lowering to route gcref-containing types through the same `GCHandle`
    idiom `mycorrhiza::task` already uses (a real codegen feature, scoped per-callsite rather than a
    blanket `ptr_to_ptr` rewrite), or (b) at minimum, catch this specific checker rejection and re-emit
    it as a normal rustc `span_err` pointing at the `GCHandle`-wrapper pattern instead of a bare panic.
    Neither is a launch blocker.

## Tier 1 — proven this campaign, ready to build on

- **EF Core**: query translation (real SQL via `Queryable.Where<T>`), writes (`Add`+`SaveChanges`),
  navigation properties (`Include()`, real `LEFT JOIN`), migrations (`Database.Migrate()`), Rust-defined
  entities (`#[dotnet_class(properties=true)]` — real CLR properties EF's model builder can discover),
  `GroupBy`/`Join`/`SelectMany`. Verified against both SQLite and real Postgres (`Npgsql.EntityFrameworkCore.PostgreSQL`).
- **Async/await**: `Task<T>` production (`TaskCompletionSource<T>.get_Task()`) and holding a managed
  handle across `.await` inside a real `async fn` (GCHandle-backed `mycorrhiza::class::Class<..>`) both
  work for the general case. Blocked on the compound case by Tier 0 item 1.
- **Auth**: JWT mint/validate from Rust (`System.IdentityModel.Tokens.Jwt`), and a Rust-implemented
  `IAuthorizationHandler` genuinely invoked by ASP.NET Core's real `[Authorize]` pipeline (401/403/200
  all correct, call counts exact).
- **Background services**: `IHostedService` implemented directly from Rust, invoked by a real
  `Host.CreateDefaultBuilder()` lifecycle; long-running work via a blocking `std::thread::spawn` loop
  (sidesteps the async ceiling entirely — a legitimate pattern, not just a workaround). Subclassing the
  abstract `Microsoft.Extensions.Hosting.BackgroundService` itself and overriding its `ExecuteAsync` also
  now works end-to-end (proven in `cargo_tests/cd_bgservice/rustlib_bgtest`, no crash under a real
  `IHostBuilder` lifecycle) — see the escape-hatch entry below.
- **Hosting**: Rust logic called from a real ASP.NET Core minimal-API host works end-to-end over live
  HTTP. (Rust *as* the route-handler delegate blocked by Tier 0 item 4; Rust-defined MVC controllers
  correctly out of scope — needs custom attribute emission for `[ApiController]`/`[Route]`, which now
  exists (see below) but hasn't been exercised for MVC specifically.)
- **Escape hatches for advanced/unsafe interop** (all backed by real proof crates): custom .NET attribute
  emission (`#[dotnet_class(attr(...))]`, denylists CoreCLR layout-affecting namespaces), an ALLOWlisted
  `extends = "..."` base-class set for `#[dotnet_class]` (`System.Object` + `BackgroundService` proven
  end-to-end so far; anything else needs `ALLOW_UNVERIFIED_BASE=1` — subclassing an arbitrary CLR base
  isn't provably safe from the Rust side, so this is opt-in per class, not a blanket bypass — see
  `EXTENDS_ALLOWLIST` in `dotnet_macros/src/lib.rs`), raw dynamic reflection invoke
  (`mycorrhiza::dynamic::invoke_dynamicN[_checked]`), and unchecked span/slice access
  (`Span::{get,set}_unchecked`).
- **NATS**: works via the older synchronous `NATS.Client`, real pub/sub round-trip against a live
  server. The *modern* `NATS.Net` is async-only end-to-end — unreachable until Tier 0 item 1 closes,
  a genuinely useful data point on how much the async work matters in practice.
- **CsvHelper, HtmlAgilityPack, PdfSharpCore**: all work via `add-nuget`, real parse/build/render proofs.

## Tier 2 — not attempted, plausible with today's capability + the Tier 0 fixes

Ordered roughly by how directly they'd benefit from the Tier 0 fixes landing:

- **The actual EF-Core-async controller shape** — this is Tier 0 item 1's fix, not new work.
- **MVC controllers** — needs Tier 0 items 2 (subclassing) and a new custom-attribute-emission
  mechanism (`[ApiController]`/`[HttpGet]`) that doesn't exist in any form yet — bigger than a Tier 0
  fix, closer to a new capability.
- **`BackgroundService` subclassing specifically** — Tier 0 item 2.
- **`services.AddHostedService<T>()` / any generic-constrained DI registration against a `Microsoft.*`
  package** — Tier 0 item 3.
- **Swashbuckle / `MicroElements.Swashbuckle.FluentValidation`** — reflection-and-attribute-heavy,
  needs custom-attribute emission (same gap as MVC controllers) to be genuinely useful from Rust-defined
  types; usable today only in the "C# host reflects over a hand-annotated wrapper" shape.
- **FluentValidation** — plain object-graph validation library, likely tractable via `add-nuget` today,
  untested.
- **OpenTelemetry stack** (`OpenTelemetry.*` — 7 packages: Console/OTLP/Prometheus/Zipkin exporters,
  ASP.NET/EF/HTTP/Runtime instrumentation) — `Activity`/`ActivitySource` APIs are moderately complex
  generic/struct-heavy surfaces; likely hits Tier 0 items 7/8 in places. Untested.
- **AWSSDK.S3, AWSSDK.SimpleEmail, OpenAI** — heavy async-first client SDKs. Same story as `NATS.Net`:
  blocked until Tier 0 item 1 closes, unless each happens to expose a sync escape hatch (unchecked).
- **BCrypt.Net-Next** — small, sync, POCO-friendly. Should be a quick, low-risk `add-nuget` win.
- **UUIDNext** — trivial generator surface, should be a quick win.
- **SSH.NET** — sync-capable but stream/socket-heavy; moderate risk of hitting Tier 0 item 7 (byte[]
  buffers are its whole API).
- **DocumentFormat.OpenXml** — large, deeply nested object-graph API (Word/Excel/PowerPoint XML).
  High complexity, unknown risk profile; would want a narrow, single-document-type proof first, not a
  broad attempt.
- **Sentry.AspNetCore** — middleware-registration-shaped, similar risk profile to Swashbuckle.
- **Testcontainers / Testcontainers.PostgreSql** — used for *testing the C# code itself*, not runtime
  logic; only relevant if Rust needs to drive test infrastructure, not part of the "replace app logic"
  goal. Low priority for this roadmap's purpose.
- **Moq / NSubstitute / xunit / AwesomeAssertions / Bogus / coverlet** — same category: test-only
  tooling, not app logic. Not part of "can Rust do what the app does."
- **CsvHelper/HtmlAgilityPack/PdfSharpCore-adjacent smaller libs** (`AnyAscii`, `FuzzySharp`,
  `Mime-Detective`, `HtmlAgilityPack` done, `Sep`) — small, likely-easy `add-nuget` targets, batch these
  together as a cheap breadth pass once Tier 0 items 7/8 are fixed (fixing those first avoids
  rediscovering the same two bugs on each new library).
- **DynamicExpresso.Core** — an expression-tree *interpreter*; conceptually adjacent to (and possibly
  complementary to) `mycorrhiza::linq`'s expression-tree *builder*. Worth a look specifically because of
  that overlap, not just as a generic `add-nuget` target.

## Tier 3 — unknown, needs the actual source read

- **`monark-backoffice-sdk`, `priority-sdk-dotnet`, `sydecar-sdk-dotnet`** — private internal packages,
  surface unknown from outside. Feasibility assessment needs someone with access to read their public
  API shape first; likely falls into whichever Tier 1/2 category their actual method signatures land in
  (if they're plain sync POCO-returning clients, probably easy; if they're async-SDK-shaped, blocked on
  Tier 0 item 1 like everything else in that bucket).

## What this roadmap does NOT cover

Whether Rust *should* replace any given slice of `primary-offerings` is a separate question from whether
it *can* — this document is capability-mapping only. The realistic target architecture discussed
alongside this campaign was always "Rust as the business-logic layer under a thin, real ASP.NET Core
host," not "Rust replaces ASP.NET Core itself" — Tier 1's hosting result (Rust logic called from a real
minimal-API host) is that architecture already working, not a fallback.
