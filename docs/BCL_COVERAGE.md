# BCL coverage matrix — what `mycorrhiza` wraps idiomatically vs raw vs not-yet

*(Theme 6 / ERGONOMICS_ROADMAP.md.) A single-glance map of which .NET Base Class Library types,
collections, and interop features have an **idiomatic** `mycorrhiza` wrapper (used like `std` — no
CLR-interop knowledge at the call site), which are reachable only through the **raw generated
bindings** / low-level intrinsics, and which are **not yet supported** (with the reason — almost
always a real backend/typechecker ceiling, not a library gap). It sets expectations and guides where
Theme-1/2 breadth work should go next.*

**Legend**

| Mark | Meaning |
|---|---|
| ✅ **idiomatic** | A hand-written `mycorrhiza` wrapper: `snake_case` methods, `&str` in / `String` out, `Option`/`Result` where they fit, the natural std traits. No `instanceN`, no `!0` def-shapes, no assembly strings at the call site. |
| 🟡 **raw only** | Reachable, but only through the generated `bindings.rs` (~4256 method wrappers) or the low-level `intrinsics` (`instanceN`/`staticN`/`virtN`, the WF-9 `dotnet_generic!` macros). Works; not ergonomic. Every ✅ wrapper exposes a `.handle()` escape hatch down to this tier. |
| ⛔ **unsupported** | Cannot be surfaced cleanly right now. Each row names the specific wall (a *typechecker* ceiling that must not be weakened, or an unimplemented backend capability). |

**Reflects the true post-run state** — every ✅ row below is exercised by a runnable example crate
that was compiled with the backend and executed on real .NET (`CARGO_DOTNET_BACKEND=native`). The
"proof" column names the crate and the checks it passed on this run:

| Example crate | Direction | Result (this run) |
|---|---|---|
| `cargo_tests/cd_collections` | .NET-from-Rust | **128/128** |
| `cargo_tests/cd_bcl` | .NET-from-Rust | **313/313** |
| `cargo_tests/cd_idiomatic` | .NET-from-Rust | **45/45** |
| `cargo_tests/cd_enumerate` | .NET-from-Rust | **22/22** |
| `cargo_tests/cd_json` | .NET-from-Rust | **47/47** |
| `cargo_tests/cd_delegates` | .NET-from-Rust | **14/14** |
| `cargo_tests/cd_async` | .NET-from-Rust | **7/7** |
| `cargo_tests/cd_generic` | .NET-from-Rust (low-level bridge) | **18/18** |
| `cargo_tests/cd_containers2` | Rust-from-C# | **30/30** |
| `cargo_tests/cd_rustvec` | Rust-from-C# | **37/37** |
| `cargo_tests/cd_typedef` | Rust-from-C# (`#[dotnet_class]`) | **16/16** |
| `cargo_tests/cd_containers` | Rust-from-C# (`RustVec` only) | **13/13** |
| `cargo_tests/cd_interop` | Rust-from-C# | **PASS** |
| `cargo_tests/cd_interop_tier2` | Rust-from-C# | **PASS** |

> **Per-core opt-in (fixed):** the shipped `RustDotnet.Containers.cs` now gates each wrapper behind a
> preprocessor symbol driven by an msbuild prop, so a project compiles only the wrappers whose Rust
> cores it actually exports: `<UseRustDotnetContainers>` → `RustVec`/`RustBoxVec`,
> `<UseRustDotnetHashMap>` → `RustHashMap`, `<UseRustDotnetString>` → `RustString`. A `RustVec`-only
> consumer (`cd_containers`) no longer sees the `rcl_map_*`/`rcl_str_*` references, and a consumer that
> exports all three (`cd_containers2`) opts into all three props. This closes the earlier `CS0117`
> break for any consumer that exports a subset of the cores.

---

## 1. Generic collections (`mycorrhiza::collections`, in the prelude)

All backed by real managed objects; used like `std`. Element `T`/`K`/`V` must cross the boundary (a
.NET primitive, a `#[repr(C)]` value type of such, or a managed handle). Proof: `cd_collections`
(128/128), by-reference iteration in `cd_enumerate` (22/22).

| .NET type | Impl assembly | Status | Wrapper surface (idiomatic) | Notes / what's missing |
|---|---|---|---|---|
| `List<T>` | CoreLib | ✅ | `new`/`push`/`get`/`set`/`insert`/`remove_at`/`contains`/`index_of`/`clear`/`first`/`last`/`pop`/`sort`/`reverse`/`len`/`is_empty`/`to_vec`/`from_slice`/`iter`; traits `Default`, `Clone` (deep, `T:Copy`), `PartialEq`/`Eq` (element-wise), `Hash`, `From<Vec<T>>`, `FromIterator`, `Extend`, `IntoIterator for &List` | `sort_by`/`binary_search`/`retain` still 🟡 (delegate-as-generic-method-arg wall for `Sort(Comparison<T>)` — see §7). |
| `Dictionary<K, V>` | CoreLib | ✅ | `new`/`insert`/`get`/`get_or_default`/`contains_key`/`remove`/`clear`/`len`/`is_empty`; `Default` | **Iteration (`keys`/`values`/`for (k,v) in &dict`) ⛔** — `KeyValuePair<K,V>` is a *generic value type* (instance `get_Key`/`get_Value` unsupported) and `KeyCollection`/`ValueCollection` are *nested generics* the typechecker rejects. See §7. |
| `HashSet<T>` | CoreLib | ✅ | `new`/`insert`/`contains`/`remove`/`clear`/`len`/`is_empty`; `Default`, `FromIterator`, `Extend`, `IntoIterator` (enumerator) | Set algebra (`UnionWith`/`IntersectWith`) 🟡. |
| `Stack<T>` | System.Collections | ✅ | `new`/`push`/`pop`/`peek`/`clear`/`len`/`is_empty`; `Default`, `IntoIterator` (LIFO) | |
| `Queue<T>` | System.Collections | ✅ | `new`/`enqueue`/`dequeue`/`peek`/`clear`/`len`/`is_empty`; `Default`, `IntoIterator` (FIFO) | |
| `SortedDictionary<K, V>` | System.Collections | ✅ | same surface as `Dictionary` + `get_or_default` | `K` must be `IComparable`. Iteration ⛔ (same wall as `Dictionary`). |
| `SortedSet<T>` | System.Collections | ✅ | same surface as `HashSet` (ascending order); `FromIterator`, `Extend`, `IntoIterator` | `T` must be `IComparable`. |
| `LinkedList<T>` | System.Collections | ✅ | `new`/`push_back`/`contains`/`remove`/`clear`/`len`/`is_empty`; `IntoIterator`, `FromIterator`, `Extend` | Node ops (`AddFirst`/`First`/`Last`) ⛔ — they return `LinkedListNode<T>`, a nested generic (typechecker wall). `push_back` routes through `ICollection<T>.Add`. |
| `PriorityQueue<E, P>` | System.Collections | ✅ | `new`/`enqueue`/`dequeue`/`peek`/`clear`/`len`/`is_empty`; `Default` | Min-priority; `P` must be `IComparable`. No iteration (`UnorderedItems` is a nested generic value type). |
| `ConcurrentDictionary<K, V>` | System.Collections.Concurrent | ✅ | `new`/`insert`/`try_add`/`get`/`get_or_default`/`contains_key`/`clear`/`len`/`is_empty` | **`TryRemove(K, out V)` ⛔** — needs a by-ref `!N` `out` argument the generic bridge can't marshal. |
| `ConcurrentQueue<T>` | System.Collections.Concurrent | ✅ | `new`/`enqueue`/`len`/`is_empty`; `IntoIterator` (snapshot), `FromIterator`, `Extend` | **`TryDequeue`/`TryPeek` ⛔** (out-param). Pattern = produce then drain by iteration. |
| `ConcurrentBag<T>` | System.Collections.Concurrent | ✅ | `new`/`add`/`len`/`is_empty`; `IntoIterator` (snapshot), `FromIterator`, `Extend` | **`TryTake`/`TryPeek` ⛔** (out-param). |
| `ConcurrentStack<T>`, `BlockingCollection<T>` | System.Collections.Concurrent | ⛔/🟡 | — | Not wrapped; same out-param wall on the `TryX` removers. |
| `ObservableCollection<T>`, `ReadOnlyCollection<T>` | System.ObjectModel | 🟡 | — | Reachable via `dotnet_generic!`; no idiomatic wrapper yet. |
| `Span<T>` / `Memory<T>` / `ReadOnlySpan<T>` | CoreLib | ⛔ | — | Generic **value-type** instance methods (deferred WF-9 Stage-1 tail; `call.rs` asserts `!is_valuetype` for instance generics). |

**Enumerator bridge (`mycorrhiza::enumerate`)** — ✅ `for x in &collection` over every reference-type
collection above, plus any `IEnumerable<T>` handle via `Enumerable::iter_enumerator()` →
`Enumerator<T>: Iterator`. Goes through the *non-generic* `IEnumerable`/`IEnumerator` interfaces then
`castclass` + a bare-`!0` `get_Current`, sidestepping the nested-generic def-shape wall. Proof:
`cd_enumerate` (22/22).

---

## 2. BCL value types & static helpers (`mycorrhiza::bcl`, in the prelude)

Hand-written idiomatic modules over the most-reached-for BCL surface. Proof: `cd_bcl` (313/313),
`cd_json` (47/47). Each `✅` exposes a `.handle()` down to the 🟡 bindings for anything unsurfaced.

| .NET type | Status | Idiomatic surface | Std traits | Notes |
|---|---|---|---|---|
| `System.String` (as `DotNetString` / `MString`) | ✅ | `from(&str)`, `to_rust_string`, `len_utf16`, `is_empty`, `contains`/`starts_with`/`ends_with`/`index_of`, `to_upper`/`to_lower`/`trim`/`substring`/`replace`/`concat`, `empty` | `Display`/`Debug`, `Eq`/`Ord` (ordinal), `Hash` (managed content hash), `From<&str>`/`From<&String>`, `FromStr`, `Default`, `Add`/`AddAssign` (concat), `PartialEq<&str>` | `MString` is the raw handle; `DotNetString` is the newtype carrying the traits. |
| `char` (`DotNetChar`) | ✅ | `as_u16` + used across the `String` surface | — | Single UTF-16 code unit. |
| `DateTime` | ✅ | `new`/`new_time`/`now`/`utc_now`/`today`/`parse`/`parse_str`, `year`…`second`/`ticks`/`day_of_year`, `add_days`…`add_months`, `date` | `Display`/`Debug`, `Eq`, `Ord` | `DateTimeOffset` not wrapped (🟡). |
| `TimeSpan` (`DotNetTimeSpan`) | ✅ | `from_ticks`/`from_days`…`from_milliseconds`/`zero`, component + `total_*` getters, `add`/`subtract`/`negate`/`duration`, `compare_to` | `Default`, `Display`/`Debug`, `Eq`, `Ord` | |
| `Guid` | ✅ | `new_v4`/`empty`/`parse`, `is_empty`, `equals`/`compare_to`/`hash_code` | `Default`, `Display`/`Debug`, `Eq`, `Ord`, `Hash` | |
| `Uri` | ✅ | `new`, `scheme`/`host`/`port`/`absolute_path`/`query`/`fragment`/`user_info`/`authority`/`path_and_query`/`original_string`/`absolute_uri`, `is_absolute`/`is_file`/`is_loopback`/`is_default_port`/`is_base_of`, static `escape_data_string`/`unescape_data_string` | `Display`/`Debug`, `Eq` | |
| `Regex` (+ `Match`/`Matches`/`Groups`/`Group`) | ✅ | `new`, `is_match`/`find`/`find_all`/`replace_all`/`count`, group accessors, static `is_match_str`/`escape`/`unescape`; iterators over matches/groups | `Display` (on the value types) | Named-group + capture navigation surfaced. |
| `Random` | ✅ | `new`/`with_seed`/`shared`, `next`/`next_below`/`next_range`, `next_i64*`, `next_f64`/`next_f32` | `Default`, `Display` | |
| `Stopwatch` | ✅ | `new`/`start_new`/static `get_timestamp`, `start`/`stop`/`reset`/`restart`/`is_running`, `elapsed_millis`/`elapsed_ticks`/`elapsed()`→`Duration` | `Default`, `Display` | |
| `StringBuilder` | ✅ | `new`/`with_capacity`/`from_str`, `append`/`append_char`/`append_line[_str]`/`append_dotnet_string`, `insert`/`remove`/`replace`/`clear`, `len`/`capacity`/`ensure_capacity`/`set_len`/`set_capacity`, `to_rust_string`/`to_dotnet_string` | `Default`, `Display`/`Debug`, `From<&str>`, `fmt::Write` | Also a legacy `system::text::StringBuilder` (raw-ish). |
| `Environment` | ✅ | `machine_name`/`user_name`/`current_directory`/`command_line`/`new_line`, `var`/`set_var`/`expand_variables`, `process_id`/`processor_count`/`tick_count[64]`/`system_page_size`, `is_64bit_*`, `exit`/`exit_code`/`fail_fast` | (unit struct) | |
| `Math` | ✅ | `sqrt`/`cbrt`/`pow`/`exp`/`ln`/`log2`/`log10`, full trig + hyperbolic, `ceil`/`floor`/`round`/`trunc`/`abs`/`sign`/`max`/`min`/`ieee_remainder`/`copy_sign`; consts `PI`/`E`/`TAU` | (unit struct) | `f64` surface. `MathF` (f32) 🟡. |
| `System.Text.Json` (as `bcl::json::Json`) | ✅ | `parse`→`Option<Json>`, `get(name)`/`index(i)`/`len`/`kind`, `as_str`/`as_i64`/`as_f64`/`as_bool`/`is_null`, `is_object`/`is_array`, `to_json_string` | `Display` | Read/navigate a `JsonNode` DOM. **Not**: construction/mutation, `JsonSerializer<T>` (needs a generic-method instantiation), object-property enumeration (needs the enumerator over `KeyValuePair`). Numeric reads decode the node's canonical JSON token (`GetValue<T>` is a generic method). |
| `Console` (`system::console`) | 🟡 | `writeln_string`/`writeln_u64`/`writeln_f64` only | — | Thin; most Rust code uses `println!` (routes through the std PAL). No `ReadLine`/formatting wrapper. |
| `Marshal` (`system::runtime::interop_services`) | 🟡 | used internally for `PtrToStringUTF8` | — | Low-level marshalling helper; not an end-user surface. |
| `Nullable<T>` | 🟡/⛔ | used inside `bcl::json` for the options args | — | A generic value type; no idiomatic `Nullable<T>` ↔ `Option<T>` wrapper yet (value-type-generic-instance tail). See also §4 for the `null`↔`Option` (reference) bridge, which **is** ✅. |
| `DateTimeOffset`, `Decimal`, `BigInteger`, `Version`, `IPAddress`, `Encoding`, `Convert`, `BitConverter` | 🟡 | — | Reachable via `bindings.rs` / `instanceN`; no idiomatic module yet. Prime Theme-2 breadth candidates. |

---

## 3. Delegates, callbacks & async (`mycorrhiza::delegate`, `mycorrhiza::task`, in the prelude)

Proof: `cd_delegates` (14/14), `cd_async` (7/7).

| Feature | Status | Surface | Wall / what's missing |
|---|---|---|---|
| `Action<T0>` / `Action<T0,T1>` | ✅ | `Action1`/`Action2`: `from_fn(extern "C" fn)`, `invoke`, `from_handle`, `handle` | Only arities 1–2; capture-less fns only. |
| `Func<T0,R>` / `Func<T0,T1,R>` | ✅ | `Func1`/`Func2`: same shape, value-returning | Only arities 1–2. |
| `Comparison<T>` | ✅ | `from_fn`/`invoke`/`handle` — `(T,T)->i32` | — |
| Invoke a *held* .NET delegate from Rust | ✅ | `from_handle(h).invoke(..)` (`callvirt Delegate::Invoke`) | — |
| **Closure captures** | ⛔ | — | A captured environment has no managed home without a boxing trampoline. Pass state via args or a `static`. |
| **Delegate as a *generic-method* argument** | ⛔ | — | `List<T>.Sort(Comparison<T>)` / `ForEach(Action<T>)` need the CIL verifier to model nested generic-param binding (`Comparison`1<!0>` param vs `Comparison`1<int32>` arg) — a separate *sound* extension, **not** a checker relaxation. |
| .NET **events** (`add_*`/`remove_*`) | ⛔ | — | Compose once a delegate is a legal generic-method argument. |
| `.await` a non-generic `Task` | ✅ | `task::Task` (`delay`/`completed`/`run`/`is_completed`), `await_unit(t).await`, `block_on` | Covers the large non-result async surface (`Task.Delay`, `Task.Run(Action)`, `FlushAsync`, …). |
| `.await` a `Task<T>` a .NET API *returned* | ✅ | `await_task(t).await` (`IsCompleted`/`Result` = bare `!0`) | Works when handed a concrete `Task<int>`. |
| Expose a Rust `async fn` as a non-generic `Task` | ✅ | `future_to_task_unit(fut)` (via non-generic `TaskCompletionSource`) | — |
| **Produce a `Task<T>` from a Rust value** | ⛔ | — | `TaskCompletionSource<T>.Task` / `Task.FromResult<T>` return the nested-generic def-shape `Task`1<!0>`, which can't land in a valid Rust local without a sound backend addition (inserted upcast in `call_generic`, or generic-method args). Same ceiling as the enumerator/Dictionary walls. |
| LINQ adapters (`.where_`/`.select`/`.to_list`) | ⛔/🟡 | — | Land once delegate-as-generic-method-arg lands (predicates are delegates over the class generic). |

---

## 4. Error & optional-value bridges (`mycorrhiza::error`, in the prelude)

Proof: `cd_idiomatic` (45/45).

| Bridge | Status | Surface | Notes |
|---|---|---|---|
| managed `null` ↔ `Option` | ✅ | `Nullable` trait (`null_ref`/`is_null_ref`/`is_present`/`map_present`/`present`), free `from_nullable(h, \|\| ..)` | Deliberately maps the live ref to a **Rust value** inside the `Option` — `Option<managed-ref>` is a true layout wall (a managed ref can't sit in an enum niche on CoreCLR). The mapping closure *captures* rather than receives the ref. |
| thrown exception ↔ `Result` | ✅ | `try_managed(\|\| ..)` / the `.try_()` combinator → `Result<T, ManagedException>` | Runs the closure inside a CIL `try/catch` (via the `rustc_clr_interop_try_catch` primitive). Catches *foreign* .NET exceptions (the ones `catch_unwind` rethrows). |
| Carry the exception object across the seam | ⛔ | — | `ManagedException` is a marker for now: a managed ref can't be smuggled through the `*mut u8` catch callback. Follow-up once a managed-ref catch ABI exists. |

---

## 5. Rust-from-C# — exporting Rust ergonomically

The mirror direction: a C# dev consuming a Rust `cdylib`. Proof: `cd_typedef` (16/16),
`cd_containers2` (30/30), `cd_rustvec` (37/37), `cd_interop`/`cd_interop_tier2` (PASS).

| Feature | Status | Surface | Notes / walls |
|---|---|---|---|
| `#[dotnet_export] fn` → `MainModule.method(..)` | ✅ | proc-macro (`dotnet_macros`); marshals `&str`/`String` (as a real managed `System.String` via the `MString` seam) + primitives, no `(ptr,len)` glue | Follow-ups (🟡/⛔): slices, `char`, `Vec<T>`, `Option`/`Result` returns. |
| `#[dotnet_class]` struct → managed class | ✅ | proc-macro; a Rust struct becomes `Class : System.Object` with a parameterized primary ctor + a parameterless ctor | Managed-type fields, properties (`get_`/`set_`), interface impls, virtual/subclassable — ⛔ (need a "re-open an existing class" comptime capability; split decl from `impl`). |
| `#[dotnet_methods] impl` → managed methods | ✅ | proc-macro; static + instance Rust fns become methods on the `#[dotnet_class]` type (getters/setters/`make`/`sum` in `cd_typedef`) | |
| `RustVec<T>` (C# → Rust growable vec) | ✅ | `export_rust_containers!()` (Rust) + shipped `RustDotnet.RustVec<T>` / `RustBoxVec<T>` (C#). `T:unmanaged` near-zero-cost; any managed `T` via `GCHandle` boxing | Size-erased core; one monomorphization backs every `T`. |
| `RustHashMap<K,V>` (C# → Rust map) | ✅ | `export_rust_hashmap!()` + shipped `RustDotnet.RustHashMap<K,V>` | Both `K`/`V` `unmanaged`, hashed by raw key bytes. |
| `RustString` (C# → Rust UTF-8 buffer) | ✅ | `export_rust_string!()` + shipped `RustDotnet.RustString` | Marshals to/from managed `System.String` as UTF-8. |
| `Class<ASM,PATH>` GCHandle wrapper (`mycorrhiza::class`) | 🟡 | `ctor0`/`ctor1`/`instanceN`/`virtN` over a `GCHandle`-rooted managed object, with `Drop`/`Clone` | Low-level building block; used when a managed object must outlive a single call. |
| Export Rust `enum`/`Result`/`Option` as C# enum/try-pattern/nullable | ⛔ | — | Removes the manual bool/out-param convention; not yet built. |
| Export Rust traits as C# interfaces | ⛔ | — | Pairs with `#[dotnet_class]` interface support. |
| C# delegates → Rust `impl Fn` | ⛔ | — | Mirror of §3 delegates; unlocks C#-drives-Rust callbacks. |

---

## 6. Low-level interop surface (the 🟡 floor everything wraps)

Always available; not ergonomic, but the honest escape hatch and what new wrappers are built from.

| Surface | Where | What it gives you |
|---|---|---|
| Generated bindings (`bindings.rs`, ~4256 method wrappers) | `mycorrhiza::bindings` (glob-re-exported at crate root) | An `instanceN`/`staticN`/`virtN` method on the concrete `RustcCLRInteropManagedClass<ASM, PATH>` for a large slice of the BCL, generated by `cargo_tests/spinacz`. |
| Managed-handle intrinsics | `mycorrhiza::intrinsics` | `RustcCLRInteropManagedClass`/`…Generic`/`…Struct`/`…GenericStruct`, `…ManagedChar`, `rustc_clr_interop_managed_checked_cast`, the `rustc_clr_interop_generic_*` magic fns, `rustc_clr_interop_delegate`, `rustc_clr_interop_try_catch`. |
| WF-9 generic-bridge macros | `mycorrhiza::{dotnet_generic, dotnet_generic_impl, gen}` (+ `generic_bridge`) | Hand-roll a typed wrapper over *any* generic .NET type/method: a handle alias + `raw_*` free fns naming the .NET member and the `!N` positions. This is how §1 is built. |
| Marker traits | `mycorrhiza::{ManagedSafe, StackOnly, IntoManagedSafe, FromManagedSafe}` | Boundary-safety markers for what may cross the seam / live only on the stack. |

---

## 7. The recurring walls (why some rows are ⛔)

These are **genuine ceilings**, not backlog — surfaced repeatedly above. None is fixable by weakening
the CIL typechecker (forbidden); each needs either a *sound* backend addition or is a true layout
limit. (Cross-ref: `docs/TRANSLATION_STATUS.md` §7, and the module docs in `collections.rs` /
`enumerate.rs` / `task.rs`.)

1. **Nested-generic def-shape return.** A methodref whose *return* is a nested generic spelling the
   enclosing generic (`IEnumerator`1<!0>`, `Task`1<!0>`, `KeyCollection<!0,!1>`, `LinkedListNode<!0>`)
   has no valid Rust local to land in. The checker soundly accepts a **bare `!N`** return against a
   concrete local (the WF-9 marker guard) but not a nested generic — and must not be relaxed. This
   blocks: `Task<T>` *production*, `Dictionary` key/value collections, `LinkedList` node ops,
   `PriorityQueue.UnorderedItems`. The **workaround pattern** (used by the enumerator bridge): take
   the *non-generic* interface route, then `castclass` to the generic view, and only read members that
   return a bare `!N`.
2. **Generic value-type instance methods.** Instance calls on a generic *value type* are unsupported
   (`src/terminator/call.rs` asserts `!is_valuetype` for the instance-generic KIND). Blocks:
   `KeyValuePair<K,V>.get_Key/get_Value`, `Span<T>`/`Memory<T>`, `List<T>.Enumerator` (the struct
   enumerator — the bridge uses the boxed interface enumerator instead), `Nullable<T>` instance
   members.
3. **By-ref `!N` (`out`) arguments.** The generic bridge marshals value-in / value-out args, not a
   `!N` `out` parameter. Blocks the concurrent collections' `TryRemove`/`TryDequeue`/`TryTake`/
   `TryPeek` removers (all hand the element back through an `out`).
4. **Delegate as a generic-method argument.** Passing a delegate parameterised by the class generic
   into a generic method (`List<T>.Sort(Comparison<T>)`, LINQ predicates) needs the verifier to model
   nested generic-param binding — a sound extension, not a relaxation. Blocks `sort_by`, `ForEach`,
   LINQ adapters, `.NET` events.
5. **Closure captures across the delegate seam.** No managed home for a capture environment without a
   boxing trampoline. Capture-less `extern "C" fn` only.
6. **`Option<managed-ref>` / a managed ref in an enum niche or coroutine state.** A managed reference
   may not live in an overlapping/union field (the GC needs an unambiguous ref-map per offset). This
   is *why* the `null`↔`Option` bridge maps to a Rust value, and why a `Task`/`TaskT` handle must not
   be held across an `.await` inside an `async fn` (only in a plain `Future` struct driven by
   `block_on`).
7. **Carrying a caught exception object.** The `try/catch` primitive reports *that* an exception was
   thrown but can't hand the object back through the `*mut u8` catch callback yet — hence
   `ManagedException` is a marker.
