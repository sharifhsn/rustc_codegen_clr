# Calling Rust from C# via `cargo dotnet` (Journey 3)

This is the consumer guide for **Journey 3**: build a Rust *library* with `cargo dotnet`, get a
C#-referenceable .NET assembly, and call the exported Rust functions from a real C# project.

The worked example lives at [`cargo_tests/cd_interop/`](../cargo_tests/cd_interop) — a Rust `cdylib`
(`rustlib/`) plus a C# console app (`csharp/`) that references it and asserts the results match Rust.

## TL;DR

```bash
# 1. Build the Rust library -> a .NET assembly + a referenceable .dll copy.
cargo dotnet build path/to/rustlib            # emits target/x86_64-unknown-dotnet/release/cd_interop.dll

# 2. Copy that .dll next to your C# project (or point HintPath straight at it).
cp path/to/rustlib/target/x86_64-unknown-dotnet/release/cd_interop.dll path/to/csharp/

# 3. Reference + call it from C#.
dotnet run --project path/to/csharp
```

## 1. Write the Rust library

Make a `cdylib` crate. Export functions with `#[no_mangle] pub extern "C"`:

```toml
# Cargo.toml
[package]
name = "cd_interop"
edition = "2021"            # 2021, not 2024 (the pinned nightly's default for this flow)
version = "0.1.0"

[lib]
crate-type = ["cdylib"]     # tells cargo dotnet to emit a referenceable assembly (not an exe)

[workspace]                 # a bare line, IF the crate lives under another workspace's root
```

```rust
// src/lib.rs
#[no_mangle]
pub extern "C" fn rust_add(a: i32, b: i32) -> i32 { a + b }
```

Why `#[no_mangle]`: it gives each export a **stable, un-mangled name** AND (via the backend) marks it
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

A bare assembly `<Reference>` with a `<HintPath>` is the minimal thing both the C# *compiler* (it needs
the `.assembly extern` BCL identities the lib emits) and the *runtime* accept — no `ProjectReference`,
no NuGet. (`ProjectReference` targets a `.csproj` built by MSBuild/Roslyn; the Rust lib is built by
`cargo dotnet`, so there is no buildable `.csproj` to point at. NuGet adds packaging ceremony with no
benefit for a local consumer — defer it.)

```xml
<!-- cd_interop_cs.csproj -->
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

## What is and isn't verified here

**Tier 1 (verified end-to-end on the real dotnet PAL through `cargo dotnet`):** primitives, UTF-8
`(ptr, len)` strings, a de-mangled `#[repr(C)]` struct value-type (with synthesized ctor/getters), and
an inbound slice. These need only `core`/`alloc` + arithmetic + the synthesized accessors — no
`mycorrhiza`, no panic, no I/O — so they pull only CoreLib extern refs.

**Tier 2 (proven on the SURROGATE target only, not yet through this real-PAL `cargo dotnet` flow):**
returning a managed `System.String` directly (`mycorrhiza::system::MString`) and a Rust-raises-a-
.NET-exception `Result` (`rustc_clr_interop_throw`). These pull `mycorrhiza` and the throw intrinsic.
See [`cargo_tests/rust_export/`](../cargo_tests/rust_export) for the full surrogate surface.
