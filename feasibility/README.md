# Feasibility harness

A reproducible development environment for building and exercising `rustc_codegen_clr`.
New users should start with the installed native flow in [`../QUICKSTART.md`](../QUICKSTART.md);
this directory owns the Docker/CI compatibility harness and lower-level backend iteration tools.

The contributor image contains .NET 8/9/10 compatibility runtimes on Linux; the public SDK supports
.NET 10 only. It keeps CI and fallback
exporter testing independent of the host while the installed `cargo dotnet` native path is also
supported on macOS. Windows remains opt-in while its wider packaging surface is finished.

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
| `Dockerfile` | Env only: pinned nightly + `rustc-dev`/`rust-src`, .NET 8/9/10, fallback `ilasm`, clang/gcc. Repo is mounted at runtime, not copied. |
| `run.sh` | Host driver: builds the image, runs a harness step with the repo mounted. |
| `harness.sh` | In-container steps: `build` / `smoke` / `test` / `demo`. |
| `onboarding_acceptance.sh` | Runs checkout setup into empty SDK/Cargo homes, proves a fresh shell discovers the installed `cargo dotnet`, then scaffolds and executes app/lib/plugin journeys outside the checkout. Also rejects a duplicate legacy `cargo install` during the one-build bootstrap. |
| `offline_restore_acceptance.sh` | Restores into fresh crate-isolated Cargo/sysroot homes, edits ordinary Rust source, runs with Cargo offline mode plus poisoned proxies, repeats without cache drift, and proves cache tampering fails before compilation. |
| `event_acceptance.sh` | Builds Rust-defined class and interface events through the default direct-PE writer, consumes both from real C# `+=`/`-=` syntax and reflection, subscribes Rust to a real BCL event with deterministic unsubscribe/drop removal, and fails if ILAsm is executed. |
| `api_docs_acceptance.sh` | Generates warning-free public Rust HTML docs and the packaged C# XML sidecar, asserts a real exported member/summary, and leaves a CI-uploadable artifact without publishing it. |
| `record_acceptance_result.sh` | Runs one special product oracle, requires its exact completion marker, scans fatal diagnostics, and atomically writes an owned runtime/profile evidence TSV for the final deterministic capability merge. |
| `write_capability_evidence_receipt.sh` | Hash-binds the strict merged report, manifest, and every contributing result TSV to the exact source SHA, dirty state, toolchain, host, and selected evidence scope. |
| `nats_managed_array_acceptance.sh` | Regenerates NATS.Client bindings, proves `byte[]` + interface-call signatures, and round-trips two payloads against an isolated real NATS server in debug and release without a C# shim. |
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

The application-shaped newcomer example has its own acceptance check. It builds and runs the
managed JSON CLI with a real input file, compares exact output, and verifies the malformed-input
diagnostic:

```bash
DOTNET_VERSION=10 feasibility/flagship_example_acceptance.sh
```

The NativeAOT product gate uses the same public `cargo dotnet publish` command a user runs. It writes
to an explicit output directory and executes the self-contained C#-host/Rust-library binary:

```bash
DOTNET_VERSION=10 feasibility/nativeaot_acceptance.sh
```

The API-documentation gate treats every rustdoc warning as an error and builds the same artifacts a
release can host or attach later. It writes
Rust HTML, C# XML, the probe NuGet package, logs, and a non-publication receipt to an explicit
directory; it does not deploy a site or push a package:

```bash
RCL_API_DOCS_DIR=/tmp/rcl-api-docs DOTNET_VERSION=10 feasibility/api_docs_acceptance.sh
```

The install-bundle gate constructs the release SDK layout, produces it twice and compares the ZIP
bytes, verifies the checksum/manifest, restores it outside the checkout, runs a newly scaffolded app
through the restored backend, then proves both archive and installed-home corruption fail closed:

```bash
DOTNET_VERSION=10 feasibility/install_bundle_acceptance.sh
```

The exported-library/PDB consumer gate builds the fixture in both debug and release, then loads the
public DLL from a normal C# host. It imports and invokes all supported C# delegate families through
both `#[dotnet_export]` and `#[dotnet_methods]`, and requires the managed Rust stack trace to resolve to
`lib.rs:line`, checks the non-inlined Rust leaf, and parses the PDB Document table rather than merely
checking that a sidecar file exists. It also verifies a whole-method LocalScope and named locals in
both profiles, including a known user-authored Rust local in debug (release optimization may remove
that particular slot). Finally, it requires the stable `/_/consumer/src/lib.rs` document path and
parses the exact standard Source Link JSON produced by `--source-link-url`:

```bash
DOTNET_VERSION=10 feasibility/pdb_consumer_acceptance.sh
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
helpers illustrate the raw backend invocation. For a standalone application, prefer the installed
driver workflow in the root [`QUICKSTART.md`](../QUICKSTART.md).

## The one-command DX (`cargo dotnet`)

`feasibility/cargo-dotnet` is the in-repository Docker development front-end for the same pipeline
that the installed Rust CLI exposes. It compiles an
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
P/Invoke, no NuGet) and calls the `#[unsafe(no_mangle)] pub extern "C"` exports as
`public static MainModule.<fn>` — ordinary managed calls, because the Rust is
compiled to managed CIL. De-mangled `#[repr(C)]` structs appear under their clean
`Crate.Type` name with synthesized ctor + getters. The worked example is
`cargo_tests/cd_interop/` (`rustlib/` cdylib + `csharp/` console app); marshalling
verified end-to-end on the real dotnet PAL: **primitives, UTF-8 `(ptr, len)`
strings, a struct value-type, and a slice**. Full consumer guide:
[`docs/INTEROP_CSHARP.md`](../docs/INTEROP_CSHARP.md).

### Architecture (and the Docker vs. native seam)

The feasibility driver is a **thin host front-end**: it resolves the repo + crate dir,
preflights, and dispatches to an **execution backend** (`CARGO_DOTNET_BACKEND`,
default `docker` for this in-repository script). The docker driver streams the shared core
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
command can never drift.

A **native** (non-Docker) driver also slots into the `CARGO_DOTNET_BACKEND` switch
(`CARGO_DOTNET_BACKEND=native`): it runs the *same* core directly on the host — no
container — with the host's real repo path instead of `/work`/`/project` and a
`command -v rustc cargo dotnet` preflight. The host-specific facts
(repo root, backend dylib extension `.so`/`.dylib`/`.dll`, linker, target spec,
cargo registry path) are passed to the core as `CD_*` env vars whose **defaults
reproduce the container layout**, so the docker path is byte-for-byte unchanged and
native is purely additive. Verified end-to-end on **macOS arm64** (J1/J2/J3, zero
Docker); a **Windows x64** path is opt-in. Full setup +
known-unknowns: [docs/CARGO_DOTNET.md §2b](../docs/CARGO_DOTNET.md#2b-native-no-docker).
The key native fact is that the .NET target is unchanged: CIL is architecture-agnostic and JITs on
the selected .NET 8, 9, or 10 runtime. The direct-PE default needs no assembler; if you explicitly
select the legacy IL path, use CoreCLR `ilasm`, not Mono.

### Honesty / current limits

- The in-repo **docker** backend (its default) needs the `rcc-dev` image
  (`feasibility/run.sh build`) and a running Docker. The **native** backend
  (`CARGO_DOTNET_BACKEND=native`) needs no Docker but requires the host toolchain,
  a compatible .NET SDK, and the host-built backend dylib + linker. The direct-PE
  default does not require `ilasm`. Native is verified on macOS arm64; Windows is
  opt-in. See [docs/CARGO_DOTNET.md §2b](../docs/CARGO_DOTNET.md#2b-native-no-docker).
- **Exit codes:** build failures and the program's exit status propagate through
  `cargo dotnet`; `cargo_tests/pal_exit_code` permanently checks that
  `std::process::exit(n)` reaches the host as `n`.
- **`getrandom`:** `getrandom` 0.2 / 0.3 / 0.4 **auto-work with ZERO wiring** via the
  `dotnet_overlays/getrandom-{0.2,0.3,0.4}` overlays (each IS getrandom, patched with
  a self-contained `target_os="dotnet"` backend arm that calls the PAL CSPRNG). So
  `rand` / `uuid` / `ahash` (and any getrandom user) just build — no custom symbol,
  no macro, no `getrandom_dotnet` dep, and no `--cfg getrandom_backend="custom"` RUSTFLAG
  (it was removed). See `dotnet_overlays/README.md`.

## The nightly pin

The repo and Docker harness both pin `nightly-2026-06-17`. Bumping that pin is a recurring
maintenance task: a new nightly may surface rustc-internal API drift even when the `cilly` IR core
is unchanged. Follow [`PORT_NOTES.md`](PORT_NOTES.md) for the re-port procedure.

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
