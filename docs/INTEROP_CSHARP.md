# Calling Rust from C# via `cargo dotnet` (Journey 3)

This is the consumer guide for **Journey 3**: build a Rust *library* with `cargo dotnet`, get a
C#-referenceable .NET assembly, and call the exported Rust functions from a real C# project.

The worked example lives at [`cargo_tests/cd_interop/`](../cargo_tests/cd_interop) — a Rust `cdylib`
(`rustlib/`) plus a C# console app (`csharp/`) that references it and asserts the results match Rust.

## TL;DR

**Recommended — auto-build (`RustDotnet.targets`): one `dotnet run`, zero manual steps.**
Import the integration and declare the Rust crate; `dotnet build`/`dotnet run` ALSO runs
`cargo dotnet build` and references the produced assembly for you (incremental):

```xml
<!-- in your .csproj -->
<Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
        Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
<Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
        Condition="!Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets') and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />
<ItemGroup>
  <RustCrate Include="../path/to/rustlib" />
</ItemGroup>
```

```bash
dotnet run --project path/to/csharp   # builds the Rust crate + references it + runs. Done.
```

**Manual (fallback) — bare `<Reference>` + `<HintPath>`** (use when you don't want the
auto-build target, e.g. you ship a pre-built `.dll`):

```bash
# 1. Build the Rust library -> a .NET assembly + a referenceable .dll copy.
cargo dotnet build path/to/rustlib            # emits target/x86_64-unknown-dotnet/release/cd_interop.dll

# 2. Copy that .dll next to your C# project (or point HintPath straight at it).
cp path/to/rustlib/target/x86_64-unknown-dotnet/release/cd_interop.dll path/to/csharp/

# 3. Reference + call it from C#.
dotnet run --project path/to/csharp
```

**Distribution — NuGet (`cargo dotnet pack`):** `cargo dotnet pack path/to/rustlib`
produces a `.nupkg` you can `<PackageReference>` from a local feed. See
[§4 NuGet packaging](#4-nuget-packaging-cargo-dotnet-pack).

## 1. Write the Rust library

Make a `cdylib` crate. Export functions with `#[unsafe(no_mangle)] pub extern "C"`:

```toml
# Cargo.toml
[package]
name = "cd_interop"
edition = "2024"
version = "0.1.0"

[lib]
crate-type = ["cdylib"]     # tells cargo dotnet to emit a referenceable assembly (not an exe)

[workspace]                 # a bare line, IF the crate lives under another workspace's root
```

```rust
// src/lib.rs
#[unsafe(no_mangle)]
pub extern "C" fn rust_add(a: i32, b: i32) -> i32 { a + b }
```

Why `#[unsafe(no_mangle)]`: it gives each export a **stable, un-mangled name** AND (via the backend) marks it
`Access::Extern`, which makes it a dead-code-elimination *root*. A library has no entrypoint to keep
its API alive, so without this the exports would be stripped. No `main` also means the `std` runtime
tail (`lang_start`) is unreachable and is DCE'd — which is why an I/O-free, panic-free library emits
cleanly.

### Marshalling conventions (verified on the real dotnet PAL)

| Category | Rust signature | C# side |
|----------|----------------|---------|
| **Primitives** | `pub extern "C" fn rust_add(a: i32, b: i32) -> i32` | `int MainModule.rust_add(int, int)` |
| **Strings** | `(name_ptr: *const u8, name_len: usize, out_ptr: *mut u8, out_cap: usize) -> usize` | `fixed (byte* …)` UTF-8 `(ptr, len)` in + caller out-buffer |
| **Struct** | `#[repr(C)] pub struct Point { pub x: i32, pub y: i32 }` + `fn point_sum(p: Point) -> i32` | `new cd_interop.Point(2, 3)`, `p.get_x()` |
| **Slice / Vec** | `(ptr: *const i32, len: usize) -> i32` | `fixed (int* …)` over a C# `int[]` |

For the higher-level typed seam, prefer `#[dotnet_export]`: strings, primitive optionals/vectors,
delegates, and `#[dotnet_enum]` types are marshalled automatically. A Rust-defined fieldless enum
with an explicit integer `#[repr]` becomes a genuine CLR enum and crosses typed exports via
`#[dotnet_export(enums(MyEnum))]`; see [the interop quickstart](QUICKSTART_INTEROP.md#2d-export-a-real-clr-enum-from-rust).

Strings and slices cross the boundary as **UTF-8 / element `(ptr, len)` pairs** (thin pointers,
directly C#-usable with `fixed`). No Rust allocation crosses the boundary, so there is nothing to free
across it. For the outbound string direction, the caller passes an output buffer Rust fills.

The `#[repr(C)]` struct lowers to a CIL value-type. Because it is a *local, non-generic* type of a
`cdylib`/`dylib`/`staticlib` crate, de-mangling (`stable_adt_name`) names it `cd_interop.Point` — the
clean, build-stable name C# references directly — and the backend synthesizes a public `.ctor` plus
per-field `get_<field>` getters.

## 2. Build it with `cargo dotnet`

```bash
cargo dotnet build path/to/rustlib
```

(Requires Docker + the `rcc-dev` image — see [feasibility/README.md](../feasibility/README.md).)

`cargo dotnet` detects the `cdylib` crate-type from cargo's JSON message stream, builds the library
under the dotnet PAL target (build-std with `panic_unwind`), and the cilly linker writes a referenceable
.NET PE to `target/x86_64-unknown-dotnet/<profile>/lib<crate>.so`. `cargo dotnet` then copies that PE
to **`<crate>.dll`** beside it (a pure file copy — the assembly identity is `<crate>` regardless of the
`.so` filename). The build summary reports both paths.

`cargo dotnet run` on a library prints a clear "this is a library, reference the .dll" message and exits
0 — a library has no entrypoint to run.

## 3. Reference + call it from C#

There are three ways to wire the Rust assembly into a C# project, from most to least automated:

| Path | What you write | When |
|------|----------------|------|
| **Auto-build (recommended)** | `<Import RustDotnet.targets>` + `<RustCrate Include="../rustlib"/>` | you control the Rust source tree and want one-command builds |
| **Manual `<Reference>` (fallback)** | a bare `<Reference><HintPath>` | you ship a pre-built `.dll` and don't want a build step |
| **NuGet `<PackageReference>`** | `cargo dotnet pack` → `<PackageReference>` | you DISTRIBUTE a pre-built Rust .NET assembly (see [§4](#4-nuget-packaging-cargo-dotnet-pack)) |

### 3a. Auto-build (recommended): `RustDotnet.targets`

`dotnet build`/`dotnet run` itself runs `cargo dotnet build` on each declared `<RustCrate>` and
references the produced assembly — **zero manual `cargo dotnet`, zero manual `.dll` copy, zero
hand-written `<Reference>`**. It is incremental (the Rust rebuild is skipped when the `.dll` is newer
than the crate's sources) and uses the same MSBuild path in plain `dotnet build`, IDEs, and CI.

`RustDotnet.targets` ships in the repo (`msbuild/RustDotnet.targets`) and — for repo-independent use —
`cargo dotnet setup` copies it into `$CARGO_DOTNET_HOME/msbuild/` (default `~/.cargo-dotnet/msbuild/`).
A project imports the installed copy (so the same `.csproj` works anywhere) with a repo-relative
fallback:

```xml
<!-- cd_interop_cs.csproj — auto-build -->
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>   <!-- byte*/int* (ptr, len) marshalling -->
  </PropertyGroup>

  <Import Project="$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets"
          Condition="'$(CARGO_DOTNET_HOME)'!='' and Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets')" />
  <Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
          Condition="!Exists('$(CARGO_DOTNET_HOME)/msbuild/RustDotnet.targets') and Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />

  <ItemGroup>
    <RustCrate Include="../rustlib" />
  </ItemGroup>
</Project>
```

`Include` is the path to the Rust crate dir (relative to the `.csproj`) — the only mandatory input.
The assembly name (and `.dll` basename) defaults to the crate's `[package] name` parsed from its
`Cargo.toml` (so the dir name may differ from the crate name, as here: dir `rustlib`, crate
`cd_interop`). Optional per-item metadata, all defaulted: `Configuration="Release|Debug"`,
`CrateName="..."` (override the assembly name), `Clean="true"` (force a clean rebuild),
`Private="true|false"` (copy the Rust `.dll` into the consumer `bin/`; default `true`, needed for
`dotnet run` to resolve it at runtime). Project-level property overrides (see
`msbuild/RustDotnet.props`): `<CargoDotnet>` (explicit tool path), `<RustDotnetForceBuild>true`,
`<RustDotnetToolPath>`, `<RustDotnetDotnetRoot>`.

If `cargo dotnet` isn't installed, the build fails with an actionable error pointing at
`cargo dotnet setup`. The full mechanics live in `msbuild/README.md`.

### 3b. Manual (fallback): a bare `<Reference>` + `<HintPath>`

A bare assembly `<Reference>` with a `<HintPath>` is the minimal thing both the C# *compiler* (it needs
the `.assembly extern` BCL identities the lib emits) and the *runtime* accept — no `ProjectReference`
(it targets a `.csproj` built by MSBuild/Roslyn; the Rust lib is built by `cargo dotnet`, so there is
no buildable `.csproj` to point at). You build the lib yourself and copy the `.dll` next to the project
(see the [Manual TL;DR](#tldr) above).

```xml
<!-- cd_interop_cs.csproj — manual -->
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>   <!-- byte*/int* (ptr, len) marshalling -->
  </PropertyGroup>
  <ItemGroup>
    <Reference Include="cd_interop"><HintPath>cd_interop.dll</HintPath></Reference>
  </ItemGroup>
</Project>
```

### The C# program (identical for all three paths)

```csharp
// Program.cs
using System.Text;

int sum = MainModule.rust_add(2, 3);                       // primitives: == 5

byte[] utf8 = Encoding.UTF8.GetBytes("World");             // strings: UTF-8 (ptr, len)
unsafe {
    fixed (byte* np = utf8) {
        byte[] outbuf = new byte[256];
        fixed (byte* op = outbuf) {
            nuint n = MainModule.greet(np, (nuint)utf8.Length, op, (nuint)outbuf.Length);
            string greeting = Encoding.UTF8.GetString(outbuf, 0, (int)n);   // "Hello, World, from Rust!"
        }
    }
}

cd_interop.Point p = new cd_interop.Point(2, 3);          // struct: de-mangled value-type
int s = MainModule.point_sum(p);                          // == 5

int[] nums = { 1, 2, 3, 4 };
unsafe { fixed (int* sp = nums) {
    int t = MainModule.sum_slice(sp, (nuint)nums.Length); // slice/Vec: == 10
}}
```

Exported functions are `public static` methods on `MainModule`. De-mangled value-types live under their
`Crate.Type` name (`cd_interop.Point`). These are ordinary **managed calls** — no P/Invoke, no
`[DllImport]`, no marshalling attributes, no reflection — because the Rust was compiled to managed CIL.

The Rust `.dll` needs no `runtimeconfig.json` of its own — it is a plain referenced managed PE; the
consuming `Exe`'s build emits the host runtime config.

## 4. NuGet packaging (`cargo dotnet pack`)

For DISTRIBUTING a pre-built Rust .NET assembly (rather than building it from source in the consumer),
`cargo dotnet pack` produces a NuGet `.nupkg`:

```bash
cargo dotnet pack path/to/rustlib            # -> path/to/rustlib/target/nupkg/<crate>.<ver>.nupkg
#   [--release|--debug] [--id NAME] [--version VER] [--out DIR]
```

For a package whose Rust source is publicly retrievable, embed an immutable Source Link template:

```bash
cargo dotnet pack path/to/rustlib --validate --source-link-url \
  'https://raw.githubusercontent.com/OWNER/REPO/COMMIT/*'
```

The PDB uses checkout-independent `/_/consumer/*` document names and maps only that root. It does
not claim that dependency, SDK, or rust-sysroot sources belong to the same repository.

### C# API documentation

Put ordinary Rust `///` comments on functions annotated with `#[dotnet_export]`. The build converts
those comments into an ECMA-334 XML documentation sidecar named after the CLR assembly. A validated
package contains both files under the same target framework, for example:

```text
lib/net8.0/cd_export.dll
lib/net8.0/cd_export.xml
```

That adjacent, identity-matched filename is the standard NuGet/MSBuild discovery contract used for
IntelliSense and compiler API help. `cargo dotnet pack --validate` rejects a missing sidecar. The
current generator documents exported `MainModule` functions; generated classes, DTO properties, and
other synthesized members remain future coverage. `feasibility/api_docs_acceptance.sh` checks the
package entry, XML well-formedness, a concrete member ID, and its source summary without publishing.

It builds the crate via the same `cargo dotnet build` path, reads the id+version from
`cargo metadata`, and hand-assembles a valid OPC package containing `lib/net8.0/<crate>.dll` (plus a
`build/<crate>.targets` for copy-local and a `.nuspec`). A C# project consumes it from a local feed:

```xml
<!-- cd_interop_nupkg.csproj -->
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <RestoreSources>$(RestoreSources);../rustlib/target/nupkg</RestoreSources>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="cd_interop" Version="0.1.0" />
  </ItemGroup>
</Project>
```

```bash
dotnet run --project path/to/csharp_nupkg    # restores the local .nupkg, references it, runs
```

The worked example is [`cargo_tests/cd_interop/csharp_nupkg/`](../cargo_tests/cd_interop/csharp_nupkg)
(verified: all six marshalling checks pass, exit 0). This local-feed path is the in-repo/dev-loop
convenience; the package is a genuine `.nupkg` and works identically from a **real** feed — verified by
publishing `cd_interop` to a private GitHub Packages NuGet feed (`dotnet nuget push .. --source
github-sharifhsn`) and consuming it from a *fresh, out-of-tree* `dotnet new console` project via a plain
`<PackageReference>`, no local-feed fallback in the csproj (`RepositoryUrl` in `Cargo.toml` is required
— GitHub Packages rejects a push whose `.nuspec` has no `<repository>` the pushing account can write
to). `nuget.org` publish itself is still out of scope (a name-squatting concern for a solo-maintainer
project, not a mechanism limitation).

> **Cache footgun.** NuGet pins `<crate> <version>` in `~/.nuget/packages`. After changing the Rust and
> re-packing the **same** version, the **stale cached copy** is served. Bump `--version`, or clear the
> cache: `dotnet nuget locals global-packages --clear`. This is a reason the auto-build `.targets`
> (§3a) stays the primary recommendation for a source-controlled crate — it has no cache.

## What is and isn't verified here

**Tier 1 (verified end-to-end on the real dotnet PAL through `cargo dotnet`):** primitives, UTF-8
`(ptr, len)` strings, a de-mangled `#[repr(C)]` struct value-type (with synthesized ctor/getters), and
an inbound slice. These need only `core`/`alloc` + arithmetic + the synthesized accessors — no
`mycorrhiza`, no panic, no I/O — so they pull only CoreLib extern refs.

**Tier 2 (proven on the SURROGATE target only, not yet through this real-PAL `cargo dotnet` flow):**
returning a managed `System.String` directly (`mycorrhiza::system::MString`) and a Rust-raises-a-
.NET-exception `Result` (`rustc_clr_interop_throw`). These pull `mycorrhiza` and the throw intrinsic.
See [`cargo_tests/rust_export/`](../cargo_tests/rust_export) for the full surrogate surface.
