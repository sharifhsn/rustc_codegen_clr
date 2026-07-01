# Interop cookbook — recipe per task

Copy-paste answers to "how do I *do X* across the Rust ⇄ .NET seam." Every recipe here reflects what
**actually works today** — each one has a runnable twin under `cargo_tests/` (named in the recipe), and
the checks in those crates all pass. Where a task is only *partially* covered, the recipe says so and
shows the honest path that works rather than a nicer one that doesn't.

> New here? Start with [QUICKSTART_INTEROP.md](QUICKSTART_INTEROP.md) (the two-direction overview) and
> [ERGONOMICS_HANDOFF.md](ERGONOMICS_HANDOFF.md) (build/verify recipe). For the C#-consumes-Rust wiring
> details see [INTEROP_CSHARP.md](INTEROP_CSHARP.md).

## Conventions used below

- **Rust-calls-.NET** recipes go in a `cargo_tests/cd_<name>` binary crate and run with
  `CARGO_DOTNET_BACKEND=native cargo dotnet run`.
- **C#-consumes-Rust** recipes have a Rust `cdylib` (`crate-type = ["cdylib"]`) plus a C# project that
  imports `RustDotnet.targets`; you run the C# side with `dotnet run -c Release`.
- The everyday imports are `use mycorrhiza::prelude::*;` — it pulls the collections, the BCL wrappers,
  the delegates, the Task bridge, `DotNetString`, and the error/optional bridges into scope like `std`.
- **After any change that only touched a string literal, `rm -rf target` first** (stale-artifact
  footgun — a native run can otherwise reuse the old `mycorrhiza` build and show the old behavior).

---

## 1. Use a .NET collection from Rust

`mycorrhiza::collections` ships real managed `List` / `Dictionary` / `HashSet` / `Stack` / `Queue`
(and `SortedDictionary` / `SortedSet` / `LinkedList` / `PriorityQueue` / the `Concurrent*` trio),
used exactly like their Rust cousins — no `get_Item`, no `callvirt`, no assembly strings.

```rust
use mycorrhiza::prelude::*;

let mut xs = List::<i32>::new();
xs.push(10);
xs.push(20);
assert_eq!(xs.len(), 2);
assert_eq!(xs.get(0), Some(10));       // bounds-checked → Option, never throws
assert_eq!(xs.get(5), None);
xs.sort();
let v: Vec<i32> = xs.to_vec();

let mut m = Dictionary::<i32, i64>::new();
m.insert(1, 100);
m.insert(1, 111);                      // overwrite (never throws)
assert_eq!(m.get(1), Some(111));
assert_eq!(m.get(99), None);           // absent → None

let mut set = HashSet::<i32>::new();
assert!(set.insert(5));                // true = newly added
assert!(!set.insert(5));               // false = already present
```

`T` must be a type that crosses the boundary: a .NET primitive, a `#[repr(C)]` value-type struct, or a
managed handle. **Runnable:** `cargo_tests/cd_collections` (38 checks).

### Iterate one with `for` (the enumerator bridge)

`for x in &collection` drives the managed `IEnumerator<T>` (`GetEnumerator`/`MoveNext`/`get_Current`),
and `.collect()`/`.extend()` work too:

```rust
use mycorrhiza::prelude::*;

let xs: List<i32> = vec![10, 20, 30].into();      // From<Vec<T>>
let mut sum = 0;
for x in &xs { sum += x; }                        // by-ref: the list is not consumed
assert_eq!(sum, 60);

let doubled: Vec<i32> = (&xs).into_iter().map(|v| v * 2).collect();  // Iterator adapters
let l: List<i32>     = (0..5).collect();          // FromIterator
let set: HashSet<i32> = vec![1, 2, 2, 3].into_iter().collect();      // dedups → {1,2,3}
```

`Stack` enumerates LIFO (top first), `Queue` FIFO (front first). **Runnable:** `cargo_tests/cd_enumerate`.

---

## 2. Parse JSON

`mycorrhiza::bcl::json` bridges `System.Text.Json` as a small serde-ish read-only DOM — `Json::parse`,
`.get("prop")`, `.index(i)`, typed scalar reads that return `Option` (never panic), and
`.to_json_string()`. It is backed by genuine managed `System.Text.Json` objects.

```rust
use mycorrhiza::bcl::json::{Json, Kind};

let src = r#"{ "name": "ada", "age": 36, "tags": ["x", "y"], "addr": { "city": "London" } }"#;
let doc = Json::parse(src).expect("valid json");   // None on malformed input

assert_eq!(doc.kind(), Kind::Object);
assert_eq!(doc.get("name").and_then(|n| n.as_str()).as_deref(), Some("ada"));
assert_eq!(doc.get("age").and_then(|n| n.as_i64()), Some(36));
assert_eq!(doc.get("missing").is_none(), true);    // absent property → None
assert_eq!(doc.get("age").and_then(|n| n.as_str()), None);  // wrong-type read → None, not a panic

let tags = doc.get("tags").unwrap();
assert_eq!(tags.len(), 2);
assert_eq!(tags.index(0).and_then(|n| n.as_str()).as_deref(), Some("x"));

let city = doc.get("addr").and_then(|a| a.get("city"));    // nested navigation
assert_eq!(city.and_then(|n| n.as_str()).as_deref(), Some("London"));

// serialize (compact, no insignificant whitespace)
let compact = Json::parse(r#"{ "a" : 1 , "b" : [2, 3] }"#).unwrap();
assert_eq!(compact.to_json_string().as_str(), r#"{"a":1,"b":[2,3]}"#);
```

Scope today: read + navigate + serialize a parsed tree. A JSON `null` reads back as `None` from
`get`/`parse`. **Runnable:** `cargo_tests/cd_json`.

> Prefer `serde_json`? It also compiles and runs on the .NET target (it's pure Rust — no interop
> needed). Reach for this `Json` bridge when you want to hand a `System.Text.Json` node to/from other
> .NET code, or to avoid pulling `serde` into a small crate.

---

## 3. Read a file (and other std I/O)

There is no special "file interop" API — **plain `std::fs` / `std::io` just work**, because they run
on the .NET Platform Abstraction Layer (the dotnet PAL implements the syscalls over the BCL). Write
ordinary Rust:

```rust
use std::io::{Read, Write};

fn main() -> std::io::Result<()> {
    std::fs::write("out.txt", b"hello pal fs")?;
    let s = std::fs::read_to_string("out.txt")?;
    assert_eq!(s, "hello pal fs");

    let meta = std::fs::metadata("out.txt")?;
    println!("{} bytes, is_file={}", meta.len(), meta.is_file());

    let mut f = std::fs::OpenOptions::new().append(true).open("out.txt")?;
    f.write_all(b"!more")?;

    std::fs::create_dir_all("subdir")?;
    let count = std::fs::read_dir("subdir")?.count();
    std::fs::remove_file("out.txt")?;
    Ok(())
}
```

`create_dir`, `read_dir`, `copy`, `rename`, `set_len`, `canonicalize`, `set_permissions`, and
`std::process::Command` (spawn / capture output) are all wired. **Runnable:** `cargo_tests/pal_fs`
(files/dirs), `cargo_tests/pal_fsmeta` (metadata). The one known gap is `hard_link`.

---

## 4. HTTP GET

There is **no shipped idiomatic HTTP client wrapper yet** (no `HttpClient` face in `mycorrhiza::bcl`).
Two honest paths that work today:

**(a) Speak HTTP over `std::net::TcpStream`.** `std::net` runs on the PAL (proven by `cargo_tests/pal_net`),
so a minimal GET is plain Rust:

```rust
use std::io::{Read, Write};
use std::net::TcpStream;

fn http_get(host: &str, path: &str) -> std::io::Result<String> {
    let mut s = TcpStream::connect((host, 80))?;
    write!(s, "GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n")?;
    let mut resp = String::new();
    s.read_to_string(&mut resp)?;
    Ok(resp)   // status line + headers + body; split on "\r\n\r\n"
}
```

**(b) A pure-Rust HTTP crate** compiled to .NET. Since the target is a real `std` target, an `std`-only
HTTP client crate compiles and runs unchanged. (Anything reaching for `openssl`/`ring`/raw libc socket
crates is riskier — prefer the `std::net`-only or rustls path.)

**Not yet available:** a `mycorrhiza::bcl::http` wrapper over `System.Net.Http.HttpClient` (this is the
natural next Theme-2 addition, and would layer on the Task bridge in §7 for `async` GETs). Don't
document a `HttpClient::get(...)` face — it doesn't exist.

---

## 5. Use a NuGet library

This is directional — be precise about which way the package flows.

**Distribute your Rust crate *as* a NuGet package (Rust → C#).** `cargo dotnet pack` turns a Rust
`cdylib` into a `.nupkg` a C# project consumes with an ordinary `<PackageReference>`:

```bash
cargo dotnet pack path/to/rustlib          # → path/to/rustlib/target/nupkg/<crate>.<version>.nupkg
```

```xml
<!-- consuming C# project -->
<ItemGroup>
  <PackageReference Include="my_rust_lib" Version="0.1.0" />
</ItemGroup>
```

The `.nupkg` bundles the produced `.dll` (and the shipped `RustDotnet` C# wrappers if you use
containers). See [INTEROP_CSHARP.md §4](INTEROP_CSHARP.md) for the full flow and the NuGet cache
footgun (`dotnet nuget locals global-packages --clear` after a rebuild at the same version).

**Consume a *third-party* NuGet BCL type from Rust.** There is no automatic "add a NuGet dep to a Rust
crate" mechanism. What works today: any type the runtime resolves (the whole BCL, plus assemblies the
host app already references) is reachable through the low-level `mycorrhiza::bindings` surface or the
`dotnet_generic!` / `dotnet_generic_impl!` macros (see `cargo_tests/cd_generic` for that machinery).
Idiomatic wrappers exist for the common BCL types (`mycorrhiza::bcl` — §6); an arbitrary NuGet package's
types would need a hand-written wrapper following the same pattern. There is **no** first-class
"reference NuGet package `Foo` from `Cargo.toml`" feature — don't imply one.

---

## 6. Call an arbitrary BCL type / method from Rust

The common Base Class Library types have idiomatic wrappers in `mycorrhiza::bcl` (in the prelude):
`DateTime`, `TimeSpan` (as `DotNetTimeSpan`), `Guid`, `Uri`, `Regex`, `Random`, `Stopwatch`,
`StringBuilder`, `Environment`, `Math`. They read like normal Rust — associated-fn constructors,
`snake_case` methods, `&str` in / `String` out, and the natural std traits:

```rust
use mycorrhiza::prelude::*;

let id  = Guid::new_v4();
let now = DateTime::now();
assert!(now > DateTime::new(2020, 1, 1));         // Ord via CompareTo

let re = Regex::new(r"(\d+)-(\d+)");
assert!(re.is_match("10-20"));
let m = re.find("x 10-20 y").unwrap();
assert_eq!(m.value().as_str(), "10-20");

let mut sb = StringBuilder::new();
sb.append("Hello, ");
sb.append("world");
assert_eq!(format!("{sb}"), "Hello, world");      // Display

assert_eq!(Math::sqrt(144.0), 12.0);
let host = Environment::machine_name();           // String out
```

**Runnable:** `cargo_tests/cd_bcl` (every wrapper, checked). For a type *not* wrapped, drop to the raw
`bindings` surface or the `dotnet_generic!` macros — see `cargo_tests/cd_generic`. Each wrapper also
exposes a `handle()` escape hatch to reach the raw managed reference.

---

## 7. Expose a Rust struct (and functions) to C#

Three shipped ways, from most-idiomatic to lowest-level.

### 7a. A Rust struct as a managed .NET class — `#[dotnet_class]`

Turns a Rust `struct` into a real managed class with a field-initializing primary constructor and
per-field accessors. `#[dotnet_methods]` re-opens the class to attach static and instance methods.

```rust
// rustlib/src/lib.rs   (crate-type = ["cdylib"])
#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]
use dotnet_macros::{dotnet_class, dotnet_methods};

#[dotnet_class(default_ctor = true, field_setters = true)]
pub struct Counter { value: i32, step: i64 }

#[dotnet_methods]
impl Counter {
    pub fn make(value: i32, step: i64) -> CounterHandle {   // static factory
        CounterHandle::ctor2::<i32, i64>(value, step)
    }
    pub fn sum(this: CounterHandle) -> i64 {                // instance method (receiver = first arg)
        let v: i32 = this.instance0::<"read_value", i32>();
        let s: i64 = this.instance0::<"read_step",  i64>();
        (v as i64) + s
    }
}
```

From C#:

```csharp
Counter c = new Counter(5, 100);   // primary ctor
c.set_value(6);                    // field setter
long s   = c.sum();                // instance method  -> 106
Counter m = Counter.make(11, 22);  // static method
```

`default_ctor = true` adds a parameterless ctor; `field_setters = true` adds `set_<field>`. A
`#[dotnet_class]` field may itself be another managed class (see `Pair` in the example). **Runnable:**
`cargo_tests/cd_typedef`.

### 7b. Export plain functions — `#[dotnet_export]` (strings, no `(ptr,len)`)

Write an ordinary Rust fn; C# calls it as a typed `MainModule.method(...)`. `&str`/`String` cross as a
real managed `System.String` — **no buffer pinning, no output-size guessing**:

```rust
use dotnet_macros::dotnet_export;

#[dotnet_export]
pub fn greet(name: &str) -> String { format!("Hello, {name}, from Rust!") }

#[dotnet_export]
pub fn add(a: i32, b: i32) -> i32 { a + b }
```

```csharp
string g = MainModule.greet("World");   // "Hello, World, from Rust!"
int    n = MainModule.add(2, 3);        // 5
```

Supported param/return types: the integer/float primitives, `bool`, `&str`, `String`, and `-> ()`.
Anything else is a **clear compile error** (marshalling is never faked) — richer types are the backlog.
**Runnable:** `cargo_tests/cd_export`.

### 7c. A raw `#[no_mangle] extern "C"` fn (full control)

A `#[no_mangle] pub extern "C" fn` becomes a `public static` on `MainModule`. Primitives and `#[repr(C)]`
value-type structs cross directly; strings/slices cross as a UTF-8 / element `(ptr, len)` pair you
marshal by hand (this is what `#[dotnet_export]` automates):

```rust
#[repr(C)]
pub struct Point { pub x: i32, pub y: i32 }     // C# sees value-type `cd_interop.Point`

#[no_mangle]
pub extern "C" fn make_point(x: i32, y: i32) -> Point { Point { x, y } }

#[no_mangle]
pub extern "C" fn point_sum(p: Point) -> i32 { p.x + p.y }
```

The backend synthesizes a public `.ctor` + per-field `get_<field>` getters for the struct. **Runnable:**
`cargo_tests/cd_interop`.

### 7d. A Rust-owned collection for C# — `export_rust_containers!`

Want a `RustVec<T>` / `RustHashMap<K,V>` / `RustString` that C# uses like a normal generic type? One
macro line in Rust, one opt-in flag in the csproj — the C# wrappers are shipped:

```rust
// rustlib/src/lib.rs
mycorrhiza::export_rust_containers!();   // RustVec<T> / RustBoxVec<T>
mycorrhiza::export_rust_hashmap!();      // RustHashMap<K,V>
mycorrhiza::export_rust_string!();       // RustString
```

```xml
<!-- csharp/App.csproj -->
<PropertyGroup><UseRustDotnetContainers>true</UseRustDotnetContainers></PropertyGroup>
```

```csharp
using RustDotnet;
using var xs = RustVec<int>.New();          xs.Push(42);  int v = xs.Get(0);
using var m  = RustHashMap<int, long>.New(); m[1] = 100L;  bool has = m.ContainsKey(1);
using var s  = RustString.New();             s.Append("hi");
```

`RustVec<T>` is near-zero-cost for `unmanaged` `T`; `RustBoxVec<T>` boxes any managed `T` via a GCHandle
and preserves reference identity. **Runnable:** `cargo_tests/cd_containers` (RustVec) and
`cargo_tests/cd_containers2` (RustHashMap + RustString).

---

## 8. Handle a callback / event

**A Rust function invoked as a .NET delegate** is shipped: `mycorrhiza::delegate` wraps a Rust
`extern "C" fn` as a real managed `Action` / `Func` / `Comparison`. Each `.invoke(...)` is the .NET
runtime dispatching *into* the Rust callback through a first-class delegate object
(`callvirt Delegate::Invoke`):

```rust
use mycorrhiza::prelude::*;                 // Action1/Action2/Func1/Func2 (Comparison via ::delegate)

extern "C" fn double_it(x: i32) -> i32 { x * 2 }

let f = Func1::<i32, i32>::from_fn(double_it);
assert_eq!(f.invoke(21), 42);               // .NET → Rust through a managed Func`1

// Re-hold a delegate handle (the shape a delegate returned from a .NET call takes):
let held = Func1::<i32, i32>::from_handle(f.handle());
assert_eq!(held.invoke(7), 14);
```

`Action1`/`Action2` are void-returning; `Func1`/`Func2` return a value; `Comparison<T>` is the
`(T,T) -> i32` comparator shape. The callbacks are capture-less top-level `extern "C" fn`s — thread
state through a `static` (e.g. an `AtomicI32`). **Runnable:** `cargo_tests/cd_delegates`.

**Async callbacks / `Task`.** You can `.await` a real .NET `Task` from Rust and hand a Rust `async fn`
back to .NET as a `Task` (`mycorrhiza::task`): `block_on`, `await_unit(Task::delay(20))`,
`Task::run(callback)`, `future_to_task_unit(rust_async_fn())`. **Runnable:** `cargo_tests/cd_async`.
Constraint: a managed `Task` handle must not be held *across* an `.await` inside an `async fn` (a GC
reference can't live in the coroutine's overlapping saved state) — await it via a plain `Future`
(`await_unit`) and keep only primitives across suspend points; the examples show the shape.

**Not yet shipped:** closure *captures* in a delegate (only capture-less `extern "C" fn`s today),
delegate-as-a-generic-method-argument (e.g. `List<T>.Sort(Comparison<T>)`), and **.NET event
subscription** (`obj.SomeEvent += handler`, i.e. the `add_*`/`remove_*` accessors). These are the
documented Theme-3 follow-ups — don't write a recipe that subscribes to a .NET event; it isn't wired.

---

## What is *not* here (so you don't reach for it)

These are honest gaps as of this writing — the natural next recipes, but not yet backed by working code:

- **Idiomatic `HttpClient`** — use `std::net` by hand (§4) for now.
- **.NET event subscription** (`+=` on an event) and **capturing closures as delegates** (§8).
- **Delegate as a generic-method argument** (`Sort(Comparison<T>)`) — needs a nested-generic-binding
  typecheck extension, not a checker relaxation.
- **A `serde` ⇄ `System.Text.Json` adapter** — the JSON bridge (§2) is a standalone DOM today.
- **`#[dotnet_export]` of `Vec<T>` / slices / `Option` / `Result` / `char`** — primitives + strings only.

For the capability map and the genuine ceilings, see
[TRANSLATION_STATUS.md](TRANSLATION_STATUS.md); for the DX backlog, [ERGONOMICS_ROADMAP.md](ERGONOMICS_ROADMAP.md).
