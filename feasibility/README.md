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
- **Calling Rust *from* C#/EF Core ergonomically:** the project's least-finished area
  (`mycorrhiza`, `dotnet_typedef!`). Not demonstrated here on purpose — it's the real
  remaining work, independent of getting the build green.
