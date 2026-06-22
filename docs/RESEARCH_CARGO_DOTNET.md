# Research: `getrandom` auto-wiring & `cargo dotnet` subcommand citizenship

> Status: research findings (read-only investigation + web research). No code was changed
> by this document. Two questions, asked by the owner:
>
> **Q1.** Why does `getrandom` not "just work" on the dotnet target the way `mio`/`socket2`/`tokio`
> do (which the `dotnet_overlays` registry patches automatically), and can the wiring be auto-fixed?
>
> **Q2.** Does `feasibility/cargo-dotnet` follow cargo custom-subcommand best practices? An honest audit.
>
> Sources are cited inline and collected at the end. Local facts were verified against the
> tree at the commit this was written on; the one empirically reproduced bug (the `cargo dotnet`
> argument convention, §2.4 deviation D0) is shown with its repro.

---

## 1. Q1 — Why `getrandom` does not auto-work, and the recommended auto-fix

### 1.1 The symptom

Our custom target spec (`x86_64-unknown-dotnet.json`) advertises `"os": "dotnet"`. `getrandom`
selects its randomness backend at **compile time** from a hardcoded, per-target allow-list
(roughly "the platforms Rust's std supports"). A target it does not recognize does **not** fall
back to a generic source — it emits a **front-end `compile_error!`** before codegen, so the build
dies with exit 101. Because `rand`, `uuid`, and `ahash` pull `getrandom` transitively, all of them
fail on the PAL unless a backend is selected manually. (This is a `getrandom`-the-crate problem
only — std's own randomness, `sys::random → rcl_dotnet_random_fill`, is unaffected.)
[getrandom docs; `getrandom_dotnet/src/lib.rs` docstring]

The escape hatch is `getrandom`'s official **custom backend**, selected per major version:

| getrandom major | Selector | Symbol the application must provide | Where it must live |
|---|---|---|---|
| 0.3 / 0.4 (identical) | `--cfg getrandom_backend="custom"` (a RUSTFLAGS / `.cargo/config.toml` *cfg*, **not** a Cargo feature) | `#[no_mangle] unsafe extern "Rust" fn __getrandom_v03_custom(dest: *mut u8, len: usize) -> Result<(), getrandom::Error>` | "MUST be defined only once for your project … ideally … in the root crate, e.g. `main.rs`"; "upstream library crates SHOULD NOT define it outside of tests and benchmarks." |
| 0.2 | Cargo **feature** `custom` (the `getrandom_backend` cfg is a 0.3+ invention 0.2 ignores) + the `register_custom_getrandom!(fn)` macro; registered fn is `fn(&mut [u8]) -> Result<(), getrandom::Error>` | macro expands to `__getrandom_custom` | "Functions can only be registered in the root binary crate. Attempting to register a function in a non-root crate will result in a linker error" — explicitly likened to `#[panic_handler]` / `#[global_allocator]`. |

The 0.2 symbol (`__getrandom_custom`) and the 0.3/0.4 symbol (`__getrandom_v03_custom`) are
**distinct**, so a graph pulling several majors can register all of them without clashing.
[getrandom docs.rs latest; getrandom 0.2 `register_custom_getrandom!` macro page — both quoted in Sources]

The repo already does half the job: `_cargo_dotnet_core.sh:640` sets
`--cfg getrandom_backend="custom"` unconditionally in `RUSTFLAGS` (harmless for crates that don't
use getrandom — the cfg is simply unused). What's missing is the **symbol**.

### 1.2 The structural reason it differs from `mio` / `socket2` / `tokio`

`mio`/`socket2`/`tokio` fail on `os="dotnet"` because their `cfg`-gated platform modules have no
dotnet arm — but that is a **self-contained, compile-time decision inside the crate's own source**.
The framework fixes them by **patching the crate**: `dotnet_overlays/{mio,socket2,tokio}` are
byte-identical upstream copies plus a few `// DOTNET PAL`-marked arms, redirected in via a generated
`.cargo/config.toml` `paths = [...]` override (`_cargo_dotnet_core.sh` `apply_overlays`). The patch
is invisible to the user's `Cargo.toml`, and `paths` is graph-wide by crate name, so a transitive
dep (mio under tokio) is covered. **Patch the dependency, done.**

`getrandom` is categorically different. Its custom backend is **not** a cfg choice inside the crate
that an overlay could flip — it is an **application-provided link symbol**:

* `getrandom`'s `custom` module declares the symbol as an ordinary unresolved external
  (`unsafe extern "Rust" { fn __getrandom_v03_custom(...) -> Result<(), Error>; }`) and expects the
  *final link* to satisfy it from the root crate. So an overlay that patches `getrandom` **cannot
  supply it** — `getrandom` is the *consumer* of the symbol, not the provider, and a patched
  `getrandom` defining its own symbol would (i) violate the documented "libraries must not define it"
  rule and (ii) force every consumer's `getrandom` to carry a hard dependency on the PAL symbol.
* The symbol's signature names **`getrandom::Error`**, a **version-specific type** (different layout
  across the 0.2 / 0.3 / 0.4 majors). Only the *consuming binary* knows which major is in its graph.

This is exactly why the shipped shim `getrandom_dotnet` **deliberately does not depend on
`getrandom`** (its `Cargo.toml` comment says so) and exposes only the version-agnostic primitive
`fill(&mut [u8])` (forwarding to the PAL's `rcl_dotnet_random_fill`, which the cilly linker patches
to `System.Security.Cryptography.RandomNumberGenerator.Fill`). Each consumer then hand-wires the
version-matched symbol against its own `getrandom` — see `cargo_tests/soak_uuid/src/main.rs`, which
defines `__getrandom_v03_custom` for getrandom 0.4. **That hand-wiring is the only manual step left,
and it is the thing the auto-fix aims to remove.**

> One-line summary of the asymmetry: an overlay can patch a **cfg selection that lives inside a
> dependency**; it cannot supply an **application-provided, version-coupled link symbol that
> `getrandom` requires the root crate to define**.

### 1.3 Auto-fix options, and why each is or isn't clean

**Option 1 — Have the linker provide `__getrandom_v03_custom` as a builtin (like `rcl_dotnet_random_fill`). REJECT.**
The symbol returns `Result<(), getrandom::Error>` and is declared `extern "Rust"`, not `extern "C"`.
`getrandom::Error` is a crate-defined newtype (`Error(NonZeroU32)` in practice) with **no
`#[repr(C)]`/`#[repr(transparent)]` guarantee** and a layout that is `getrandom`'s private business
and can change across patch releases. The cilly linker patches in symbols with *known* ABIs
(libc shims, `rcl_*` hooks); it has no principled, version-stable way to construct a
`Result<(), Error>` value for an arbitrary getrandom version. (The 0.2 `__getrandom_custom` returning
a bare `u32` *is* ABI-stable and synthesizable in principle — 0 = success — but 0.2 *also* gates the
whole path behind the Cargo `custom` feature, which a linker cannot enable; see Option 4.) Forging an
`extern "Rust"` symbol with a guessed layout is a silent-miscompile risk, not a clean error. **Dead end for ≥0.3.**

**Option 2 — `cargo dotnet` generates the version-matched symbol from `Cargo.lock` and injects it. RECOMMENDED.**
This is the clean design and matches machinery the tool already has (it parses `Cargo.lock` for the
overlay version check, and it generates `.cargo/config.toml`). It also mirrors an established
ecosystem precedent: `cargo-wasi`/`wasm-pack` derive the **exact** `wasm-bindgen` version from the
project and auto-install the matching companion — i.e. "read the lockfile, supply version-matched
glue" is a known-good pattern. Mechanics:

1. After resolution, detect whether `getrandom` is in the graph and which **major(s)** (0.2 / 0.3 / 0.4).
2. For **0.3/0.4**: the cfg is already set (core line 640). The only missing piece is the symbol, and
   because it is a plain global `#[no_mangle]`, it can technically be defined by **any** crate in the
   link, not only root — the "root crate / defined once" guidance is *collision-avoidance*, not a hard
   language rule for 0.3/0.4. So generate a tiny shim crate (e.g. `__cargo_dotnet_getrandom_shim`)
   pinned to the **locked** getrandom version (so `getrandom::Error` resolves correctly), defining
   `__getrandom_v03_custom` forwarding to `getrandom_dotnet::fill`, and inject it into the build (via
   the generated config graph / an auto-added dependency edge). **Net effect: zero consumer wiring for
   the common case** (rand/uuid/ahash are all on 0.3/0.4 today).
3. For **0.2**: 0.2 needs both (i) the Cargo `custom` **feature** enabled on the getrandom in the graph
   — which can't be toggled from RUSTFLAGS/config alone; it needs a feature edit or a getrandom-0.2
   overlay that defaults `custom` on — and (ii) the `register_custom_getrandom!` macro invoked in the
   **root** crate, which a dependency genuinely cannot do (front-end "root crate only" check → linker
   error, like `#[global_allocator]`). So 0.2 cannot be made fully zero-wiring by an external crate.

**Option 3 — A dependency provides the `no_mangle` symbol generically (why doesn't `getrandom_dotnet` just do it?).**
For 0.3/0.4 a dependency *can* define `__getrandom_v03_custom` (it's a global symbol). The reason
`getrandom_dotnet` doesn't is precisely the version coupling: to write the return type it must
`use getrandom::Error`, and **there is no single getrandom version correct for all consumers** — a
crate hardcoding `getrandom = "0.4"` would emit a symbol typed against 0.4's `Error`, silently
mismatching a 0.3 consumer (same symbol name, different type). Option 2's *lock-pinned, per-build
generated* shim is exactly "a dependency provides the symbol" done **correctly** (version resolved
per build, not hardcoded). For 0.2 a dependency can define the function but not the macro invocation,
so a dep can't cover 0.2.

**Option 4 — Safety net: the `unsupported` backend for graphs that pull `getrandom` but never call it.**
`--cfg getrandom_backend="unsupported"` lets `getrandom` *compile* on `os="dotnet"` and returns
`Err(Error::UNSUPPORTED)` at runtime. This is the cleanest fix for crates that pull `getrandom`
transitively but never actually use randomness (surprisingly common). It cannot be the default (it
would make `rand`/`uuid::new_v4` fail at runtime), but `cargo dotnet` could expose it as an opt-in
(e.g. `--getrandom unsupported`).

### 1.4 Recommended auto-shim design (synthesis) + honest limits

* **Default to Option 2 for 0.3/0.4.** Detect getrandom + major from `Cargo.lock` (reuse the existing
  lock parse), generate a lock-pinned shim crate defining `__getrandom_v03_custom` → `getrandom_dotnet::fill`,
  inject it, keep the existing unconditional `--cfg getrandom_backend="custom"`. **Guard:** detect a
  user-provided symbol (e.g. `soak_uuid` already defines it) and **skip generation**, else you get a
  duplicate-symbol link error. Announce the injection in build output (it is "magic" and should be loud).
* **0.2 stays partly manual.** Either keep the documented one-liner (feature `custom` +
  `register_custom_getrandom!` in root — it's ~4 lines, already in `getrandom_dotnet/README.md`), or
  ship a **getrandom-0.2 overlay** in `dotnet_overlays` that force-enables the in-crate `custom` path
  and bakes in the PAL forwarding. 0.2 is the *only* major that is overlay-able precisely because its
  escape hatch (feature + in-crate module) lives inside the crate, unlike 0.3/0.4's app symbol.
* **Offer `--getrandom unsupported`** for never-actually-used graphs.
* **Do not** pursue the linker-builtin route for ≥0.3 (Error-type ABI is not constructible by cilly).

**Limits, stated honestly:**
1. The generated-symbol approach relies on `#[no_mangle]` being global; it must **guard against a
   user already defining it** (duplicate-symbol).
2. It only has a legal injection target when the build produces a **binary**. A pure-**library** build
   that transitively needs getrandom has no root to carry the symbol — that case must remain
   consumer-wired or be declared unsupported.
3. 0.2's root-only macro + Cargo-feature gate cannot be fully automated without owning the root
   crate or shipping a 0.2 overlay.
4. It couples `cargo dotnet` to getrandom's symbol name/signature across majors (it already bumped
   0.2→0.3) — a small version-matrix maintenance tax, of the same shape as the rustc-API bit-rot the
   project already accepts.

---

## 2. Q2 — Does `cargo dotnet` follow cargo-subcommand best practices?

### 2.1 The cargo custom-subcommand contract (The Cargo Book)

From [The Cargo Book — External Tools] (quoted verbatim):

* **Dispatch:** "translating a cargo invocation of the form `cargo (?<command>[^ ]+)` into an
  invocation of an external tool `cargo-${command}`. The external tool must be present in one of the
  user's `$PATH` directories." Cargo "defaults to prioritizing external tools in `$CARGO_HOME/bin`
  over `$PATH`."
* **Argument convention (load-bearing for the bug below):** "the first argument to the subcommand
  will be the filename of the custom subcommand … **The second argument will be the subcommand name
  itself.** … Any additional arguments on the command line will be forwarded unchanged." So for
  `cargo dotnet build X`, the external tool sees `argv[1] = "dotnet"`, `argv[2] = "build"`, `argv[3] = "X"`.
* **Help:** "`cargo help ${command}` would invoke `cargo-${command} ${command} --help`."
* **Callback / project info:** "Custom subcommands may use the `CARGO` environment variable to call
  back to Cargo." "The `cargo metadata` command can be used to obtain information about the current
  project."
* **Machine output:** `--message-format=json` emits one JSON object per line; `reason ∈
  {compiler-message, compiler-artifact, build-script-executed, build-finished}`.

### 2.2 Conventions idiomatic tools follow

Be a `cargo install`-able **Rust binary** (clap-based), not a shell script; use `cargo metadata`
for project info; **respect/forward the standard flags** (`--manifest-path`, `--release`/`--profile`,
`--features`/`--all-features`/`--no-default-features`, `--target`, `--target-dir`, `--workspace`/`-p`,
`--offline`/`--locked`/`--frozen`); honor `CARGO_*` env; relay cargo JSON diagnostics; sane exit
codes; **do not mutate the global toolchain**. The `clap-cargo` crate exists to provide the standard
flag groups (`Manifest`/`Workspace`/`Features`) so tools match cargo's surface.

### 2.3 Exemplar tools (the "unusual target / post-process artifacts" cohort), cited

| Tool | How it's built | How it handles the unusual target/SDK | What it gets right |
|---|---|---|---|
| **cross** | Rust binary, `cargo install` (binstall-able) | "the **exact same CLI as Cargo** but relies on Docker or Podman"; provides "all the ingredients … **without touching your system installation**"; host rustup + rust-src "completely untouched"; engine via `CROSS_CONTAINER_ENGINE` | same-CLI flag parity; **container isolation, no host mutation** |
| **cargo-xwin** | Rust binary, `cargo install --locked` (also pip/Docker) | downloads the MSVC CRT + Windows SDK into a **cache dir** (`XWIN_CACHE_DIR`), **not** the rustup install; "avoids mutating the rustup installation" | **cache, don't mutate the toolchain**; standard-flag passthrough |
| **cargo-zigbuild** | Rust binary, `cargo install --locked` | swaps the linker to zig only when `--target` is given; "**If you do not provide a `--target`, Zig is not used and the command effectively runs a regular `cargo build`**"; doesn't auto-mutate rustup (user runs `rustup target add`) | transparent passthrough; minimal, opt-in intervention |
| **cargo-component** | Rust binary | references wasm components via `Cargo.toml`, auto-runs the post-process adapter (core module → component); unrecognized commands pass through to cargo | the **overlay + auto-post-process** model `cargo dotnet` is built on |
| **wasm-pack / cargo-wasi** | Rust binary | build → bindgen → pack; **auto-finds/installs the version-matched `wasm-bindgen`** | the **lockfile-derived companion** precedent (= the Q1 auto-shim) |
| **cargo-nextest / cargo-hack** | Rust binary | forward `--locked`/`--offline`/`--frozen`/`--quiet` into the inner cargo; "propagates … most of the passed flags to cargo" | disciplined **flag passthrough** |
| **cargo-binstall** | Rust binary | reads `[package.metadata.binstall]` via `cargo metadata` | **config in Cargo.toml metadata**, not env |

The common thread: a real `cargo install`-able binary, cargo-CLI-shaped flag forwarding, no global
toolchain mutation (containerize like cross or cache like xwin), and metadata/JSON-driven plumbing.

### 2.4 Honest audit — aspect | `cargo dotnet` today | idiomatic | verdict

| # | Aspect | `cargo dotnet` today | Idiomatic | Verdict |
|---|---|---|---|---|
| **D0** | **cargo arg convention** | `cargo-dotnet:538` reads `sub="${1:-help}"`, i.e. treats `argv[1]` as the subcommand. But under real cargo dispatch `argv[1] = "dotnet"`. **Empirically reproduced:** `cargo-dotnet dotnet build /x` → `unknown subcommand 'dotnet'` (exit 1), whereas `cargo-dotnet build /x` works. So the advertised `cargo dotnet build` (docs claim it "works as a cargo subcommand") is **broken under genuine cargo dispatch**; only the direct `cargo-dotnet build …` form works — which is all `dev.sh` and the examples actually exercise. | Strip the prepended subcommand name (`[ "${1:-}" = dotnet ] && shift`). | **MUST-FIX (P0).** One line. The headline UX is currently broken through cargo. |
| **D1** | Implementation language | 720-line bash front-end + 829-line bash core; install via copy to `~/.cargo/bin` (a hand-rolled `cargo install`) | A `cargo install`-able clap Rust binary (every exemplar) | **Deviation; acceptable for an experiment.** Bash iterates fast and the pipeline is shell-shaped; convert before "1.0". Also Windows-fragile (`.cmd` + MSYS detection). |
| **D2** | **`setup` mutates the SHARED rust-src** | `setup` step [6/6] "warms" the PAL injection, which patches `$(rustc --print sysroot)`'s **rust-src component in place** (`cargo-dotnet:353` "toolchain $TOOLCHAIN rust-src patched"). Every *other* build on that toolchain now compiles against a mutated std, invisibly and unversioned. | cross containerizes; cargo-xwin caches; **neither touches rustup/rust-src** | **MOST non-idiomatic. SHOULD-FIX (P1).** Acceptable only as an experimental expedient (build-std needs a patched std, no stable "overlay std" hook). Fix: inject into a **private/copied** rust-src under `CARGO_DOTNET_HOME` (point build-std at it via a custom sysroot / `RUST_SRC_PATH`), or keep mutation **container-only**; at minimum a dedicated **named** throwaway toolchain + `setup --revert` + a sentinel. |
| **D3** | `.cargo/config.toml` handling | `apply_overlays` **regenerates the file FROM SCRATCH** every build (`_cargo_dotnet_core.sh:687`, `> .cargo/config.toml`), preserving only the keys it knows about | Merge/append, or inject via `cargo --config KEY=VAL` / a side config, never clobber a tracked file | **Deviation; SHOULD-FIX (P1).** Data-loss footgun for any crate that has its own `.cargo/config.toml`. The `paths`-override *mechanism* itself is good and well-precedented (cargo-component-style). |
| **D4** | Standard-flag passthrough | Accepts only a positional `PATH` + `--release`/`--debug`/`--clean`/`-v`/`--`; `CARGOFLAGS=(--release)` is hardcoded (`core:633`); **hard-errors on any unknown flag** ("unknown flag '$1'"). No `--features`/`--manifest-path`/`--target-dir`/`-p`/`--workspace`/`--offline`/`--locked`. | Forward unknown flags verbatim to the inner `cargo build` (cargo-component/zigbuild); adopt clap-cargo groups | **Deviation; biggest UX gap. SHOULD-FIX (P2).** A user can't `cargo dotnet run --features foo -p mycrate --locked`; notably this also blocks getrandom 0.2's `custom` feature through the tool. |
| **D5** | Config channel | env-only: `CARGO_DOTNET_BACKEND`, `CARGO_DOTNET_HOME`, `RCC_IMAGE`, `ILASM_PATH`, `OPTIMIZE_CIL` | env is fine as a secondary channel; user-facing knobs should also be flags / a `[package.metadata.dotnet]` table (cross's `Cross.toml`, binstall) | **Mild deviation; acceptable.** Add a config table when it becomes a binary. |
| **D6** | JSON diagnostics relay | Uses `--message-format=json` internally to locate the artifact (good), but to the *user* it greps the human log; no `--message-format` passthrough | Relay cargo JSON so editors/CI get structured diagnostics (nextest) | **Mild deviation; acceptable to defer.** Add a `--message-format=json` passthrough that disables the grep. |
| **D7** | `CARGO` callback | hardcodes `cargo` / `cargo +TOOLCHAIN` | honor `$CARGO` | **Minor; low priority.** |
| **D8** | `--version` | not implemented (only `--help`/`help`) | conventional `--version` (the Book mandates `--help`, but idiomatic tools provide both) | **Minor.** Add `cargo dotnet --version` from the VERSION manifest. |
| **D9** | Docker-vs-native modes | dev=docker, installed=native | cross legitimizes the Docker pattern; native is the real journey | **Acceptable / strength.** Keep both; native default for installed is right. |
| **D10** | Global `target-family=["unix"]` flip | the target spec is `os="dotnet"` but `target-family=["unix"]`, a deliberate "lie" so `cfg(unix)` crates compile unpatched; the core spends ~250 lines neutralizing the now-reachable unix paths | not a subcommand-convention issue per se | **Architectural bet; acceptable-with-eyes-open.** Pragmatic ecosystem coverage at a real ongoing correctness/maintenance cost (any new `cfg(unix)` path hitting an unmodeled libc symbol is a latent break). Track it; evaluate a true `os="dotnet"`/`family=[]` model long-term. |

**What it does genuinely well:** correct `cargo-X` install location (`~/.cargo/bin`, shows in
`cargo --list`); a coherent `setup`/`build`/`run`/`pack` verb set with `--release` default + `-- ARGS`
forwarding and exit-code propagation; an **install-once-use-anywhere** mode (`CARGO_DOTNET_HOME`);
`cargo metadata --no-deps` for name/version in `pack`; a real OPC `.nupkg` + MSBuild integration; the
`paths`-override overlay registry (a good, cargo-component-validated pattern) that never edits the
user's `Cargo.toml`; and a thoughtful overlay-vs-lock version-mismatch warning and NuGet
cache-staleness warning. None of the deviations are disqualifying for an experimental backend.

---

## 3. Prioritized recommendations (effort × value)

| Pri | Recommendation | Effort | Value |
|---|---|---|---|
| **P0** | **Fix the cargo argument convention (D0).** Before `cargo-dotnet:538`, strip a leading `dotnet`: `[ "${1:-}" = dotnet ] && shift`. Verify with an actual `cargo dotnet build <crate>` after `setup`, not just `cargo-dotnet build`. | Trivial (1 line) | High — the advertised `cargo dotnet build` is broken under real dispatch. |
| **P1** | **Stop mutating the SHARED rust-src (D2).** Inject the PAL into a private/copied rust-src under `CARGO_DOTNET_HOME` (custom sysroot / `RUST_SRC_PATH`), or keep mutation container-only; meanwhile dedicate a named toolchain, add `setup --revert` + a sentinel, and disclose the side effect loudly. | Medium | High — the one real "good citizen" / correctness risk; bleeds into all native builds on that toolchain. |
| **P1** | **Stop clobbering the user's `.cargo/config.toml` (D3).** Merge/append, write a tool-owned config, or inject via `cargo --config`; refuse/back-up if a user config exists. | Low–Medium | Medium–High — silent data-loss footgun. |
| **P2** | **Forward standard cargo flags (D4).** Pass through `--features`/`--no-default-features`/`--manifest-path`/`--target-dir`/`-p`/`--workspace`/`--offline`/`--locked`, and forward unknown flags verbatim to the inner `cargo build`. Also unblocks getrandom 0.2's `custom` feature. | Medium | High — the biggest "feels like cargo" gap. |
| **P2** | **Land the getrandom auto-shim (Q1 Option 2).** From `Cargo.lock`, generate + inject a lock-pinned `__getrandom_v03_custom` (→ `getrandom_dotnet::fill`) for 0.3/0.4, guarded to skip if the user already defines it; keep 0.2 a documented step (or a 0.2 overlay); add an opt-in `--getrandom unsupported`. Removes the last manual caveat printed by `cargo dotnet setup`. Do **not** make the linker provide the symbol. | Medium | High — zero-wiring getrandom for the common rand/uuid/ahash case. |
| **P3** | **Re-implement as a `cargo install`-able clap Rust binary (D1).** Use `clap-cargo` for standard flag groups, `cargo_metadata` instead of grep/sed, honor `$CARGO` (D7), add `--version` (D8); distribute via `cargo install` + `[package.metadata.binstall]` (cargo-xwin pattern). Keep the bash script as the in-repo dev driver until then. Natural home for P0/P2/P3-polish. | High | Medium–High — the long-term idiomatic form; also fixes Windows fragility. |
| **P3** | **Polish (D6/D8):** add `--message-format=json` passthrough for IDE/CI; add `--version`. | Low | Low–Medium. |
| **P3** | **Track the `target-family=["unix"]` bet (D10):** enumerate `cfg(unix)` paths reaching unmodeled libc symbols; consider a true `os="dotnet"`/`family=[]` model long-term. | High | Medium (risk reduction). |

**Keep as-is (correct):** the custom JSON target + build-std, the `dotnet_overlays` `paths`-override
registry concept, the install-once mode, exit-code propagation, the Docker/native split, the
NuGet cache-staleness warning, and the overlay-vs-lock version-mismatch warning.

---

## Sources

**getrandom**
* getrandom docs (latest): backend selection / `compile_error!` on unsupported target; opt-in
  backend list incl. `custom`/`unsupported` (`unsupported` "Always returns `Err(Error::UNSUPPORTED)`");
  the `--cfg getrandom_backend="custom"` selector; `__getrandom_v03_custom` signature; "MUST be
  defined only once for your project", "ideally … in the root crate, e.g. `main.rs`", "upstream
  library crates SHOULD NOT define it outside of tests and benchmarks." — https://docs.rs/getrandom/latest/getrandom/
* getrandom `src/backends.rs` / `src/backends/custom.rs` (cfg cascade; the `extern "Rust" { fn __getrandom_v03_custom … }` decl). — https://github.com/rust-random/getrandom/blob/master/src/backends.rs
* getrandom `Error` newtype (`Error(NonZeroU32)`, no `repr` guarantee → not FFI-stable). — https://docs.rs/getrandom/0.2.3/getrandom/struct.Error.html
* getrandom 0.2 `register_custom_getrandom!`: the `custom` Cargo feature; `fn(&mut [u8]) -> Result<(), Error>`; "Functions can only be registered in the root binary crate. Attempting to register a function in a non-root crate will result in a linker error"; the `#[panic_handler]`/`#[global_allocator]` analogy. — https://docs.rs/getrandom/0.2.10/getrandom/macro.register_custom_getrandom.html

**Cargo subcommand contract & conventions**
* The Cargo Book — External Tools: cargo-X dispatch + `$CARGO_HOME/bin` precedence; the argv
  convention ("second argument will be the subcommand name itself"); `cargo help X` → `cargo-X X --help`;
  the `CARGO` callback; `cargo metadata`; `--message-format=json` reasons. — https://doc.rust-lang.org/cargo/reference/external-tools.html
* The Rust Book — Extending Cargo with Custom Commands. — https://doc.rust-lang.org/book/ch14-05-extending-cargo.html
* clap-cargo (standard `Manifest`/`Workspace`/`Features` flag groups). — https://github.com/crate-ci/clap-cargo

**Exemplar tools**
* cross: "exact same CLI as Cargo but relies on Docker or Podman"; "without touching your system
  installation"; `CROSS_CONTAINER_ENGINE`. — https://github.com/cross-rs/cross
* cargo-xwin: `cargo install --locked`; caches MSVC CRT + Windows SDK in `XWIN_CACHE_DIR`, doesn't
  mutate rustup. — https://github.com/rust-cross/cargo-xwin
* cargo-zigbuild: `cargo install --locked`; "If you do not provide a `--target`, Zig is not used and
  the command effectively runs a regular `cargo build`." — https://github.com/rust-cross/cargo-zigbuild
* cargo-component (overlay + auto-post-process model). — https://github.com/bytecodealliance/cargo-component
* wasm-pack / cargo-wasi (version-matched wasm-bindgen companion — the lockfile-derived-glue precedent). — https://github.com/bytecodealliance/cargo-wasi , https://rustwasm.github.io/docs/wasm-pack/
* cargo-nextest / cargo-hack (flag passthrough). — https://nexte.st/ , https://github.com/taiki-e/cargo-hack
* cargo-binstall (`[package.metadata.binstall]` via cargo metadata). — https://github.com/cargo-bins/cargo-binstall

**Local files verified (absolute paths)**
* `/Users/sharif/Code/rustc_codegen_clr/feasibility/cargo-dotnet` — `:538` `sub="${1:-help}"` (the D0
  bug); `:353` "rust-src patched" (D2); `:570` "unknown flag" hard-error (D4). Repro: `cargo-dotnet
  dotnet build /x` → `unknown subcommand 'dotnet'` (exit 1) vs `cargo-dotnet build /x` works.
* `/Users/sharif/Code/rustc_codegen_clr/feasibility/_cargo_dotnet_core.sh` — `:640` `--cfg
  getrandom_backend="custom"`; `:633` `CARGOFLAGS=(--release)` (D4); `:687` `> .cargo/config.toml`
  regenerate-from-scratch (D3); `apply_overlays` `paths`-override + lock version-mismatch warn.
* `/Users/sharif/Code/rustc_codegen_clr/cargo_tests/getrandom_dotnet/{src/lib.rs,Cargo.toml,README.md}`
  — the version-agnostic `fill()` shim; Cargo.toml comment on why it doesn't depend on getrandom.
* `/Users/sharif/Code/rustc_codegen_clr/cargo_tests/soak_uuid/src/main.rs` — per-consumer
  `__getrandom_v03_custom` wiring for getrandom 0.4 (the hand-wiring the auto-shim removes).
* `/Users/sharif/Code/rustc_codegen_clr/dotnet_overlays/{REGISTRY.toml,README.md}` — the
  `paths`-override overlay model (mio/socket2/tokio).
* `/Users/sharif/Code/rustc_codegen_clr/x86_64-unknown-dotnet.json` — `"os":"dotnet"`,
  `"target-family":["unix"]` (D10).
