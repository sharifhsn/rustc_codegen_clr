# Office, Unity, and MAUI ergonomics execution plan

Status: active implementation goal. This ledger is exhaustive: every item below is intended to
land with a product-shaped fixture, documentation, and the smallest decisive acceptance gate. An
item may be rejected only when a compatibility experiment proves the host cannot safely load the
generated assembly; rejection must leave behind the proof and a supported alternative.

## Product compatibility truth

### Excel and Office

| Host shape | Current conclusion | Required proof/product |
|---|---|---|
| Excel-DNA `.xll` add-in | Primary immediate target. Excel-DNA supports .NET 6-10, including `net10.0-windows`, and can package managed dependencies into a deployable add-in. | Windows x64 scaffold and acceptance: exported Rust UDF, typed range input/output, exception mapping, progress/cancellation where Excel permits it, packed `.xll`, and native-Rust kernel asset. |
| VSTO add-in | Not a direct target for the public .NET 10 assembly. Microsoft keeps VSTO on .NET Framework 4.8 and does not support modern .NET in the same Office process through VSTO. | Document an explicit bridge architecture: thin C# VSTO shim plus out-of-process .NET 10 service, or use Excel-DNA when in-process UDF/add-in behavior is required. Do not claim direct VSTO support. |
| Office COM automation | Research target, not an inferred promise. Modern .NET can expose COM components through `comhost`, but Office add-in loading, type-library production, registry, bitness, apartment policy, and coexistence require an Office-specific proof. | Windows x64 `comhost` spike with explicit `ComVisible`, `Guid`, interface shape, registration-free option, STA rules, and teardown. Promote only if Excel loads it reliably beside other add-ins. |
| Office web add-in | Cross-platform Office route. The in-Office surface is HTML/JavaScript; managed Rust belongs in a .NET server or local companion, not directly in the task-pane runtime. | ASP.NET Core service template/fixture callable from an Office web add-in, with authentication/deployment documented. |
| Office for macOS | No `.xll`/VSTO parity claim. | Office web add-in plus managed-Rust server/companion is the supported direction until a native Office-host proof exists. |

### Unity

Unity 6 managed plugins support .NET Standard 2.1 or the Unity .NET Framework profile; Unity's
documented compatibility table does not support .NET Core-targeted plugins. The public SDK's
`net10.0` assembly therefore must not be copied into `Assets/Plugins` and called supported.

Two separate journeys are required:

1. **Managed Rust Unity plugin:** implement a real `netstandard2.1` compatibility profile, restrict
   referenced APIs to that contract, emit compatible assembly identity/metadata, load the output in
   the Unity Editor, and run it under both Mono and IL2CPP player builds before making a claim.
2. **Native Rust Unity plugin:** package a narrow C ABI per Unity platform and generate a clean C#
   wrapper. This is useful sooner, but remains native P/Invoke and is not evidence for managed Rust.

### .NET MAUI

MAUI is a profile matrix rather than one runtime:

| Target | Runtime/compilation implication | Required proof |
|---|---|---|
| Windows | .NET 10 CoreCLR; closest to the existing public contract. | Build and run a MAUI Windows app consuming a managed Rust assembly and optional `win-x64` native kernel. |
| Android | Mono is the normal .NET 10 runtime; CoreCLR is experimental. APK packaging uses Android ABIs, not the desktop RID layout alone. | First prove plain managed IL under Mono. Then separately test experimental CoreCLR. Add `android-arm64`/emulator native assets and Java/Android packaging acceptance. |
| iOS/Mac Catalyst | JIT is unavailable on device; NativeAOT/full trimming is central. Native libraries may require static frameworks rather than loose dynamic libraries. | Publish, inspect all AOT/trimming warnings, run simulator/device acceptance, and add `ios-*`/`maccatalyst-*` framework packaging. |

## Exhaustive implementation ledger

### A. Typed application data

- [x] Provide a typed DTO baseline. `#[dotnet_dto]` already emits a managed class with PascalCase
  writable properties, parameterless/full constructors, and proven `Decimal`, `DateOnly`,
  `Nullable<T>`, and managed-string fields in `cargo_tests/cd_typed_dto`.
- [x] Add `#[dotnet_record]` as the immutable sibling of `#[dotnet_dto]`: it emits the primary CLR
  constructor and PascalCase getter-only properties, with no parameterless constructor or setters.
  `cd_typed_dto` verifies the metadata and constructed values.
- [x] Emit the complete managed record surface: primary constructor, PascalCase read-only
  properties, field-wise `IEquatable<T>`/`object.Equals`, null-safe `==`/`!=`, matching
  `GetHashCode`, diagnostic `ToString`, and `Deconstruct(out ...)`. Every field routes through
  `EqualityComparer<T>.Default`, preserving framework null, string, floating-point, and nested
  value semantics. `cd_typed_dto` proves typed/object/wrong-type/null equality, equal hashes,
  `NaN`, null strings, reflection metadata, formatting, and positional C# deconstruction against a
  verifier-clean direct PE loaded by CoreCLR.
- [x] Map simple Rust structs explicitly: `#[dotnet_dto]`/`#[dotnet_record]` emit reference types,
  while `#[dotnet_value]` emits a genuine `System.ValueType` with writable PascalCase properties.
  `cd_typed_dto` proves C# construction/property access, by-value passage into managed Rust, and a
  Rust calculation over the value's directly loaded typed fields. Reference semantics are never
  inferred from Rust layout.
- [x] Marshal ordinary owned primitive `Vec<T>` parameters and returns as normal managed `T[]`
  values. `RustOwnedVec<T>` deliberately selects the existing disposable, Rust-owned low-copy
  `RustVec<T>` contract. The compiler now has a verified typed managed-array load operation in
  addition to allocation/store; `cd_export` proves arrays and the explicit ownership alternative.
- [x] Extend the managed convenience surface from primitive arrays to generated DTO/value-type
  arrays. `ManagedArray<T>` is the explicit GC-owned `T[]` projection for synchronous code, so
  reference handles never enter Rust `Vec` storage. `cd_typed_dto` proves `RatePoint[]` field reads,
  `InvoiceDto[]` length access, and allocation identity on round-trip for both shapes.
- [x] Add an `IReadOnlyList<T>` projection over managed arrays and collection implementations.
  The direct interface handle exposes `len`/`at`, targets `Count` through its actual declaring
  `IReadOnlyCollection<T>` interface, and round-trips the caller's implementation object unchanged.
  `cd_typed_dto` proves arrays of both value types and reference DTOs through this surface.
- [x] Marshal scoped primitive `&[T]`/`&mut [T]` parameters as real
  `ReadOnlySpan<T>`/`Span<T>` on synchronous exported methods. `cd_typed_dto` proves a C#
  `stackalloc ReadOnlySpan<int>` flowing into Rust without allocation and a `stackalloc
  Span<double>` being mutated by Rust in place. The macro rejects span parameters on async exports
  with a compile-fail diagnostic directing callers to `Memory<T>`/`ReadOnlyMemory<T>`; the borrowed
  view can therefore never escape its valid call scope.
- [x] Make `ReadOnlyMemory<T>`/`Memory<T>` the retained and async-safe buffer surface. Their CLR
  values are boxed behind opaque GC roots, so Rust futures retain only native tokens; exported
  methods accept and return framework-native memory values without pointer plumbing. `cd_typed_dto`
  proves sliced-view/backing-array identity and write-through, while `cd_async_export` proves both
  read-only and mutable buffers across a genuine suspension point.
- [x] Add a runtime policy for optional managed references without placing GC references in Rust
  enum layouts. `ManagedOption<T>` keeps the underlying CLR reference-or-null signature, stores a
  non-null value as a pointer-only rooted `ManagedRef<T>`, and maps absence to real `null`.
  `cd_typed_dto` proves nullable generated DTOs and `cd_async_export` proves nullable strings across
  suspension. C# nullable-reference annotations remain tracked separately under API metadata.
- [x] Add product-shaped CLR-identity-preserving mappings for `Guid`, `DateTime`,
  `DateTimeOffset`, and `decimal`. `cd_typed_dto` proves all four as ordinary C# property types and
  round-trips their values through a Rust-created managed record.
- [x] Add the common managed collection-interface projections needed by Excel, Unity, and MAUI.
  `ReadOnlyList<T>` projects `IReadOnlyList<T>`; rooted `MutableList<T>`, `MutableDictionary<K,V>`,
  and `ManagedEnumerable<T>` project `IList<T>`, `IDictionary<K,V>`, and `IEnumerable<T>` without
  requiring a concrete BCL implementation. `List<T>::into_enumerable` is a no-copy producer path.
  The typed and async C# fixtures prove mutation, implementation identity, arbitrary C# enumerable
  consumption, Rust-produced sequences, and roots surviving suspension.
- [x] Give the Excel scaffold a typed range edge: `RUST.PORTFOLIO_FV_TABLE(object[,] rows)` validates
  a three-column schema, converts cells to typed `double`/`int` arguments, returns an `object[,]`,
  and reports shape/conversion errors as Excel-visible values rather than JSON or pointer handles.
  A future zero-copy numeric-array specialization remains an optimization, not an onboarding gap.

### B. Async, cancellation, progress, and lifecycle

- [x] Accept ordinary non-generic `#[dotnet_export] async fn` returning a normal value or `()` and
  generate the `future_to_task`/`Task<T>` bridge. `cargo_tests/cd_async_export` proves a genuine
  suspension point and C# `await`; async `Result<T, E>` remains tied to the exception-policy item.
- [x] Add an idiomatic, CLR-identity-preserving `CancellationToken` wrapper with polling,
  `ThrowIfCancellationRequested`, and callback registration. `CancellationRegistration` owns the
  managed registration, delegate, and Rust closure together; synchronous disposal waits out racing
  callbacks, while failed non-blocking unregistration returns the still-live guard.
- [x] Add an `IProgress<T>` wrapper so Rust APIs call `progress.report(value)` without exposing
  delegate plumbing. `cd_typed_dto` proves a real C# implementation passed into managed Rust.
- [x] Add rooted cancellation and progress projections for Rust async state machines.
  `Cancellation` boxes/GCHandle-roots the token and `ProgressReporter<T>` GCHandle-roots the
  interface, so the coroutine stores no raw GC reference. `cd_async_export` proves both across a
  genuine suspension while C# still passes ordinary `CancellationToken` and `IProgress<int>`.
- [x] Generate cooperative native cancellation adapters and map cancellation to
  `OperationCanceledException`/cancelled tasks. Managed tokens and retained-native controllers now
  expose `ensure_not_canceled()` for `?`-based cooperative checks. An explicit
  `#[dotnet_export(cancellation = "task")] async fn -> Result<T, E>` policy maps `Ok` to success and
  `Err` to `TaskCompletionSource.SetCanceled`; C# proves `IsCanceled`, not `IsFaulted`, and catches
  `OperationCanceledException` from both `Task<T>` and non-generic `Task`. Async `Result` without a
  policy remains a compile error so domain errors are never silently discarded as cancellation.
- [x] Export `IDisposable` and `IAsyncDisposable` implementations from Rust-owned types.
  `#[dotnet_methods(disposable, async_disposable)]` validates the exact `Dispose()` and
  `ValueTask DisposeAsync()` shapes and declaratively attaches both framework interfaces.
  `future_to_value_task_unit` constructs the real non-generic `ValueTask(Task)` value. The typed
  fixture proves C# `using` and `await using`, a real managed-task suspension, repeated synchronous
  disposal, verifier rejection of a raw managed handle captured across suspension, the safe
  detach-before-suspend native-token pattern, and exactly-once Rust `Drop`.
- [x] Add a standard managed job/registration abstraction over `CallbackRegistration`.
  `NativeJob`/`NativeJobController` provide exactly-once result and error channels, cooperative
  cancellation, native-only callback progress, retryable explicit stop, terminal-state
  preservation, and quiescence-aware drop. `cd_native_job` proves a C#-natural `IDisposable`
  adapter: C# owns its `CancellationTokenRegistration`, native callbacks enqueue plain Rust
  progress values, and `PumpProgress()` delivers `IProgress<int>` on the caller/dispatcher thread.
  The acceptance also proves a failed stop preserves the live registration for retry and leaves no
  native worker alive after disposal.
- [x] Produce `IAsyncEnumerable<T>` from Rust async streams. `AsyncEnumerable::spawn` and
  `try_spawn` run an ordinary Rust future behind a capacity-one managed channel, so C# consumes a
  real BCL `ValueTask<bool>` iterator with one-item backpressure. The bundled managed lease stops
  Rust exactly once on completion, cancellation, early `await foreach` disposal, or abandonment;
  producer errors fault enumeration. `cd_async_export` proves every lifecycle path under CoreCLR.
- [x] Add UI-dispatch helpers for WinUI/MAUI synchronization contexts and Unity's main-thread rule.
  `UiDispatcher::try_dispatch` owns each Rust closure through a managed exactly-once lease;
  execution, immediate rejection, adapter exceptions, and finalization all reclaim it. The bundled
  `IRustUiDispatcher` has dependency-free adapters for a captured `SynchronizationContext` and host
  check/enqueue delegates. WinUI maps `HasThreadAccess`/`TryEnqueue`, MAUI maps
  `!IsDispatchRequired`/`Dispatch`, and Unity captures its installed synchronization context on the
  main thread. `cd_async_export` proves worker-to-UI execution, managed thread identity, panic
  containment, rejection cleanup, and abandoned-queue finalization. Actual WinUI, MAUI, and Unity
  host launches remain separate product-fixture gates under E/G; this item proves the shared
  dispatch and lifetime contract, not those still-open compatibility profiles.
- [x] Add Excel-safe async/UDF patterns that respect Excel's calculation threading and do not call
  the COM object model from illegal threads. The Excel-DNA 1.9 scaffold registers a
  `Task<object>` UDF with a final formula-lifetime `CancellationToken`, copies only scalar inputs
  into `Task.Run`, and polls that token inside a CPU-bound managed-Rust stress sweep. It preserves
  cancellation, converts normal Rust validation failures into worksheet values, explicitly marks
  only pure synchronous functions `IsThreadSafe`, and enables explicit exports. The scaffold and
  acceptance reject Excel COM application access in `Functions.cs`; documentation routes any
  intentional Excel mutation through `ExcelAsyncUtil.QueueAsMacro`. The macOS acceptance builds and
  packs the real x64 `.xll`; a Windows Excel process launch remains the separate product proof in E.

### C. CLR-native public API shape

- [x] Emit PascalCase managed names by default for `#[dotnet_export]`, `#[dotnet_methods]`, and
  `#[dotnet_interface]`, while preserving exact `name = "..."` overrides. `cd_typed_dto` omits the
  rename on `annualized_rate`; the C# consumer calls the generated `AnnualizedRate` member.
- [x] Emit complete XML documentation for supported public managed shapes. Rustdoc summaries and
  `# Arguments`/`# Parameters`, `# Returns`, `# Errors`/`# Exceptions`, and `# Type Parameters`
  sections become escaped `<summary>`, `<param>`, `<returns>`, `<exception>`, and `<typeparam>`
  elements. Exports, enums, DTO/record/value/class types, constructors (including forwarded base
  arguments), generated properties/methods, and generic interfaces/methods use correct CLR member
  IDs. Explicit parameter names flow into the PE metadata instead of depending on carrier debug
  info. `api_docs_acceptance.sh` validates the XML, runs a clean NuGet consumer, reflects type and
  method generic parameters plus constructor/interface method parameter names, and passes twice in
  succession without retaining removed sidecar entries.
- [x] Emit nullable-reference annotations and a nullable context that C# IntelliSense understands.
  `ManagedOption<T>` projects as `T?`; required managed strings, handles, tasks, delegates, arrays,
  and collections project as non-null. The direct PE writer and legacy IL exporter emit the real
  compiler-recognized `NullableContextAttribute(byte)` / `NullableAttribute(byte)` metadata on
  exported functions, class methods, interface members and properties, DTO constructors, DTO
  properties, parameters, and returns without changing runtime signatures. The clean NuGet
  consumer runs with nullable analysis and warnings-as-errors, uses `NullabilityInfoContext` to
  verify each shape, and round-trips nullable strings through exports, interfaces, and DTOs.
- [x] Map declared Rust error types/policies to specific managed exception classes, preserving inner
  native status/message information. `error = "exception"` remains the Display-only compatibility
  path; `error = "managed"` requires the error type to implement `ManagedError`, selects a familiar
  managed base (`ArgumentException`, `InvalidOperationException`, `IOException`,
  `TimeoutException`, `NotSupportedException`, or `Exception`), preserves `Display` as `Message`,
  and exposes `HasNativeStatus` / `NativeStatus` on the packaged helper exception. The clean NuGet
  consumer catches the typed argument exception and verifies its base, message, and status.
- [x] Prefer real CLR properties over `get_*`/`set_*` methods and real events over exposed
  registration methods. Generated DTO/class/interface accessors project as CLR properties;
  `#[dotnet_event]` projects exact `add_<Name>` / `remove_<Name>` accessors into Event metadata
  even when default managed naming is PascalCase. `api_docs_acceptance.sh` reflects properties,
  while `event_acceptance.sh` proves class and interface events plus Rust-side subscription with
  direct PE output and zero ILAsm fallback. Public synthesized constructors are DCE roots so
  C#-only event owners remain constructible.
- [x] Make familiar interfaces (`IReadOnlyList<T>`, `IEnumerable<T>`, `IProgress<T>`, `IDisposable`,
  `IAsyncDisposable`) straightforward to implement and project. `ReadOnlyList<T>`,
  `ManagedEnumerable<T>`, and `Progress<T>` are direct interface projections, while
  `#[dotnet_methods(disposable, async_disposable)]` validates and attaches the lifecycle
  interfaces declaratively. The clean `cd_typed_dto` C# consumer passes arbitrary framework
  implementations, receives a Rust-produced enumerable, reports progress, and exercises both
  `using` and `await using` against the generated direct-PE assembly.
- [x] Support safe custom attributes on methods, properties, fields, parameters, and return values,
  not only type-level metadata. `attr(...)`, `return_attr(...)`, `param_attr(name, ...)`,
  `#[dotnet_attr(...)]`, `#[dotnet_property_attr(...)]`, interface-property attributes, and
  `static_field_attr(...)` all lower through one structured ECMA-335 representation with no raw
  blob escape hatch. Named arguments retain the ECMA-335 FIELD/PROPERTY distinction through macro,
  compile-time decoding, linking, and direct-PE emission. The proc macro and backend independently denylist runtime-semantic
  `System.Runtime.CompilerServices` / `System.Runtime.InteropServices` attributes. The clean
  packaged API consumer reflects every target, including constructor and named-property values,
  from a direct-PE assembly.
- [x] Put assembly name, root namespace, module type, public namespaces, package identity, and
  compatibility profile under one validated project configuration. Schema 1's single
  `[package.metadata.dotnet]` table now resolves into a typed `ManagedProjectConfig`; namespaces
  must be valid, unique, and include the root namespace, while the profile must match the
  evidence-gated `cargo dotnet profiles` registry. The full contract is recorded in artifact
  receipts and the multi-library MSBuild/packaged-consumer acceptance verifies it end to end.
- [x] Generate host metadata through the safe attribute surface without inventing host contracts.
  The Excel scaffold emits real field-based `ExcelFunctionAttribute`/`ExcelArgumentAttribute`
  metadata from Rust; the packed-artifact acceptance reflects the exact names, descriptions,
  category, and thread-safety flag. Automatic Excel formula discovery remains in the separate real
  Windows launch gate. Unity managed plugins have no universal registration attribute, and MAUI's
  baseline binding surface is ordinary CLR properties/events/interfaces already covered above;
  project-specific attributes can use the same safe field/property-aware surface when required.
- [x] Add API snapshot compatibility checks for every generated public shape. The checked-in
  `cd-export.public-api.txt` is produced from a real backend-built DLL and records exported
  functions, constructors, fields, properties, generic interfaces, parameter/return nullability,
  and safe custom attributes. `api_compat_acceptance.sh` compares the generated contract byte for
  byte and proves a representative binary-breaking change is rejected without a major-version
  increase. Linker-rooted implementation symbols now use CLR `assembly` visibility, so the
  baseline contains the intended 50-line consumer contract rather than hundreds of rustc/runtime
  internals.

### D. Declarative, explicit native facades

- [x] Add a `#[native_api]`/equivalent declaration layer that consumes ordinary raw declarations
  and explicit policies rather than inferring ownership. `native_api!` calls unmodified generated
  declarations and requires each conversion, out value, status policy, close function, and success
  projection to be written in the facade declaration; incomplete shapes fail at compile time.
- [x] Generate `status_zero`/`status_nonnegative` and custom status-policy adapters. The facade
  accepts either built-in policy or a caller-defined path/closure returning `NativeStatusError`.
- [x] Generate `try_out` plumbing for one or more explicitly named out values. Storage is read only
  after the declared status policy succeeds; single values and tuples are supported.
- [x] Generate typed `native_handle!`/`OwnedHandle` wrappers from explicit close/release policies.
  `success = handle Type(value)` includes success-with-null rejection. The real SQLite fixture now
  uses this path for `Database::open` and passes the CoreCLR P/Invoke acceptance.
- [x] Generate borrowed and owned UTF-8/UTF-16 string handling with explicit free functions.
  `native_api!` supports scoped `utf8`/`utf16` arguments, required native-owned results, and
  status-plus-owned-message errors. Every owned policy names its free function; copying and freeing
  still occur on decode failure, success-with-null becomes `NullString`, and success-with-message
  is rejected. SQLite proves the owned UTF-8 error path; the cross-platform retained-callback
  fixture round-trips non-ASCII native-owned UTF-16 through CoreCLR.
- [x] Generate scoped callback storage and panic-contained trampolines. A
  `native_api! { scoped_callback Storage as trampoline(...) { on_panic = ...; } }` declaration
  emits the typed `Callback<(Args...), Return>` storage and its failure-returning C trampoline.
  SQLite uses it for row enumeration and proves panic containment through the direct-PE fixture.
- [x] Generate retained asynchronous callback registration guards from explicit register,
  unregister, token, fallback, and quiescence contracts. `native_api! { retained_callback ... }`
  emits the thread-safe trampoline and API-specific guard only when all policies are named;
  `quiescence = unregister_waits` is a required caller assertion, never inferred from the ABI.
- [x] Generate retryable API-specific stop failures that return the still-live guard. The generated
  `StopFailure::into_registration` and `RetryableStop` implementation preserve the exact callback
  context, native token, and unregister closure. The cross-platform worker proves deterministic
  first-stop failure, retry, join/quiescence, drop, and panic-to-status behavior; the same generated
  registration backs the C# managed-job acceptance.
- [x] Generate C#-natural managed job wrappers over those Rust guards where requested.
  `#[dotnet_native_job]` turns one API-specific start adapter into a managed status enum and
  factory-owned `IDisposable` class with queued progress pumping, cooperative cancellation,
  exactly-once result/error extraction, retryable stop, diagnostics, and idempotent disposal. Its
  state-ID constructor is assembly-internal rather than forgeable from consumer C#; the full
  native-worker acceptance reflects that metadata and exercises every lifecycle outcome.
- [x] Keep raw bindgen declarations as the escape hatch and produce diagnostics whenever a safe
  wrapper policy is incomplete or contradictory.
  `native_api!` now has declaration-specific fallbacks for functions, handles, scoped callbacks,
  and retained callbacks. Each diagnostic names the facade, prints the observed policy, lists the
  complete safety contract, and points back to ordinary raw extern/bindgen declarations. The
  cross-platform policy-diagnostics acceptance proves raw-only compilation plus missing function,
  contradictory UTF projection, incomplete handle, scoped-callback, and retained-callback failures.

### E. Product scaffolding and existing-project attachment

- [x] `cargo dotnet new --excel` scaffold baseline: typed scalar and three-column range UDFs, stable
  Excel-DNA 1.9.0 package, explicit x64-only policy, packed-add-in build acceptance, and
  Excel-visible validation errors are implemented.
- [ ] Finish the Excel product proof with an optional native kernel and a real Windows Excel launch.
  The mdBook now has a complete scaffold/build/formula/architecture/deployment guide and states the
  Windows-only preview boundary explicitly.
- [x] `cargo dotnet new --maui`: the generated MAUI host is deliberately Windows-only today, uses
  the `maui-windows-net10` schema-1 backend profile, builds Rust through `RustDotnet.targets`, and
  explicitly declines to generate Android/iOS/Mac Catalyst TFMs until their acceptance gates pass.
  The cross-platform scaffold contract passes; a Windows workload build and launch remain a
  separate product-evidence requirement below.
- [x] `cargo dotnet new --winui`, `--webapi`, and `--worker` product templates. All four product
  hosts share one typed schema-1 `Namespace.Backend` Rust assembly contract. The generated Web API
  endpoint and Worker execute managed Rust on macOS in `product_hosts_acceptance.sh`; WinUI remains
  planned and Windows-only until its build and launch gate exists.
- [x] `cargo dotnet attach <csproj> --rust-crate <path>` for existing SDK-style solutions. It
  requires schema-1 identity, validates host/profile compatibility before mutation, writes one
  marked atomic/idempotent block, supports `--containers` and `--dry-run`, and refuses to guess
  around hand-authored wiring. The product-host acceptance attaches a clean console project and
  executes the referenced managed Rust API.
- [ ] `cargo dotnet attach-unity <UnityProject> --rust-crate <path>` only after the
  `netstandard2.1` managed profile is proven; copy/reference artifacts and preserve Unity `.meta`
  semantics deterministically.
- [ ] Attachment must add/validate the MSBuild import, `<RustCrate>`, target profile, generated
  container opt-ins, native assets, namespace/assembly identity, and starter service without
  overwriting unrelated project content.
- [ ] Make Visual Studio/Rider/Unity/MAUI build and debug invoke the Rust build without a separate
  terminal-only step.

### F. Deployment, diagnostics, and packaging

- [x] Teach `cargo dotnet doctor` to enumerate declared native imports and distinguish a missing
  library, unsupported/missing RID, architecture mismatch, and missing entry point before launch.
  The scanner records the Rust symbol plus effective `#[link_name]`, checks the host-RID binary
  architecture and export table directly, and treats an unstaged system library as an explicit
  warning rather than a fabricated failure. SQLite acceptance proves the complete staged path.
- [x] Reject unsupported foreign signatures early with library, Rust symbol, native entry point,
  offending type, and supported alternatives in the diagnostic.
  The compiler now validates parameters, returns, nested callbacks, callback ABIs, and variadics.
  `pinvoke_acceptance.sh` proves both an invalid Rust reference and a non-C callback fail with the
  complete declaration identity and a concrete portable replacement, while the SQLite and retained
  asynchronous callback fixtures still execute through CoreCLR.
- [x] Make `cargo dotnet` cache keys include every code-generating input. In addition to Cargo's
  project/profile/dependency fingerprints and the private-sysroot receipt, inert `RUSTFLAGS` cfgs
  now key the backend binary, linker binary, .NET runtime/profile inputs, Source Link configuration,
  and the complete `dotnet_macros`/`mycorrhiza`/`rust-dotnet-pinvoke` SDK source trees. Regression
  tests prove an in-place SDK source edit changes the key while ignored `target/` noise does not.
- [x] Execute a clean packed consumer for every package layout, not only inspect ZIP entries.
  `nuget_acceptance.sh` now restores and executes isolated C# consumers for the portable managed,
  transitive-NuGet, bundled-helper, and RID-managed/native/resource layouts. The RID consumer uses
  SDK restore selection and runs the restored managed assembly rather than stopping at ZIP checks.
- [x] Include README, license, repository URL, symbols, Portable PDB, Source Link, XML docs, target
  profile, supported RIDs, and native dependency notices in NuGet output.
  Every package carries a validated `build/rustdotnet/package-metadata.json` contract with exact
  package/assembly/TFM identity, compatibility-profile support and host RIDs, included native RIDs,
  Source Link and sidecar state, Cargo metadata, and owner/RID/path notices matching every packaged
  native asset. The full-metadata acceptance package proves README, MIT license, repository URL,
  XML docs, Portable PDB, and Source Link configuration together; the separate Portable-PDB gate
  proves the embedded standard Source Link payload resolves logical Rust documents.
- [x] Diagnose host/profile incompatibility explicitly for VSTO, Unity, MAUI platform targets, and
  older/newer CoreCLR hosts rather than failing later in the loader.
  `cargo dotnet doctor --workspace` now joins each `<RustCrate>` to its schema-1 Cargo profile and
  checks the MSBuild profile, TFM matrix, VSTO/MAUI/WinUI markers, evidence state, and current host
  RID. VSTO gets the Excel-DNA/out-of-process alternatives; Unity explains the missing
  `netstandard2.1` artifact; planned/unsupported profiles are hard failures; preview profiles are
  explicit non-fatal warnings. Unit fixtures cover VSTO, Unity, MAUI, CoreCLR drift, and Windows
  Excel preview, while `profile_diagnostics_acceptance.sh` generates Web API and MAUI projects and
  proves supported versus planned JSON on Linux, macOS, and Windows CI.
- [x] Add named, machine-readable compatibility profiles through `cargo dotnet profiles --json`:
  `net10-coreclr`, `excel-dna-net10-windows`, `unity-netstandard2.1`, `maui-windows-net10`,
  Android, Apple, and explicitly unsupported in-process VSTO. Only `net10-coreclr` is marked fully
  supported; the others retain preview/planned/unsupported evidence states until their gates pass.
- [ ] Extend native asset handling beyond desktop RIDs to Android ABIs and iOS/Mac Catalyst static
  framework rules without pretending one host can cross-produce every binary.
- [ ] Add trimming/AOT annotations and warnings, then prove NativeAOT for Excel-DNA and each MAUI
  target that advertises it.
- [ ] Add signing guidance/hooks for NuGet, Excel `.xll`, Windows packages, Unity packages, and
  Apple/Android artifacts without storing or inventing credentials.

### G. Product fixtures, documentation, CI, and release

- [ ] Excel risk/analysis add-in: typed worksheet UDF, array/range input, managed Rust orchestration,
  native Rust numerical kernel, error mapping, packed `.xll`, and Windows x64 launch proof.
- [ ] Unity sample: managed Rust simulation library loaded by the Editor and an IL2CPP player; a
  separate native-plugin sample proves the explicit P/Invoke path.
- [ ] MAUI sample: shared C# UI plus managed Rust offline analysis/sync engine, with Windows first
  and per-platform gates added only after real execution.
- [ ] WinUI and ASP.NET/Worker fixtures exercising the same DTO, async, cancellation, disposal,
  native-facade, packaging, and diagnostic surfaces so improvements remain general.
- [ ] Update mdBook, quickstart, API docs, compatibility matrix, release notes, and troubleshooting
  around the exact shipped journeys.
- [ ] Add Linux/macOS gates for shared compiler/tooling behavior and Windows/mobile host gates for
  product execution; never turn compile-only evidence into a runtime-support claim.
- [ ] Ship only after clean-clone installation and consumer acceptance prove the documented path.

## Dependency order

1. Excel-DNA scaffold/acceptance and compatibility-profile model.
2. Typed DTOs plus standard arrays/memory and C# metadata polish.
3. Direct async export, cancellation, progress, disposal, and managed jobs.
4. Declarative native facades and native diagnostics.
5. MAUI Windows fixture and product scaffolds/attachment.
6. Unity `netstandard2.1` profile experiment and fixture; native Unity plugin in parallel only as a
   separately labelled journey.
7. Android/iOS/Mac Catalyst packaging and runtime/AOT proofs.
8. Office COM experiment; keep Excel-DNA as the supported Office route unless the experiment passes.
