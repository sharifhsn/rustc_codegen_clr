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

---

## Theme 1 — .NET-from-Rust: collections & iteration

| Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|
| **Enumerator bridge ⚑** | `for x in anyEnumerable` — wrap `IEnumerator<T>` (`MoveNext`/`get_Current`) generically | M | ★★★ | Unlocks iteration over *every* .NET collection + LINQ results + `Dictionary` keys/values/entries. The single highest-leverage collections item. Uses the existing generic bridge. |
| Collection conveniences | `with_capacity`, `first`/`last`/`pop`, `Index`/`IndexMut`, `retain`, `sort`, `sort_by`, `binary_search`, `clone` (deep, via `ToArray`/ctor) | M | ★★ | Per-collection; mostly more `dotnet_generic_impl!` lines + a few ctor arities. |
| `FromIterator` / `Extend` / `IntoIterator` | `let l: List<i32> = (0..5).collect();`, `l.extend(iter)`, `for x in &l` | S–M | ★★ | Depends on the enumerator bridge for `&List` iteration by ref. |
| `Vec`↔`List` / `array`↔`List` conversions | `List::from_slice`, `list.to_vec()`, `From<Vec<T>>` | S | ★★ | `to_vec` already trivially doable via index loop; make it a trait. |
| Dictionary iteration | `for (k, v) in &dict`, `.keys()`, `.values()` | M | ★★ | Needs enumerator bridge over `KeyValuePair<K,V>`. |
| More collections | `SortedDictionary`, `SortedSet`, `LinkedList`, `PriorityQueue`, `ReadOnlyCollection`; concurrent: `ConcurrentDictionary`, `ConcurrentQueue`, `ConcurrentBag` | M each | ★–★★ | Same pattern as the existing five; watch the impl-assembly gotcha (`System.Collections`, `…Concurrent`). |
| `KeyValuePair<K,V>`, `Nullable<T>` wrappers | idiomatic `Nullable<T>` ↔ `Option<T>` | S–M | ★★ | `Nullable` is a valuetype generic → tests the value-type-generic-instance-method tail (Span<T> lands here too). |

## Theme 2 — .NET-from-Rust: idiomatic traits & type wrappers

| Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|
| Std-trait impls for managed wrappers | `Display`/`Debug` (via `ToString`), `PartialEq`/`Eq` (via `Equals`), `Hash` (`GetHashCode`), `Ord` (`IComparable`), `Clone`, `Default` | M | ★★★ | Makes *every* managed wrapper feel Rusty (`println!("{obj}")`, `==`, use as `HashMap` keys). A derive/blanket-ish approach amortizes it. |
| First-class managed `String` | `DotNetString` with `Display`/`From<&str>`/`Deref`-ish + the common String methods; seamless `&str`↔`String` | M | ★★ | `MString` exists; polish into the idiomatic type + conversions. |
| `Option`/`Result` bridges | `.NET null` ↔ `Option`, `.NET` exception ↔ `Result` (ergonomic wrappers over the `try_catch` primitive) | M | ★★★ | Error handling is pervasive; a `.try_()` combinator that turns a throwing call into `Result` is a big safety+ergonomics win. |
| Common BCL type wrappers | `DateTime`, `TimeSpan`, `Guid`, `Uri`, `Regex`, `Random`, `Stopwatch`, `StringBuilder`, `Environment`, `Math` (idiomatic, not raw `instanceN`) | S–M each | ★★ | The bindings expose these low-level; hand-write the ~15 most-used as idiomatic modules. |
| `System.Text.Json` bridge | `json::parse` / `to_string` over `JsonSerializer`/`JsonDocument` | M–L | ★★ | Or bridge `serde` ⇄ .NET; huge for real apps. |
| Enum interop | .NET enum ↔ Rust enum (values + names) | M | ★ | |

## Theme 3 — .NET-from-Rust: the big capabilities

| Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|
| **Delegates & callbacks ⚑** | pass a Rust closure to a `Func<T>`/`Action<T>` param; hold/invoke a .NET delegate; subscribe to .NET **events** | L–XL | ★★★ | The biggest missing capability for *real* .NET usage — most non-trivial APIs (events, LINQ, async continuations, UI) take callbacks. Builds on the existing `calli`/`ldftn` fn-ptr path (the DerfWrongPtr fix). Likely the highest-value single item in this whole doc. |
| **Task / async bridge ⚑** | `.await` a `Task<T>` from Rust; expose a Rust `async fn` as a .NET `Task` | L | ★★★ | async already runs on the PAL; this is the *interop* adapter (Task ↔ Rust Future). Unlocks the entire modern-async .NET surface (HttpClient, EF, ASP.NET). |
| LINQ-style adapters | `.where_()`, `.select()`, `.to_list()` over `IEnumerable` | M | ★★ | Nice once the enumerator bridge + delegates land (predicates are delegates). |
| `Span<T>`/`Memory<T>` | zero-copy views into managed arrays | M | ★★ | Needs generic value-type instance methods (the deferred WF-9 Stage-1 tail). |

## Theme 4 — Rust-from-C#: exporting Rust ergonomically

| Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|
| **`#[dotnet_export]` auto-marshal ⚑** | write `#[dotnet_export] fn greet(name: &str) -> String`; C# calls `greet("x")` and gets a `string` — no hand-written `(ptr,len)` buffer dance | M–L | ★★★ | The Rust-from-C# counterpart to what `collections` did for the other direction. A proc-macro (the `dotnet_macros` crate exists) + a few backend marshalling helpers. Makes exporting Rust as easy as the container macro made containers. |
| Extend `#[dotnet_class]` | virtual methods; managed-type fields; properties (`get_`/`set_`); static methods; multiple ctors; implement a .NET interface | L | ★★★ | Virtual methods need a "re-open an existing class" comptime capability (split class decl from an `impl` block). This is the path to real, subclassable/​interface-implementing managed types. |
| Export Rust `enum` / `Result` / `Option` | Rust enum → C# enum; `Result` → try-pattern/exception; `Option` → nullable | M | ★★ | Removes the manual bool/out-param convention. |
| Export Rust traits as C# interfaces | a Rust trait object usable polymorphically from C# | L | ★★ | Advanced; pairs with `#[dotnet_class]` interface support. |
| More reusable containers for C# | `RustHashMap<K,V>`, `RustString`, `IEnumerable<T>` over a `RustVec` | M | ★★ | Same `export_*!` + shipped-C#-wrapper pattern as `RustVec<T>`. |
| C# delegates → Rust | pass a C# `Action`/`Func` into Rust as `impl Fn` | L | ★★ | The mirror of Theme-3 delegates; unlocks C#-drives-Rust callbacks. |

## Theme 5 — tooling & onboarding (cargo-dotnet)

| Item | What the user gets | Effort | Payoff | Notes / deps |
|---|---|---|---|---|
| **`cargo dotnet new` ⚑** | scaffold a ready-to-run project from a template: `--lib` (Rust cdylib + C# consumer), `--app` (Rust-on-.NET binary), `--plugin` | M | ★★★ | The onboarding keystone — zero-to-running in one command instead of assembling Cargo.toml + csproj + targets by hand. |
| **`mycorrhiza::prelude` ⚑** | `use mycorrhiza::prelude::*;` brings collections, common wrappers, and the macros into scope | S | ★★ | Trivial, high perceived value. Do it first. |
| `cargo dotnet test` / `bench` | run Rust `#[test]`/`#[bench]` on .NET | M | ★★ | Lets library authors validate on the real target. |
| Better interop diagnostics | map `TypeLoadException` → the impl-assembly gotcha; `MissingMethod` → "did you export/`export_rust_containers!`?"; a `cargo dotnet doctor` for interop | M | ★★ | Turns today's cryptic runtime failures into actionable messages. |
| `cargo dotnet publish` (+ `--aot`) | self-contained / NativeAOT single-file output as one command | M | ★★ | AOT is proven; wire the first-class command. |
| NuGet packaging polish | `cargo dotnet pack --nuget` → a real `.nupkg` (Rust `.dll` + shipped C# wrappers + metadata), publishable | M | ★★ | Makes a Rust library distributable to C# devs the normal way. |

## Theme 6 — docs, examples, discoverability

| Item | What the user gets | Effort | Payoff | Notes |
|---|---|---|---|---|
| Cookbook / recipes | "how do I: read a file, HTTP GET, parse JSON, use a NuGet library, expose a Rust struct, handle an event" | M | ★★ | Recipe-per-task; the highest-traffic doc format. |
| Hosted API docs | rustdoc for `mycorrhiza` + the C# XML docs, published | S–M | ★★ | The wrappers already carry doc comments. |
| Flagship examples | a real app end-to-end (e.g. a small web service using an ASP.NET/HttpClient path, or a CLI using a .NET library) | M–L | ★★ | Proof + copy-paste starting point; depends on delegates/async for the juicy ones. |
| BCL coverage matrix | which types/methods have idiomatic wrappers vs raw bindings vs unsupported | S | ★ | Sets expectations; guides Theme-2 work. |

---

## Recommended sequence (opinionated)

The keystones aren't independent — there's a natural order where each unlocks the next:

1. **Quick wins first** (a day total): `prelude`, collection conveniences, `Vec`↔`List`, std-trait impls (`Display`/`Debug`/`Eq`/`Hash`). Immediate "it feels like std" payoff, no new machinery.
2. **Enumerator bridge** (Theme 1 ⚑): unlocks all iteration, dict iteration, and the LINQ/predicate story downstream.
3. **`cargo dotnet new` + prelude** (Theme 5 ⚑): the onboarding cliff. Cheap, huge for adoption.
4. **`#[dotnet_export]` auto-marshal** (Theme 4 ⚑): makes the Rust-from-C# side as turnkey as containers already are — symmetry with the .NET-from-Rust polish.
5. **Delegates & callbacks** (Theme 3 ⚑): the hard, highest-ceiling item. Unlocks events, LINQ lambdas, and is a prerequisite for the async bridge's ergonomics. Tackle once the cheaper wins are banked.
6. **Task/async bridge** (Theme 3 ⚑): with delegates in hand, this opens the entire modern .NET surface.
7. Then breadth: more collections, BCL type wrappers, JSON, extended `#[dotnet_class]`, publish/pack polish, cookbook.

## Walls (won't-do / can't-do cleanly — from TRANSLATION_STATUS §7)

Not ergonomics gaps — genuine ceilings, listed so they're not mistaken for backlog:
- A **transparent zero-cost open generic whose overlapping layout holds a managed reference** (CLI §9.5). The two-mode `RustVec`/`RustBoxVec` bridge is the accepted answer.
- **Static borrow-safety across the seam** — once a value crosses into managed code, Rust's compile-time ownership guarantee can't be enforced (functional correctness yes; the *guarantee* no).
- **Arbitrary novel inline asm.** Common patterns are coverable; a hand-rolled novel block isn't.
