# Feasibility harness

A reproducible environment for building and exercising `rustc_codegen_clr`, created
to answer: *can this be revived and used to run Rust inside a .NET 8 backend?*

The project is only tested on **Linux x86_64 / .NET 8 CoreCLR**. This harness packages
that environment in Docker so results don't depend on your host (e.g. macOS/arm64,
which is doubly off the tested path: wrong OS *and* wrong arch).

## Quick start

```bash
# build cilly + the codegen backend (this is the "compiles on latest nightly?" check)
feasibility/run.sh build

# compile a standalone Rust program with the backend and run it on .NET
feasibility/run.sh smoke

# run the project's own stable test suite (CI subset)
feasibility/run.sh test

# tiny "Rust logic running on .NET" demo
feasibility/run.sh demo
```

On Apple Silicon the image builds for arm64 by default (fast, fine for the *build*
check). To run the .NET tests on the on-path arch, force x86_64 (slower, emulated):

```bash
PLATFORM=linux/amd64 feasibility/run.sh test
```

## What's here

| File | Purpose |
|------|---------|
| `Dockerfile` | Env only: pinned nightly + `rustc-dev`/`rust-src`, .NET 8 SDK, `ilasm` (via mono), clang/gcc. Repo is mounted at runtime, not copied. |
| `run.sh` | Host driver: builds the image, runs a harness step with the repo mounted. |
| `harness.sh` | In-container steps: `build` / `smoke` / `test` / `demo`. |
| `dev.sh` | **Deterministic dev loop** (see below): force-rebuild, build+run a cargo_tests crate, disassemble IL, gate-with-diff. Works around Docker mtime-skew + cwd-drift. |
| `cargo-dotnet` | **The one-command Rust→.NET DX** (see below): `cargo dotnet build/run` on ANY crate dir, zero user config. A thin host front-end over the shared pipeline core. |
| `_cargo_dotnet_core.sh` | The crate-agnostic **pipeline core** (PAL inject + backend RUSTFLAGS + build-std + overlay auto-apply + libc patch + build/run). Single source of truth; `cargo-dotnet` AND `dev.sh pal-build` both run it. |
| `demo/` | A small Rust program (`add.rs`) + script that compiles it with the backend and runs it on CoreCLR. |

## Deterministic dev tooling (`dev.sh`)

`run.sh`/`harness.sh` lean on cargo's incremental cache, which is **unreliable under Docker's
host-mount mtime skew** on macOS — edits to `cilly` silently don't reach the linker, so you debug a
stale binary. `dev.sh` is the iteration loop that avoids that (and never rebuilds the image). It
resolves the repo root from its own path, so it works from any cwd.

```bash
feasibility/dev.sh backend            # force clean-rebuild of cilly + linker + backend
feasibility/dev.sh run <crate>        # build (forced relink) + run cargo_tests/<crate>; verifies the
                                      #   binary is fresh and fails loudly if not. --clean = full clean.
feasibility/dev.sh buildstd           # shorthand for `run build_std` (real core+alloc+std)
feasibility/dev.sh il <crate> <sym>   # disassemble method(s) whose mangled name contains <sym>
                                      #   e.g. `il build_std rust_alloc`  (uses ikdasm)
feasibility/dev.sh gate               # force-rebuild + `cargo test ::stable` (CI skips), then DIFF
                                      #   against the known-22 baseline and report only NEW failures
feasibility/dev.sh sh '<bash>'        # arbitrary command in the container (correct mount, color off)
```

Cargo-output crates (`build_std`) link to an **ELF apphost** run directly (`./build_std`), with the
real `.dll` assembly + `.runtimeconfig.json` beside it; `il` disassembles that `.dll` with `ikdasm`
(`monodis` is not installed).

`run.sh` mounts the repo at `/work` and masks `/work/target` with a named Docker volume
(`rcc-target`), so the container's Linux build artifacts never clobber the host's `target/`
and persist across runs for caching. The project's own test harness hardcodes `target/release`
paths, which is why the build goes there (not a custom dir).

`build` and `test` are the load-bearing commands: `build` is the "does it compile on this
nightly?" check, and `test` runs the project's own `cargo test ::stable` suite (the real
end-to-end runtime validation — it drives build-std + ilasm + dotnet itself). The `smoke`/`demo`
helpers illustrate the raw backend invocation but need the build-std cargo setup from the repo's
`QUICKSTART.md` to actually run a standalone program.

## The one-command DX (`cargo dotnet`)

`feasibility/cargo-dotnet` is the **user-facing one command** that compiles an
arbitrary Rust crate to a runnable .NET assembly with **zero hand-config** — no
`RUSTFLAGS`, no `[patch.crates-io]`, no vendoring, no `.cargo/config` edits. It is
a [cargo subcommand](https://doc.rust-lang.org/cargo/reference/external-tools.html#custom-subcommands):
any `cargo-X` on `PATH` makes `cargo X` work.

```bash
cargo dotnet build [PATH] [--release|--debug] [--clean] [-v]
cargo dotnet run   [PATH] [--release|--debug] [--clean] [-v] [-- ARGS...]
cargo dotnet help
```

- **`PATH`** — the crate dir to build (default `.`). **Arbitrary**: under
  `cargo_tests/` *or* any fully external path (e.g. `/tmp/myproj`).
- **`--release`** is the default (project convention); `--debug` opts out.
- **`--clean`** does a `cargo clean` first (rebuilds std; bulletproof, slow).
- **`run`** builds then executes the produced apphost, forwarding `-- ARGS` and
  **propagating its exit code** (a build failure → non-zero; see the honesty note).
- **`-v`** shows the unfiltered build log.

### Putting it on `PATH`

```bash
export PATH="$PWD/feasibility:$PATH"        # then `cargo dotnet run …`
# or symlink it where cargo lives:
ln -s "$PWD/feasibility/cargo-dotnet" ~/.cargo/bin/cargo-dotnet
```

Or invoke it directly without the `cargo` shim:
`feasibility/cargo-dotnet run cargo_tests/cd_pure`.

### What it does (zero config)

The command supplies the `x86_64-unknown-dotnet` target spec, `build-std`, the
codegen backend RUSTFLAGS, the dotnet-PAL injection into `rust-src`, the libc
registry patch, and **auto-applies the `dotnet_overlays` registry** — so
`mio`/`socket2`/`tokio` (and their transitive deps) "just work" via a generated
per-project `.cargo/config.toml` `paths` override the user never sees or edits.
The user writes a **normal `Cargo.toml`** (plus a bare `[workspace]` line if the
crate is placed *under* this repo's workspace root; a truly external crate needs
nothing). The two zero-config proof crates are `cargo_tests/cd_pure` (pure Rust)
and `cargo_tests/cd_tokio` (a tokio loopback TCP echo whose only dep line is a
plain `tokio = { version = "1", features = [...] }`).

### Library output — calling Rust *from* C# (J3)

`cargo dotnet` also builds a Rust **library** (`crate-type = ["cdylib"]`) into a
**C#-referenceable .NET assembly**. It detects the crate-type from cargo's JSON
stream, emits `target/x86_64-unknown-dotnet/<profile>/lib<crate>.so` (a managed
PE), and copies it to **`<crate>.dll`** beside it (a pure file copy — the assembly
identity is `<crate>` regardless of the `.so` name). `cargo dotnet run` on a
library prints a "reference the .dll from C#" note and exits 0 (no entrypoint).

A C# project references it with a bare assembly `<Reference>` + `<HintPath>` (no
P/Invoke, no NuGet) and calls the `#[no_mangle] pub extern "C"` exports as
`public static MainModule.<fn>` — ordinary managed calls, because the Rust is
compiled to managed CIL. De-mangled `#[repr(C)]` structs appear under their clean
`Crate.Type` name with synthesized ctor + getters. The worked example is
`cargo_tests/cd_interop/` (`rustlib/` cdylib + `csharp/` console app); marshalling
verified end-to-end on the real dotnet PAL: **primitives, UTF-8 `(ptr, len)`
strings, a struct value-type, and a slice**. Full consumer guide:
[`docs/INTEROP_CSHARP.md`](../docs/INTEROP_CSHARP.md).

### Architecture (and the Docker vs. native seam)

`cargo-dotnet` is a **thin host front-end**: it resolves the repo + crate dir,
preflights, and dispatches to an **execution backend** (`CARGO_DOTNET_BACKEND`,
default `docker`). The docker driver streams the shared core
(`feasibility/_cargo_dotnet_core.sh`) into the `rcc-dev` container. The repo is
**always** mounted at `/work` (backend dylib, overlays, target spec); how the
target crate is mounted depends on whether it lives **in-repo** or **external**:

- **In-repo** (`cargo_tests/<crate>`, …): no separate mount — the crate already
  lives in the `/work` tree, so the cwd is set to `/work/<relpath>`. A sibling
  **relative** path-dep (`getrandom_dotnet = { path = "../getrandom_dotnet" }`)
  then resolves to `/work/cargo_tests/getrandom_dotnet` exactly as the pre-Phase-D
  `dev.sh pal-build` did.
- **External** (a crate **outside** the repo, e.g. `/tmp/…`): mounted separately
  at `/project` (`-w /project`); it must use **absolute** path-deps plus any extra
  read-only sibling mounts the caller adds (the external-crate / J4 contract).

Either way the produced apphost lands in the user's own `target/`. `dev.sh
pal-build <crate> [--run]` **delegates** to this same front-end + core (always an
in-repo `cargo_tests/<crate>`), so the probe regression path and the user-facing
command can never drift. A future **native** (non-Docker) driver slots into the
`CARGO_DOTNET_BACKEND` switch: same core, with the host's real repo/crate paths
instead of `/work`/`/project` and a `command -v dotnet ilasm` host preflight — UX
and pipeline unchanged.

### Honesty / current limits

- This wraps the Docker harness, so it needs the `rcc-dev` image
  (`feasibility/run.sh build`) and a running Docker. The native path is later
  packaging work — the front-end/core are already structured for it.
- **Exit codes:** build failures and the program's own exit code propagate
  faithfully. But on the dotnet PAL a **panic** (or `std::process::exit(n)`)
  surfaces as an unhandled managed exception and the apphost still returns **0** —
  a pre-existing PAL limitation (the managed exception is not translated to a
  non-zero process exit), independent of `cargo dotnet`.
- **`getrandom`:** the command passes `--cfg getrandom_backend="custom"` (harmless
  for crates that don't use it); a crate that pulls `getrandom` still needs the
  custom-backend shim symbol — see `dotnet_overlays/README.md`.

## The nightly pin

The repo's `rust-toolchain.toml` says `channel = "nightly"` (unversioned), so a fresh
checkout grabs *today's* nightly — which is how it bit-rotted. The `Dockerfile` pins
`NIGHTLY=nightly-2026-06-17` for reproducibility. Bumping it forward is the recurring
maintenance tax: each bump may surface a fresh batch of rustc-internal API drift to fix
(see `docs/ARCHITECTURE.md` for why only the thin rustc-facing crates rot, while the
`cilly` IR core stays stable).

## Status / honesty

- **Build on latest nightly:** the point of this harness — see the top-level summary / PR.
- **Rust running on .NET:** the supported direction; `smoke`/`demo` exercise it.
- **Calling Rust *from* C# (J3):** demonstrated end-to-end via `cargo dotnet`'s
  library output — a real C# console app references a `cargo dotnet`-built Rust
  `cdylib` and calls its exports, asserting the results match Rust (primitives,
  strings, a struct, a slice). See the "Library output" subsection above,
  `cargo_tests/cd_interop/`, and [`docs/INTEROP_CSHARP.md`](../docs/INTEROP_CSHARP.md).
  The richer / more idiomatic surface — managed `System.String` returns and
  Rust-raises-a-.NET-exception `Result`s (`mycorrhiza`, `dotnet_typedef!`) — is
  proven on the surrogate target (`cargo_tests/rust_export/`) but not yet through
  this real-PAL flow; that is the remaining interop work.
