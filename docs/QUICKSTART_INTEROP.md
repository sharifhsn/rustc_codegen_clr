# Interop quickstart — Rust ⇄ .NET

A copy-paste guide to the two things people actually want: **call .NET from Rust**, and **call Rust
from C#**. Everything here is a managed↔managed call (Rust is compiled to CIL by this backend), so
there is no P/Invoke, no marshalling attributes, and no `unsafe` unless you ask for it.

> Deeper references: [CARGO_DOTNET.md](CARGO_DOTNET.md) (the tool), [INTEROP_CSHARP.md](INTEROP_CSHARP.md)
> (C#-consumes-Rust details), [TRANSLATION_STATUS.md](TRANSLATION_STATUS.md) (what maps to what and
> the ceilings), [ARCHITECTURE.md](ARCHITECTURE.md) (why). Every snippet below has a runnable twin
> under `cargo_tests/` — named in each section.

## 0. One-time setup

```bash
curl -fsSL https://github.com/sharifhsn/rustc_codegen_clr/releases/download/rust-dotnet-v0.0.1/install.sh | sh
cargo dotnet doctor
```

On Windows, use the PowerShell installer from the main quickstart. You need the .NET 10 SDK and
rustup; the SDK records the exact toolchain without changing your global rustup default. Build or
run a crate with `cargo dotnet build` / `cargo dotnet run` instead of plain `cargo`.

---

## 1. Call .NET from Rust (the `mycorrhiza` crate)

Add the dependency:

```toml
[dependencies]
mycorrhiza = { path = "…/mycorrhiza" }   # or a git/version dep once published
```

### 1a. Use the .NET generic collections — like `std`

`mycorrhiza::collections` ships real managed `List`/`Dictionary`/`HashSet`/`Stack`/`Queue`, used
exactly like their Rust cousins. No knowledge of the CLR generic-interop machinery required.

```rust
use mycorrhiza::collections::{List, Dictionary};

let mut xs = List::<i32>::new();
xs.push(10);
xs.push(20);
assert_eq!(xs.len(), 2);
assert_eq!(xs.get(0), Some(10));           // bounds-checked → Option
for x in xs.iter() { /* … */ }

let mut m = Dictionary::<i32, i64>::new();
m.insert(1, 100);
assert_eq!(m.get(1), Some(100));           // never throws on a missing key
```

`T` must be a type that crosses the boundary: a .NET primitive, a `#[repr(C)]` value-type struct, or a
managed handle. Runnable: `cargo_tests/cd_collections`.

### 1b. Call arbitrary BCL methods

The `mycorrhiza::system` / generated `bindings` surface wraps thousands of BCL methods (Console, Math,
StringBuilder, String, …):

```rust
use mycorrhiza::system::console::Console;
Console::writeln_f64(mycorrhiza::System::Math::sqrt(144.0));   // 12
```

Runnable: `cargo_tests/interop_method_sample`. For a generic BCL method not yet wrapped, drop to the
`dotnet_generic!` / `dotnet_generic_impl!` macros (`cargo_tests/cd_generic`) — but for the common
collections, prefer §1a.

### 1c. Define a managed .NET class from a Rust struct

`#[dotnet_class]` turns a Rust struct into a real .NET class with a field-initializing constructor and
per-field accessors — callable from C#:

```rust
use dotnet_macros::dotnet_class;

#[dotnet_class]
pub struct Counter {
    value: i32,
    step: i64,
}
```

From C#: `new Counter(5, 100)`, `c.read_value()`, `c.read_step()`. Runnable: `cargo_tests/cd_typedef`.

---

## 2. Call Rust from C#

Your Rust crate becomes a **.NET class library**. Its `#[unsafe(no_mangle)] pub extern "C"` functions are
`public static` methods on `MainModule`; a C# project references the produced `.dll` and calls them as
ordinary managed methods.

### 2a. A plain Rust library consumed from C#

```toml
# rustlib/Cargo.toml
[lib]
crate-type = ["cdylib"]
```

```rust
// rustlib/src/lib.rs
#[unsafe(no_mangle)]
pub extern "C" fn rust_add(a: i32, b: i32) -> i32 { a + b }
```

```xml
<!-- csharp/App.csproj — auto-builds the Rust crate + references its assembly -->
<Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets"
        Condition="Exists('$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets')" />
<ItemGroup>
  <RustCrate Include="../rustlib" />
</ItemGroup>
```

```csharp
// csharp/Program.cs
Console.WriteLine(MainModule.rust_add(2, 3));   // 5
```

`dotnet run` builds the Rust crate *and* the C#. Strings cross as UTF-8 `(ptr, len)`; `#[repr(C)]`
structs cross as value types with a synthesized ctor. Runnable: `cargo_tests/cd_interop`.

### 2b. Generic Rust containers from C# — batteries included

Want a `RustVec<T>` (a Rust-owned list) from C#? Don't hand-write it. In the Rust crate, one line:

```rust
mycorrhiza::export_rust_containers!();   // emits the size-erased core into your assembly
```

In the C# project, opt in — the wrappers are shipped and auto-compiled:

```xml
<PropertyGroup>
  <UseRustDotnetContainers>true</UseRustDotnetContainers>
</PropertyGroup>
<Import Project="$(HOME)/.cargo-dotnet/msbuild/RustDotnet.targets" … />
<ItemGroup><RustCrate Include="../rustlib" /></ItemGroup>
```

```csharp
using RustDotnet;

using var xs = RustVec<int>.New();          // T : unmanaged — near-zero-cost, raw bytes
xs.Push(42);
int v = xs.Get(0);

using var objs = RustBoxVec<string>.New();  // ANY managed T — GCHandle-boxed, keeps reference identity
objs.Push("hello");
```

One Rust monomorphization backs `RustVec<T>` for every `T` you instantiate. Runnable:
`cargo_tests/cd_containers` (Rust side: one macro line; C# side: no hand-written interop at all).

### 2c. Export a Rust function with `#[dotnet_export]` — no `(ptr, len)` dance

§2a's hand-written export makes strings cross as a UTF-8 `(ptr, len)` pair — C# has to pin a byte
buffer, guess an output size, and re-decode (see `cargo_tests/cd_interop`). `#[dotnet_export]` removes
all of that: write an ordinary Rust fn with `&str`/`String`/primitive parameters and a `&str`/`String`/
primitive return, and C# calls it as a plain typed method.

```rust
use dotnet_macros::dotnet_export;

#[dotnet_export]
pub fn greet(name: &str) -> String {
    format!("Hello, {name}, from Rust!")
}
```

```csharp
string g = MainModule.greet("World");   // -> "Hello, World, from Rust!"
```

The macro leaves your function untouched (still callable from Rust) and emits a hidden
`#[unsafe(no_mangle)] extern "C"` shim that marshals the managed seam: `&str`/`String` cross as a real managed
`System.String` (so C# sees `string`, **not** a pointer pair — no buffer, no free, no re-decode), and
the numeric/`bool` primitives pass through unchanged. **No C#-side glue is needed at all** — the shim
already presents a clean `string`/`int`/`double`/`bool` signature on `MainModule`.

Supported today includes the integer/float primitives, `bool`, `&str`, `String`, primitive
`Option<T>`/`Vec<T>`, concrete delegates, and enums registered as shown below. Unsupported shapes
produce a **clear compile error** (marshalling is never faked). The consuming `cdylib` depends on
`mycorrhiza` + `dotnet_macros`. Runnable: `cargo_tests/cd_export` and
`cargo_tests/cd_export_ergonomics`.

### 2d. Export a real CLR enum from Rust

`#[dotnet_enum]` keeps the Rust API idiomatic while emitting genuine CLR enum metadata: C# sees
`Type.IsEnum == true`, the requested underlying integer type, literal named fields, and can use
normal casts, reflection, and `switch` expressions.

```rust
use dotnet_macros::{dotnet_enum, dotnet_export};

#[dotnet_enum(name = "Example.Status")]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Status {
    Pending = 0,
    Ready = 4,
    Done, // 5
}

#[dotnet_export(enums(Status))]
pub fn roundtrip_status(status: Status) -> Status { status }
```

```csharp
Status status = MainModule.roundtrip_status(Status.Ready);
string label = status switch {
    Status.Pending => "waiting",
    Status.Ready => "ready",
    Status.Done => "done",
    _ => "unknown",
};
```

The enum must be fieldless and use `#[repr(i8/u8/i16/u16/i32/u32/i64/u64)]`. Discriminants may be
integer literals or implicit increments. List each enum crossing an exported function in that
function's `enums(...)` argument; unknown inbound numeric values are rejected before Rust could
construct an invalid enum discriminant. Runnable proof: `cargo_tests/cd_export_ergonomics`.

---

## 3. What crosses the boundary

| You have (Rust) | C# sees | Notes |
|---|---|---|
| `i8..i128`, `u8..u128`, `f32/f64`, `bool` | the matching primitive | direct |
| `#[repr(C)] struct` of the above | a value-type `struct` | de-mangled name for `cdylib` exports; synthesized ctor/getters |
| `&str` / `String` (as `*const u8`, `usize`) | `byte*` + `nuint` | UTF-8; nothing crosses ownership (the hand-written §2a form) |
| `&str` / `String` in a `#[dotnet_export] fn` | a managed `string` | §2c — no `(ptr, len)`, no glue |
| `#[dotnet_class] struct` | a managed class | §1c |
| `#[dotnet_enum] enum` | a genuine managed enum | §2d — reflection, literals, typed exports, `switch` |
| `mycorrhiza::collections::*` | the real BCL collection | §1a |
| a C# `T` in `RustVec<T>`/`RustBoxVec<T>` | a Rust-owned list | §2b |

The genuinely-hard cases (a transparent zero-cost open generic overlapping a managed reference; static
borrow-safety across the seam; arbitrary novel inline asm) are documented in
[TRANSLATION_STATUS.md §7](TRANSLATION_STATUS.md) — each with a working bridge for everything but the
specific guarantee it gives up.
