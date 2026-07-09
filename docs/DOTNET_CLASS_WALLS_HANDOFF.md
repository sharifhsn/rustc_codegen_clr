# `#[dotnet_class]` walls — session hand-off / cold-start guide

Everything a fresh agent needs to **continue closing `#[dotnet_class]`/`#[dotnet_methods]` architectural
walls** without the accumulated context: what's done and verified, the exact build/verify footguns that
cost real time this session, the remaining walls with concrete (code-grounded, not guessed) diagnoses,
and the two architecture fixes worth doing before adding more surface.

Origin: the user asked to "loop until all real walls are closed or at least concretely identified why
it's not possible" on `#[dotnet_class]`'s ergonomics, plus a request for an architecture perspective on
whether the mechanism itself should change shape. This doc is the answer to both, plus a punch list.

---

## 1. Current state

- **Branch:** `main`. Commits are local-only — this repo's `origin` may be FractalFir's upstream; check
  before pushing, never push without the user asking explicitly.
- **4 of 10 identified walls are CLOSED, verified end-to-end, and committed:**
  - `ddc1e6d8` — **Wall 1 (static fields)** + **Wall 2 (operator overloads)** + **Wall 3 (async fn
    rejection, was already done, just verified)**.
  - `8682cbec` — **Wall 4 (base ctors that take arguments)**.
- **6 walls remain OPEN**, each concretely diagnosed below (§3) — not implemented this session.
- Proof crate: [`cargo_tests/cd_class_ergonomics/`](../cargo_tests/cd_class_ergonomics/) — 11/11 C#
  checks passing, covering all four closed walls. Structure:
  - `rustlib/` — the Rust crate exercising all four capabilities.
  - `csharp/` — the C# consumer (`dotnet run -c Release` inside it to re-verify).
  - `basecs/` — a tiny hand-written C# class library (`Widget(int seed)`, no parameterless ctor) that
    exists SPECIFICALLY to prove Wall 4 against a base with a non-trivial constructor. Build it first
    (`cd basecs && dotnet build -c Release`) before building `rustlib`/`csharp` from clean.

---

## 2. How to build & verify (READ THIS — the footguns cost real time)

**Toolchain:** pinned `nightly-2026-06-17` via `rust-toolchain.toml`. `cargo --version` inside the repo
should print `1.98.0-nightly (... 2026-06-11)`. If it doesn't, something is shadowing the rustup shim.

**The installed backend is NOT your local `target/release` build.** `cargo dotnet build`'s default
"native" backend reads `~/.cargo-dotnet/bin/{librustc_codegen_clr.dylib,linker}`. To test ANY backend
change (anything touching `src/`, `cilly/`, or `mycorrhiza/src/comptime.rs`'s intrinsics), you must:

```bash
cargo build --release                       # backend dylib
cargo build --release -p cilly --bins        # linker binary
cp target/release/librustc_codegen_clr.dylib ~/.cargo-dotnet/bin/librustc_codegen_clr.dylib
cp target/release/linker ~/.cargo-dotnet/bin/linker
```

Then rebuild the target crate with `--clean` at least once to rule out stale `.bc` (see the build-std
fingerprint trap in memory / `docs/GAPS.md`).

**`ALLOW_UNVERIFIED_BASE=1`**: `#[dotnet_class(extends = "...")]` only accepts bases on a small
proven-safe allowlist (`dotnet_macros/src/lib.rs::EXTENDS_ALLOWLIST` — currently just `System.Object`
and `BackgroundService`). To subclass anything else (like the Wall-4 test's `Widget`), set this env var
at RUST COMPILE TIME (i.e. when `cargo dotnet build` invokes rustc on the Rust crate, not the C# build).

**Manually exporting `RUSTFLAGS` breaks any crate depending on `dotnet_macros`.** `dotnet_macros`
depends on `syn`/`quote`/`proc-macro2`, which have `build.rs` scripts; a blanket `RUSTFLAGS` export
wrongly applies the custom .NET backend to HOST-side build-script compilation too, crashing with
`missing methiod ... getenv`. Always use `cargo dotnet build <path>` (unset RUSTFLAGS) for any crate
using `dotnet_macros` — never hand-export RUSTFLAGS for these.

**`cargo test ::stable` currently shows ~244 failures — this is PRE-EXISTING, not something to chase.**
Root cause, confirmed via a strict A/B `git stash` comparison (identical failures with and without this
session's changes): every `cargo_tests/*` directory carries a stale, gitignored `.cargo/config.toml`
(left over from earlier `cargo-dotnet` sessions) that forces a JSON custom target spec
(`~/.cargo-dotnet/target/x86_64-unknown-dotnet.json`) which a newer nightly cargo now rejects with
`` `.json` target specs require -Zjson-target-spec ``. This only affects the `compile_test.rs` harness's
own `cargo build`/`std`-dependent test variants (names containing `cargo_release`/`cargo_debug`, or
`run_test!{std, ...}` entries like `once_lock_test`/`uninit_fill`) — NOT the direct-rustc-invoked bulk of
the suite. Sanity-checked: 106/125 non-fuzz, non-cargo, non-build-std tests pass with zero new failure
categories. **Do not "fix" this by deleting the stale configs repo-wide without understanding whether
some other in-flight workflow depends on them** — flag it to the user first; it's a `cargo-dotnet`
tooling-generation conflict, not a code bug.

---

## 3. The four closed walls — exact surface

All on `#[dotnet_class]` (from `dotnet_macros`), verified in `cargo_tests/cd_class_ergonomics/rustlib/src/lib.rs`:

```rust
// Wall 1: real .NET static field + synthesized get_/set_ accessors Rust can call.
#[dotnet_class(static_field(Count: i32))]
pub struct Counter {}
// Rust reads/writes via: CounterHandle::static0::<"get_Count", i32>() / static1::<"set_Count", i32, ()>(v)
// C# reads/writes via: Counter.Count directly.

// Wall 2: any #[dotnet_methods] method named a CLR operator (op_Addition, op_Equality, op_Inequality,
// op_UnaryNegation, ... — full list in both src/comptime.rs::CLR_OPERATOR_METHOD_NAMES and
// dotnet_macros' copy of the same const) is now forced static + stamped SpecialName automatically,
// so C# binds real `+`/`==`/`!=` syntax, not just X.op_Addition(a, b).

// Wall 3: `#[dotnet_methods] impl Foo { pub async fn bar() {...} }` is a clean compile error, not a
// silent miscompile — the coroutine state machine has no faithful .NET lowering.

// Wall 4: base classes without a parameterless .ctor.
#[dotnet_class(extends = "[Asm]Ns.Base", base_ctor_args(i32, String))]
pub struct Derived { own_field: i32 }
// The primary/default ctor becomes .ctor(base_arg0, base_arg1, ..., own_field0, ...) — base args are
// ALWAYS leading params, forwarded verbatim into base::.ctor(...). No way to compute them dynamically
// from Rust — see the intrinsic's doc in mycorrhiza/src/comptime.rs for why (comptime class-shape
// intrinsics describe static metadata only, not an interpretable expression body).
```

---

## 4. The six open walls — concrete diagnosis (verified against code, not guessed)

Each entry names the exact file/mechanism checked so the next agent doesn't have to re-derive it.

**Wall 5 — value-type `#[dotnet_class]` ergonomics.** `src/comptime.rs::finish_type`'s ENTIRE
ctor-emission block is gated `if !class.is_value_type { ... }` — value types get **zero** synthesized
ctor path today (no primary ctor, no default ctor, no field accessors via that path). Real, structural
gap, not yet scoped in detail. Start here: read how value types ARE constructed today (probably via a
raw `initobj`/default-value pattern elsewhere) before designing the ctor surface.

**Wall 6 — sealed/abstract class modifiers.** `cilly::Access` (`cilly/src/lib.rs:22`) is
`{Extern, Public, Private}` — a pure visibility enum, no modifier bits. `MethodDef` already has
`is_abstract` (method-level, for interface members) but `ClassDef` has nothing for "this whole class is
abstract/sealed" at the TYPE level. Needs: a new `ClassDef` field + a `TypeAttributes` flag write in the
PE exporter (`cilly/src/ir/pe_exporter/export.rs`) + macro surface (`sealed = true`/`abstract = true` on
`#[dotnet_class]`). Moderate, well-scoped, same shape as Wall 1. **If you add this field, remember §5's
`asm_link.rs` landmine — the class-level equivalent of `translate_method_def` for `ClassDef` must also be
checked for the same field-loss risk.**

**Wall 7 — generic `#[dotnet_class]` classes.** `PendingClass.type_generics` is ALREADY tracked and
threaded through for `#[dotnet_interface]` (generic interfaces work — see WF-9 unlock in memory). The
doc comment on that field says generic CLASS definitions are asserted-rejected in `finish_type`, citing
"no explicit layout on .NET generics." BUT: this comptime-emitted path uses `None` for every field
offset already (unlike ordinary monomorphized Rust ADTs, which DO use explicit `[FieldOffset]`) — so the
wall's true scope may be narrower than the assert suggests. This needs a real, dedicated investigation
session (read `cilly/src/ir/class.rs`'s generic-rejection assert, trace why it exists, try relaxing it
for the comptime-only, no-explicit-layout path specifically) before attempting an implementation. Bigger
and riskier than Walls 1-4/6/8/10 — don't attempt as a quick pass.

**Wall 8 — indexers (`obj[i]` syntax).** `.NET` indexers are a `Property` named `Item` with index
parameters — no param-list support exists in the current property-accessor path
(`has_properties`/`rustc_codegen_clr_add_field_properties`), which only handles zero-arg get/set per
field. Feasible: extend that mechanism (or add a parallel `add_indexer_def` intrinsic) to accept a param
list, mirroring how `finish_type` already builds get/set method pairs for ordinary properties. Same
difficulty class as Wall 1.

**Wall 9 — nested types.** Grepped `cilly/src/ir/class.rs` for any enclosing/nested-type concept —
nothing exists. `ClassDef` has zero support for a parent/nested relationship. Real architectural gap:
needs a PE `NestedClass` metadata table write (ECMA-335 §II.22.32) in the exporter, plus deciding how a
nested Rust type maps to a nested `#[dotnet_class]` declaration syntactically. Moderate-to-large — the
metadata-table work is the unknown-size piece, worth a scoping pass before committing to it.

**Wall 10 — explicit interface implementation.** `PendingClass.interfaces` is satisfied PURELY by
implicit name+signature binding today (see that field's doc in `src/comptime.rs`). Explicit
implementation (`void IFoo.Method()` — needed when a class implements two interfaces with colliding
member names, or wants a private implementation) needs a `MethodImpl` table entry, which is EXACTLY the
mechanism `rustc_codegen_clr_mark_last_method_override`/`method_overrides` already implements and proves
working for base-class overrides (`cd_bgservice`'s `BackgroundService.ExecuteAsync` override). This is
the most tractable of the six remaining — it's mostly "generalize an existing, proven mechanism to a
second target (interface method, not base-class method)," not new machinery.

**Suggested order if picking this back up:** 10 (reuses proven machinery) → 6 → 8 (same shape as Wall 1)
→ 9 → 5 → 7 (biggest, do last, needs its own investigation phase first).

---

## 5. Architecture perspective — two fixes worth doing before adding more `MethodDef`/`ClassDef` fields

The user asked directly: "if there's a cleaner architecture possible for how `dotnet_class` works, I'm
interested in seeing your perspective." Answer, grounded in what actually broke this session:

**The core mechanism should NOT change.** `#[dotnet_class]`/`#[dotnet_methods]` encode class shape as a
sequence of fake generic-function calls (all bodies `diverge!()`, never executed) whose *monomorphized
types* the backend's `src/comptime.rs` interpreter reads directly off real MIR by walking the synthetic
`rustc_codegen_clr_comptime_entrypoint` function. This is genuinely clever: it gets real, fully-resolved
`Ty<'tcx>` type information for free from rustc's own instance-resolution machinery. A hand-rolled
serialized-spec alternative (proc-macro emits a `static` byte blob, backend deserializes it directly, no
MIR interpretation) would have to reimplement Rust-type-to-.NET-type lowering from scratch outside the
normal codegen path — a strictly worse trade. Don't replace this trick.

**Fix 1 — generalize `src/comptime.rs`'s dispatch off substring matching.** It still uses
`fname.contains("rustc_codegen_clr_add_static_field_def")`-style dispatch — the exact fragility class
already fixed everywhere else in the codebase (`src/utilis/mod.rs::MagicFn`/`classify_magic_fn`, an
exact-DefId enum replacing three independently hand-copied substring-matched lists, see commit
`c8a7680b` from earlier this campaign). `comptime.rs` is the one remaining place using the old style.
Low risk, mechanical, same pattern to copy.

**Fix 2 — `cilly/src/ir/asm_link.rs::translate_method_def`'s field-by-field `MethodDef` reconstruction
is a proven, REPEAT-OFFENDING landmine.** This function is the linker's cross-assembly `MethodDef`
merge step (runs every time a `.bc` crosses the linker — i.e. every real build). It rebuilds via
`MethodDef::new()` (defaults every additive field to `false`/empty) then manually re-applies ONLY the
fields someone remembered to list: `overrides`, `is_abstract`, `out_params`, `generic_params`. **This
session's Wall 2 bug was exactly this class of bug for the THIRD time** — `is_special_name` was added to
`MethodDef` but not added to this re-application list, so it silently vanished on every real (linked)
build while unit tests kept passing. The existing code even has doc comments on the neighboring
`out_params`/`generic_params` blocks warning about this exact hazard — the warning didn't stop it from
happening again.

The structural fix: since almost every field on `MethodDef` is either plain data (needs blind
re-application) or an interned handle (needs explicit retranslation anyway), switch from
"`MethodDef::new()` + growing whitelist" to an **exhaustive destructure with no `..` catch-all**:
```rust
let MethodDef { name, sig, overrides, is_abstract, out_params, generic_params, is_special_name, /* every field, no .. */ } = def;
```
This turns "a new field was added to `MethodDef` but not threaded through the linker" from a **silent
runtime bug** into a **compile error** (the destructure won't compile until the new field is explicitly
handled). This is the single highest-value structural fix available before Wall 6 (which needs a new
`ClassDef`-level field with the identical class of risk) or any future `MethodDef` field is added.
Small diff, no behavior change for existing fields, pure safety-net.

---

## 6. Where to start

1. Read this doc fully before touching code — it front-loads every footgun that cost time this session.
2. If continuing wall-closing: apply Fix 1 and Fix 2 above FIRST (small, mechanical, de-risks everything
   downstream), then work Wall 10 → 6 → 8 → 9 → 5 → 7 in that order.
3. Every wall needs the same rhythm that worked for 1-4: implement (macro + intrinsic + comptime.rs
   dispatch + `finish_type` wiring) → rebuild backend+linker → reinstall to `~/.cargo-dotnet/bin/` →
   extend `cargo_tests/cd_class_ergonomics` with a real C# consumer check → run it → only then commit.
   Don't trust a `cargo check` pass alone — this session's Wall 2 bug was invisible to `cargo check` and
   only surfaced via a C# reflection probe on the actually-linked output.
