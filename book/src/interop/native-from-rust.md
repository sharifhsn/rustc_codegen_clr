# Call native libraries from Rust

`rustc_codegen_clr` turns ordinary Rust FFI declarations into CLR P/Invoke methods. You do not need
a C# shim, a custom declaration macro, or a native Rust wrapper library.

This page assumes `cargo dotnet` is installed and Rust plus the .NET 10 SDK are available. Run
`cargo dotnet doctor` first if the basic quickstart does not work.

## Complete SQLite example

Create an application:

```bash
cargo dotnet new sqlite-dotnet --app
cd sqlite-dotnet
```

Restore a cross-platform native SQLite package. `cargo dotnet` selects the current host RID,
stages the native file, and records the package so a fresh clone can restore it again:

```bash
cargo dotnet add-native SQLitePCLRaw.lib.e_sqlite3 3.53.3 --library e_sqlite3
```

If you have a C header, generate the raw declarations instead of transcribing them:

```bash
cargo dotnet bindgen vendor/sqlite3_api.h \
  --library e_sqlite3 \
  --allowlist-function 'sqlite3_(open|close|exec|errmsg|free|libversion_number)' \
  --allowlist-type 'sqlite3.*'
```

This writes `src/native.rs` with ordinary `#[link] unsafe extern "C"` declarations. Add
`mod native;` to `main.rs`. Re-run the same command with `--check` in CI to reject stale generated
bindings without changing the file.

For a declaration small enough to write directly, `src/main.rs` can still contain:

```rust
#[link(name = "e_sqlite3")]
unsafe extern "C" {
    #[link_name = "sqlite3_libversion_number"]
    fn version_number() -> i32;
}

fn main() {
    let version = unsafe { version_number() };
    assert!(version >= 3_000_000, "unexpected SQLite version: {version}");
    println!("SQLite P/Invoke OK: {version}");
}
```

Build and execute it:

```bash
cargo dotnet run
```

The expected output ends with `SQLite P/Invoke OK:` and SQLite's numeric version. The repository's
executable version of this guide is `cargo_tests/pinvoke_sqlite`, exercised by
`feasibility/pinvoke_acceptance.sh` on Linux x64, macOS Apple Silicon, and Windows x64 CI runners.

Commit `.cargo-dotnet-nuget-deps.json`; it is the reproducible dependency record. Do not commit
`.cargo-dotnet-nuget-assets/`: build, run, and restore recreate that host-specific directory.

## Declaration rules

- `#[link(name = "...")]` is the logical library name passed to .NET's native resolver.
- `extern "C"` emits `cdecl`; `extern "system"` emits the platform-default P/Invoke convention.
- `#[link_name = "..."]` changes the native entry point while your Rust function keeps its local
  name.
- The call remains `unsafe`. Validate pointers, buffer lengths, ownership, and native status codes
  before exposing a safe wrapper.

The supported boundary is C functions using primitives, raw pointers, opaque handles, callbacks,
and validated `#[repr(C)]` data. Variadic functions and imported statics fail during codegen with a
targeted diagnostic: expose them through a fixed-signature C function. C++ classes, exceptions,
and ownership are not a stable C ABI; put a small `extern "C"` shim in front of them.

## Generate declarations from headers

`cargo dotnet bindgen` uses upstream rust-bindgen and libclang. Install libclang once:

- macOS: install Xcode Command Line Tools with `xcode-select --install`;
- Debian/Ubuntu: install `libclang-dev`; or
- Windows: install LLVM and set `LIBCLANG_PATH` to its `bin` directory if it is not discovered.

Use repeatable `--allowlist-function`, `--allowlist-type`, `--blocklist-item`, and `--clang-arg`
options to keep output small and to provide include paths. The generator accepts C headers only:
this is deliberate, because bindgen can describe C++ layouts but CLR P/Invoke cannot provide C++
ABI or exception guarantees.

## Put the unsafe ABI behind a safe facade

The workspace crate `rust-dotnet-pinvoke` lets a library author contain the ABI in one private
module while the rest of the application uses ordinary safe Rust:

```toml
[dependencies]
rust-dotnet-pinvoke = "0.0.1"
```

The installed SDK supplies this version through its build-local Cargo configuration, just like
`mycorrhiza` and `dotnet_macros`.

- scoped `with_utf8_cstr` and `with_utf16_cstr` arguments, plus owned and borrowed string types;
- `try_out`, which exposes an out value only after the native status policy reports success;
- `status_zero` and `status_nonnegative`, retaining native status codes;
- `native_handle!`, which declares a typed RAII handle with an explicit native close function;
- `NativeCallError`, including native status-plus-message errors and null-handle contract failures;
- `take_utf8_string`, which copies and frees a native-owned error string exactly once;
- `OwnedHandle` for dynamic or captured cleanup policies;
- `native_api!`, which generates a safe status/out/handle wrapper only after the declaration names
  its argument conversion, out values, status policy, close function, and success projection;
- heap-stable `Callback<Args, Return>` storage; and
- `callback_trampoline!` for abort-on-panic APIs, or `callback_trampoline_return!` when the native
  callback contract provides a failure return value.
- `CallbackRegistration` plus `thread_safe_callback_trampoline_return!` for callbacks retained and
  invoked asynchronously by native worker threads.
- `NativeJob`/`NativeJobController` for retained operations that need cooperative cancellation,
  progress, exactly-once result/error extraction, retryable stop, and terminal lifecycle state.

The executable SQLite example follows that architecture: generated declarations stay in
`native.rs`, all pointers and `unsafe` calls stay in `sqlite.rs`, and `main.rs` uses only the safe
`Database::open`, `execute`, and `query` methods. Ownership and status policy remain explicit in the
facade because neither bindgen nor a macro can safely infer them from a C header.

For the common open-handle shape, `native_api!` removes repetitive temporary storage and null
checking while keeping every safety decision visible:

```rust
native_api! {
    pub handle Database(native::sqlite3) {
        close = native::sqlite3_close;
    }

    pub fn open_database(filename: &str) -> Database {
        utf8 filename => filename_pointer;
        out database: *mut native::sqlite3 => database_pointer;
        unsafe_call = native::sqlite3_open(filename_pointer, database_pointer);
        status = status_zero;
        success = handle Database(database);
    }
}
```

The generated function returns `Result<Database, NativeCallError>`. `unsafe_call` is contained
inside the wrapper; the out pointer is read only after `status_zero` succeeds; and a successful
null handle becomes `NativeCallError::NullHandle`. Multiple `out` declarations can be projected
with `success = tuple(...)`, and `status` may name a custom policy returning `NativeStatusError`.
The SQLite acceptance uses this exact declaration against the real native package.

Use `utf16 value => value_pointer` for a borrowed wide-string argument. Native-owned string results
must name their matching allocator pair explicitly:

```rust
native_api! {
    pub fn copy_utf16(value: &str) -> String {
        utf16 value => value_pointer;
        out copied: *mut u16 => copied_pointer;
        unsafe_call = native::ac_copy_utf16(value_pointer, copied_pointer);
        status = status_zero;
        success = owned_utf16(copied, free = free_utf16, null = error);
    }
}
```

`owned_utf8` and `owned_utf16` always copy before invoking the declared free function, including
decode-failure paths. `null = error` maps success-with-null to `NativeCallError::NullString`.
Status-plus-message APIs use `error_out ...` with `error = owned_utf8(...)` or
`owned_utf16(...)`; this preserves the numeric status and decoded message together and also rejects
the contradictory success-with-message case. SQLite exercises the owned UTF-8 error path, while
the retained-callback fixture round-trips non-ASCII native-owned UTF-16 through CoreCLR on every
supported host.

Scoped callbacks can live in the same facade declaration. This generates both the heap-stable
storage alias and the panic-contained C trampoline, while leaving the callback's return-on-panic
contract explicit:

```rust
native_api! {
    scoped_callback RowCallback as row_callback(
        columns: c_int,
        values: *mut *mut c_char,
        names: *mut *mut c_char,
    ) -> c_int {
        on_panic = 1;
    }
}

let mut callback = RowCallback::new(|(columns, values, names)| {
    // The native API finishes using this context before the scoped call returns.
    0
});
```

The SQLite query fixture uses this declaration. APIs that retain the context after the call must
use `CallbackRegistration` instead; changing storage policy is never inferred from the callback
signature.

For a retained worker, declare the complete register/unregister contract. The macro generates the
thread-safe trampoline, API-specific registration type, retry-preserving stop failure, and
`RetryableStop` implementation:

```rust
native_api! {
    pub retained_callback Registration, StopFailure as callback_trampoline(
        value: c_int,
    ) -> c_int {
        start(fail_first_unregister: bool);
        token = *mut native::ac_registration;
        register(context, out_registration) = native::ac_register(
            Some(callback_trampoline), context,
            i32::from(fail_first_unregister), out_registration,
        );
        unregister(registration) = native::ac_unregister(*registration);
        status = status_zero;
        on_panic = 77;
        quiescence = unregister_waits;
    }
}
```

`quiescence = unregister_waits` is a required safety assertion, not an inferred property: the
named unregister call must stop future callbacks and wait for every in-flight callback before
returning success. A failed `stop()` returns `StopFailure`; `into_registration()` recovers the same
live guard for retry. `NativeJob` consumes the generated `RetryableStop` implementation directly.
The retained-worker and managed-job acceptances use this generated declaration.

For a retained callback, wrap the native register/unregister pair with `CallbackRegistration`. The
guard owns both the thread-safe callback context and native token. Retained callbacks require
`Fn + Send + Sync`; mutable state therefore uses explicit atomics or locks chosen by the caller,
rather than a hidden runtime mutex. A failed explicit unregister
returns the still-live guard for retry. Successful unregister must stop future calls **and wait for
all in-flight callbacks** before returning. If an implicit `Drop` cannot establish that guarantee,
the helper deliberately leaks the callback and token instead of freeing memory native code may
still call. The executable `pinvoke_async_callback` fixture proves cross-thread invocation after
register returns, retryable unregister, quiescence, normal `Drop`, and panic-to-native-status
containment.

Use `NativeJob::start` when that registration backs a user-visible long-running operation. The
callback captures its `NativeJobController`, which contains only thread-safe native Rust state. A
managed adapter should keep CLR objects outside the callback. Apply `#[dotnet_native_job]` to the
one API-specific function that accepts `NativeJobController<Result, Error, Progress>` and returns
`Result<Registration, StartError>`; the attribute generates the managed status enum,
`IDisposable` class, progress pump, cancellation/result/error methods, and retryable stop surface.
Every ownership/error/sentinel option is explicit, and the generated state constructor is
assembly-internal so external C# can create jobs only through `Start`.

Let C# own its
`CancellationTokenRegistration`, forward cancellation through `request_cancellation`, enqueue
plain progress values from native callbacks, and deliver them through `IProgress<T>` on the host's
dispatcher/update thread. `cargo_tests/cd_native_job` proves this shape against a real retained
native Rust worker and a normal C# `IDisposable` consumer, including reflection proof that no public
state-ID constructor escapes.

## System and local libraries

A system library needs only its `#[link]` declaration when the name is available on every target
host. Use `add-native` for NuGet-provided binaries because it handles RID selection, clean-clone
restore, output staging, and packaging.

For an unpublished or locally built library, vendor one binary per RID:

```bash
cargo dotnet add-native-file ./build/libsample.dylib --library sample --rid osx-arm64
```

This copies it under `native/<rid>/`, records `.cargo-dotnet-native-files.json`, stages the current
RID beside applications, and preserves every recorded RID under `runtimes/<rid>/native/` when
packing. Commit both the manifest and vendored binaries. Cross-RID compilation of those binaries
remains the native library's responsibility.

## Troubleshooting

- `DllNotFoundException`: check that the `#[link]` name matches `--library`, rerun `cargo dotnet
  add-native`, then run `cargo dotnet doctor`.
- `EntryPointNotFoundException`: check spelling and use `#[link_name]` when the Rust-local name is
  different from the native export.
- `BadImageFormatException`: the native file is normally for the wrong RID or architecture. Remove
  `.cargo-dotnet-nuget-assets/` and rerun `add-native` on the target host.
- Missing .NET 10 while another `dotnet` is on PATH: `cargo dotnet` also checks the standard
  `$HOME/.dotnet` installation and selects it when it contains the requested runtime.
