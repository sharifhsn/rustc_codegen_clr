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
| `cargo_tests/cd_collections` | .NET-from-Rust | **141/141** |
| `cargo_tests/cd_bcl` | .NET-from-Rust | **324/324** |
| `cargo_tests/cd_decimal` | .NET-from-Rust (`DotNetDecimal`) | **11/11** |
| `cargo_tests/cd_span` | .NET-from-Rust (`Span<T>`/value-type generics) | **45/45** |
| `cargo_tests/cd_sync` | .NET-from-Rust (`SharedOnce`, channels, locks) | **43/43** |
| `cargo_tests/cd_idiomatic` | .NET-from-Rust | **45/45** |
| `cargo_tests/cd_enumerate` | .NET-from-Rust | **22/22** |
| `cargo_tests/cd_json` | .NET-from-Rust | **47/47** |
| `cargo_tests/cd_delegates` | .NET-from-Rust | **14/14** |
| `cargo_tests/cd_async` | .NET-from-Rust | **9/9** |
| `cargo_tests/cd_linq_expr` | .NET-from-Rust (LINQ/EF expression trees) | **89/89** |
| `cargo_tests/cd_generic` | .NET-from-Rust (low-level bridge) | **18/18** |
| `cargo_tests/cd_containers2` | Rust-from-C# | **30/30** |
| `cargo_tests/cd_rustvec` | Rust-from-C# | **37/37** |
| `cargo_tests/cd_typedef` | Rust-from-C# (`#[dotnet_class]`) | **16/16** |
| `cargo_tests/cd_containers` | Rust-from-C# (`RustVec` only) | **13/13** |
| `cargo_tests/cd_interop` | Rust-from-C# | **PASS** |
| `cargo_tests/cd_interop_tier2` | Rust-from-C# | **PASS** |

> **Numbers drift.** These are live per-crate `chk!` pass/total counts re-run against current `HEAD`
> (2026-07-06) — expect them to keep growing as breadth work lands; treat "N/N" as "still all green",
> not as a permanently fixed count. `STATE_OF_THE_PROJECT.md`'s own summary row can lag behind the
> exact number for the same reason.

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
(141/141), by-reference iteration in `cd_enumerate` (22/22).

| .NET type | Impl assembly | Status | Wrapper surface (idiomatic) | Notes / what's missing |
|---|---|---|---|---|
| `List<T>` | CoreLib | ✅ | `new`/`push`/`get`/`set`/`insert`/`remove_at`/`contains`/`index_of`/`clear`/`first`/`last`/`pop`/`sort`/`sort_by`/`for_each`/`reverse`/`len`/`is_empty`/`to_vec`/`from_slice`/`iter`; traits `Default`, `Clone` (deep, `T:Copy`), `PartialEq`/`Eq` (element-wise), `Hash`, `From<Vec<T>>`, `FromIterator`, `Extend`, `IntoIterator for &List` | `sort_by`(`Sort(Comparison<T>)`)/`for_each`(`ForEach(Action<T>)`) now **✅** — the delegate-as-generic-method-arg wall (formerly §8 wall #4) is closed. `binary_search`/`retain` still unwrapped (no backend wall; just unbuilt). |
| `Dictionary<K, V>` | CoreLib | ✅ | `new`/`insert`/`get`/`get_or_default`/`contains_key`/`remove`/`clear`/`len`/`is_empty`/`keys`/`values`; `Default`; `Enumerable` (`for (k,v) in &dict`) | **Iteration now ✅** — `keys()`/`values()`/`for (k,v) in &dict` drive the enumerator over `KeyValuePair<K,V>` (the value-type-generic-instance-method wall closed; see §8 note). `KeyCollection`/`ValueCollection` themselves (as .NET view types, not Rust iterators) are still not surfaced. |
| `HashSet<T>` | CoreLib | ✅ | `new`/`insert`/`contains`/`remove`/`clear`/`len`/`is_empty`; `Default`, `FromIterator`, `Extend`, `IntoIterator` (enumerator) | Set algebra (`UnionWith`/`IntersectWith`) 🟡. |
| `Stack<T>` | System.Collections | ✅ | `new`/`push`/`pop`/`peek`/`clear`/`len`/`is_empty`; `Default`, `IntoIterator` (LIFO) | |
| `Queue<T>` | System.Collections | ✅ | `new`/`enqueue`/`dequeue`/`peek`/`clear`/`len`/`is_empty`; `Default`, `IntoIterator` (FIFO) | |
| `SortedDictionary<K, V>` | System.Collections | ✅ | same surface as `Dictionary` + `get_or_default`, `keys`/`values` iteration | `K` must be `IComparable`. |
| `SortedSet<T>` | System.Collections | ✅ | same surface as `HashSet` (ascending order); `FromIterator`, `Extend`, `IntoIterator` | `T` must be `IComparable`. |
| `LinkedList<T>` | System.Collections | ✅ | `new`/`push_back`/`contains`/`remove`/`clear`/`len`/`is_empty`; `IntoIterator`, `FromIterator`, `Extend` | Node ops (`AddFirst`/`First`/`Last`) ⛔ — they return `LinkedListNode<T>`, a nested generic (typechecker wall, §8). `push_back` routes through `ICollection<T>.Add`. |
| `PriorityQueue<E, P>` | System.Collections | ✅ | `new`/`enqueue`/`dequeue`/`peek`/`clear`/`len`/`is_empty`; `Default` | Min-priority; `P` must be `IComparable`. No iteration (`UnorderedItems` is a nested generic value type, §8). |
| `ConcurrentDictionary<K, V>` | System.Collections.Concurrent | ✅ | `new`/`insert`/`try_add`/`get`/`get_or_default`/`contains_key`/`clear`/`len`/`is_empty` | **`TryRemove(K, out V)` ⛔** — needs a by-ref `!N` `out` argument the generic bridge can't marshal. |
| `ConcurrentQueue<T>` | System.Collections.Concurrent | ✅ | `new`/`enqueue`/`len`/`is_empty`; `IntoIterator` (snapshot), `FromIterator`, `Extend` | **`TryDequeue`/`TryPeek` ⛔** (out-param). Pattern = produce then drain by iteration. |
| `ConcurrentBag<T>` | System.Collections.Concurrent | ✅ | `new`/`add`/`len`/`is_empty`; `IntoIterator` (snapshot), `FromIterator`, `Extend` | **`TryTake`/`TryPeek` ⛔** (out-param). |
| `ConcurrentStack<T>`, `BlockingCollection<T>` | System.Collections.Concurrent | ⛔/🟡 | — | Not wrapped; same out-param wall on the `TryX` removers. |
| `ObservableCollection<T>`, `ReadOnlyCollection<T>` | System.ObjectModel | 🟡 | — | Reachable via `dotnet_generic!`; no idiomatic wrapper yet. |
| `Span<T>` / `ReadOnlySpan<T>` | CoreLib | ✅ | `mycorrhiza::span::{Span, ReadOnlySpan}` — zero-copy view over a Rust `&mut [T]`/`&[T]`: `from_slice`, `len`/`is_empty`, `get`/`set`, `fill`, `clear`, `.handle()` to pass to a .NET API | Generic **value-type** instance methods (former §8 wall #2) are now supported for the concrete members exercised here (`get_Length`, `Fill`, `Clear`, the byref `get_Item` indexer). `Memory<T>` (the heap-backed sibling) is still unwrapped. `ReadOnlySpan<T>.get_Item` is unreachable (`ref readonly T` — a `modreq`-decorated byref no plain `!0&` methodref can match); read the backing Rust slice directly instead. Proof: `cd_span` (45/45). |

**Enumerator bridge (`mycorrhiza::enumerate`)** — ✅ `for x in &collection` over every reference-type
collection above, plus any `IEnumerable<T>` handle via `Enumerable::iter_enumerator()` →
`Enumerator<T>: Iterator`. Goes through the *non-generic* `IEnumerable`/`IEnumerator` interfaces then
`castclass` + a bare-`!0` `get_Current`, sidestepping the nested-generic def-shape wall. Also backs
`Dictionary`/`SortedDictionary` key/value iteration over the *value-type-generic* `KeyValuePair<K,V>`.
Proof: `cd_enumerate` (22/22), `cd_collections` (141/141).

---

## 2. BCL value types & static helpers (`mycorrhiza::bcl`, in the prelude)

Hand-written idiomatic modules over the most-reached-for BCL surface. Proof: `cd_bcl` (324/324),
`cd_decimal` (11/11), `cd_json` (47/47). Each `✅` exposes a `.handle()` down to the 🟡 bindings for
anything unsurfaced.

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
| `Decimal` (`DotNetDecimal`, `mycorrhiza::bcl::decimal`) | ✅ | `from_i64`/`from_i32`/`parse`, `to_f64`, `to_dotnet_string`, `+`/`-`/`*`/`/` (`Add`/`Sub`/`Mul`/`Div`, via the real `Decimal` operators — bit-identical to C#) | `Display`, `PartialEq`/`Eq`, `PartialOrd`/`Ord` (via `Decimal.Compare` — an exact base-10 total order, not culture-sensitive, so a real `Ord` applies) | Closes the former Theme-2 backlog item ("`Decimal` operator/trait completeness audit"). Proof: `cd_decimal` (11/11), also folded into `cd_bcl`. |
| `Nullable<T>` (`mycorrhiza::nullable`) | ✅ | `some(value)` (`new Nullable<T>(value)`); `NullableExt::to_option()` → `Option<T>` (`HasValue`/`Value`, only reads `Value` when present) | — | The former value-type-generic-instance-method wall (§8) is closed for `Nullable<T>`'s two members. See also §4 for the `null`↔`Option` (reference) bridge, which is a separate, older ✅. |
| `DateTimeOffset`, `BigInteger`, `Version`, `IPAddress`, `Encoding`, `Convert`, `BitConverter` | 🟡 | — | Reachable via `bindings.rs` / `instanceN`; no idiomatic module yet. Prime Theme-2 breadth candidates. |

---

## 3. Delegates, callbacks & async (`mycorrhiza::delegate`, `mycorrhiza::task`, in the prelude)

Proof: `cd_delegates` (14/14), `cd_async` (9/9), `cd_closures`, `cd_linq_expr` (89/89).

| Feature | Status | Surface | Wall / what's missing |
|---|---|---|---|
| `Action<T0>` / `Action<T0,T1>` | ✅ | `Action1`/`Action2`: `from_fn(extern "C" fn)`, `invoke`, `from_handle`, `handle` | Only arities 1–2. |
| `Func<T0,R>` / `Func<T0,T1,R>` | ✅ | `Func1`/`Func2`: same shape, value-returning | Only arities 1–2. |
| `Comparison<T>` | ✅ | `from_fn`/`invoke`/`handle` — `(T,T)->i32` | — |
| Invoke a *held* .NET delegate from Rust | ✅ | `from_handle(h).invoke(..)` (`callvirt Delegate::Invoke`) | — |
| **Closure captures** | ✅ | `Action1::from_closure(move \|x\| ..)` (and the other delegate arities) — the capture environment is boxed and leaked for `'static` lifetime, invoked through a monomorphic trampoline | Leaked, not freed — fine for long-lived callbacks/`static`-shaped usage; not a fit for a hot per-call closure churned in a loop. Former §8 wall #5. Proof: `cd_closures`. |
| **Delegate as a *generic-method* argument** | ✅ | `List<T>.Sort(Comparison<T>)` / `ForEach(Action<T>)` — see `sort_by`/`for_each` in §1 | The verifier now models the nested generic-param binding (`Comparison`1<!0>` param vs concrete `Comparison`1<int32>` arg) as a sound extension. Former §8 wall #4 — the doc's headline "must not be relaxed" caution was about the *checker*, not this addition, and holds: nothing was relaxed. |
| .NET **events** (`add_*`/`remove_*`) | ⛔ | — | Not yet composed, though the delegate-as-generic-arg prerequisite is now in place. |
| `.await` a non-generic `Task` | ✅ | `task::Task` (`delay`/`completed`/`run`/`is_completed`), `await_unit(t).await`, `block_on` | Covers the large non-result async surface (`Task.Delay`, `Task.Run(Action)`, `FlushAsync`, …). |
| `.await` a `Task<T>` a .NET API *returned* | ✅ | `await_task(t).await` (`IsCompleted`/`Result` = bare `!0`) | Works when handed a concrete `Task<int>`. |
| Expose a Rust `async fn` as a non-generic `Task` | ✅ | `future_to_task_unit(fut)` (via non-generic `TaskCompletionSource`) | — |
| **Produce a `Task<T>` from a Rust value** | ✅ | `future_to_task(fut)` packages an `async fn -> T` into a real `Task<T>` via `TaskCompletionSource<T>.get_Task()` | The nested-generic-return wall (former §8 wall #1) is closed for this specific producer path. `Task.FromResult<T>` itself is still unused (it needs generic-*method* `!!N` *argument* support the backend doesn't emit) — the `TaskCompletionSource<T>` route is sufficient and used instead. |
| LINQ / EF `IQueryable.Where(Expression<Func<T,bool>>)` | ✅ | `mycorrhiza::linq` — `Expr`/`Predicate`/`TypedPredicate<T>`/`Field<Root,Val>` build a real `System.Linq.Expressions` tree from Rust (params, binops, member access, `box`-boxed constants), compiled and handed to `IQueryable.Where` | Built on **expression trees**, not delegates — sidesteps the delegate-as-generic-arg wall entirely (EF needs the *tree*, not a compiled predicate). `.select`/`.to_list`/other LINQ operators beyond `Where` are not yet wrapped. Proof: `cd_linq_expr` (89/89). |

---

## 4. Error & optional-value bridges (`mycorrhiza::error`, in the prelude)

Proof: `cd_idiomatic` (45/45).

| Bridge | Status | Surface | Notes |
|---|---|---|---|
| managed `null` ↔ `Option` | ✅ | `Nullable` trait (`null_ref`/`is_null_ref`/`is_present`/`map_present`/`present`), free `from_nullable(h, \|\| ..)` | Deliberately maps the live ref to a **Rust value** inside the `Option` — `Option<managed-ref>` is a true layout wall (a managed ref can't sit in an enum niche on CoreCLR). The mapping closure *captures* rather than receives the ref. |
| thrown exception ↔ `Result` | ✅ | `try_managed(\|\| ..)` / the `.try_()` combinator → `Result<T, ManagedException>` | Runs the closure inside a CIL `try/catch` (via the `rustc_clr_interop_try_catch` primitive). Catches *foreign* .NET exceptions (the ones `catch_unwind` rethrows). |
| `?`-operator ergonomics for `ManagedException` | ✅ | `impl_from_managed_exception!(MyError, MyError::Managed)` generates `From<ManagedException> for MyError`, so `try_managed(\|\| ..)?` bubbles straight into a consumer's own error type | A blanket `impl<E> From<ManagedException> for E` isn't legal Rust (orphan rules), so each consumer's error type still needs one macro invocation — but that closes the former "every consumer hand-rolls the conversion" gap. |
| Carry the exception object across the seam | ⛔ | — | `ManagedException` is a marker for now: a managed ref can't be smuggled through the `*mut u8` catch callback. Follow-up once a managed-ref catch ABI exists. |

---

## 5. Synchronization & channels (`mycorrhiza::sync`, in the prelude)

Real managed synchronization primitives, cross-language-shareable (a `GCHandle`-rooted `SharedLock`
etc. can be observed/coordinated from C# too, not just used internally by Rust). Proof: `cd_sync`
(43/43).

| Feature | Status | Surface | Notes |
|---|---|---|---|
| `SharedLock` / `SharedMutex<T>` / `SharedRwLock<T>` | ✅ | `SemaphoreSlim`/`ReaderWriterLockSlim`-backed lock + data-owning guard types (`lock()`, `lock_async()`, `read()`/`write()`) | Real CLR locks, not a Rust-side spinlock — safe to hand the same lock handle to C#. |
| `SharedOnce<T>` | ✅ | `std::sync::OnceLock<T>`-shaped: `new()`, `get()`, `get_or_init(f)` | Built on `SharedLock`'s double-checked-lock pattern; the "safe `Once`/lazy-init wrapper" Theme-2 backlog item. |
| `channel<T>()` / `bounded_channel<T>(capacity)` | ✅ | `std::sync::mpsc`-shaped `Sender<T>`/`Receiver<T>` pair over `System.Threading.Channels` | Genuinely multi-producer multi-consumer (unlike `std::sync::mpsc`'s single-consumer restriction) — the Theme-2 "channel-style primitive" backlog item. C# can be a producer or consumer on the same channel. |
| `Semaphore`, `Signal`, `CountdownEvent`, `Barrier` | ✅ | Thin idiomatic wrappers over the matching `System.Threading` primitives | |

---

## 6. Rust-from-C# — exporting Rust ergonomically

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

## 7. Low-level interop surface (the 🟡 floor everything wraps)

Always available; not ergonomic, but the honest escape hatch and what new wrappers are built from.

| Surface | Where | What it gives you |
|---|---|---|
| Generated bindings (`bindings.rs`, ~4256 method wrappers) | `mycorrhiza::bindings` (glob-re-exported at crate root) | An `instanceN`/`staticN`/`virtN` method on the concrete `RustcCLRInteropManagedClass<ASM, PATH>` for a large slice of the BCL, generated by `cargo_tests/spinacz`. |
| Managed-handle intrinsics | `mycorrhiza::intrinsics` | `RustcCLRInteropManagedClass`/`…Generic`/`…Struct`/`…GenericStruct`, `…ManagedChar`, `rustc_clr_interop_managed_checked_cast`, the `rustc_clr_interop_generic_*` magic fns, `rustc_clr_interop_delegate`, `rustc_clr_interop_try_catch`. |
| WF-9 generic-bridge macros | `mycorrhiza::{dotnet_generic, dotnet_generic_impl, gen}` (+ `generic_bridge`) | Hand-roll a typed wrapper over *any* generic .NET type/method: a handle alias + `raw_*` free fns naming the .NET member and the `!N` positions. This is how §1 is built. |
| Marker traits | `mycorrhiza::{ManagedSafe, StackOnly, IntoManagedSafe, FromManagedSafe}` | Boundary-safety markers for what may cross the seam / live only on the stack. |

---

## 8. The recurring walls (why some rows are still ⛔)

These are **genuine ceilings**, not backlog — surfaced repeatedly above. None is fixable by weakening
the CIL typechecker (forbidden); each closed one needed a *sound* backend addition (never a
relaxation), and each still-open one names the reason it's a true layout/ABI limit rather than an
unwritten wrapper. (Cross-ref: `docs/TRANSLATION_STATUS.md` §7, and the module docs in
`collections.rs` / `enumerate.rs` / `task.rs` / `span.rs` / `nullable.rs`.)

1. **Nested-generic def-shape return — partially closed.** A methodref whose *return* is a nested
   generic spelling the enclosing generic (`IEnumerator`1<!0>`, `Task`1<!0>`, `KeyCollection<!0,!1>`,
   `LinkedListNode<!0>`) has no valid Rust local to land in directly. The checker still soundly accepts
   only a **bare `!N`** return against a concrete local (the WF-9 marker guard) — nested-generic
   returns are still rejected and must not be relaxed. What changed: `Dictionary`/`SortedDictionary`
   key/value iteration and `Task<T>` *production* are now both live, but neither weakens the checker —
   both route around the wall instead. Iteration uses the pre-existing **non-generic-interface
   workaround** (take `IEnumerable`, `castclass` to the generic view, read only bare-`!N` members).
   `Task<T>` production uses `TaskCompletionSource<T>.get_Task()` the same way. Still blocked by the
   *unrouted* form of this wall: `LinkedList` node ops (`AddFirst`/`First`/`Last` return
   `LinkedListNode<T>`), `PriorityQueue.UnorderedItems`, and `Task.FromResult<T>` directly (needs
   generic-*method* argument support instead, see wall 3).
2. **Generic value-type instance methods — closed for the exercised members.** Instance calls on a
   generic *value type* were unsupported (`src/terminator/call.rs` asserted `!is_valuetype` for the
   instance-generic KIND); that assertion has been relaxed *soundly* for concrete instantiations, and
   is now exercised by `Span<T>.get_Length`/`Fill`/`Clear`/the byref indexer, `KeyValuePair<K,V>.
   get_Key`/`get_Value` (via the enumerator bridge), and `Nullable<T>.get_HasValue`/`get_Value`. Not
   every generic value type is wrapped yet — `Memory<T>` and `List<T>.Enumerator` (the *struct*
   enumerator; the bridge still uses the boxed interface enumerator) remain unwrapped, but that's
   unbuilt breadth, not a reopened wall.
3. **By-ref `!N` (`out`) arguments — still open.** The generic bridge marshals value-in / value-out
   args, not a `!N` `out` parameter. Blocks the concurrent collections' `TryRemove`/`TryDequeue`/
   `TryTake`/`TryPeek` removers (all hand the element back through an `out`), and `Task.FromResult<T>`/
   any other generic-*method* call needing a type argument the bridge can't yet supply.
4. **Delegate as a generic-method argument — closed.** Passing a delegate parameterised by the class
   generic into a generic method (`List<T>.Sort(Comparison<T>)`, `ForEach(Action<T>)`) needed the
   verifier to model nested generic-param binding (`Comparison`1<!0>` param vs a concrete
   `Comparison`1<int32>` arg) — implemented as a sound extension, not a relaxation, and now backs
   `List::sort_by`/`for_each` (§1). LINQ's `IQueryable.Where` predicate does **not** use this path —
   EF needs an *expression tree*, not a compiled delegate, so `mycorrhiza::linq` builds
   `System.Linq.Expressions` trees instead (§3) and never needed this wall closed. `.NET` events
   (`add_*`/`remove_*`) could now compose on top of this but aren't wired yet.
5. **Closure captures across the delegate seam — closed.** A captured environment now has a managed
   home: `Action1::from_closure`/etc. box the closure's environment (leaked for `'static`) and invoke
   it through a monomorphic per-signature trampoline. Trade-off: the boxed environment is leaked, not
   freed, so this fits long-lived/`static`-shaped callbacks, not a closure churned per-call in a hot
   loop — a capture-less `extern "C" fn` is still the zero-overhead choice when captures aren't needed.
6. **`Option<managed-ref>` / a managed ref in an enum niche or coroutine state — still open.** A managed reference
   may not live in an overlapping/union field (the GC needs an unambiguous ref-map per offset). This
   is *why* the `null`↔`Option` bridge maps to a Rust value, and why a `Task`/`TaskT` handle must not
   be held across an `.await` inside an `async fn` (only in a plain `Future` struct driven by
   `block_on`).
7. **Carrying a caught exception object.** The `try/catch` primitive reports *that* an exception was
   thrown but can't hand the object back through the `*mut u8` catch callback yet — hence
   `ManagedException` is a marker.
