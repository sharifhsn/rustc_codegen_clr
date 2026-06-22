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
> `docker`). A future **native** (non-Docker) driver slots into the same switch: it runs the *same*
> pipeline core against the host's real repo path and `command -v dotnet ilasm`, with the UX and
> pipeline unchanged. It is not yet implemented (`CARGO_DOTNET_BACKEND=native` errors today) — it is a
> later packaging concern, and the front-end + core are already structured for it. Full mechanics:
> [feasibility/README.md](../feasibility/README.md).

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

A C# project references it with a bare assembly `<Reference>` + `<HintPath>` (no `ProjectReference`, no
NuGet). Exported `#[no_mangle] pub extern "C"` functions are `public static` methods on `MainModule`;
de-mangled `#[repr(C)]` structs appear under their clean `Crate.Type` name with a synthesized ctor +
per-field getters.

```xml
<ItemGroup>
  <Reference Include="cd_interop"><HintPath>cd_interop.dll</HintPath></Reference>
</ItemGroup>
```

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
