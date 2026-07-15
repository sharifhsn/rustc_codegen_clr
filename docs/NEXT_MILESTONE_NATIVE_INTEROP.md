# First-class native interop

Status: implemented; macOS Apple Silicon runtime acceptance passes locally. Linux x64, macOS
Apple Silicon, and Windows x64 acceptance is wired into CI and the release workflow.

## User surface

Rust uses its standard FFI syntax. The backend does not require a declaration macro:

```rust
#[link(name = "e_sqlite3")]
unsafe extern "C" {
    #[link_name = "sqlite3_libversion_number"]
    fn version_number() -> i32;
}
```

For a NuGet-provided native library:

```bash
cargo dotnet add-native SQLitePCLRaw.lib.e_sqlite3 3.53.3 --library e_sqlite3
cargo dotnet run
cargo dotnet pack
```

`add-native` selects the current host RID unless `--rid` is explicit, restores native-only NuGet
graphs, stages the selected files, and records the package in `.cargo-dotnet-nuget-deps.json`.
Fresh-clone build, run, test, restore, and pack therefore use the same dependency record as managed
NuGet packages. The host-specific `.cargo-dotnet-nuget-assets/` directory is disposable.

The mdBook guide is [`book/src/interop/native-from-rust.md`](../book/src/interop/native-from-rust.md).
The executable proof is [`cargo_tests/pinvoke_sqlite`](../cargo_tests/pinvoke_sqlite), driven by
[`feasibility/pinvoke_acceptance.sh`](../feasibility/pinvoke_acceptance.sh).

## Workspace architecture

The repository is one root Cargo workspace. The relevant ownership boundaries are:

| Crate | Responsibility |
|---|---|
| `rust-dotnet-sdk-core` | Host facts, public managed identity, and supported .NET runtime model. |
| `rust-dotnet-assets` | NuGet restore graph parsing, RID-native selection, staging, collision checks, and package projection. |
| `rust-dotnet-bindgen` | C-header parsing through libclang, deterministic Rust generation, and `#[link]` projection. |
| `rust-dotnet-pinvoke` | Safe-facade building blocks for strings, status/error policy, out values, typed handles, and callbacks. |
| `cargo-dotnet` | CLI parsing, project mutation, build/run orchestration, diagnostics, restore, and pack. |
| `cilly` | Serializable native-import records, link-time resolution, verification, and PE/IL emission. |
| `rustc_codegen_clr` | Adapter from rustc's native-library and foreign-module queries into `cilly`. |

`tools/cargo-dotnet` no longer has its own workspace or lockfile. Reusable code does not path-include
CLI source files; assets and fixtures physically live in `rust-dotnet-assets`.

## Compiler contract

During local-crate codegen the backend reads `tcx.native_libraries(LOCAL_CRATE)`, joins each record
to `tcx.foreign_modules(LOCAL_CRATE)`, and records every foreign function as:

```text
NativeImport {
  rust_symbol,
  entry_point,
  library,
  call_conv,
  preserve_errno,
}
```

The linker merges these records across artifacts. Exact duplicates are harmless; contradictory
declarations for one Rust symbol fail loudly. Missing-method resolution prefers a declared import
over the linker's legacy compiler/runtime extern map.

`MethodImpl::Extern` carries the logical library, optional entry-point override, calling convention,
and `preserve_errno`. Both exporters consume the same data:

- direct PE writes `ModuleRef`, `ImplMap`, the native import name, calling-convention flags, and
  `SupportsLastError` when requested;
- the IL exporter writes the equivalent `pinvokeimpl(...)` declaration.

User-declared imports enable last-error preservation by default so CoreCLR captures the platform
error slot before managed runtime work can overwrite it. The linker's internal runtime-shim map
retains its existing per-symbol policy.

Because the serialized shape changed, the assembly artifact ABI is version 6 (`CILLYAR6`). Version
5 and older envelopes are rejected with a rebuild diagnostic instead of being guessed or decoded
against the wrong schema.

## Supported boundary

The public surface supports:

- native functions declared with `extern "C"` or `extern "system"`;
- integer and floating-point primitives;
- raw pointers and opaque pointer handles;
- `#[repr(C)]` data only where the existing layout verifier accepts the shape;
- `#[link_name]` entry-point overrides;
- cdecl, platform-default, stdcall, fastcall, and thiscall metadata in the IR/exporters; and
- explicit caller-owned buffers and strings.

Automated above the raw ABI:

- C-header ingestion with regex allowlists, clang include arguments, and a stale-output check;
- owned UTF-8 and UTF-16 input strings plus borrowed native-string validation;
- status-aware out parameters that cannot be read on native failure;
- typed RAII native handles with explicitly named cleanup functions;
- owned native error strings copied and freed exactly once;
- heap-stable callback contexts and generated aborting or failure-returning panic containment;
- retained-callback registration guards that require `Fn + Send + Sync`, preserve the guard on
  unregister failure, and free the context only after native quiescence;
- RID-qualified vendoring of local native files for build, run, test, and pack.

Fundamental ABI exclusions:

- variadic functions or imported statics;
- C++ classes, mangling, or exceptions;
- ordinal imports;
- cross-producing all native RIDs from one host.

P/Invoke is the Rust-to-native path. Calling Rust from C# remains direct managed export interop and
should not be routed through P/Invoke.

## Asset and runtime behavior

`rust-dotnet-assets` accepts native-only packages: a primary managed DLL is optional in the resolved
graph. `add-nuget` still explicitly requires one because its job is managed binding generation;
`add-native` instead requires at least one selected native asset.

The parser handles both `runtimeTargets` and the SDK's RID-selected direct `native` group. This is
necessary for packages such as `SQLitePCLRaw.lib.e_sqlite3`, whose `net10.0/<rid>` target projects
the chosen binary under `native`.

When an older `dotnet` is first on PATH but the requested .NET 10 runtime exists under
`$HOME/.dotnet`, the CLI and asset restore select the user-local host. This prevents a valid
side-by-side installation from failing merely because Homebrew or a system package placed .NET 8
earlier on PATH.

## Helper crate boundary

`rust-dotnet-pinvoke` keeps ownership policy explicit and is `no_std` capable at its core. A safe
facade can contain every raw call so application code sees ordinary methods and `Result`. Its
optional allocation and standard-library layers provide:

- `NativeStatusError`, preserving the native numeric code;
- owned and borrowed UTF-8/UTF-16 native strings;
- `Out<T>`, `try_out`, and explicit status policies;
- `NativeCallError`, including copied native error messages and null-handle failures;
- `native_handle!` for named handle types and `OwnedHandle<T, F>` for dynamic cleanup;
- scoped UTF-8/UTF-16 argument closures and owned native-string cleanup; and
- heap-stable callbacks with aborting or failure-returning panic containment; and
- `CallbackRegistration` and thread-safe trampolines for native APIs that retain callbacks beyond
  the registering call.
- `NativeJob` and `NativeJobController` for progress, cooperative cancellation, exactly-once
  completion/error channels, retryable stop, and terminal lifecycle state.
- `NativeJobController::ensure_not_canceled` for `?`-friendly worker checks. When a native-backed
  operation is surfaced directly as an async export, `cancellation = "task"` maps its explicit
  Rust error branch to CLR canceled-task state rather than a fault or sentinel result.
- `#[dotnet_native_job]` for generating the C#-natural status enum and factory-owned `IDisposable`
  shell over one API-specific start adapter. The generated state constructor is CLR
  assembly-internal, while `Start`, progress pumping, cancellation, results, stop retry, and
  diagnostics remain public.
- `native_api!` for explicit borrowed UTF-8/UTF-16 conversion, native-owned strings with matching
  free functions, one or more initialized-out values, custom status policies, null-checked handle
  projection, deterministic close functions, scoped panic-contained callback declarations, and
  retained registration/stop guards with an explicit quiescence assertion.

It does not transport compiler metadata, acquire packages, or infer ownership. Generated bindgen
declarations remain the raw escape hatch; an API-specific facade is the only place that needs to
handle their pointers and `unsafe` contract.

## Acceptance

The decisive oracle regenerates declarations from a checked-in C header, restores SQLite's native
package, compiles the standard Rust declarations into a direct PE, copies the selected host asset
beside the app, opens an in-memory database, executes inserts, receives query rows through a
panic-contained Rust callback, closes the owned handle, and checks the library version.

Local macOS Apple Silicon acceptance produced:

```text
SQLite P/Invoke OK: 3053003; query sum=42
```

`feasibility/pinvoke_async_callback_acceptance.sh` builds a separate native Rust `cdylib` whose
worker thread retains and invokes a managed Rust callback after registration returns. Three
sequential registrations in one CoreCLR process prove retry after deterministic unregister failure,
unregister-before-free on `Drop`, worker join/quiescence, reusable callback storage, and callback
panic conversion without ABI unwinding. It then runs a .NET 10 C# consumer over a managed Rust
`NativeJob` adapter and proves host-thread progress pumping, `CancellationToken` forwarding,
exactly-once result/error extraction, retryable stop, `IDisposable`, and zero surviving workers.
It also reflects the generated class and rejects any public constructor while requiring exactly one
assembly-internal factory constructor.
The same acceptance runs on Linux x64, macOS Apple Silicon, and Windows x64 CI and release hosts.

Automated gates:

- `fork-gate.yml`: Linux product smoke, Windows x64, and macOS Apple Silicon jobs;
- `release.yml`: the same native acceptance on every public SDK bundle host; and
- `cargo test -p cilly pinvoke`: import precedence and direct-PE metadata flags.

## Remaining hardening after the first release

These are follow-up improvements, not hidden prerequisites for the raw SQLite path:

1. Validate unsupported foreign signature shapes earlier and emit a targeted diagnostic naming the
   library and symbol. Today unsupported lowering may fail later in normal codegen verification.
2. Teach `cargo dotnet doctor` to enumerate declared native imports and distinguish missing library,
   wrong RID, and missing entry point before execution.
3. Add packed-consumer runtime execution in addition to the existing package-layout assertions.

The old `NATIVE_PASSTHROUGH` GCC/`nm` experiment is not this feature. It remains internal and must
not be documented as the portable public native-interoperability path.
