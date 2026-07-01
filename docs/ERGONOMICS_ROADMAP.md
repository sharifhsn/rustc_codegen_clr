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

> **Snapshot (2026-06-30):** the ergonomics campaign is essentially complete. Themes 1–5 shipped their
> keystones and most breadth items; Theme 6 shipped the cookbook + BCL matrix. The remaining ⬜/🟡 items
> are (a) backend-blocked tails (Dictionary iteration, `Span<T>`, `Nullable<T>` — all need the deferred
> WF-9 generic-value-type-instance-method work) and (b) genuinely optional breadth (enum interop, hosted
> docs, more exported containers). Nothing on the critical path remains.

---

## Theme 1 — .NET-from-Rust: collections & iteration

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| ✅ | **Enumerator bridge ⚑** | `for x in anyEnumerable` — wrap `IEnumerator<T>` (`MoveNext`/`get_Current`) generically | M | ★★★ | Landed `fb35b88` in `mycorrhiza/src/enumerate.rs` (`Enumerator<T>` + `Enumerable`). `IntoIterator for &List/&HashSet/&Stack/&Queue`. Proof: `cd_enumerate` 22/22. Dictionary-entry iteration is the deferred tail (below). |
| ✅ | Collection conveniences | `first`/`last`/`pop`, `sort`, `reverse`, `to_vec`, `from_slice`, deep `clone` | M | ★★ | Landed `24055af`. `ops::Index` intentionally NOT provided (managed List can't return `&T` into managed memory — `get()`/`iter()` cover it by value). `retain`/`binary_search`/`sort_by` need a delegate arg (delegates shipped `5277560`; not yet wired to these). |
| ✅ | `FromIterator` / `Extend` / `IntoIterator` | `let l: List<i32> = (0..5).collect();`, `l.extend(iter)`, `for x in &l` | S–M | ★★ | List had `FromIterator`/`Extend` (`24055af`); `IntoIterator for &_` + HashSet `FromIterator`/`Extend` (`fb35b88`). Exercised in `cd_enumerate`. |
| ✅ | `Vec`↔`List` / `array`↔`List` conversions | `List::from_slice`, `list.to_vec()`, `From<Vec<T>>` | S | ★★ | Landed `24055af` (`From<Vec<T>>`, `to_vec`, `from_slice`). |
| 🟡 | Dictionary iteration | `for (k, v) in &dict`, `.keys()`, `.values()` | M | ★★ | **Blocked by a backend gap** (documented `collections.rs:413`): entries are `KeyValuePair<K,V>` (generic value type → `get_Key`/`get_Value` are instance methods on a valuetype, asserted-unsupported in `src/terminator/call.rs`); `get_Keys/get_Values` return a *nested* generic `KeyCollection<!0,!1>` the CIL typechecker soundly rejects. Needs the WF-9 value-type-generic tail — NOT weakenable at the library level. |
| ✅ | More collections | `SortedDictionary`, `SortedSet`, `LinkedList`, `PriorityQueue`; concurrent: `ConcurrentDictionary`, `ConcurrentQueue`, `ConcurrentBag` | M each | ★–★★ | Landed `57e01c1` (7 added; all in the prelude). `cd_collections` grew to 128/128. `ReadOnlyCollection` not added (thin wrapper, low value). |
| 🟡 | `KeyValuePair<K,V>`, `Nullable<T>` wrappers | idiomatic `Nullable<T>` ↔ `Option<T>` | S–M | ★★ | Reference-type `null ↔ Option` shipped (`error.rs`, `eb78316`). The *valuetype* `Nullable<T>` wrapper is NOT done — same value-type-generic-instance-method tail as Dictionary iteration / `Span<T>`. |

## Theme 2 — .NET-from-Rust: idiomatic traits & type wrappers

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| 🟡 | Std-trait impls for managed wrappers | `Display`/`Debug`, `PartialEq`/`Eq`, `Hash`, `Ord`, `Clone`, `Default` | M | ★★★ | `DotNetString` has `Display`/`Debug`/`Eq`/`Hash` (`24055af`); List has element-wise `Clone`/`Eq`/`Hash` (deliberately NOT ref-identity `Object.Equals`). `Ord` and a general blanket/derive across *all* wrappers not done (`String.CompareTo` is culture-sensitive; a blanket `Display`-via-`ToString` would print type names for collections — the honest call was per-type). |
| ✅ | First-class managed `String` | `DotNetString` with `Display`/`From<&str>` + conversions; seamless `&str`↔`String` | M | ★★ | Landed `24055af` + `eb78316` (`DotNetString` newtype over the `MString` seam; real UTF-16 round-trip incl. non-ASCII). |
| ✅ | `Option`/`Result` bridges | `.NET null` ↔ `Option`, `.NET` exception ↔ `Result` (ergonomic wrappers over the `try_catch` primitive) | M | ★★★ | Landed `eb78316` (`mycorrhiza::error`: `Nullable` trait + `from_nullable`; `try_managed` / `.try_()` combinator → `Result<T, ManagedException>`). Proof: `cd_idiomatic` 45/45. |
| ✅ | Common BCL type wrappers | `DateTime`, `TimeSpan`, `Guid`, `Uri`, `Regex`, `Random`, `Stopwatch`, `StringBuilder`, `Environment`, `Math` (idiomatic) | S–M each | ★★ | Landed `957ca95` — 10 modules under `mycorrhiza/src/bcl/`, all in the prelude. Proof: `cd_bcl` 313/313. (Required a value-type-correct instance-call helper `vt_*` in `intrinsics.rs`; ctors go via `Parse` where a valuetype `.ctor` isn't reachable — see the follow-ups.) |
| ✅ | `System.Text.Json` bridge | `json::parse` / `to_string` over a `JsonNode` reference model | M–L | ★★ | Landed `3d6217d` (`mycorrhiza::bcl::json`: parse/navigate/serialize on the reference-typed `JsonNode`/`JsonObject`/`JsonArray`, sidestepping the valuetype `JsonElement`). Proof: `cd_json` 47/47. `serde` ⇄ .NET not attempted. |
| ⬜ | Enum interop | .NET enum ↔ Rust enum (values + names) | M | ★ | Not started (low payoff). |

## Theme 3 — .NET-from-Rust: the big capabilities

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| 🟡 | **Delegates & callbacks ⚑** *(core SHIPPED)* | wrap a Rust `extern "C" fn` as a managed `Action`/`Func`/`Comparison` and invoke it (`.NET → Rust` via `callvirt Delegate::Invoke`); hold/re-hold a delegate handle | L–XL | ★★★ | **Done for concrete signatures** (`5277560`) — magic fn `rustc_clr_interop_delegate` (`src/terminator/call.rs`) builds a memoised per-signature shim class (holds the native ptr, `calli`s it from `Invoke`) then `newobj`s the real generic delegate over `ldftn shim::Invoke`. Face: `mycorrhiza::delegate` (`Action1/2`, `Func1/2`, `Comparison`, in the prelude). Proof: `cd_delegates` 14/14. **Deferred tail:** closure *captures* (boxed-env trampoline); delegate as a **generic-method** argument parameterised by the class generic (`List<T>.Sort(Comparison<T>)` — needs nested generic-param binding in the verifier, a separate sound extension); .NET **events**. |
| ✅ | **Task / async bridge ⚑** | `.await` a `Task<T>` from Rust; expose a Rust `async fn` as a .NET `Task` | L | ★★★ | Landed `94d8e59` (`mycorrhiza::task`: `await_task` polls `IsCompleted`/reads `Result`; `future_to_task` drives a Rust `Future` into a `TaskCompletionSource<T>`). Proof: `cd_async` 7/7 (completed / timer-delayed / `Task.Run` / mid-await pending→ready / async-fn→Task / `block_on`). |
| ⬜ | LINQ-style adapters | `.where_()`, `.select()`, `.to_list()` over `IEnumerable` | M | ★★ | Not started. Prerequisites (enumerator bridge + delegates) are now in place, so this is now a pure library item. |
| 🟡 | `Span<T>`/`Memory<T>` | zero-copy views into managed arrays | M | ★★ | Marker type `RustcCLRInteropManagedGenericValueType` exists (`intrinsics.rs:329`) but no idiomatic wrapper. **Blocked by** generic value-type instance methods (the deferred WF-9 Stage-1 tail — same gap as Dictionary iteration / `Nullable<T>`). |

## Theme 4 — Rust-from-C#: exporting Rust ergonomically

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| ✅ | **`#[dotnet_export]` auto-marshal ⚑** | write `#[dotnet_export] fn greet(name: &str) -> String`; C# calls `MainModule.greet("x")` and gets a `string` — no hand-written `(ptr,len)` buffer dance | M–L | ★★★ | Landed `d08aba3` — proc-macro in `dotnet_macros` (`dotnet_export`). Marshals `&str`/`String`/primitives; strings cross as a real managed `System.String` (the `MString` seam), so **zero C#-side glue** — no backend change. Proof: `cd_export` 11/11. Follow-ups: slices, `char`, `Vec<T>`, `Option`/`Result` returns. |
| 🟡 | Extend `#[dotnet_class]` | virtual methods; managed-type fields; properties; static methods; multiple ctors; implement a .NET interface | L | ★★★ | Landed `c1c90ce` — static methods, multiple ctors, field setters, `read_<field>` accessors, managed-type fields. Proof: `cd_typedef` 16/16. **Not yet:** virtual methods (needs a "re-open a class" comptime capability) and implementing a .NET interface (the subclass/interface tail). |
| ⬜ | Export Rust `enum` / `Result` / `Option` | Rust enum → C# enum; `Result` → try-pattern/exception; `Option` → nullable | M | ★★ | Not started (removes the manual bool/out-param convention). |
| ⬜ | Export Rust traits as C# interfaces | a Rust trait object usable polymorphically from C# | L | ★★ | Not started; pairs with `#[dotnet_class]` interface support. |
| 🟡 | More reusable containers for C# | `RustHashMap<K,V>`, `RustString`, `IEnumerable<T>` over a `RustVec` | M | ★★ | `RustHashMap<K,V>` + `RustString` landed `8ce47bd` (proof: `cd_containers2` 30/30). `IEnumerable<T>` over a `RustVec` not done. |
| ⬜ | C# delegates → Rust | pass a C# `Action`/`Func` into Rust as `impl Fn` | L | ★★ | Not started; the mirror of Theme-3 delegates. |

## Theme 5 — tooling & onboarding (cargo-dotnet)

| Status | Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|---|
| ✅ | **`cargo dotnet new` ⚑** | scaffold a ready-to-run project: `--lib` (Rust cdylib + C# consumer), `--app` (Rust-on-.NET binary), `--plugin` (`#[dotnet_class]` + C# host) | M | ★★★ | Landed `8f7eb61` (`tools/cargo-dotnet/src`). Verified end-to-end: `--app`/`--lib`/`--plugin` each scaffold+build+run (myapp 6/6, mylib 3/3, plugin 2/2). |
| ✅ | **`mycorrhiza::prelude` ⚑** | `use mycorrhiza::prelude::*;` brings collections, wrappers, and macros into scope | S | ★★ | Landed `24055af` (`mycorrhiza/src/prelude.rs`); every `cd_*` example dogfoods it. |
| ✅ | `cargo dotnet test` | run Rust `#[test]` on .NET | M | ★★ | Landed `8f7eb61` (`test` subcommand; a `#[test]` runs on .NET, 1 passed). `bench` not added. |
| ✅ | Better interop diagnostics (`cargo dotnet doctor`) | map `TypeLoadException`/`MissingMethod` → actionable fix; a `doctor` command | M | ★★ | Landed `8f7eb61` (`doctor` translates the known runtime-failure signatures). |
| ⬜ | `cargo dotnet publish` (+ `--aot`) | self-contained / NativeAOT single-file output as one command | M | ★★ | Not wired as a subcommand. AOT is proven at the codegen level (see `gaps-campaign` memory: whole-program NativeAOT green), but no first-class `publish` command yet. |
| ✅ | NuGet packaging | `cargo dotnet pack` → a real `.nupkg` (Rust `.dll` + metadata), publishable | M | ★★ | Landed (`pack` subcommand, `tools/cargo-dotnet/src/pack.rs`; native, no bash; TFM `lib/<tfm>/`). |

## Theme 6 — docs, examples, discoverability

| Status | Item | What the user gets | Effort | Payoff | Notes |
|---|---|---|---|---|---|
| ✅ | Cookbook / recipes | "how do I: read a file, HTTP GET, parse JSON, use a NuGet library, expose a Rust struct, handle an event" | M | ★★ | Landed `8616f24` — [INTEROP_COOKBOOK.md](INTEROP_COOKBOOK.md), recipe-per-task, grounded in the shipped `cd_*` crates. |
| ⬜ | Hosted API docs | rustdoc for `mycorrhiza` + the C# XML docs, published | S–M | ★★ | Not done (wrappers carry doc comments; nothing published). |
| ⬜ | Flagship examples | a real app end-to-end (e.g. a small web service, or a CLI using a .NET library) | M–L | ★★ | Not done. The `cd_*` crates are focused capability proofs, not a single end-to-end app. Delegates/async now unblock the juicy ones. |
| ✅ | BCL coverage matrix | which types/methods have idiomatic wrappers vs raw bindings vs unsupported | S | ★ | Landed `8e1fa66` — [BCL_COVERAGE.md](BCL_COVERAGE.md). |

---

## Recommended sequence (opinionated) — STATUS: 1–7 all shipped

The keystones weren't independent — this is the order that was executed; every step below landed and is
verified against a `cd_*` proof (see [ERGONOMICS_STATUS.md](ERGONOMICS_STATUS.md)):

1. ✅ **Quick wins** — `prelude`, collection conveniences, `Vec`↔`List`, honest std-trait impls. (`24055af`)
2. ✅ **Enumerator bridge** (Theme 1 ⚑) — `for x in &collection` over `IEnumerator<T>`. (`fb35b88`, `cd_enumerate` 22/22) — *Dictionary-entry* iteration is the deferred backend-blocked tail.
3. ✅ **`cargo dotnet new` + prelude** (Theme 5 ⚑) — plus `doctor`/`test`. (`8f7eb61`)
4. ✅ **`#[dotnet_export]` auto-marshal** (Theme 4 ⚑) — `#[dotnet_export] fn` → `MainModule.method(...)`, managed-string marshalling, no glue. (`d08aba3`, `cd_export` 11/11)
5. 🟡 **Delegates & callbacks** (Theme 3 ⚑) — a Rust `extern "C" fn` → managed `Action`/`Func`/`Comparison`. (`5277560`, `cd_delegates` 14/14). Deferred: closure captures, delegate-as-generic-method-arg, events.
6. ✅ **Task/async bridge** (Theme 3 ⚑) — `.await` a `Task<T>`, expose an `async fn` as a `Task`. (`94d8e59`, `cd_async` 7/7)
7. ✅ **Breadth** — more collections (`57e01c1`, `cd_collections` 128/128), BCL wrappers (`957ca95`, `cd_bcl` 313/313), JSON (`3d6217d`, `cd_json` 47/47), error/text ergonomics (`eb78316`, `cd_idiomatic` 45/45), extended `#[dotnet_class]` (`c1c90ce`, `cd_typedef` 16/16), more C#-consumable containers (`8ce47bd`, `cd_containers2` 30/30), pack, cookbook + BCL matrix.

**What's left** (all optional or backend-blocked): the WF-9 **generic value-type instance-method** tail —
which unblocks Dictionary iteration, `Span<T>`, valuetype `Nullable<T>` at once — plus delegate captures/
events, LINQ adapters, enum interop, `publish --aot`, hosted docs, a flagship end-to-end app. See
[ERGONOMICS_STATUS.md](ERGONOMICS_STATUS.md) §"Next steps".

## Walls (won't-do / can't-do cleanly — from TRANSLATION_STATUS §7)

Not ergonomics gaps — genuine ceilings, listed so they're not mistaken for backlog:
- A **transparent zero-cost open generic whose overlapping layout holds a managed reference** (CLI §9.5). The two-mode `RustVec`/`RustBoxVec` bridge is the accepted answer.
- **Static borrow-safety across the seam** — once a value crosses into managed code, Rust's compile-time ownership guarantee can't be enforced (functional correctness yes; the *guarantee* no).
- **Arbitrary novel inline asm.** Common patterns are coverable; a hand-rolled novel block isn't.
