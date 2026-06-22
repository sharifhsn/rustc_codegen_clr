# `cargo dotnet` — compile arbitrary Rust into a .NET / C# project, in one command

This is the **average-user entry point** for the project. If you just want to take a Rust crate and
**run it on .NET**, or **call a Rust library from C#**, this is the page. It assumes no knowledge of
the codegen backend internals — those live in [docs/ARCHITECTURE.md](ARCHITECTURE.md).

> **What this gives you:** `cargo dotnet build|run` compiles an *arbitrary* Rust crate to a runnable
> .NET assembly (or a C#-referenceable `.dll`) with **zero hand-config** — no `RUSTFLAGS`, no
> `[patch.crates-io]`, no vendoring, no `.cargo/config` edits. You write a normal `Cargo.toml`; the
> command supplies the .NET target, `build-std` with the real .NET PAL, the codegen backend + linker,
> and auto-applies the crate-overlay registry so syscall-using deps (`mio`/`socket2`/`tokio`) just work.

---

## 1. What it is

`feasibility/cargo-dotnet` is a [cargo custom subcommand](https://doc.rust-lang.org/cargo/reference/external-tools.html#custom-subcommands):
once it is on `PATH`, `cargo dotnet …` works. It is a **thin host front-end** that resolves your crate,
preflights, and dispatches to the shared pipeline core (`feasibility/_cargo_dotnet_core.sh`) which does
the actual work: inject the dotnet PAL into `rust-src`, set the backend RUSTFLAGS, run `build-std`,
apply the [`dotnet_overlays`](../dotnet_overlays/README.md) registry, patch the libc registry, and
build/run.

```bash
cargo dotnet build [PATH] [--release|--debug] [--clean] [-v]
cargo dotnet run   [PATH] [--release|--debug] [--clean] [-v] [-- ARGS...]
cargo dotnet pack  [PATH] [--release|--debug] [--id NAME] [--version VER] [--out DIR]
cargo dotnet help
```

| arg / flag | meaning |
|---|---|
| `PATH` | the crate dir to build (default `.`). **Arbitrary** — under `cargo_tests/` *or* any fully external path (e.g. `/tmp/myproj`). |
| `--release` | release profile — **the default** (project convention). |
| `--debug` | debug profile (opt out of release). |
| `--clean` | `cargo clean` first; rebuilds std. Bulletproof but slow — reach for it if a stale-cache result looks wrong. |
| `-v` / `--verbose` | unfiltered build log. |
| `-- ARGS` | (`run` only) args forwarded to the .NET program; **its exit code propagates** back out (see [§6 honest limits](#6-what-works--honest-limits)). |
| `--id` / `--version` / `--out` | (`pack` only) override the NuGet package id / version / output dir (see [§5 consuming from C#](#5-consuming-a-rust-library-from-c)). |

The `pack` subcommand builds the crate (a `cdylib`) and emits a NuGet `.nupkg` of its .NET assembly to
`<crate>/target/nupkg/<id>.<ver>.nupkg`, so a C# project can `<PackageReference>` it from a local feed.
See [§5](#5-consuming-a-rust-library-from-c).

Because the Rust is compiled to **managed CIL** (not native code behind a P/Invoke wall), the produced
assembly *is* .NET: Rust functions are ordinary managed methods, Rust panics are managed exceptions.
That single fact is what makes both the run-on-.NET and the call-from-C# stories work without FFI glue.

---

## 2. Prerequisites & setup

The build runs inside the project's reproducible Docker harness (the `rcc-dev` image), which ships the
pinned nightly + `rustc-dev`/`rust-src`, the .NET 8 SDK, and `ilasm` (via Mono). The project is only
tested on **Linux x86_64 / .NET 8 CoreCLR**, and the harness pins that environment so results don't
depend on your host (macOS/arm64 is doubly off-path: wrong OS *and* wrong arch).

```bash
# One-time: build the harness image (also the "does it still compile on nightly?" check).
feasibility/run.sh build
```

Then put `cargo dotnet` on `PATH` (or invoke it directly):

```bash
export PATH="$PWD/feasibility:$PATH"          # then: cargo dotnet run …
# or symlink it where cargo looks for subcommands:
ln -s "$PWD/feasibility/cargo-dotnet" ~/.cargo/bin/cargo-dotnet
# or skip the cargo shim entirely:
feasibility/cargo-dotnet run cargo_tests/cd_pure
```

You need a running Docker daemon. That is the **only** host dependency — the image carries `dotnet`
and `ilasm`.

> **The Docker-vs-native seam.** `cargo-dotnet` dispatches on `CARGO_DOTNET_BACKEND` (default
> `docker`). A **native** (non-Docker) driver (`CARGO_DOTNET_BACKEND=native`) runs the *same*
> pipeline core directly on the host — no container — against the host's real repo path and
> `command -v rustc cargo dotnet` + a CoreCLR `ilasm`. The UX and pipeline are unchanged; only the
> host-specific paths/tools differ (supplied to the shared core as `CD_*` env vars whose defaults
> reproduce the container layout, so the Docker path is byte-for-byte unchanged). See
> **[§2b Native (no Docker)](#2b-native-no-docker)** below and
> [feasibility/README.md](../feasibility/README.md).

> **Dev path vs installed tool.** Running `feasibility/cargo-dotnet` (or `dev.sh pal-build`) from a
> checkout is the **development** path — it builds from the repo layout and defaults to the Docker
> backend. For the **end-user** journey — install once, then build/run *any* crate in *any* directory
> with no repo present — use **[§2c Install once, use anywhere](#2c-install-once-use-anywhere)**. The
> two are additive: the same script source is both the dev front-end and (after `cargo dotnet setup`
> copies it to `~/.cargo/bin`) the installed tool.

---

## 2b. Native (no Docker)

The native driver runs the whole pipeline on the host's own toolchain — useful where Docker is
unavailable or slow (e.g. a developer laptop). It is **additive**: the Docker path is the default and
is unchanged. Select it with `CARGO_DOTNET_BACKEND=native`.

> **Why the target is still `x86_64-unknown-dotnet` on a non-x86 host.** The backend output is .NET
> **CIL**, which is architecture-agnostic — a compiled assembly JITs and runs on the host's native
> .NET 8 regardless of host CPU. The `x86_64` in the target name is rustc's *layout model* for the
> dotnet output, not the host arch. So you do **not** change the target spec; you only build the
> toolchain binaries (backend dylib + linker + build-std) for the host.

### macOS (arm64, Apple Silicon) — verified

This flow is verified end-to-end on macOS arm64 (J1/J2/J3 all pass, zero Docker). Setup is
**user-local, no sudo**:

1. **Toolchain** (match the pinned nightly to avoid rustc-API drift):
   ```bash
   rustup toolchain install nightly-2026-06-17 \
     --component rust-src --component rustc-dev --component llvm-tools-preview
   rustup override set nightly-2026-06-17           # in the repo root
   ```
2. **.NET 8 SDK** (the Homebrew cask is .NET 10 — match the Docker .NET 8 instead):
   ```bash
   curl -sSL https://dot.net/v1/dotnet-install.sh | bash -s -- --channel 8.0 --install-dir "$HOME/.dotnet"
   export PATH="$HOME/.dotnet:$PATH"; export DOTNET_ROOT="$HOME/.dotnet"
   ```
3. **`ilasm` — use the CoreCLR ILAsm, NOT Mono.** Mono's `ilasm` only emits PE32/i386 images, which
   the native (macOS arm64 / Windows) CoreCLR loader **rejects** (`FileLoadException 0x8007000C`). The
   CoreCLR ILAsm from NuGet emits a PE the host CoreCLR loads:
   ```bash
   # download + extract the osx-arm64 ILAsm tool, then place its `ilasm` here:
   #   runtime.osx-arm64.Microsoft.NETCore.ILAsm  (version 8.0.0, matching .NET 8)
   mkdir -p "$HOME/.dotnet/ilasm-tool"
   # cp <extracted>/runtimes/osx-arm64/native/ilasm "$HOME/.dotnet/ilasm-tool/ilasm"
   chmod +x "$HOME/.dotnet/ilasm-tool/ilasm" && xattr -c "$HOME/.dotnet/ilasm-tool/ilasm"
   ```
   The native driver auto-discovers `$HOME/.dotnet/ilasm-tool/ilasm`; override with `ILASM_PATH`.
   *(`brew install mono` is the documented Mono fallback for the Docker/Linux flow, but it does NOT
   work for the native macOS run — its PE32 output won't load on arm64 CoreCLR.)*
4. **Build the host-native backend:**
   ```bash
   (cd cilly && cargo build --release) && cargo build --release -p rustc_codegen_clr
   # -> target/release/librustc_codegen_clr.dylib + target/release/linker
   ```
5. **Run any crate, no Docker:**
   ```bash
   CARGO_DOTNET_BACKEND=native feasibility/cargo-dotnet run cargo_tests/cd_pure
   CARGO_DOTNET_BACKEND=native feasibility/cargo-dotnet run cargo_tests/cd_tokio
   CARGO_DOTNET_BACKEND=native feasibility/cargo-dotnet build cargo_tests/cd_interop/rustlib
   ```

> **One backend change made this work:** the CoreCLR `ilasm` caps class names at 1023 chars, which
> the backend's deeply-nested monomorphized generic names exceed (Mono had no such cap). The IL
> exporter now deterministically shortens any >1023-char class name (readable head + a 64-bit hash of
> the full name), applied identically at the type's definition and every reference. Names within the
> limit are emitted unchanged, so the Linux/Docker output and the `::stable` suite are unaffected.

### Windows (x86_64) — best-effort, UNTESTED

The same native pipeline is wired for Windows x64, but **it has not been run or verified on Windows**
(no Windows host was available). The changes are defensive OS-detection branches that do not affect
macOS/Linux. Treat the steps below as a starting recipe, not a guarantee.

Prereqs (user-local where possible):
- **Bash**: `cargo-dotnet` is a bash script. Run it under **Git Bash** (Git for Windows), MSYS2, or
  WSL. A `feasibility\cargo-dotnet.cmd` shim forwards a normal-shell `cargo dotnet ...` to bash with
  the native backend. It prefers Git Bash (`%ProgramFiles%\Git\bin\bash.exe`, then `bash.exe` on
  PATH) and falls back to WSL — the WSL branch translates the script path with `wslpath` so WSL bash
  receives a `/mnt/c/...` POSIX path. Git Bash / MSYS2 is the smoother route (it accepts a Windows
  path directly and shares the host's `%USERPROFILE%`/`.cargo` layout); WSL runs in a separate Linux
  filesystem, so under WSL you would build the **Linux** backend and use the Linux flow instead.
- **Toolchain**: `rustup toolchain install nightly-2026-06-17-x86_64-pc-windows-msvc` with
  `--component rust-src --component rustc-dev`. (The MSVC host toolchain + the Build Tools' linker
  environment are required to build the backend and the native launcher.)
- **.NET 8 SDK** on PATH (`dotnet.exe`).
- **`ilasm`**: the CoreCLR ILAsm tool for win-x64 — NuGet
  `runtime.win-x64.Microsoft.NETCore.ILAsm` (8.0.x). Place its `ilasm.exe` at
  `%USERPROFILE%\.dotnet\ilasm-tool\ilasm.exe` (Git Bash sees this as `$HOME/.dotnet/ilasm-tool/ilasm.exe`
  and the native driver auto-discovers it), or set `ILASM_PATH` to it. **Setting a CoreCLR `ilasm` is
  effectively mandatory on Windows**: if `ILASM_PATH` is unset, the cilly linker's Windows default
  (`cilly/src/ir/asm.rs::get_default_ilasm`, `#[cfg(target_os = "windows")]`) probes a bare `ilasm`
  on PATH and then the **legacy .NET Framework** assembler under `C:\Windows\Microsoft.NET\Framework`
  — a different, much older ilasm whose output the CoreCLR loader may reject (and which panics if no
  Framework is installed). The native driver always exports `ILASM_PATH` so this fallback is bypassed.
  (Do **not** use Mono's ilasm — same PE32 problem as macOS.)
- **Backend**: build it host-native — `librustc_codegen_clr.dll` + `linker.exe` under
  `target\release\`. The native driver detects the `.dll`/`.exe` extensions automatically (via
  `uname`/`OSTYPE`), locates `linker.exe`, and (for the rare bin crate whose cargo JSON omits the
  `executable` field) probes the `.exe` apphost via the `CD_EXE_EXT` host fact. The common run path
  reads cargo's own `"executable"` field, which already carries `.exe` on Windows.

Run (from Git Bash):
```bash
CARGO_DOTNET_BACKEND=native feasibility/cargo-dotnet run cargo_tests/cd_pure
```
or from a normal Windows shell: `feasibility\cargo-dotnet.cmd run cargo_tests\cd_pure`.

**Known unknowns on Windows** (unverified — no Windows host was available to run any of this):
- Whether the CoreCLR win-x64 `ilasm` accepts the same flags/`.il` the macOS one does, and whether
  the win-x64 CoreCLR loads the produced PE without further flags.
- Whether the native launcher/apphost (compiled by the linker via `rustc -O` with the MSVC linker)
  builds and links cleanly with the MSVC toolchain.
- Path-separator handling deep in the pipeline. The shell core is POSIX-path throughout (Git Bash
  presents `C:\` as `/c/`), but `rust-src` injection, the registry-libc patch, and the generated
  `.cargo/config.toml` absolute `paths` all assume forward-slash paths — untested on a Windows
  `rust-src` layout.
- The `cargo-dotnet.cmd` WSL branch's `wslpath` translation and delayed-expansion logic is written
  but unexercised.
Report findings if you run it.

---

## 2c. Install once, use anywhere

§2b runs the native pipeline **from a repo checkout**. The end-user journey removes the repo from the
loop entirely: **clone once, run `cargo dotnet setup`, then build/run any crate in any directory** —
no repo, no `RUSTFLAGS`, no env. `setup` provisions a self-contained install home and drops an
installed `cargo-dotnet` on your PATH.

```bash
# 1. Clone the repo (the ONLY time you need it).
git clone https://github.com/FractalFir/rustc_codegen_clr && cd rustc_codegen_clr

# 2. Provision everything (idempotent — safe to re-run).
feasibility/cargo-dotnet setup        # builds the backend, populates the install home,
                                      # installs `cargo-dotnet` to ~/.cargo/bin, warms rust-src

# 3. From now on, in ANY crate directory (the repo can be deleted):
cd ~/my-rust-crate
cargo dotnet run                      # build + run on .NET
cargo dotnet build                    # build only
cargo dotnet run /path/to/other-crate # or point it at a path
```

### What `setup` provisions

`cargo dotnet setup` is **detect-then-act / idempotent** — each step is skipped when already
satisfied (or re-done with `--force`):

1. **Toolchain** — ensures the pinned `nightly-2026-06-17` + `rust-src`/`rustc-dev`/`llvm-tools-preview`
   (via `rustup`). It does **not** hijack your global `rustup default`; the installed front-end pins the
   toolchain per-build via `RUSTUP_TOOLCHAIN` instead.
2. **.NET 8 SDK** — installs to `$HOME/.dotnet` (via `dotnet-install.sh`) if neither `$HOME/.dotnet/dotnet`
   nor a PATH `dotnet` is present.
3. **CoreCLR `ilasm`** — installs the host-RID `runtime.<rid>.Microsoft.NETCore.ILAsm` (8.0.0) NuGet
   package's `ilasm` to `$HOME/.dotnet/ilasm-tool/ilasm` (**not** Mono — Mono's PE32 output is rejected
   by the native CoreCLR loader; see §2b).
4. **Backend + install home** — builds the backend dylib + `linker` (`cargo +<toolchain> build --release`)
   from the checkout and copies them, the `dotnet_pal/` + `dotnet_overlays/` trees, the target spec, and a
   copy of the pipeline core into `CARGO_DOTNET_HOME`.
5. **Front-end** — copies the `cargo-dotnet` script (a real copy, not a symlink — it survives the repo
   being moved/deleted) to `~/.cargo/bin/cargo-dotnet`, so `cargo dotnet …` works as a cargo subcommand.
6. **rust-src PAL injection** — runs the per-toolchain `rust-src` PAL injection once, sourced from the
   install home, to fail-fast (verifies `rust-src` is writable and every arm applies).

Flags: `--from-repo <path>` (default: the checkout `setup` is run from), `--home <dir>`
(default `CARGO_DOTNET_HOME`), `--toolchain <name>`, `--skip-toolchain` / `--skip-dotnet` /
`--skip-ilasm`, `--force`.

### `CARGO_DOTNET_HOME` (the install home)

Default `$HOME/.cargo-dotnet`, overridable by the `CARGO_DOTNET_HOME` env var. It is self-contained —
the installed front-end runs the whole native pipeline from it with **`CD_REPO` pointed at the home,
the repo absent**:

```
$CARGO_DOTNET_HOME/
  VERSION                                # manifest: git rev, build date, host triple, pinned toolchain
  core.sh                                # copy of the pipeline core (_cargo_dotnet_core.sh)
  cargo-dotnet                           # source of the ~/.cargo/bin copy
  bin/
    librustc_codegen_clr.<dylib|so|dll>  # the codegen backend (host-native)
    linker[.exe]                         # the cilly linker
  target/x86_64-unknown-dotnet.json      # the target spec (CIL is arch-agnostic — unchanged)
  dotnet_pal/                            # copy of the repo PAL arms (the rust-src injection source)
  dotnet_overlays/                       # copy of the overlay registry (mio/socket2/tokio + REGISTRY.toml)
```

The installed front-end self-locates the home, detects host OS (`.dylib`/`.so`/`.dll`), self-heals
`dotnet` onto PATH from `$HOME/.dotnet` if needed, exports the `CD_*` seam at the home, and runs
`core.sh` against the current crate dir — **the default backend is `native`** (no Docker). All the
generated `.cargo/config.toml` paths and exported `CD_*` resolve into `CARGO_DOTNET_HOME`, never the
repo. (Verified: with the repo physically moved aside, an external crate still builds and runs.)

### Shell rc lines

`setup` prints any lines you should add to your shell rc. Typically:

```bash
export PATH="$HOME/.cargo/bin:$PATH"      # if ~/.cargo/bin is not already on PATH
export DOTNET_ROOT="$HOME/.dotnet"        # only if `dotnet` is not on PATH
export PATH="$HOME/.dotnet:$PATH"         # so the linker can find `dotnet`
```

(The installed front-end already self-heals `dotnet` from `$HOME/.dotnet`, but exporting it makes other
tools see it too.)

### Caveats & maintenance

- **`getrandom`** — the command passes `--cfg getrandom_backend="custom"`, but a crate that actually
  pulls `getrandom` still needs the custom-backend shim symbol (a documented follow-up). Pure
  compute/`println!` crates and most deps just work.
- **rust-src is a shared, per-toolchain mutation.** The PAL injection patches the *toolchain's*
  `rust-src` in place. Every arm is `#[cfg(target_os = "dotnet")]`-gated, so it is inert for any
  non-dotnet build under that nightly — but it **is** a global side effect (the install is not fully
  hermetic; it depends on and writes into the rustup toolchain). This is pre-existing repo behaviour,
  not new to the installed tool.
- **After a repo update**, re-run `cargo dotnet setup --from-repo <path> --force` to rebuild the
  backend and refresh the install home.

> The in-repo `feasibility/cargo-dotnet` (and `dev.sh pal-build`) remain the **development** path,
> unchanged: when invoked from a checkout (the sibling pipeline core present) the script uses the repo
> layout and the Docker backend by default. The installed tool is purely additive.

---

## 3. Quickstart

### 3a. A pure-Rust crate (no deps)

Write a normal crate — a `Cargo.toml` and a `src/main.rs`, nothing else:

```toml
# Cargo.toml
[package]
name = "hello_dotnet"
version = "0.1.0"
edition = "2021"            # 2021, not 2024 (the pinned nightly's default for this flow)

[dependencies]

[workspace]                 # a BARE line — only needed if the crate sits UNDER another
                            # workspace root (e.g. inside this repo). A truly external crate
                            # (its own root) needs nothing here.
```

```rust
// src/main.rs
fn main() {
    let words: Vec<String> = (1..=3).map(|i| format!("item-{i}")).collect();
    println!("hello from cargo dotnet (pure Rust on the .NET PAL)");
    println!("words = [{}]", words.join(", "));   // exercises std::alloc + the dotnet PAL
}
```

Run it:

```bash
cargo dotnet run path/to/hello_dotnet
```

The worked, asserted version is [`cargo_tests/cd_pure`](../cargo_tests/cd_pure) (compute + heap +
`println!`, with `assert_eq!`s so a miscompile would exit non-zero):

```bash
cargo dotnet run cargo_tests/cd_pure
```

### 3b. A crate with a real syscall-level dependency

Add an ordinary dependency to `Cargo.toml` — exactly what you'd write for native Rust. No `[patch]`, no
path/vendor override:

```toml
[dependencies]
tokio = { version = "1", features = ["rt", "macros", "net", "io-util"] }
```

```bash
cargo dotnet run cargo_tests/cd_tokio
```

[`cargo_tests/cd_tokio`](../cargo_tests/cd_tokio) is a tokio loopback TCP echo (the client sends three
line-framed messages, the server echoes each back uppercased). Its only dep line is the plain `tokio`
above — yet it pulls `mio` + `socket2` transitively and **runs I/O-driven async on the .NET PAL**.
`cargo dotnet` auto-applied the overlay registry to make those deps compile on the .NET target; you saw
none of it. That is the subject of the next section.

---

## 4. Using dependencies that need an overlay

Most crates need nothing special. The .NET target spec carries `target-family = ["unix"]`, so
`cfg(unix)` / `cfg(target_family="unix")` are true and **plain `cfg(unix)` crates compile unpatched** —
they pick their existing unix arms straight onto the .NET PAL.

A few load-bearing crates need a small *source* edit that no cfg flip can supply (e.g. `mio` selects
its readiness backend by `target_os`, which has no `dotnet` concept). These live in the central
**[`dotnet_overlays`](../dotnet_overlays/README.md)** registry — one vendored copy of each crate,
upstream-byte-identical except the lines marked `// DOTNET PAL`.

**Auto-apply (you do nothing):** on each build, the pipeline core reads `dotnet_overlays/REGISTRY.toml`,
finds every overlay whose crate name + version appears in your `Cargo.lock` (direct *or* transitive),
and regenerates your project's `.cargo/config.toml` with a top-level `paths = [ … ]` override pointing
at the overlay dirs. `paths` is keyed by crate **name** and is graph-wide, so one entry covers both a
direct `mio` and `mio`-under-`tokio`. It needs **zero** edits to your tracked `Cargo.toml`. On a
name-match with a version *mismatch*, it warns loudly and skips (no silent "overlay didn't apply →
miscompile" footgun).

Today the registry ships overlays for **`mio`**, **`socket2`**, and **`tokio`** (each a heterogeneous,
minimal edit). **Adding a new overlay** is a small recipe — vendor the pinned upstream, apply the
minimal `// DOTNET PAL`-marked edit, add a `[[overlay]]` block to `REGISTRY.toml`. Full recipe and the
line-by-line rationale for the existing three: [dotnet_overlays/README.md](../dotnet_overlays/README.md).

> **`getrandom` note.** The command passes `--cfg getrandom_backend="custom"` (harmless for crates that
> don't use it); a crate that pulls `getrandom` still needs the custom-backend shim symbol — see the
> overlay README.

---

## 5. Consuming a Rust library from C#

`cargo dotnet` also builds a Rust **library** (`crate-type = ["cdylib"]`) into a **C#-referenceable
.NET assembly**, so a C# project can call exported Rust functions as ordinary managed methods — **no
P/Invoke, no `[DllImport]`, no marshalling attributes, no reflection**, because the Rust *is* managed
CIL.

```bash
# 1. Build the Rust cdylib -> a managed PE + a referenceable .dll copy.
cargo dotnet build path/to/rustlib     # emits target/x86_64-unknown-dotnet/release/<crate>.dll

# 2. Reference that .dll from a C# project and run it.
dotnet run --project path/to/csharp
```

`cargo dotnet` detects the `cdylib` crate-type from cargo's JSON message stream, builds it under the
dotnet PAL, writes the managed PE to `target/x86_64-unknown-dotnet/<profile>/lib<crate>.so`, and copies
it to **`<crate>.dll`** beside it (a pure file copy — the assembly *identity* is `<crate>` regardless of
the `.so` filename). `cargo dotnet run` on a library prints a "reference the .dll from C#" note and
exits 0 (a library has no entrypoint).

**Recommended — let `dotnet build` build the Rust for you (`RustDotnet.targets`).** Import the
integration and declare the crate; a single `dotnet build`/`dotnet run` runs `cargo dotnet build` on it
and references the produced assembly — zero manual `cargo dotnet`, zero `.dll` copy, zero hand-written
`<Reference>` (incremental: the Rust rebuild is skipped when its `.dll` is newer than its sources).
`cargo dotnet setup` installs `RustDotnet.targets` into `$CARGO_DOTNET_HOME/msbuild/`, so it works in
any external project:

```xml
<Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
        Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
<ItemGroup>
  <RustCrate Include="../rustlib" />
</ItemGroup>
```

**Manual (fallback).** A C# project can also reference a pre-built `.dll` with a bare assembly
`<Reference>` + `<HintPath>` (no `ProjectReference`). Exported `#[no_mangle] pub extern "C"` functions
are `public static` methods on `MainModule`; de-mangled `#[repr(C)]` structs appear under their clean
`Crate.Type` name with a synthesized ctor + per-field getters.

```xml
<ItemGroup>
  <Reference Include="cd_interop"><HintPath>cd_interop.dll</HintPath></Reference>
</ItemGroup>
```

**Distribution (NuGet).** `cargo dotnet pack path/to/rustlib` produces a `.nupkg` of the assembly that a
C# project can `<PackageReference>` from a local feed (`<RestoreSources>`). Full guide, including the
NuGet cache footgun, in [docs/INTEROP_CSHARP.md](INTEROP_CSHARP.md).

```csharp
int sum = MainModule.rust_add(2, 3);                 // primitives: == 5
cd_interop.Point p = new cd_interop.Point(2, 3);     // de-mangled value-type
int s = MainModule.point_sum(p);                     // == 5
```

### Marshalling (verified end-to-end on the real dotnet PAL)

| Category | Rust signature | C# side |
|----------|----------------|---------|
| **Primitives** | `pub extern "C" fn rust_add(a: i32, b: i32) -> i32` | `int MainModule.rust_add(int, int)` |
| **Strings** | `(name_ptr: *const u8, name_len: usize, out_ptr: *mut u8, out_cap: usize) -> usize` | `fixed (byte* …)` UTF-8 `(ptr, len)` in + caller out-buffer |
| **Struct** | `#[repr(C)] pub struct Point { pub x: i32, pub y: i32 }` + `fn point_sum(p: Point) -> i32` | `new cd_interop.Point(2, 3)`, `p.get_x()` |
| **Slice / Vec** | `(ptr: *const i32, len: usize) -> i32` | `fixed (int* …)` over a C# `int[]` |

Strings and slices cross as **UTF-8 / element `(ptr, len)` pairs** (thin pointers, directly C#-usable
with `fixed`); no Rust allocation crosses the boundary, so there is nothing to free across it. The
worked example is [`cargo_tests/cd_interop`](../cargo_tests/cd_interop) (`rustlib/` cdylib +
`csharp/` console app).

**Full consumer guide (the `.csproj`, the C# program, why a bare `<Reference>`, what is/isn't verified):
[docs/INTEROP_CSHARP.md](INTEROP_CSHARP.md).** It also documents the **Tier-2** surface proven on the
surrogate target but not yet through this real-PAL flow: returning a managed `System.String` directly
and a Rust-raises-a-.NET-exception `Result` (both pull `mycorrhiza` + the throw intrinsic).

---

## 6. What works / honest limits

### The platform — real Rust std on the .NET PAL (no surrogate)

Under the `target-family = ["unix"]` flip, the dotnet PAL backs **real** `std`, not a stand-in:

- **Files** — `std::fs` over `System.IO` (open/read/write/seek/flush, mkdir/rmdir, rename, readdir,
  truncate, getcwd/chdir, canonicalize, **symlink/readlink**, **pread/pwrite**).
- **Net** — `std::net` TCP/UDP over `System.Net.Sockets`; **I/O-driven async** (tokio `TcpStream`/
  `TcpListener`) via the mio reactor.
- **Threads / sync / time** — `System.Threading`; `panic = unwind` with `catch_unwind` working
  end-to-end; monotonic + wall clock.
- **`std::os::unix`** — AF_UNIX (`UnixStream`/`UnixListener`), `MetadataExt` (size + timestamps),
  symlinks, the fd onion (`AsRawFd`/`FromRawFd`).

### Honest limits — `ENOSYS` / synthetic, never silently faked

Some POSIX primitives have no managed equivalent on stock CoreCLR and surface as
`Err(Unsupported)`/`ENOSYS` or a documented synthetic value (never a silent lie):

- **`fork` / `vfork` / `execve`** — cannot clone/replace a running JIT+GC managed runtime.
- **Inode identity** — `st_ino` / `st_dev` / `st_nlink` → 0/1 (breaks same-file detection); hard
  `link()` unsupported.
- **Ownership** — `st_uid` / `st_gid`, `chown`, full POSIX mode bits → synthetic / readonly-bit only.
- **Memory** — `mmap(MAP_FIXED)` / file-backed / shared mmap, `mprotect` guard pages, `brk`/`sbrk`.
- **Signals** — raw signal *delivery* / arbitrary `sigaction` handlers (only SIGINT/TERM/HUP/QUIT via
  `PosixSignalRegistration`); abstract-namespace unix sockets, SCM_RIGHTS fd-passing, ucred.

The full categorized libc map (CLEAN / LEAKY / IMPOSSIBLE per cluster, with the BCL mapping for each)
is [docs/LIBC_SHIM_SCOPE.md](LIBC_SHIM_SCOPE.md); the `std::os::unix` plan + leaky-bits ledger is
[docs/STD_OS_UNIX_PLAN.md](STD_OS_UNIX_PLAN.md).

### Soak — the breadth evidence

~74 real crates have been driven through `cargo dotnet` on the dotnet PAL under the flip; **73/74 pass**.
The one non-pass is `regex` (a deep allocator issue), not a class-level gap. 11+ class-level codegen
fixes landed over that campaign.

### Exit-code caveat

Build failures and the program's own exit code propagate faithfully. But on the dotnet PAL a **panic**
(or `std::process::exit(n)`) currently surfaces as an unhandled managed exception while the apphost
still returns **0** — a pre-existing PAL limitation independent of `cargo dotnet`.

---

## 7. The four proven journeys (worked examples)

Each is a real, runnable crate under [`cargo_tests/`](../cargo_tests) — copy its shape.

| # | Journey | Where | Proves |
|---|---------|-------|--------|
| **J1** | Pure Rust → .NET | [`cargo_tests/cd_pure`](../cargo_tests/cd_pure) | zero-config DX on fresh pure-Rust code (compute + heap + `println!`). |
| **J2** | Syscall-deps → .NET | [`cargo_tests/cd_tokio`](../cargo_tests/cd_tokio) | a plain `tokio` dep runs I/O-driven async on the PAL via **auto-applied overlays**. |
| **J3** | Consumed from C# | [`cargo_tests/cd_interop`](../cargo_tests/cd_interop) | a Rust `cdylib` → `.dll` → `<Reference>`, all four marshalling categories called from C#. |
| **J4** | North-star | (cross-repo) | a **real, dependency-using production library** (serde/chrono/uuid data-models) was imported by C# and ran its pagination logic as .NET CIL, returning the correct result. |

**J4** is the capability yardstick: a real production Rust module — *not* a toy — built with deep
third-party dependencies, consumed from C# and executing its business logic on .NET. It ran via a
transient FFI wrapper over a read-only cross-repo mount (leak-safe). It exercises every layer at once:
correct codegen, real std on the PAL, the overlay registry, and the C# consumption path. Passing it is
the strongest single signal that the stack works end-to-end on a non-contrived workload.

---

## 8. Where to go next

- **The C# consumer guide** — `.csproj`, the C# program, marshalling tiers:
  [docs/INTEROP_CSHARP.md](INTEROP_CSHARP.md).
- **The overlay recipe** — add a dep that needs a source edit:
  [dotnet_overlays/README.md](../dotnet_overlays/README.md).
- **`cargo-dotnet` flags & mechanics** (mount model, the Docker/native seam, the shared pipeline core):
  [feasibility/README.md](../feasibility/README.md).
- **The libc/POSIX-over-.NET design** (the categorized map, the fd-table + errno spine):
  [docs/LIBC_SHIM_SCOPE.md](LIBC_SHIM_SCOPE.md).
- **The `std::os::unix` plan + leaky-bits ledger:** [docs/STD_OS_UNIX_PLAN.md](STD_OS_UNIX_PLAN.md).
- **The backend itself** (the CIL-trees IR, the V1→V2 split, Rust→.NET mapping gotchas):
  [docs/ARCHITECTURE.md](ARCHITECTURE.md).
- **The full Rust↔.NET completeness map:** [docs/TRANSLATION_STATUS.md](TRANSLATION_STATUS.md).
