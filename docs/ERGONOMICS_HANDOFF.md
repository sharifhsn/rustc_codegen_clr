# Ergonomics work — session hand-off / cold-start guide

Everything a fresh session needs to **continue the interop-ergonomics work** without the accumulated
context: current state, how to build & verify (the footguns that cost hours), the surface as it
stands, the exact patterns to copy, and where to start. The *what-to-build* backlog is
[ERGONOMICS_ROADMAP.md](ERGONOMICS_ROADMAP.md); this is the *how-to-operate + start-here*.

---

## 1. Current state

- **Branch:** `gaps-campaign`. **~84 commits ahead of `mine/gaps-campaign`; NOTHING is pushed.**
  Commit locally; **never push** (`origin` = FractalFir's upstream; push to `mine` only when the owner
  explicitly asks). End every commit message with
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- **The ergonomics campaign is essentially COMPLETE.** All six themes shipped their keystones and most
  breadth; the full per-theme completion report is in [ERGONOMICS_STATUS.md](ERGONOMICS_STATUS.md), the
  status-marked backlog in [ERGONOMICS_ROADMAP.md](ERGONOMICS_ROADMAP.md). Verified surface (newest first):
  - `8e1fa66` / `8616f24` docs — **BCL_COVERAGE.md** (idiomatic/raw/unsupported matrix) + **INTEROP_COOKBOOK.md**.
  - `3d6217d` **System.Text.Json bridge** (`mycorrhiza::bcl::json`, `JsonNode` reference model) — `cd_json` 47/47.
  - `57e01c1` **more collections** (Sorted{Dictionary,Set}, LinkedList, PriorityQueue, Concurrent{Dictionary,Queue,Bag}) — `cd_collections` 128/128.
  - `8ce47bd` **RustHashMap<K,V> + RustString** reusable C#-consumable containers — `cd_containers2` 30/30.
  - `94d8e59` **Task/async bridge** (`mycorrhiza::task`: `.await` a `Task<T>`, expose `async fn` as `Task`) — `cd_async` 7/7.
  - `5277560` **Delegates & callbacks** (Theme-3 ⚑) — a Rust `extern "C" fn` → managed `Action`/`Func`/`Comparison`;
    magic fn `rustc_clr_interop_delegate` (`src/terminator/call.rs`) synthesises a memoised per-signature
    **shim class** (holds the native ptr, `calli`s from `Invoke`) then `newobj`s the generic delegate over
    `ldftn shim::Invoke`. Face: `mycorrhiza::delegate` (in the prelude). Proof: `cd_delegates` 14/14.
    Deferred: closure captures, delegate-as-generic-method-arg (needs a nested-`!N`-binding typecheck
    extension — do NOT relax the checker), .NET events.
  - `c1c90ce` **extend `#[dotnet_class]`** — static methods, multiple ctors, field setters, managed-type fields — `cd_typedef` 16/16.
  - `d08aba3` **`#[dotnet_export]`** (Theme-4 ⚑) — `#[dotnet_export] fn greet(name: &str) -> String` → C# calls
    `MainModule.greet("x")` and gets a `string`, NO `(ptr,len)` dance; strings cross as a real managed
    `System.String` (`MString` seam) → **zero C#-side glue, no backend change**. Proof: `cd_export` 11/11.
  - `8f7eb61` **`cargo dotnet new`/`doctor`/`test`** (Theme-5 ⚑ onboarding) — scaffold `--app`/`--lib`/`--plugin`; plus `pack` (NuGet).
  - `eb78316` **error/text ergonomics** (`mycorrhiza::error`: `null→Option`, `throw→Result`, `DotNetString`) — `cd_idiomatic` 45/45.
  - `957ca95` **`mycorrhiza::bcl`** — DateTime/TimeSpan/Guid/Uri/Regex/Random/Stopwatch/StringBuilder/Environment/Math — `cd_bcl` 313/313.
  - `fb35b88` **enumerator bridge** (Theme-1 ⚑) — `for x in &collection` over `IEnumerator<T>` — `cd_enumerate` 22/22.
  - `24055af` **`mycorrhiza::prelude`** + collection conveniences + honest std traits — `cd_collections` grew to 70/70.
  - (earlier) `d1b7042` reusable `RustVec<T>`/`RustBoxVec<T>`; `9262668` `mycorrhiza::collections`; `8c28ce8`/`d08aba3`
    `dotnet_macros`; WF-9 generic bridge (`22aae7d`/`e2b493e`/`f73bca7`/`dcb8481`); the 10-commit macro-refactor (`c86e699`..`a3739b2`).
- **What works today:** both interop directions are functional AND ergonomic across the whole surface —
  `.NET-from-Rust` via `mycorrhiza::{collections,bcl,error,task,delegate,enumerate}` + `prelude`;
  `Rust-from-C#` via `export_rust_containers!`/`RustHashMap`/`RustString` + `#[dotnet_export]` +
  `#[dotnet_class]`; onboarding via `cargo dotnet new`. See [QUICKSTART_INTEROP.md](QUICKSTART_INTEROP.md)
  and [INTEROP_COOKBOOK.md](INTEROP_COOKBOOK.md).

---

## 2. How to build & verify (READ THIS — the footguns cost hours)

**Toolchain:** `nightly-2026-06-17`. For native `cargo dotnet`, the nightly toolchain bin **must be first
on PATH**, or PAL injection goes into the *stable* rust-src and the build fails silently with a stale dll:
```bash
export PATH="$(rustc +nightly-2026-06-17 --print sysroot)/bin:$PATH"
```

**If you changed ONLY `mycorrhiza/` or an example crate → NO backend rebuild.** The installed backend
(`~/.cargo-dotnet/bin/{librustc_codegen_clr.dylib,linker}`) compiles mycorrhiza for you. Most ergonomics
work (collections, wrappers, macros) is mycorrhiza-only → skip straight to the native test.

**If you changed backend code (`src/` or `cilly/`) → rebuild BOTH and refresh the install** (cilly lives
in the linker bin AND the dylib; refreshing only one is the classic stale-install trap):
```bash
cargo build --release -p cilly --bins      # linker
cargo build --release                       # backend dylib
cp target/release/librustc_codegen_clr.dylib ~/.cargo-dotnet/bin/
cp target/release/linker                     ~/.cargo-dotnet/bin/
```

**Native interop test (the REAL verification for interop work — the ::stable gate does NOT exercise
mycorrhiza/WF-9):**
```bash
# A Rust binary that calls .NET (cd_collections, cd_generic):
cd cargo_tests/cd_collections
export PATH="$(rustc +nightly-2026-06-17 --print sysroot)/bin:$PATH"
rm -rf target                               # see the stale-artifact gotcha below
RCL_ICE_LOG=1 CARGO_DOTNET_BACKEND=native cargo dotnet run   # prints pass then total; expect equal

# A C#-consumes-Rust crate (cd_containers, cd_rustvec, cd_interop, cd_typedef):
cd cargo_tests/cd_containers/csharp
export PATH="$(rustc +nightly-2026-06-17 --print sysroot)/bin:$PATH"
rm -rf bin obj ../rustlib/target
CARGO_DOTNET_BACKEND=native dotnet run -c Release            # prints "<name>: N/N checks passed"
```

**Docker `::stable` gate (ONLY needed if you touched backend code — proves no codegen regression):**
```bash
./feasibility/dev.sh gate     # baseline 426 pass / 14 fail; success line: "no real regressions"
```

**Durable gotchas (each cost real time this session):**
- **STALE ARTIFACT:** `cargo dotnet` can reuse a stale `mycorrhiza` build when you changed only a *string
  literal* (e.g. an assembly name) — the run shows the OLD behavior. Fix: `rm -rf target` (or the C#
  crate's `bin obj ../rustlib/target`) to force a clean recompile. If a native run's output looks
  identical to before your change, this is why.
- **RCL_ICE_LOG=1** makes the backend mirror swallowed codegen panics to `/tmp/rcl_ice.txt` (rustc +
  `cargo dotnet` otherwise hide them behind "the compiler unexpectedly panicked"). Always set it while
  iterating; `cat /tmp/rcl_ice.txt` after a failed build.
- **Assembly names:** `List`/`Dictionary`/`HashSet` are in `System.Private.CoreLib`; **`Stack`/`Queue`
  (and most non-core collections) are in `System.Collections`** — a wrong assembly → runtime
  `TypeLoadException: Could not load type …`. `mycorrhiza` is the *impl* assembly rule (a ref assembly
  forwards and throws at JIT).
- **`mycorrhiza` is a *target* crate** (compiled BY the backend for `os=dotnet`). `cargo check -p
  mycorrhiza` on the host mostly type-checks, but the authoritative check is the native interop test.
- **rust-src `thread_local/mod.rs`** may have a duplicated `target_os = "dotnet"` `cfg_select!` arm (a
  stale `Storage`/`value_align` arm shadowing the correct `EagerStorage`/`LazyStorage` one) that breaks
  ALL native std builds (`E0432 unresolved imports dotnet::Storage`). If a fresh checkout/rustup hits
  that, delete the stale first arm. Path:
  `$(rustc +nightly-2026-06-17 --print sysroot)/lib/rustlib/src/rust/library/std/src/sys/thread_local/mod.rs`.
- **proc-macros work in the build-std flow** (they're host-compiled). The `dotnet_macros` crate
  (`proc-macro = true`) is the workspace's only one.

---

## 3. The surface as it stands (files to know)

| Piece | File(s) | What it is |
|---|---|---|
| **Generic collections** | `mycorrhiza/src/collections.rs` | `List`/`Dictionary`/`HashSet`/`Stack`/`Queue`. Per-collection submodule: `dotnet_generic!` alias + `dotnet_generic_impl!` `raw_*` free fns + a move-only `pub struct` delegating to them. **This is the file you extend for Theme-1 collection work.** |
| **Generic bridge (the machinery)** | `mycorrhiza/src/generic_bridge.rs` | `gen!(N)` (→ `!N` marker), `dotnet_generic!` (handle alias), `dotnet_generic_impl!` (arity-muncher emitting `rustc_clr_interop_generic_*` calls). **Add a new method-arity arm here** (it currently covers ctor + 0/1/2 value-arg × void/ret; a 0-arg-void arm was added for `Clear()`). |
| **Raw generic intrinsics** | `mycorrhiza/src/intrinsics.rs` | `RustcCLRInteropManagedGeneric` (the handle; **unconditionally `Copy`**), the `RustcCLRInteropTypeGeneric<N>`/`MethodGeneric<N>` markers, the `rustc_clr_interop_generic_*` magic fns. Backend recognizes these in `src/type/mod.rs` + handles them in `src/terminator/call.rs` (`call_generic`/`ctor_generic` + `check_generic_marker` binding-consistency guard). |
| **Reusable C#→Rust container (Rust)** | `mycorrhiza/src/containers.rs` | `export_rust_containers!()` — emits the size-erased `rcl_vec_*` `#[no_mangle]` core into the invoking cdylib. **Pattern to copy for a new exported container** (e.g. `export_rust_hashmap!`). |
| **Reusable container (C#)** | `msbuild/RustDotnet.Containers.cs` | `RustDotnet.RustVec<T>`/`RustBoxVec<T>`. Auto-included by `RustDotnet.targets` when `<UseRustDotnetContainers>true`. Install copy: `feasibility/cargo-dotnet` (~L330). |
| **`#[dotnet_class]`** | `dotnet_macros/src/lib.rs` (proc-macro) + `mycorrhiza/src/comptime.rs` (intrinsics) + `src/comptime.rs` (backend interpreter) | Rust struct → managed class. Extend here for virtual methods / managed fields / properties (Theme-4). |
| **`#[dotnet_export]`** | `dotnet_macros/src/lib.rs` (proc-macro; `dotnet_export` fn + `marshal_param`/`marshal_return`) | Rust fn → C#-callable `MainModule.method`. Marshals `&str`/`String` (via the `MString` managed seam) + primitives; hidden `#[no_mangle] extern "C"` shim per fn. **Add a supported type by extending `marshal_param`/`marshal_return`** (each returns a `Marshal{seam_ty,to_rust,from_rust}` or an `Err(msg)` compile error). NO backend/C# change. Runnable: `cargo_tests/cd_export`. |
| **BCL bindings** | `mycorrhiza/src/bindings.rs` (generated by `cargo_tests/spinacz`) + `mycorrhiza/src/system/` | ~4256 low-level method wrappers. Idiomatic wrappers (Theme-2) wrap these. |
| **Idiomatic BCL wrappers** | `mycorrhiza/src/bcl/` (`datetime`/`timespan`/`guid`/`uri`/`regex`/`random`/`stopwatch`/`stringbuilder`/`environment`/`mathf`/`json`) | Theme-2 hand-written idiomatic modules over the raw bindings + the `System.Text.Json` bridge. Value-type instance calls use the `vt_*` helper in `intrinsics.rs`. Proof: `cd_bcl` 313/313, `cd_json` 47/47. |
| **Enumerator bridge** | `mycorrhiza/src/enumerate.rs` | `Enumerator<T>` (wraps `IEnumerator<T>` as `impl Iterator`) + `Enumerable::iter_enumerator()`; backs `IntoIterator for &collection`. Proof: `cd_enumerate` 22/22. |
| **Error/text ergonomics** | `mycorrhiza/src/error.rs` + `DotNetString` (`system/`) | `Nullable`/`from_nullable` (`null→Option`), `try_managed`/`.try_()` (`throw→Result<T,ManagedException>`). Proof: `cd_idiomatic` 45/45. |
| **Task/async bridge** | `mycorrhiza/src/task.rs` | `await_task` (poll `IsCompleted`/read `Result`), `future_to_task` (drive a Rust `Future` into a `TaskCompletionSource<T>`). Proof: `cd_async` 7/7. |
| **Delegates** | `mycorrhiza/src/delegate.rs` + `src/terminator/call.rs` (`rustc_clr_interop_delegate`) | Rust `extern "C" fn` → managed `Action`/`Func`/`Comparison`; `.NET → Rust` via `callvirt Delegate::Invoke`. Proof: `cd_delegates` 14/14. |
| **`cargo dotnet` tooling** | `tools/cargo-dotnet/src/` (`cli.rs`, `pack.rs`, …) | Subcommands: `build`/`run`/`new`/`doctor`/`test`/`setup`/`pack`. `new` scaffolds `--app`/`--lib`/`--plugin`; `pack` emits a `.nupkg`. |
| **Example crates (your test + copy templates)** | `cargo_tests/cd_*` — see the pass-count table below | Each is a runnable proof. **Add a new `cd_*` per new capability.** |

**Shipped `cd_*` proofs (all verified green natively — `CARGO_DOTNET_BACKEND=native`):**

| Crate | Kind | Exercises | Verified |
|---|---|---|---|
| `cd_collections` | Rust→.NET | all collections + conveniences + honest std traits | **128/128** |
| `cd_enumerate` | Rust→.NET | `for x in &collection` over the enumerator bridge | **22/22** |
| `cd_bcl` | Rust→.NET | the 10 idiomatic BCL wrappers | **313/313** |
| `cd_json` | Rust→.NET | `System.Text.Json` parse/navigate/serialize | **47/47** |
| `cd_idiomatic` | Rust→.NET | `null→Option`, `throw→Result`, `DotNetString` | **45/45** |
| `cd_async` | Rust→.NET | `.await` a `Task<T>`, `async fn`→`Task` | **7/7** |
| `cd_delegates` | Rust→.NET | `Action`/`Func`/`Comparison` invoked by .NET | **14/14** |
| `cd_generic` | Rust→.NET | the low-level WF-9 generic bridge | (baseline) |
| `cd_typedef` | C#→Rust | `#[dotnet_class]` (static/ctors/fields) | **16/16** |
| `cd_export` | C#→Rust | `#[dotnet_export]` auto-marshal | **11/11** |
| `cd_containers` | C#→Rust | `RustVec<T>`/`RustBoxVec<T>` | **13/13** |
| `cd_containers2` | C#→Rust | `RustHashMap<K,V>` + `RustString` | **30/30** |

**In-Rust test convention** (see `cd_collections/src/main.rs`): a `chk!(got, want)` macro tallies
`pass`/`total`, prints a `9000000xx` marker on failure, prints `pass`/`total`, returns `ExitCode`.
C# side (`cd_containers/csharp/Program.cs`): a `Check(name, got, want)` helper prints `N/N checks passed`.

---

## 4. Patterns to copy (recipes)

- **Add a method to an existing collection** (e.g. `List::first`): add a `raw_*` line to that submodule's
  `dotnet_generic_impl!` in `collections.rs` (name the .NET member + `gen!(0)` for `!0` positions), then a
  thin `pub fn` on the struct delegating to it. If the .NET method's arity/void-ness isn't covered by a
  `dotnet_generic_impl!` arm, add the arm to `generic_bridge.rs` first.
- **Add a whole collection type** (e.g. `SortedSet`): copy a submodule in `collections.rs`; set the
  correct **impl assembly** (CoreLib vs `System.Collections` — verify at runtime, the TypeLoadException
  tells you); re-export the `pub struct`.
- **Add a std-trait impl** (`Display`/`Eq`/…): impl on the wrapper struct, calling the appropriate .NET
  method (`ToString`/`Equals`/`GetHashCode`/`CompareTo`) via the bindings or a `dotnet_generic_impl!` line.
- **Add a reusable exported container for C#**: a new `export_*!` macro in `containers.rs` (emit
  `#[no_mangle]` fns into the user crate) + a C# wrapper in `msbuild/RustDotnet.Containers.cs` (or a new
  `.cs`, wired into `RustDotnet.targets` + the setup copy in `feasibility/cargo-dotnet`).
- **Add an example crate**: `cargo_tests/cd_<name>/` with `Cargo.toml` (`mycorrhiza = { path = "../../mycorrhiza" }`,
  bare `[workspace]`, no `panic="abort"` — the native build-std has no `panic_abort`), `src/main.rs` using
  the `chk!` convention, and a `.gitignore` (`/target`, `Cargo.lock`, `/.cargo/config.toml`). For a
  C#-consumes-Rust crate, mirror `cd_containers` (rustlib + csharp + the 3-way `RustDotnet.targets` import).

---

## 5. Where to start (the campaign is essentially done — this is the remaining tail)

The quick wins, the enumerator bridge, `cargo dotnet new`, `#[dotnet_export]`, delegates (core),
Task/async, the BCL wrappers, JSON, the extra collections, and the cookbook **all shipped and are
verified** (see §1 + [ERGONOMICS_STATUS.md](ERGONOMICS_STATUS.md)). What remains, in leverage order:

1. **The one backend unlock — WF-9 generic value-type instance methods** (`src/terminator/call.rs`, the
   `!is_valuetype` assert for KIND=1). This single change is a *backend* change (rebuild + install +
   `./feasibility/dev.sh gate` required — see §2) and it unblocks THREE roadmap items at once:
   **Dictionary iteration** (`for (k,v) in &dict` over `KeyValuePair<K,V>`), **`Span<T>`/`Memory<T>`**,
   and the valuetype **`Nullable<T>`** wrapper. Pass the by-value valuetype receiver by managed-pointer
   (address-of) for `call instance`. Do NOT weaken `cilly/src/ir/typecheck.rs`. Highest leverage left.
2. **Delegate tail (mycorrhiza + `src/`):** closure *captures* (boxed-env trampoline), delegate as a
   **generic-method** argument (`List<T>.Sort(Comparison<T>)` — needs a nested-`!N`-binding typecheck
   *extension*, sound, NOT a relaxation), .NET **events** (`add_*`). Unblocks LINQ predicate adapters.
3. **Pure-library breadth (mycorrhiza-only, no backend rebuild):** LINQ-style adapters (`.where_`/
   `.select`/`.to_list`) now that the enumerator bridge + delegates exist; enum interop; `IEnumerable<T>`
   over a `RustVec`; C#→Rust delegates; virtual methods / interface impl for `#[dotnet_class]`.
4. **Tooling/docs polish:** `cargo dotnet publish --aot` as a first-class command (AOT is codegen-proven);
   hosted rustdoc + C# XML docs; a flagship end-to-end example app.

For any of the above: pick the item in [ERGONOMICS_ROADMAP.md](ERGONOMICS_ROADMAP.md) (status-marked),
copy the nearest `cd_*` proof (§3 table) as the verification harness, follow the §4 recipe, and obey the
§2 build/verify loop. Genuine **walls** (won't-do) are at the bottom of the roadmap.

## 6. "Done" checklist for any ergonomics change

- [ ] The relevant `cd_*` example exercises the new surface and prints all-green natively.
- [ ] `cd_collections` (128/128) and, if C#-side touched, `cd_containers` still green (no regression).
- [ ] If (and only if) backend code changed: `./feasibility/dev.sh gate` = "no real regressions" (426/14).
- [ ] Committed locally on `gaps-campaign` with the `Co-Authored-By` line. **Not pushed.**
- [ ] Memory + this doc / the roadmap updated if the surface or the plan moved.
