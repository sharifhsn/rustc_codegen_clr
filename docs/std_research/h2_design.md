# H2: a native `.NET` Rust target with a managed `std::sys` — full design

Goal: a **real, shippable** `dotnet` target whose `std` runs on the .NET runtime via the BCL —
no surrogate, no libc, no POSIX-emulation hacks. This is the destination architecture.

> On schedule: the new code is ~4–6 KLOC, which is fast to write. The risk is **design
> correctness**, not volume — three things gate it: (1) the codegen must actually compile `std`
> (shared with H1), (2) the **interop ABI** (Rust↔.NET calls/objects/exceptions), and (3) the
> **GC boundary** (Rust's unmanaged heap + raw pointers vs managed .NET objects). Get those
> right and the PAL itself is mostly mechanical BCL wrapping.

## 0. What we keep, what we delete

**Keep (shared foundation):** the `cilly` IR + codegen lowering, the `linker`, the managed-call
interop lowering (`callvirt_managed` in `src/terminator/call.rs:108`), and the `mycorrhiza`
BCL-binding library (14k LOC, much reusable).

**Delete once the PAL lands:** the surrogate target (pretend-`x86_64-linux`), all libc P/Invoke,
and every `target.md` hack — errno tracking, `environ`/`set_env` emulation, the `argv`
`.init_array`→`.cctor` trick, emulated pthreads, fork. **Yes — the surrogate is fully replaced.**

**Shared with H1 (the one unavoidable prerequisite):** the dotnet `std::sys` is *itself Rust code
compiled through cg_clr*, so the current `alloc`/`std` compile regressions (`FieldOwnerMismatch`,
fat-pointer nesting) block H2 exactly as they block H1. Those are codegen fixes, retained either
way. **Compiling std is a prerequisite; the surrogate-vs-native choice is only about the PAL.**

## 1. The layer cake

```
 L4  target spec  (dotnet.json / upstream `dotnet` target): os, datalayout, atomics, panic
 L3  std::sys::pal::dotnet + sys/* cfg branches  ← the PAL (NEW, ~2–3k Rust, lives in std patches)
 L2  runtime support lib (mycorrhiza → dotnet_rt): typed BCL bindings the PAL calls (~grow 14k)
 L1  interop ABI: managed calls / newobj / fields / managed-object handles / exceptions  (HARDEN)
 L0  codegen (cilly): MIR→CIL lowering, incl. the interop intrinsics  (EXISTS; fix std regressions)
```

Principle (from cranelift/gcc): **keep L0 dumb, push platform complexity up into L2/L3.** The PAL
is thin Rust that calls L2; L2 wraps the BCL via L1; L0 just lowers calls.

## 2. L4 — the target spec

Define a real `dotnet` target (evolve `clr64-unknown-mono.json`). Concrete decisions:

| field | value | why |
|---|---|---|
| `os` | `"dotnet"` | new `target_os`; std cfgs on it. (`target_family = ["dotnet"]`.) |
| `arch` | `"clr64"` (virtual) | CIL is arch-independent, but Rust needs a *fixed* layout. |
| `data-layout` / `pointer-width` | LP64, 64 | from the existing stub; matches cg_clr's type layout. |
| `max-atomic-width` | `64` | no native 128-bit atomics. **Baseline .NET 9+** to get native 8/16-bit atomics (`Interlocked` byte/short) and *delete* the lock-emulation. |
| `panic-strategy` | `unwind` | via native .NET exceptions (abort as fallback). |
| `os-str`/`path` | see §4 | the cross-OS wrinkle. |

This spec is also where we *declare capabilities* so rustc stops emitting things the runtime can't
honor — removing whole classes of surrogate hacks. Shippable out-of-tree as a JSON file; upstream
later for `rustup target add dotnet`.

## 3. L1 — the interop ABI (the crux)

Today: `rustc_clr_interop_managed_call*::<"Asm","Class",is_valuetype,"Method",is_static,Ret,Args>(..)`
is recognized by name and lowered to CIL `call`/`callvirt`/`newobj`. That's test-grade; the PAL
needs it productionized into a small, complete primitive set:

- **Managed object handle** — a `ManagedObj` opaque, pointer-sized type backing a .NET object that
  Rust stores in its (unmanaged) structs. Backed by a **`GCHandle`** so the GC neither moves nor
  collects it while Rust holds it. Lifecycle: alloc on receipt from a BCL call, **free on `Drop`**.
  (e.g. `std::fs::File` wraps a `ManagedObj` over a `System.IO.FileStream`.)
- **Call primitives:** `call_static`, `call_instance`, `call_virtual`, `new_obj`, `get_field`/
  `set_field`, `get_static`/`set_static` — generalize the current generic-encoded intrinsic; wrap
  in `mycorrhiza` macros so the PAL writes `dotnet::call!(System.IO.File::ReadAllBytes(path))`.
- **Marshaling (keep minimal):** primitives 1:1; `&[u8]`/`&str` ↔ `System.String`/`byte[]` only at
  the boundary (explicit UTF-8↔UTF-16); Rust owns its bytes, convert lazily. Avoid hidden copies.
- **Exceptions:** a `try_catch` primitive wrapping a BCL call, catching `System.Exception` →
  `Result`; the PAL maps exception types → `io::ErrorKind`. (cg_clr already lowers Rust
  cleanup→.NET `catch`, so the machinery exists — see the panic work in v0_2_1.)

**Gaps to close before the PAL is solid:** robust object-handle lifecycle (GCHandle alloc/free/pin),
exception→Result bridging, generic/array marshaling. The `dotnet_typedef!` *reverse* direction
(`comptime.rs`, currently `todo!`) is **not needed** — the PAL only calls *into* .NET.

## 4. L3 — the PAL module map (std `sys`)

Each `std::sys` module → .NET API. (`sys/pal/dotnet/` holds the core; `sys/{fs,net,...}` get
`cfg(target_os="dotnet")` branches.)

| std module | .NET backing | notes / wins |
|---|---|---|
| `alloc` | `NativeMemory.Alloc/Free` (unmanaged) | **NOT the GC heap** — Rust needs stable raw addresses. |
| `args` | `Environment.GetCommandLineArgs` | **deletes the `.init_array`/`.cctor` argv hack.** |
| `env` | `Environment.{Get,Set}EnvironmentVariable` | **native — no errno, no `set_env` desync.** |
| `stdio` | `Console.OpenStandard{Output,Error,Input}` (Stream) | |
| `time` | `Stopwatch.GetTimestamp`, `DateTime.UtcNow` | monotonic + wall clock. |
| `thread` | `System.Threading.Thread` (+ `Thread.Name`) | **native threads, not emulated pthreads.** |
| `thread_local` | `ThreadLocal<T>` / key→managed dict | dtors need care (run on thread exit). |
| `thread_parking` / `sync` | `SemaphoreSlim`/`Monitor` | implement the futex-like park primitive, then **reuse std's generic `Mutex`/`Condvar`/`RwLock`** (less code, correct). |
| `fs` | `System.IO.{File,FileStream,Directory}` | exceptions → `io::Error`. |
| `net` | `System.Net.Sockets.Socket` | |
| `process` | `System.Diagnostics.Process` | |
| `random` | `RandomNumberGenerator.GetBytes` | HashMap seed, etc. |
| `cmath` | `System.Math`/`MathF` | partly done already. |
| `backtrace` | `System.Diagnostics.StackTrace` | optional/nice-to-have. |
| `os_str` / `path` | see wrinkle ↓ | |

**The cross-OS wrinkle (decide early):** a .NET assembly is OS-portable, but Rust's `path`
separator and `OsStr` encoding are *static per target*. .NET strings are UTF-16 and can hold
unpaired surrogates → **`OsStr` = WTF-8** (like Windows), converting WTF-8↔`System.String` at the
boundary. For paths, either (a) present a unix-like model (`/`) and let .NET normalize, or (b) make
separators dynamic via `Path.DirectorySeparatorChar`. Recommend (a) for simplicity; document it.

## 5. Panic / unwinding

`panic=unwind` via native .NET exceptions: a `panic_unwind` impl for `dotnet` that throws a managed
exception carrying the boxed Rust payload; `catch_unwind` catches it. cg_clr already maps Rust
cleanup blocks → .NET `catch` (v0_2_1), so this is **one of the places .NET is *easier* than a
native backend** — no `.eh_frame`/personality machinery. Ship `panic=abort` first, then flip on unwind.

## 6. Build & ship

Out-of-tree, no upstream required to start (the cranelift/gcc model, but with a real PAL in the patches):
1. `dotnet.json` target spec.
2. **std patches**: add `library/std/src/sys/pal/dotnet/` + wire the `cfg(target_os="dotnet")`
   branches in `sys/*`. Apply **commit-per-patch** (gcc's `prepare.rs` model) so they survive
   nightly bumps.
3. `cargo build --target dotnet.json -Zbuild-std=core,alloc,std,panic_unwind`.
4. No extra runtime assembly needed — `mycorrhiza`/PAL are Rust compiled through cg_clr; they call
   the BCL via L1 intrinsics. Output is a self-contained .NET assembly run by `dotnet`.

Later: upstream the target + `sys::pal::dotnet` (tier-3, per `target.md`) so `rustup target add
dotnet` works without the fork.

## 7. Phased plan (concrete, gated by the measurement framework)

| Phase | Deliverable / gate | New LOC (rough) |
|---|---|---|
| **P1 — codegen prereq** (shared w/ H1) | real `core`+`alloc`+`std` *compile* via build-std (fix `FieldOwnerMismatch` + fat-ptr) | ~100–500 codegen |
| **P2 — minimal PAL** | `dotnet` target + `sys::pal::dotnet` skeleton (alloc, stdio, args, env, exit) → **`println!` + args + env hello-world on .NET, zero libc** | ~600 PAL + ~300 L1 |
| **P3 — core platform** | harden L1 (managed objects, exception→io::Error, GCHandle lifecycle); time, fs, thread, thread_local, sync → **multithreaded + file-IO program** | ~1.5k PAL/L2 + ~500 L1 |
| **P4 — full surface** | net, process, random, backtrace; flip `panic=unwind` → **real CLI/network app passes the differential validator** | ~1.5k PAL/L2 |
| **P5 — cutover** | delete surrogate target + libc + emulation hacks; make `dotnet` default; (optional) upstream | net deletions |

Throughout, the [measurement framework](std_roadmap.md) (M1 build-std walk, M2 named-gap backlog,
M3 differential validator, M5 sysroot bisect) tells us the frontier and correctness at every step.

## 8. Risk register (where the real difficulty is)

1. **Codegen can't yet compile std (P1).** Hard dependency; until `alloc`/`std` build, no PAL runs.
2. **Interop ABI robustness.** Object-handle lifecycle + exception bridging must be solid — the PAL
   leans on it pervasively. Design + test L1 as its own milestone before scaling the PAL.
3. **GC boundary.** Unmanaged Rust heap + raw pointers coexisting with `GCHandle`-pinned managed
   objects; leaks/UAF here are subtle. Every `ManagedObj` needs a clear ownership + `Drop` story.
4. **Path/OsStr cross-OS semantics** (§4) — pick a model and commit.
5. **Atomics/threading correctness** over the CLR memory model; baseline .NET 9 for sub-word atomics.

## 9. First concrete steps

1. Land P1 (un-block std compilation) — the gating work, and immediately useful.
2. In parallel, **harden L1 as a standalone milestone**: a tiny `#![no_std]` program that opens a
   `System.IO.FileStream` via a `ManagedObj`, reads bytes, catches an exception → `Result`, and
   frees the handle on drop. Proving the object/exception/GCHandle story on a toy is the highest-
   leverage de-risking move before writing 3k LOC of PAL on top of it.
3. Draft `dotnet.json` + the `sys/pal/dotnet` skeleton; get `println!`+args+env hello-world running.

## 10. L1 de-risk results (COMPLETE — the core bet holds, all probes pass)

Probes: [`test/std/interop_derisk.rs`](../../test/std/interop_derisk.rs) (A/B1/B2/C-propagation),
[`test/std/interop_try_catch.rs`](../../test/std/interop_try_catch.rs) (C-catch),
[`test/std/interop_catch.rs`](../../test/std/interop_catch.rs) (the `catch_unwind` negative
control). All compiled through cg_clr and run on .NET 8. Findings:

| capability | result |
|---|---|
| BCL **static call** + primitive marshal | ✅ `System.Math.Max(3,7)=7` (prints 1007) |
| object **ctor** + **instance calls** + holding a managed ref across calls | ✅ `StringBuilder` ctor/`Append`/`get_Length`=1 (prints 2001) |
| **GCHandle** store-across-GC round-trip (the GC-boundary mechanism) | ✅ `Alloc`/`get_Target`/`Free`; object survived the round-trip (prints 3001 then 3999) |
| **.NET exception** propagation from a BCL call | ✅ propagates through the Rust frame as a real, well-typed exception with a clean managed stack trace (`ArgumentOutOfRangeException` at `StringBuilder.Remove` → `…main()`) |
| **catching** a foreign/.NET exception from Rust | ✅ via the dedicated `rustc_clr_interop_try_catch` primitive (prints 1, 5001, 6009, 9999, clean exit) — `catch_unwind` does **not** (it rethrows non-`RustException`) |

**The architectural bet is validated end-to-end:** Rust calls .NET, constructs and holds managed
objects, stores them across GCs via `GCHandle`, and both *propagates* and *catches* .NET
exceptions in Rust frames. Three concrete outcomes feed back into H2:

- **Fixed codegen bug #1** (committed): managed value types
  (`RustcCLRInteropManagedStruct<ASM,CLASS,SIZE>`) were rejected because `type.rs` required 2
  generics; a value type carries 3 (the `SIZE` const) — fixed to 3. Unblocks all managed valuetypes.
- **Fixed codegen bug #2:** `System.Object`/`System.String` were emitted as
  `class [System.Runtime]System.Object` (ELEMENT_TYPE_CLASS) in signature position instead of the
  canonical `object`/`string` element type. The CLR matches BCL method signatures by encoding, so
  e.g. `GCHandle.Alloc(object)` never bound (`MissingMethodException`). Fixed in
  `cilly/.../il_exporter/mod.rs` `type_il`. **This is load-bearing for H2:** the PAL passes/returns
  `object` to/from the BCL constantly.
- **New codegen feature — `rustc_clr_interop_try_catch`:** wraps an indirect call in a CIL
  `try/catch [System.Runtime]System.Object`, returning 0 (normal) / 1 (caught, after running a
  catch fn). Built as `insert_interop_try_catch` in `cilly/.../builtins/mod.rs`, modeled on
  `insert_catch_unwind` minus its `RustException` filter. This is the primitive std I/O will use to
  turn BCL exceptions into `io::Error`. (Follow-up for H2: hand the exception *object* to the catch
  fn — needs a managed-ref ABI — so the PAL can read the exception type/message.)

L1 is done. Next is **P1** (make real std *compile* — the `FieldOwnerMismatch`/`CallArgTypeWrong`
regressions) then **P2** (the `std::sys::pal::dotnet` PAL, §4), per the phased plan in §7.
