# Ergonomics roadmap — making the interop crates delightful to use

The interop *capability* is largely proven (see [TRANSLATION_STATUS.md](TRANSLATION_STATUS.md)); this
doc is the **library/DX horizon** — turning capabilities into things a user reaches for without
thinking. Two audiences: a **Rust dev targeting .NET** (uses `mycorrhiza`) and a **C# dev consuming
Rust** (uses a `cdylib` + the shipped wrappers). The north star is "feels native in both directions."

**Already shipped** (baseline this doc builds on): `mycorrhiza::collections` (List/Dictionary/HashSet/
Stack/Queue, used like `std`); the WF-9 generic bridge + `dotnet_generic!` macros; `#[dotnet_class]`
(struct → managed class with a ctor); the reusable `RustVec<T>`/`RustBoxVec<T>` containers
(`export_rust_containers!` + shipped C# wrappers); the `bindings.rs` BCL surface (~4256 methods);
`cargo dotnet build/run/setup/pack`; [QUICKSTART_INTEROP.md](QUICKSTART_INTEROP.md).

**Effort:** S ≈ hours · M ≈ 1–2 days · L ≈ 3–5 days · XL ≈ 1–3 weeks.
**Payoff:** ★ nice · ★★ unlocks a real workflow · ★★★ keystone (unlocks a whole category).
**⚑ = keystone** (disproportionate leverage; sequence these first within their theme).

**Status legend** (Status column, verified against the shipped `cargo_tests/cd_*` proofs — see
[ERGONOMICS_STATUS.md](ERGONOMICS_STATUS.md) for the full completion report):
✅ **done** (landed + verified) · 🟡 **partial** (core landed; a documented tail is deferred, usually
on a real backend gap) · ⬜ **not started**.

> **Reconciled snapshot (2026-07-13):** the former generic-value-type backend tail and its user-facing
> dependents are shipped: Dictionary iteration, `Nullable<T>`, `Span<T>`/`ReadOnlySpan<T>`, nested
> generic arguments, and delegate-as-generic-argument all have backend fixtures. Enum interop, LINQ,
> NativeAOT publishing, and a flagship application are also present. Remaining items are optional
> breadth or documented CLR boundaries, not hidden blockers.

---

## Theme 1 — .NET-from-Rust: collections & iteration

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| ✅ | **Enumerator bridge ⚑** | `for x in anyEnumerable` — wrap `IEnumerator<T>` (`MoveNext`/`get_Current`) generically | M | ★★★ | Landed `fb35b88` in `mycorrhiza/src/enumerate.rs` (`Enumerator<T>` + `Enumerable`). `IntoIterator for &List/&HashSet/&Stack/&Queue`. Proof: `cd_enumerate` 22/22. Dictionary-entry iteration is the deferred tail (below). |
| ✅ | Collection conveniences | `first`/`last`/`pop`, `sort`, `reverse`, `to_vec`, `from_slice`, deep `clone` | M | ★★ | Landed `24055af`. `ops::Index` intentionally NOT provided (managed List can't return `&T` into managed memory — `get()`/`iter()` cover it by value). `retain`/`binary_search`/`sort_by` need a delegate arg (delegates shipped `5277560`; not yet wired to these). |
| ✅ | `FromIterator` / `Extend` / `IntoIterator` | `let l: List<i32> = (0..5).collect();`, `l.extend(iter)`, `for x in &l` | S–M | ★★ | List had `FromIterator`/`Extend` (`24055af`); `IntoIterator for &_` + HashSet `FromIterator`/`Extend` (`fb35b88`). Exercised in `cd_enumerate`. |
| ✅ | `Vec`↔`List` / `array`↔`List` conversions | `List::from_slice`, `list.to_vec()`, `From<Vec<T>>` | S | ★★ | Landed `24055af` (`From<Vec<T>>`, `to_vec`, `from_slice`). |
| ✅ | Dictionary iteration | `for (k, v) in &dict`, `.keys()`, `.values()` | M | ★★ | Shipped on the sound generic-value-type instance-call path; `cd_collections` exercises entry/key/value iteration through `KeyValuePair<K,V>`. |
| ✅ | More collections | `SortedDictionary`, `SortedSet`, `LinkedList`, `PriorityQueue`; concurrent: `ConcurrentDictionary`, `ConcurrentQueue`, `ConcurrentBag` | M each | ★–★★ | Landed `57e01c1` (7 added; all in the prelude). `cd_collections` grew to 128/128. `ReadOnlyCollection` not added (thin wrapper, low value). |
| ✅ | `KeyValuePair<K,V>`, `Nullable<T>` wrappers | idiomatic `Nullable<T>` ↔ `Option<T>` | S–M | ★★ | `mycorrhiza::nullable::{some,none,NullableExt}` ships; `cd_vtgen` proves asymmetric `KeyValuePair` getters and both `Nullable<T>` instance getters without weakening typechecking. |

## Theme 2 — .NET-from-Rust: idiomatic traits & type wrappers

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| 🟡 | Std-trait impls for managed wrappers | `Display`/`Debug`, `PartialEq`/`Eq`, `Hash`, `Ord`, `Clone`, `Default` | M | ★★★ | `DotNetString` has `Display`/`Debug`/`Eq`/`Hash` (`24055af`); List has element-wise `Clone`/`Eq`/`Hash` (deliberately NOT ref-identity `Object.Equals`). `Ord` and a general blanket/derive across *all* wrappers not done (`String.CompareTo` is culture-sensitive; a blanket `Display`-via-`ToString` would print type names for collections — the honest call was per-type). |
| ✅ | First-class managed `String` | `DotNetString` with `Display`/`From<&str>` + conversions; seamless `&str`↔`String` | M | ★★ | Landed `24055af` + `eb78316` (`DotNetString` newtype over the `MString` seam; real UTF-16 round-trip incl. non-ASCII). |
| ✅ | `Option`/`Result` bridges | `.NET null` ↔ `Option`, `.NET` exception ↔ `Result` (ergonomic wrappers over the `try_catch` primitive) | M | ★★★ | Landed `eb78316` (`mycorrhiza::error`: `Nullable` trait + `from_nullable`; `try_managed` / `.try_()` combinator → `Result<T, ManagedException>`). Proof: `cd_idiomatic` 45/45. |
| ✅ | Common BCL type wrappers | `DateTime`, `TimeSpan`, `Guid`, `Uri`, `Regex`, `Random`, `Stopwatch`, `StringBuilder`, `Environment`, `Math` (idiomatic) | S–M each | ★★ | Landed `957ca95` — 10 modules under `mycorrhiza/src/bcl/`, all in the prelude. Proof: `cd_bcl` 313/313. (Required a value-type-correct instance-call helper `vt_*` in `intrinsics.rs`; ctors go via `Parse` where a valuetype `.ctor` isn't reachable — see the follow-ups.) |
| ✅ | `System.Text.Json` bridge | `json::parse` / `to_string` over a `JsonNode` reference model | M–L | ★★ | Landed `3d6217d` (`mycorrhiza::bcl::json`: parse/navigate/serialize on the reference-typed `JsonNode`/`JsonObject`/`JsonArray`, sidestepping the valuetype `JsonElement`). Proof: `cd_json` 47/47. `serde` ⇄ .NET not attempted. |
| ✅ | Enum interop | .NET enum ↔ Rust enum (values + names) | M | ★ | `dotnet_enum!` generates integer/variant/managed-handle conversions; `cd_gmethod` round-trips `System.DayOfWeek` through `Enum.GetName<TEnum>`. |

## Theme 3 — .NET-from-Rust: the big capabilities

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| 🟡 | **Delegates & callbacks ⚑** *(core SHIPPED)* | wrap Rust functions/closures as managed `Action`/`Func`/`Comparison` and invoke them | L–XL | ★★★ | Concrete signatures and capturing closures ship; `cd_vtgen` proves a delegate whose argument is the enclosing class generic (`List<i32>.ForEach(Action<i32>)`). The remaining tail is first-class .NET event subscription/removal. |
| ✅ | **Task / async bridge ⚑** | `.await` a `Task<T>` from Rust; expose a Rust `async fn` as a .NET `Task` | L | ★★★ | Landed `94d8e59` (`mycorrhiza::task`: `await_task` polls `IsCompleted`/reads `Result`; `future_to_task` drives a Rust `Future` into a `TaskCompletionSource<T>`). Proof: `cd_async` 7/7 (completed / timer-delayed / `Task.Run` / mid-await pending→ready / async-fn→Task / `block_on`). |
| 🟡 | **Async streams** | consume `IAsyncEnumerable<T>` incrementally from Rust | M–XL | ★★★ | Consumer bridge shipped: `AsyncEnumerable`/`AsyncEnumerator` drive real `MoveNextAsync` `ValueTask<bool>` operations with rooted handles; delayed-channel `cd_async_stream` passes debug + release. Producing a stream from a Rust `async fn` remains blocked on coroutine GC-reference layout. |
| ✅ | LINQ-style adapters | expression trees and `IQueryable.Where`/grouping pipelines | M | ★★ | `mycorrhiza::linq` builds and compiles expression trees and hands typed predicates to `IQueryable`; `cd_linq`, `cd_linq_expr`, and `cd_linq_groupby` cover in-memory and provider-shaped paths. |
| ✅ | `Span<T>`/`Memory<T>` | borrowed zero-copy spans plus GC-owned memory safe to retain across async boundaries | M | ★★ | `Span<T>`/`ReadOnlySpan<T>` borrow Rust slices zero-copy. `Memory<T>`/`ReadOnlyMemory<T>` copy into a managed array, then support managed length/slice/mutation and `CopyTo`; `cd_span` 68/68. |

## Theme 4 — Rust-from-C#: exporting Rust ergonomically

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| ✅ | **`#[dotnet_export]` auto-marshal ⚑** | write idiomatic Rust signatures; C# receives typed strings, arrays/containers, nullable primitives, tasks, and exceptions | M–L | ★★★ | Marshals `&str`/`String`/primitives, `Vec<T>`→`RustVec<T>`, `Task<T>`, primitive `Option<T>`↔`Nullable<T>`, and `Result<T,E>` with explicit `error="exception"`. Proof: `cd_export`, `cd_export_ergonomics`. Broader enum/try-pattern and managed-reference `Option<T>` shapes remain explicit work. |
| ✅ | Extend `#[dotnet_class]` | virtual methods; managed-type fields; properties; static methods; multiple ctors; implement a .NET interface | L | ★★★ | Static methods, multiple ctors, properties/field accessors, managed fields, `implements = "[Asm]Ns.IContract"`, inheritance, and explicit base-slot overrides all ship. Proof: `cd_typedef` 16/16, `cd_iface` 9/9, `cd_override` 5/5, and `cd_bgservice_bgtest`. Deeper base-constructor-chain shapes remain optional breadth, not a missing version of this surface. |
| 🟡 | Export Rust `enum` / `Result` / `Option` | Rust enum → C# enum; `Result` → try-pattern/exception; `Option` → nullable | M | ★★ | Genuine CLR enums via `#[dotnet_enum]`, primitive `Option<T>` ↔ `Nullable<T>`, and `Result<T,E>` → managed exception are shipped. C# try-pattern DTOs and managed/non-primitive `Option<T>` remain. |
| ✅ | Export Rust traits as C# interfaces | a Rust trait declaration becomes a genuine CLR interface usable polymorphically from C# | L | ★★ | `#[dotnet_interface]` ships with inheritance, generic interfaces/methods, properties, events, static abstract members, and default interface methods. Proof: `cd_interface`, `cd_iface_inherit`, `cd_iface_generic`, `cd_iface_genmethod`, `cd_iface_prop`, `cd_iface_event`, `cd_static_iface`, and `cd_dim`. |
| ✅ | More reusable containers for C# | `RustHashMap<K,V>`, `RustString`, and `IEnumerable<T>` views over Rust-owned vectors | M | ★★ | `RustHashMap<K,V>` + `RustString` ship (`cd_containers2` 30/30); `RustVec<T>` and `RustBoxVec<T>` implement `IEnumerable<T>` with `foreach` and LINQ proof in `cd_rustvec` 37/37. |
| 🟡 | C# delegates → Rust | accept a C# `Action`/`Func`/`Comparison` in an exported Rust API and invoke it through a typed wrapper | L | ★★ | `#[dotnet_export]` and `#[dotnet_methods]` import `Action1`–`Action3`, `Func1`–`Func3`, and `Comparison` with primitive or managed-`MString` signatures; `cd_export_ergonomics` proves the three-argument and non-ASCII string paths from C#. Tail: arity 4+, automatic owned-string/value marshalling, and an `impl Fn` adapter. |

## Theme 5 — tooling & onboarding (cargo-dotnet)

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| ✅ | **`cargo dotnet new` ⚑** | scaffold a ready-to-run project: `--lib` (Rust cdylib + C# consumer), `--app` (Rust-on-.NET binary), `--plugin` (`#[dotnet_class]` + C# host) | M | ★★★ | Landed `8f7eb61` (`tools/cargo-dotnet/src`). Verified end-to-end: `--app`/`--lib`/`--plugin` each scaffold+build+run (myapp 6/6, mylib 3/3, plugin 2/2). |
| ✅ | **`mycorrhiza::prelude` ⚑** | `use mycorrhiza::prelude::*;` brings collections, wrappers, and macros into scope | S | ★★ | Landed `24055af` (`mycorrhiza/src/prelude.rs`); every `cd_*` example dogfoods it. |
| ✅ | `cargo dotnet test` | run Rust `#[test]` on .NET | M | ★★ | Landed `8f7eb61` (`test` subcommand; a `#[test]` runs on .NET, 1 passed). `bench` not added. |
| ✅ | Better interop diagnostics (`cargo dotnet doctor`) | map `TypeLoadException`/`MissingMethod` → actionable fix; a `doctor` command | M | ★★ | Landed `8f7eb61` (`doctor` translates the known runtime-failure signatures). `--json` schema 1 exposes environment, workspace-wiring, and translated-failure reports to CI/editor/support tooling; onboarding acceptance exercises both modes. |
| ✅ | `cargo dotnet publish` (NativeAOT) | self-contained native output as one command | M | ★★ | `cargo dotnet publish <csproj>` drives the existing `RustDotnet.targets` host through ILC, supports explicit RID/output, and is exercised by `feasibility/nativeaot_acceptance.sh` plus the fork gate. |
| ✅ | NuGet packaging | `cargo dotnet pack` → a real `.nupkg` (Rust `.dll` + metadata), publishable | M | ★★ | Landed (`pack` subcommand, `tools/cargo-dotnet/src/pack.rs`; native, no bash; TFM `lib/<tfm>/`). |

## Theme 6 — docs, examples, discoverability

| Status | Item | What the user gets | Effort | Payoff | Notes |
|---|---|---|---|---|---|
| ✅ | Cookbook / recipes | "how do I: read a file, HTTP GET, parse JSON, use a NuGet library, expose a Rust struct, handle an event" | M | ★★ | Landed `8616f24` — [INTEROP_COOKBOOK.md](INTEROP_COOKBOOK.md), recipe-per-task, grounded in the shipped `cd_*` crates. |
| 🟨 | API docs artifact and hosting | Warning-free Rust HTML + packaged C# XML continuously generated; external hosting still pending | S–M | ★★ | Strict artifact gate done (`feasibility/api_docs_acceptance.sh`, `-D warnings`); no site/package publication is performed. |
| ✅ | Flagship example | a real app end-to-end (a CLI using a .NET library) | M–L | ★★ | [`examples/issue-dashboard`](../examples/issue-dashboard/README.md) parses a user-supplied issue export with managed `System.Text.Json`, aggregates it in ordinary Rust, and has deterministic sample output. `feasibility/flagship_example_acceptance.sh` verifies the normal and malformed-input paths. The `cd_*` crates remain the focused capability proofs. |
| ✅ | BCL coverage matrix | which types/methods have idiomatic wrappers vs raw bindings vs unsupported | S | ★ | Landed `8e1fa66` — [BCL_COVERAGE.md](BCL_COVERAGE.md). |

---

## Recommended sequence (opinionated) — STATUS: 1–7 all shipped

The keystones weren't independent — this is the order that was executed; every step below landed and is
verified against a `cd_*` proof (see [ERGONOMICS_STATUS.md](ERGONOMICS_STATUS.md)):

1. ✅ **Quick wins** — `prelude`, collection conveniences, `Vec`↔`List`, honest std-trait impls. (`24055af`)
2. ✅ **Enumerator bridge** (Theme 1 ⚑) — `for x in &collection` over `IEnumerator<T>`. (`fb35b88`, `cd_enumerate` 22/22) — *Dictionary-entry* iteration is the deferred backend-blocked tail.
3. ✅ **`cargo dotnet new` + prelude** (Theme 5 ⚑) — plus `doctor`/`test`. (`8f7eb61`)
4. ✅ **`#[dotnet_export]` auto-marshal** (Theme 4 ⚑) — `#[dotnet_export] fn` → `MainModule.method(...)`, managed-string marshalling, no glue. (`d08aba3`, `cd_export` 11/11)
5. ✅ **Delegates, callbacks, and exported events** (Theme 3 ⚑) — Rust functions and capturing
   closures → managed `Action`/`Func`/`Comparison`; generic-method delegate arguments; genuine
   class/interface event metadata. (`cd_delegates`, `cd_closures`, `cd_event`, `cd_iface_event`.)
6. ✅ **Task/async bridge** (Theme 3 ⚑) — `.await` a `Task<T>`, expose an `async fn` as a `Task`. (`94d8e59`, `cd_async` 7/7)
7. ✅ **Breadth** — more collections (`57e01c1`, `cd_collections` 128/128), BCL wrappers (`957ca95`, `cd_bcl` 313/313), JSON (`3d6217d`, `cd_json` 47/47), error/text ergonomics (`eb78316`, `cd_idiomatic` 45/45), extended `#[dotnet_class]` (`c1c90ce`, `cd_typedef` 16/16), more C#-consumable containers (`8ce47bd`, `cd_containers2` 30/30), pack, cookbook + BCL matrix.

**What's left** (optional breadth or explicit CLR-boundary work): third-party event-subscription
helpers, managed-reference `Option<T>`, C# delegates
consumed as Rust closures, async-stream production, nullable signature annotations, and hosted API docs.
Managed `Memory<T>`, primitive nullable/exception export, trait/interface export, and
`IEnumerable<T>` containers are complete. See
[ERGONOMICS_STATUS.md](ERGONOMICS_STATUS.md) §"Next steps".

## Walls (won't-do / can't-do cleanly — from TRANSLATION_STATUS §7)

Not ergonomics gaps — genuine ceilings, listed so they're not mistaken for backlog:
- A **transparent zero-cost open generic whose overlapping layout holds a managed reference** (CLI §9.5). The two-mode `RustVec`/`RustBoxVec` bridge is the accepted answer.
- **Static borrow-safety across the seam** — once a value crosses into managed code, Rust's compile-time ownership guarantee can't be enforced (functional correctness yes; the *guarantee* no).
- **Arbitrary novel inline asm.** Common patterns are coverable; a hand-rolled novel block isn't.
