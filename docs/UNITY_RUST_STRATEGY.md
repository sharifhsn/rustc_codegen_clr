# Rust-first Unity strategy

Status: Unity 6000.3.19f1 on macOS Apple Silicon is runtime-proven. The repository emits a
Unity-specific `netstandard2.1` managed facade and provides `cargo dotnet unity` staging commands.
The acceptance fixtures import and call managed Rust plus an optional native P/Invoke kernel in
EditMode and PlayMode, then build, launch, and verify both Mono and IL2CPP players. A second clean
project installs the generated UPM package with no Rust source-tree path and launches both
backends. Do not infer support for other Unity versions, operating systems, architectures, or
native plug-in platforms.

## Executive decision

The practical path to writing Unity games in Rust is not “replace every C# script on day one.” It
is a three-layer architecture in which each language owns the work it is best positioned to do:

```text
Unity scene, Inspector, rendering, input, lifecycle, and platform services
        thin generated/handwritten C# adapters
                ordinary typed .NET calls
        managed Rust game/domain assembly (rustc_codegen_clr)
                safe Rust facade
                narrow, explicit C ABI
        optional native Rust performance library (rustc + P/Invoke)
```

The primary product promise should be:

> Write the durable gameplay, simulation, rules, state, networking, save, tooling, and application
> logic in Rust. Keep Unity-specific object lifecycle and Inspector glue in small C# adapters. Move
> only measured hot kernels into native Rust.

This is already a compelling Rust-first game architecture. It preserves Unity's normal editor
workflow and gives managed Rust clean access to CLR data, delegates, tasks, events, cancellation,
and exceptions. It also avoids paying a native FFI cost for every gameplay call. Native Rust remains
available for SIMD-heavy, algorithmic, or existing-library workloads.

The eventual “zero routine C#” experience is a later tier: generate the adapters, serializers, and
preservation metadata from Rust declarations. Directly deriving Rust types from `MonoBehaviour` or
calling broad `UnityEngine` APIs from managed Rust is deliberately not the first milestone.

## Why Unity is a real new target

General C# interoperability is no longer the main unknown. The repository already has the bulk of
the language surface needed by a Unity host: exported classes and methods, DTOs and value types,
arrays and memory, collection interfaces, delegates and events, tasks and async streams,
cancellation, disposal, exception policies, nullable metadata, native facades, retained callback
guards, Portable PDBs, and a host-neutral UI dispatcher.

Unity introduces a different compatibility contract:

- Unity supports managed plug-ins targeting .NET Standard and its .NET Framework profile, but not
  .NET Core. The default cross-platform profile is .NET Standard 2.1. See Unity's
  [.NET profile support](https://docs.unity3d.com/6000.1/Documentation/Manual/dotnet-profile-support.html).
- Unity explicitly warns that not every compiler producing .NET assemblies is compatible, and
  recommends testing before committing significant work. Managed plug-ins can contain ordinary
  classes or classes attached to GameObjects, and code using Unity APIs must reference Unity's own
  assemblies. See [Managed plug-ins](https://docs.unity3d.com/6000.1/Documentation/Manual/plug-ins-managed.html).
- IL2CPP is ahead-of-time compilation. Reflection-only reachability, generated generic
  instantiations, dynamic code generation, reverse P/Invoke callbacks, and threads on the Web
  target have additional restrictions. See [Scripting restrictions](https://docs.unity3d.com/6000.1/Documentation/Manual/scripting-restrictions.html).
- UnityLinker strips managed code and needs statically visible roots or preservation metadata.
  `link.xml` must live under `Assets`, not inside a package. See
  [Preserving code using annotations](https://docs.unity3d.com/6000.1/Documentation/Manual/managed-code-stripping-preserving.html).
- Plug-ins are assets with per-platform import settings. Native plug-ins remain loaded for the
  Editor session, which changes the iteration and upgrade workflow. See
  [Import and configure plug-ins](https://docs.unity3d.com/6000.1/Documentation/Manual/plug-in-inspector.html).

The fixture pins Unity `6000.3.19f1` in `ProjectSettings/ProjectVersion.txt`. Update releases can be
added to the matrix later, but “whatever Unity Hub happens to install” is not a reproducible
compiler target.
See [Unity 6 release support](https://unity.com/releases/unity-6/support).

## Product tiers and honest claims

| Tier | User experience | Initial support claim |
|---|---|---|
| U0: native kernel | Normal Unity C# project calls an ordinary native Rust library through generated C# bindings. | Proven in EditMode and launched Mono/IL2CPP macOS players. This is P/Invoke, not managed Rust. |
| U1: managed domain | Thin C# `MonoBehaviour` adapters call a `netstandard2.1` managed Rust assembly using ordinary CLR types. | Proven for the pinned Unity version on macOS Apple Silicon in EditMode and launched Mono/IL2CPP players. |
| U2: hybrid | The managed Rust layer orchestrates game logic and privately calls a packaged native Rust kernel. | The target architecture. Requires all U1 gates plus per-platform native packaging and callback/lifetime proof. |
| U3: generated Unity adapters | Rust annotations generate the routine C# component, serialization, registration, and linker glue. | Ergonomic milestone after the contracts stabilize. Generated C# remains an intentional boundary. |
| U4: direct Unity API | Managed Rust references selected Unity assemblies and can implement selected Unity-facing types directly. | Experimental and version-gated. Promote API families individually. |
| U5: broad production matrix | Desktop, Android, iOS, and Web player support with Mono/IL2CPP distinctions. | Platform-by-platform claims only. Consoles require private SDK/devkit evidence and are never inferred. |

U1 and U2 are the strategic center. U3 makes them feel native to Unity developers. U4 is useful but
not required to make serious games mostly in Rust.

## Ownership boundaries

### C# owns the Unity edge

The C# layer should remain small and unsurprising:

- `MonoBehaviour`, `ScriptableObject`, Editor windows, custom inspectors, and property drawers.
- Serialized fields, scene and prefab references, Unity object null semantics, and asset GUIDs.
- `Awake`, `Start`, `Update`, `FixedUpdate`, `LateUpdate`, collision callbacks, and teardown.
- Calls that must touch `UnityEngine.Object` or run on Unity's main thread.
- Input System, rendering, audio, animation, Addressables, platform SDK, and other package adapters.
- IL2CPP-visible static reverse-P/Invoke trampolines where native code calls back asynchronously.

This edge should translate Unity-native values into a stable application contract, call managed
Rust, and apply the returned commands or snapshots. It should not contain game rules.

### Managed Rust owns the game

The `rustc_codegen_clr` assembly should own:

- deterministic simulation, entity/domain state, abilities, quests, inventory, economy, AI, and
  save/load policy;
- validation, orchestration, event routing, state machines, and command processing;
- network protocol and prediction/reconciliation logic that does not require a Unity object;
- data transformations, content rules, localization policy, and mod/domain APIs;
- asynchronous workflows represented as normal `Task`/`Task<T>` where the target permits them;
- typed public DTOs, records, value types, arrays, read-only lists, delegates, and events;
- safe ownership of optional native services.

Managed Rust must not retain naked Unity or CLR references in Rust layouts. Existing rooted managed
handles and lifetime guards remain the contract.

### Native Rust owns measured kernels

Native Rust is appropriate for:

- pathfinding, visibility, procedural generation, compression, codecs, cryptography, and heavy
  numerical simulation;
- existing native Rust crates that cannot compile against the managed profile;
- stable, coarse-grained operations over contiguous blittable buffers;
- background jobs whose cancellation, progress, and teardown are explicit.

It is not the default location for ordinary game logic. Thousands of tiny P/Invoke calls per frame
would produce a worse architecture than managed Rust calls and would complicate IL2CPP, platform
packaging, and profiling.

## The frame contract

The default game loop should use coarse-grained, allocation-controlled calls:

1. C# gathers input and Unity observations into blittable or managed DTO batches.
2. C# calls one managed Rust `Tick(in FrameInput, float deltaTime)` or a small number of subsystem
   methods.
3. Managed Rust advances domain state and optionally invokes a native kernel on a contiguous
   buffer.
4. Managed Rust returns a command batch, immutable snapshot, or writes into caller-owned memory.
5. C# applies transforms, animation triggers, audio, spawning, destruction, and UI changes.

The public boundary should prefer:

- small value types for scalar state;
- managed arrays or `ReadOnlyMemory<T>`/`Memory<T>` for retained buffers;
- scoped spans only for synchronous calls proven compatible with the Unity profile and IL2CPP;
- stable integer entity IDs or handles instead of retained `GameObject` references;
- command/event batches instead of chatty object-per-call APIs;
- explicit `Dispose`/shutdown for native resources and asynchronous registrations.

Performance is measured, not assumed. Each sample records calls per frame, bytes copied, managed
allocations per frame, GC collections, native transition count, and time in each layer.

## Workstream 1: model a Unity target independently from CoreCLR versions

The current runtime model conflates several distinct facts: BCL API surface, target-framework
moniker, BCL assembly identities/versions, runtimeconfig generation, helper assembly target, and
host evidence. Unity makes that coupling untenable.

Replace `DotnetVersion`/`DotnetRuntime` as the only axis with an explicit managed target contract:

```text
ManagedTarget
  profile: CoreClr10 | UnityNetStandard21
  tfm: net10.0 | netstandard2.1
  reference_set: Microsoft.NETCore.App.Ref/10 | NETStandard.Library.Ref/2.1
  framework_assembly_policy: versioned CoreCLR | Unity-compatible facade identities
  entry_shape: app | library/plugin
  runtimeconfig: required | forbidden
  helper_variant: coreclr | unity
  feature_set: named BCL capability set
  aot_policy: optional | IL2CPP-safe
```

Concrete changes:

- Add the contract to `crates/rust-dotnet-sdk-core`, serialize it into receipts, and include it in
  cache keys and `cilly`'s immutable artifact ABI envelope.
- Add a `UnityNetStandard21` target in `cilly`, without pretending it is a numbered CoreCLR
  runtime. A Unity library must not emit a CoreCLR `runtimeconfig.json`.
- Make PE `AssemblyRef` policy target-aware. Validate names, versions, public-key tokens, and type
  forwarding against assemblies from the pinned Unity installation and the .NET Standard 2.1
  reference pack.
- Make linker-provided builtins select implementations from the target's actual API capability
  table rather than `runtime.major() >= N` checks.
- Reject mixed managed-target artifacts during link with a complete diagnostic.
- Keep `net10-coreclr` unchanged; Unity work must not weaken or silently retarget the released path.

The CLI selects this through the compatibility profile, not a misleading `--dotnet 2.1`:

```bash
cargo dotnet build ./game-logic --compatibility-profile unity-netstandard2.1
```

For the pinned Unity profile the build automatically uses the no-unwind ABI (`NO_UNWIND=1`).
Panics are therefore terminal diagnostics; they are not translated into managed exceptions as they
are on the normal CoreCLR profile. Keep Unity-facing exports infallible or return explicit result
codes/DTOs.

For compatibility, `--dotnet 10` can remain the CoreCLR shorthand. A later cleanup can rename it to
`--managed-target` without breaking 0.0.1 scripts.

### U1 compiler-profile exit gate

A tiny exported Rust library must:

1. contain only the intended .NET Standard/Unity-compatible `AssemblyRef` rows;
2. carry the correct target-framework metadata;
3. have no `.runtimeconfig.json` and no CoreCLR-only companion assembly;
4. pass the internal CIL verifier and an independent PE metadata inspection;
5. load and execute in the pinned Unity Editor;
6. execute in a standalone Mono player;
7. survive UnityLinker and execute in a standalone IL2CPP player.

Compile success, `Assembly.Load` in desktop .NET, or a successful IL2CPP build without launching the
player is not sufficient.

## Workstream 2: define a Unity-safe managed API surface

`mycorrhiza` currently exposes a larger .NET surface than Unity can safely promise. A Unity profile
needs compile-time capability control rather than runtime discovery.

Split the surface into named feature families:

- `core`: primitives, strings, arrays, exceptions, delegates, basic tasks, cancellation,
  disposal, common collections, DTO/value/record metadata;
- `unity-safe`: the exact subset proven on Unity Mono and IL2CPP;
- `coreclr`: APIs that require modern CoreCLR assemblies or behavior;
- optional features such as `linq`, `dynamic-invoke`, `async-streams`, `reflection`, and
  `threading` that must earn Unity support independently.

The first Unity surface should avoid:

- `System.Reflection.Emit`, `dynamic`, runtime-generated closed generic types, and other JIT-only
  paths;
- helper code that discovers exports only through reflection;
- thread-pool assumptions on targets such as Web;
- CoreCLR implementation assemblies such as `System.Private.CoreLib` where Unity requires another
  identity;
- APIs available in .NET 10 but absent from .NET Standard 2.1 or Unity's implementation.

The profile checker must fail during the Rust build with the Rust call site and a portable
alternative. It must not wait for a Unity import error or player crash.

### Companion helper strategy

`Mycorrhiza.Interop.Helpers` currently targets `net8.0` and contains both portable helpers and
reflection-heavy conveniences. It needs one of these explicit outcomes:

1. preferably multi-target `net8.0;netstandard2.1`, with conditional source and an API-compat
   baseline for the Unity variant; or
2. split into a tiny `Mycorrhiza.Interop.Unity` assembly plus the existing CoreCLR helper.

Choose by dependency closure, not aesthetics. If `DynamicInvoker`, LINQ expression rewriting, or
async streams pull too much unsupported behavior into the Unity helper, split the helper. The Unity
artifact must be self-describing and must not accidentally copy the `net8.0` DLL.

## Workstream 3: make IL2CPP and UnityLinker first-class compiler outputs

IL2CPP support is an AOT reachability product, not a checkbox.

### Generated AOT manifest

Every Unity build should emit a machine-readable manifest containing:

- exported managed types, constructors, methods, properties, events, delegates, and interfaces;
- all closed generic instantiations created by generated Rust-facing metadata;
- delegate signatures used for managed/native transitions;
- reverse-P/Invoke entry points and their required static trampoline types;
- reflection-only uses, if any, with their reason;
- native imports, entry points, calling conventions, and target platforms.

`cargo dotnet unity generate` consumes that manifest to produce:

- a project-side `Assets/RustDotnetGenerated/link.xml` because Unity does not support `link.xml`
  inside a package;
- C# AOT hint methods for closed generic instantiations that IL2CPP cannot infer;
- `[Preserve]`/`AlwaysLinkAssembly` annotations where statically appropriate;
- static `[MonoPInvokeCallback]` C# trampolines for native-to-managed callbacks;
- a report that distinguishes statically proven reachability from conservative preservation.

Preserve the narrowest surface that works. Preserving entire assemblies is acceptable for the
first spike but is not the final product because it hides reachability bugs and increases build
time and player size.

### Reverse callbacks

The existing native callback guards solve Rust ownership and asynchronous unregistration. Unity
adds an IL2CPP entry-shape constraint. The robust route is:

```text
native Rust callback
    -> generated static C# MonoPInvokeCallback trampoline
    -> stable token lookup
    -> managed Rust callback/event dispatch
    -> Unity main-thread queue when required
```

The guard owns the token and callback until native unregistration has completed. Domain reload,
player shutdown, scene unload, and failed registration must all converge on exactly-once release.
Do not pass an arbitrary managed Rust instance method directly as a native callback and assume
IL2CPP will preserve or marshal it.

### Generics

Generated public APIs should use closed, statically discoverable generics. Macro expansion records
every constructed `Task<T>`, collection interface, delegate, and generic managed type in the AOT
manifest. Dynamic generic construction remains a compile-time error in the Unity profile unless a
user supplies an explicit rooted instantiation.

## Workstream 4: Unity package and project ergonomics

The unit of integration should be a generated Unity Package Manager package plus a small generated
project-side preservation folder:

```text
Packages/com.rustdotnet.game/
  package.json
  Runtime/
    Managed/Game.Managed.dll
    Managed/Mycorrhiza.Interop.Unity.dll
    Game.RustDotnet.Runtime.asmdef
    Adapters/*.cs
    Plugins/<platform native layout>/...
  Editor/
    Game.RustDotnet.Editor.asmdef
    RustDotnetBuildHooks.cs
    RustDotnetSettingsProvider.cs
  Tests/
    Editor/
    Runtime/

Assets/RustDotnetGenerated/
  link.xml
  AotHints.g.cs
  ReversePInvoke.g.cs
  *.meta
```

UPM packages can contain scripts, assemblies, native plug-ins, and assets; use that standard shape
rather than inventing an opaque installer. See
[Creating custom packages](https://docs.unity3d.com/6000.1/Documentation/Manual/CustomPackages.html).

All generated Unity assets get deterministic `.meta` GUIDs derived from package identity plus
logical path. Regeneration must preserve GUIDs, scene references, user-edited C# files, and Plugin
Importer settings. Generated files carry a schema/version header and are replaced atomically.

### CLI experience

The current first-run journey:

```bash
cargo dotnet new MyRustGame --unity --unity-version 6000.3
cd MyRustGame
cargo dotnet unity doctor --project .
cargo dotnet unity build .
```

The existing-project journey:

```bash
cargo dotnet unity attach /path/to/UnityProject /path/to/game-logic \
  --native-crate /path/to/native --native-export rust_entry
cargo dotnet unity package /path/to/UnityProject /tmp/game-rust-upm \
  --name com.example.game.rust --version 0.0.1
```

The command should:

1. validate the Unity project and managed/native crate contracts;
2. inspect assembly references and requested native exports before copying anything;
3. stage generated assets, adapters, linker roots, and receipt atomically;
4. leave unrelated project files untouched. Use `unity package` to materialize a UPM directory
   with deterministic `.meta` GUIDs; package output must not overlap the project and replacement
   requires `--force` plus a matching generated package name.

`cargo dotnet unity doctor` should diagnose, before Unity opens:

- unsupported Unity version or missing editor/modules;
- .NET Framework versus .NET Standard API compatibility level;
- wrong managed target or accidental `net10.0` helper;
- stale generated assets or receipt/schema drift;
- unsupported scripting backend/build target pair;
- missing native ABI, wrong architecture, missing symbol, or duplicate plug-in;
- missing project-side `link.xml`/AOT hints;
- Unity package import settings inconsistent with the current target;
- domain-reload-sensitive native library changes requiring an Editor restart.

### Build integration and iteration

Unity cannot consume an MSBuild import the same way an SDK-style C# project can. Use a generated
Editor assembly that invokes `cargo dotnet unity build` before a player build and provides menu
items for Build, Doctor, Clean Generated Artifacts, and Open Rust Project.

For ordinary development:

- a file watcher builds Rust outside Unity;
- successful output is staged to a temporary directory, fully validated, then atomically swapped;
- Unity asset refresh is debounced so one Rust build causes one reload;
- failed Rust builds never overwrite the last known-good Unity plug-in;
- native library changes show a restart-required notice because Unity cannot unload them;
- managed-only changes preserve the fastest reload path available in the pinned Unity version.

No build hook should silently run a many-minute IL2CPP build. Editor/Mono iteration and explicit
player verification are separate commands.

## Workstream 5: generated Unity adapters

The initial manually readable C# adapter establishes the contract. Once it is stable, macros can
generate the routine parts.

A possible Rust surface:

```rust
#[unity_service]
pub struct CombatSimulation {
    world: World,
}

#[unity_component(adapter = "CombatController")]
impl CombatSimulation {
    #[unity_start]
    pub fn start(&mut self, config: CombatConfig) -> Result<(), GameError> { ... }

    #[unity_update]
    pub fn tick(&mut self, input: FrameInput, delta_seconds: f32) -> FrameCommands { ... }

    #[unity_event]
    pub fn drain_events(&mut self) -> ManagedArray<CombatEvent> { ... }
}
```

This should generate a normal C# `CombatController : MonoBehaviour` with serialized fields,
construction/teardown, exception logging, frame marshaling, and preservation roots. The generated
C# remains checked into or materialized under the Unity project so a Unity developer can inspect
and debug it.

Do not make a macro guess Unity serialization for arbitrary Rust layouts. Supported serialized
fields should be an explicit, small set mapped to C# adapter fields. Complex configuration uses a
generated C# `ScriptableObject` schema or a documented DTO conversion.

### Direct Unity API phase

After U2 is reliable, experiment with a separately versioned `mycorrhiza-unity` crate generated
from the pinned Unity managed assemblies. Start with value-oriented and stable APIs:

- `Vector2`, `Vector3`, `Quaternion`, `Color`, `Bounds`, and `Ray` as verified value types;
- logging and time snapshots through adapters;
- selected non-`UnityEngine.Object` utility APIs.

Delay `UnityEngine.Object`, `GameObject`, `Component`, coroutines, engine-owned collections, and
subclassing until lifetime, null, thread, serialization, and IL2CPP behavior are proven. Bindings
must be keyed by Unity version/module assembly hash and may not imply compatibility across Unity
releases.

## Workstream 6: native plug-ins by platform

Native support is a matrix, not one `cdylib` copied everywhere.

| Target | Artifact direction | First gate |
|---|---|---|
| macOS Editor/player | universal or explicit arm64/x64 `.bundle`/`.dylib` with importer metadata | Apple Silicon Editor and standalone player launch |
| Windows Editor/player | x64 `.dll` | Windows Editor and standalone player launch |
| Linux player | x64 `.so` | Linux standalone player launch |
| Android | per-ABI `.so` under Unity's Android plug-in layout | arm64 device/emulator player launch |
| iOS | static library or `.xcframework`, called as `__Internal` after player generation | simulator plus physical-device launch and signing-owned handoff |
| Web | separately researched Emscripten/static integration; no threads by default | browser player launch, never inferred from desktop |

`cargo dotnet unity build-native --target <unity-target>` should produce a manifest with hashes,
architectures, exports, dependencies, calling convention, minimum OS, and importer settings. It
must refuse to claim cross-produced artifacts it cannot validate.

The native facade should expose:

- generated safe managed-Rust wrappers from C headers where practical;
- fixed-width integers, explicit ownership, UTF-8 buffers, and caller-owned output buffers;
- status-to-exception/result policy;
- scoped callbacks and retained-registration guards;
- background-job handles with cancellation, progress pumping, completion, and teardown;
- no Rust ABI, unwinding, borrowed references, `Vec`, `String`, trait objects, or layout-unstable
  enums across the boundary.

## Workstream 7: threading, async, and lifecycle

Unity APIs are generally main-thread-bound. Capture the installed Unity synchronization context in
`Awake`/`Start`, then pass `SynchronizationContextUiDispatcher` into managed Rust. `UiDispatcher`
can safely queue Rust closures back to the host, but the Unity profile and helper variant must prove
the path under domain reload and player shutdown.

Rules:

- `Update` pumps managed/native progress and command queues; workers never call Unity APIs.
- Every long-lived native registration is owned by a component/service and disposed before its
  native library or world state is torn down.
- Domain reload and “Enter Play Mode Options” combinations get explicit tests.
- `OnDisable`, `OnDestroy`, application quit, failed initialization, and cancellation are all
  idempotent teardown paths.
- Web support disables or replaces APIs that assume threads.
- Rust panics are caught before managed/native boundaries and become configured managed exceptions
  or fatal diagnostics; they never unwind through C or Unity.
- Async APIs are appropriate for loading, networking, saves, and tooling, not per-frame calls.

## Workstream 8: debugging and profiling

The product is not usable if a Unity developer sees only an IL2CPP crash or a native address.

Required debugging surface:

- Portable PDBs copied beside managed Rust DLLs and verified in Unity Editor debugging;
- Source Link or local source mapping to the Rust source path;
- Rust panic and managed exception messages annotated with managed type/method and Rust source;
- `cargo dotnet unity doctor --last-editor-log` that recognizes common assembly load, linker,
  `DllNotFoundException`, `EntryPointNotFoundException`, architecture, and AOT failures;
- generated adapter code with `#line`/source comments linking back to the Rust declaration;
- native debug symbols per platform where release packaging permits them;
- development-build symbolication evidence for IL2CPP and native Rust frames.

Profiling requirements:

- generated coarse markers around each managed Rust export and native facade;
- an allocation benchmark for the frame contract;
- transition counts and copied bytes visible in a development overlay or log;
- comparison among C# baseline, managed Rust, native Rust, and Burst where the algorithms are
  genuinely equivalent.

Burst is not a backend for arbitrary Rust-produced managed IL. Unity describes Burst as a compiler
for a restricted, unmanaged subset of C# and as supplemental to Mono/IL2CPP, not a replacement.
Treat Burst as a competing/complementary Unity-native option, not as evidence that managed Rust
will be Burst-compiled. See [Burst compilation](https://docs.unity3d.com/6000.0/Documentation/Manual/script-compilation-burst.html).

## The proving sample: Rust Tactics Arena

Build one product-shaped sample rather than a collection of disconnected `Add(2, 3)` fixtures.

The sample is a small deterministic tactics/simulation game:

- Unity/C# owns scene setup, input, camera, animation, rendering, audio, and Inspector config.
- Managed Rust owns unit state, abilities, turn/state machine, deterministic tick, AI decisions,
  events, save/load schema, replay hash, and test scenarios.
- Native Rust owns an optional batched pathfinding/visibility kernel.
- C# passes one frame/turn input batch and applies one returned command batch.
- A background native job demonstrates cancellation, progress, retained callback registration,
  main-thread dispatch, and safe shutdown.
- EditMode tests run the managed Rust rules without a scene.
- PlayMode tests exercise the adapter and scene.
- A standalone Mono player and a standalone IL2CPP player run the same deterministic replay and
  write the same final hash.

This sample flexes the real architecture without pretending Rust should directly render every
GameObject. It also creates a meaningful performance corpus: many agents, path queries, save/load,
and event batches.

## Test and evidence matrix

| Gate | What it proves | Required before claim |
|---|---|---|
| Static profile | PE metadata, target framework, references, helper closure, public API, native manifest | Before any Unity run |
| Editor import | Unity imports DLL/package with zero compile/import errors | U1 preview |
| EditMode | Typed calls, exceptions, DTOs, collections, dispatcher, deterministic rules | U1 preview |
| PlayMode | Lifecycle, scene adapter, reload/teardown, async pump | U1 preview |
| Standalone Mono | Built player launches and completes replay oracle | U1 preview |
| Standalone IL2CPP | Linker/AOT build launches and completes the same oracle | U1 supported on that desktop target |
| Native desktop | Packaged native kernel, callbacks, cancellation, unload/restart diagnostics | U2 per desktop target |
| Stripping matrix | Minimal/Medium/High or the current Unity equivalents preserve intended APIs | U2 supported |
| Android/iOS/Web | Real player launch on each named target with its constraints | Only that platform's claim |
| Clean consumer | Fresh repo/project installs package and follows docs without source-tree paths | Release |

Run Unity tests and builds through documented batch-mode commands and retain Editor logs, NUnit XML,
player logs, build reports, artifact manifests, and replay hashes as CI artifacts. Unity's Test
Framework supports EditMode, PlayMode, and player targets; command-line player builds should invoke
one target per Unity process because target switching in batch mode is constrained.

## CI topology

Unity licensing and editor size argue for a layered pipeline:

### Every pull request

- Rust workspace checks and existing CoreCLR acceptance remain unchanged.
- Unity profile unit tests, PE/manifest inspection, generated package golden tests, idempotence, and
  deterministic GUID checks run without Unity.
- A tiny .NET Standard 2.1 reference-assembly compatibility fixture catches accidental API drift.

### Unity presubmit

- A pinned Unity 6.3 LTS macOS or Linux editor imports the package.
- EditMode and PlayMode tests run in batch mode.
- One desktop IL2CPP player is built, launched, and checked against the replay oracle.
- Native plug-in proof runs only on platforms whose artifact is present.

The repository's `Unity macOS acceptance` workflow is intentionally `workflow_dispatch` on a
self-hosted Apple-Silicon runner labeled `unity-6000.3.19f1`: the license, pinned Editor, IL2CPP
module, and Xcode toolchain are external prerequisites that ordinary GitHub-hosted runners do not
provide. It runs both the product-shaped fixture and the clean scaffold/UPM consumer gate and
retains their NUnit, build, and launched-player logs as artifacts.

### Nightly/release matrix

- macOS Apple Silicon, Windows x64, and Linux x64 Editor/player gates.
- Mono and IL2CPP where Unity offers them.
- stripping levels and domain reload configurations.
- Android arm64 after the desktop architecture is stable.
- iOS simulator/device in credentialed Apple infrastructure.
- Web separately after threading/native packaging policy exists.

Support state in `cargo dotnet profiles --json` is generated from retained evidence. A profile moves
from `planned` to `preview` or `supported` only when required jobs for that exact Unity version,
backend, OS, architecture, and native-asset shape pass.

## Phased execution

### Phase 0: compatibility spike (complete on macOS Apple Silicon)

- `cargo dotnet new PATH --unity` creates the project-side C# adapter, asmdef, Rust crate, and
  preservation metadata.
- `cargo dotnet unity doctor` validates the editor, netstandard2.1 facade, and native module.
- `cargo dotnet unity build PROJECT RUST_CRATE` stages the managed DLL/PDB/XML and receipt
  atomically; `cargo dotnet unity native PROJECT NATIVE_CRATE --export NAME` stages a verified
  native plug-in.
- The isolated fixture proves managed and native calls in EditMode and launched Mono/IL2CPP
  players under Unity 6000.3.19f1.
- Emit a no-helper, no-native `Add`/DTO managed Rust DLL.
- Prove Editor import, method execution, Mono player, and IL2CPP player.

Exit: a retained four-part evidence bundle—PE report, Editor test, Mono player run, IL2CPP player
run. If this fails, diagnose the exact metadata/opcode/runtime incompatibility before building UX.

### Phase 1: managed domain MVP

- Establish the `mycorrhiza` Unity-safe feature set.
- Produce the Unity helper variant.
- Prove DTO/value types, arrays, delegates/events, exceptions, cancellation, disposal, and UI
  dispatch.
- Add AOT manifest and narrow linker preservation generation.
- Build the first tactics simulation slice.

Exit: U1 preview on the proven host; no native code required.

### Phase 2: product-shaped Unity workflow

- Add the current `cargo dotnet unity doctor/build/native/attach/package` workflow and keep
  `open`/player/test automation evidence-gated.
- Generate UPM layout, direct typed adapters, `.asmdef`, deterministic `.meta`, `link.xml`, and
  receipts without reflection on the default call path.
- Materialize package linker roots into project-side `Assets` with an Editor-only hook; keep
  refresh and package replacement atomic and rollback-safe.
- Verify Rider/Visual Studio can navigate generated C# and Rust sources without hand wiring.

Exit: a new user with Rust, .NET, Unity Hub, and the pinned editor can open the sample and press Play
from a clean clone using one documented setup command.

### Phase 3: hybrid/native path

- Package the native pathfinding kernel for macOS, Windows, and Linux.
- Integrate generated native wrappers, background jobs, retained callbacks, cancellation, progress,
  and teardown.
- Add architecture/export/dependency diagnostics and platform importer settings.
- Measure managed versus native boundary costs.

Exit: U2 supported on each desktop target that passes both Mono/IL2CPP and native player gates.

### Phase 4: generated component ergonomics

- Stabilize `#[unity_service]`/`#[unity_component]` contracts.
- Generate readable C# adapters and explicit serialized config mapping.
- Add event, lifecycle, exception, and disposal policies.
- Prove upgrades preserve GUIDs, scene references, and user files.

Exit: routine C# glue is generated, while complex Unity-specific presentation code remains normal
C#.

### Phase 5: platform expansion

- Android arm64 managed and native player.
- iOS simulator and device with static/xcframework native packaging.
- Web with no-thread policy and separate native integration research.
- Optional direct Unity value-type/API bindings.

Exit: claims are promoted one platform/API family at a time. Console work is out of public scope
until an authorized developer can run private platform evidence.

## Repo work map

| Area | Responsibility |
|---|---|
| `crates/rust-dotnet-sdk-core` | First-class managed target/profile contract and receipts |
| `cilly/src/artifact.rs` | Serialized Unity ABI/profile identity, never encoded as fake CoreCLR version |
| `cilly/src/ir/pe_exporter/` | Unity-compatible framework references, metadata, PDB, target-framework attributes |
| `src/` | Target-aware lowering/builtins and clear rejection of unavailable runtime behavior |
| `mycorrhiza/` | Feature-gated Unity-safe managed API and main-thread/lifetime abstractions |
| `mycorrhiza_interop_helpers/` | Multi-target or split Unity helper with AOT-safe closure |
| `dotnet_macros/` | Export manifest, generic roots, Unity adapter declarations, diagnostics |
| `crates/rust-dotnet-pinvoke` | Unity platform ABI policies and retained registration/job guards |
| `tools/cargo-dotnet/` | Unity commands, doctor, scaffold/attach, package generator, receipts, logs |
| `cargo_tests/` | Compiler and managed-surface regressions independent of licensed Unity runners |
| `feasibility/unity/` | Pinned Unity project, batch scripts, oracle, and retained runtime evidence |
| `.github/workflows/` | Static, Editor, player, native, and release evidence matrix |

## Definition of “easy to use”

The Unity effort is not finished when a maintainer can hand-copy a DLL. It is ready for a public
preview when a new user can:

1. install the released SDK and the documented Unity editor/modules;
2. run one scaffold or attach command;
3. open the Unity project without import errors;
4. inspect a small, readable generated C# adapter;
5. put configuration on a GameObject and press Play;
6. set a breakpoint or receive a source-located error for managed Rust;
7. build and launch a documented IL2CPP desktop player;
8. opt into a native Rust kernel without writing pointer/callback lifetime plumbing;
9. run `unity doctor` and receive an actionable answer when any artifact/profile/platform is wrong;
10. upgrade/regenerate without losing scene references or custom code.

The release documentation must state the exact Unity version, scripting backend, platform,
architecture, feature set, and native asset combinations that passed. “Works with Unity” without
that matrix is not an acceptable claim.

## Explicit non-goals

- Do not copy the current `net10.0` artifact into Unity and label it experimental support.
- Do not require direct UnityEngine bindings before shipping a useful Rust-first architecture.
- Do not claim that Burst compiles managed Rust output.
- Do not move ordinary game logic into native Rust solely for performance branding.
- Do not use reflection as the primary export/registration mechanism under IL2CPP.
- Do not hide generated C# glue or linker roots in an opaque binary tool.
- Do not treat an IL2CPP build that was never launched as runtime evidence.
- Do not claim Android, iOS, Web, consoles, architectures, or Unity versions by analogy.
- Do not weaken the existing verifier, managed-reference rooting rules, panic containment, or
  native callback ownership to satisfy a Unity fixture.

## Immediate next milestone

The managed `netstandard2.1` assembly, direct typed C# call surface, native symbol-verified plug-in,
package layout, EditMode/PlayMode execution, automatic scaffold scene, clean UPM consumer, and
launched Mono/IL2CPP players are now concrete on the pinned macOS host. The sample covers typed
DTO/value data, arrays, nullable values, enums, delegates, managed strings, and a deterministic
tactics-turn/replay slice. The next promotion target is cancellation, disposal, main-thread
dispatch, retained native job teardown, and broader game-domain behavior under the same gates.
