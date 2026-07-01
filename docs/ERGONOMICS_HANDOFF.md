# Ergonomics work — session hand-off / cold-start guide

Everything a fresh session needs to **continue the interop-ergonomics work** without the accumulated
context: current state, how to build & verify (the footguns that cost hours), the surface as it
stands, the exact patterns to copy, and where to start. The *what-to-build* backlog is
[ERGONOMICS_ROADMAP.md](ERGONOMICS_ROADMAP.md); this is the *how-to-operate + start-here*.

---

## 1. Current state

- **Branch:** `gaps-campaign`. **~66 commits ahead of `mine/gaps-campaign`; NOTHING is pushed.**
  Commit locally; **never push** (`origin` = FractalFir's upstream; push to `mine` only when the owner
  explicitly asks). End every commit message with
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- **The interop/ergonomics arc that just shipped** (newest first):
  - `5cc9496` docs: ERGONOMICS_ROADMAP.md (the full backlog).
  - `48f5967` docs: QUICKSTART_INTEROP.md.
  - `d1b7042` **reusable C#→Rust container** — `mycorrhiza::export_rust_containers!()` + shipped
    `RustDotnet.RustVec<T>`/`RustBoxVec<T>` (msbuild/RustDotnet.Containers.cs). Proof: `cd_containers` 13/13.
  - `9262668` **`mycorrhiza::collections`** — `List`/`Dictionary`/`HashSet`/`Stack`/`Queue`, used like std.
    Proof: `cd_collections` 38/38.
  - `8c28ce8` **`#[dotnet_class]`** proc-macro (crate `dotnet_macros`) — Rust struct → managed class + ctor.
    Proof: `cd_typedef` 4/4.
  - (earlier) WF-9 generic bridge (`22aae7d`/`e2b493e`/`f73bca7`/`dcb8481`) + the 10-commit macro-refactor
    campaign (`c86e699`..`a3739b2`).
- **What works today** (the baseline to build on): both interop directions are functional AND ergonomic —
  `.NET-from-Rust` via `mycorrhiza::collections`; `Rust-from-C#` via `export_rust_containers!` + shipped
  `RustVec<T>`; `#[dotnet_class]` for managed types. See [QUICKSTART_INTEROP.md](QUICKSTART_INTEROP.md).

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
| **Raw generic intrinsics** | `mycorrhiza/src/intrinsics.rs` | `RustcCLRInteropManagedGeneric` (the handle; **unconditionally `Copy`**), the `RustcCLRInteropTypeGeneric<N>`/`MethodGeneric<N>` markers, the `rustc_clr_interop_generic_*` magic fns. Backend recognizes these in `rustc_codegen_clr_type/src/type.rs` + handles them in `src/terminator/call.rs` (`call_generic`/`ctor_generic` + `check_generic_marker` binding-consistency guard). |
| **Reusable C#→Rust container (Rust)** | `mycorrhiza/src/containers.rs` | `export_rust_containers!()` — emits the size-erased `rcl_vec_*` `#[no_mangle]` core into the invoking cdylib. **Pattern to copy for a new exported container** (e.g. `export_rust_hashmap!`). |
| **Reusable container (C#)** | `msbuild/RustDotnet.Containers.cs` | `RustDotnet.RustVec<T>`/`RustBoxVec<T>`. Auto-included by `RustDotnet.targets` when `<UseRustDotnetContainers>true`. Install copy: `feasibility/cargo-dotnet` (~L330). |
| **`#[dotnet_class]`** | `dotnet_macros/src/lib.rs` (proc-macro) + `mycorrhiza/src/comptime.rs` (intrinsics) + `src/comptime.rs` (backend interpreter) | Rust struct → managed class. Extend here for virtual methods / managed fields / properties (Theme-4). |
| **BCL bindings** | `mycorrhiza/src/bindings.rs` (generated by `cargo_tests/spinacz`) + `mycorrhiza/src/system/` | ~4256 low-level method wrappers. Idiomatic wrappers (Theme-2) wrap these. |
| **Example crates (your test + copy templates)** | `cargo_tests/{cd_collections,cd_containers,cd_generic,cd_typedef,cd_rustvec,cd_interop,interop_method_sample}` | Each is a runnable proof. `cd_collections` = the high-level collections; `cd_generic` = the low-level bridge; `cd_containers`/`cd_rustvec` = C#→Rust; `cd_typedef` = `#[dotnet_class]`. **Add a new `cd_*` per new capability.** |

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

## 5. Where to start (from ERGONOMICS_ROADMAP.md)

Recommended sequence, with the concrete first move for each:

1. **Quick wins (~a day, mycorrhiza-only, no backend rebuild):**
   - `mycorrhiza::prelude` (new `prelude.rs` re-exporting `collections::*`, the macros, common wrappers).
   - Collection conveniences in `collections.rs`: `List::first/last/pop/insert/sort/clear`, `Index` via
     `ops::Index`, `Dictionary::get_or_default`.
   - std traits: `Display`/`Debug` (via `ToString`), `PartialEq`/`Eq` (via `Equals`), `Hash`
     (`GetHashCode`) for the collection + common wrappers.
   - `Vec`↔`List` (`From<Vec<T>>`, `to_vec`).
   - **Verify:** extend `cd_collections` with the new methods → run it → expect all green.
2. **Enumerator bridge ⚑** (`mycorrhiza/src/`): wrap `IEnumerator<T>` (`GetEnumerator`→handle,
   `MoveNext`→bool, `get_Current`→`!0`) as a generic `impl Iterator`. Then `Dictionary`/`HashSet`/`Stack`
   iteration + `IntoIterator for &List`. New `cd_enumerate` example.
3. **`cargo dotnet new` ⚑** (`tools/cargo-dotnet/src/`): a `new` subcommand scaffolding a `--lib`/`--app`
   template from the example crates.
4. **`#[dotnet_export]` auto-marshal ⚑** (`dotnet_macros` + a few `src/` marshalling helpers): the
   Rust-from-C# counterpart to the container macro.
5. **Delegates & callbacks ⚑** (hard; `src/` + a mycorrhiza wrapper): `Func`/`Action` ↔ Rust closures via
   the `calli`/`ldftn` path. Then **async/Task ⚑**.

Full item list, effort/payoff, deps, and the genuine **walls** (won't-do) are in
[ERGONOMICS_ROADMAP.md](ERGONOMICS_ROADMAP.md).

## 6. "Done" checklist for any ergonomics change

- [ ] The relevant `cd_*` example exercises the new surface and prints all-green natively.
- [ ] `cd_collections` (18/18→) and, if C#-side touched, `cd_containers` still green (no regression).
- [ ] If (and only if) backend code changed: `./feasibility/dev.sh gate` = "no real regressions" (426/14).
- [ ] Committed locally on `gaps-campaign` with the `Co-Authored-By` line. **Not pushed.**
- [ ] Memory + this doc / the roadmap updated if the surface or the plan moved.
