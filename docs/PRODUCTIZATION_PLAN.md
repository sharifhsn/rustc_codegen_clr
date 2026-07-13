# Rust/.NET productization plan

> Long-term integration plan. The experimental 0.0.1 GitHub SDK release is governed by
> [`RELEASE_BLOCKERS.md`](RELEASE_BLOCKERS.md), not by completion of every item here.

Status: proposed execution plan

Execution status (2026-07-11):

- Phase 0 inventory and receipts are implemented under `acceptance/` and `feasibility/`; the
  representative native-differential, managed-self-check, C#-host, MSBuild, and NuGet acceptance
  slices pass locally. The receipt correctly labels the active dirty tree as forensic, not baseline
  evidence.
- MSBuild now uses an explicit success stamp, deletes stale DLL/receipt state before a requested
  rebuild, accepts declared external inputs, rejects 32-bit consumers, and has an executable
  acceptance script.
- `cargo dotnet pack` produces byte-identical packages for identical inputs and has both a unit and
  live two-package comparison.
- `add-nuget` now delegates restore and transitive asset selection to the .NET SDK's
  `project.assets.json`; a fresh external Newtonsoft.Json binding run passes.
- Every successful cargo-dotnet artifact now receives a SHA-256 identity receipt.
- Native build, test, pack, NuGet-bindgen, and setup paths now use a content-addressed private sysroot,
  a cargo-dotnet-owned Cargo home, and an explicit build-local Cargo config. Ambient rust-src,
  registry sources, and user Cargo config remain unchanged in executable probes. The cross-process
  lock remains as a conservative cache-provisioning barrier until parallel acceptance is complete.
- A deterministic host-specific SDK bundle now packages and verifies every source-derived install
  input, atomically restores a repo-independent home, persists a per-file integrity lock, and rejects
  tampering during `doctor` and builds. This closes the checkout dependency for consumers, but the
  full Phase-1 offline distribution still needs rustup/.NET and registry-cache packaging.
- The first Primary Offerings pilot is selected and specified in
  [PRIMARY_OFFERINGS_PILOT.md](PRIMARY_OFFERINGS_PILOT.md); checked-in integration remains gated on
  the hermetic build path.

This document turns the project's existing compiler and interop capabilities into three supported
product outcomes:

1. A developer can add a Rust module to a large C# repository and use the repository's normal
   build, test, debug, publish, and deployment workflows.
2. A Rust library can be published as a NuGet package that behaves like an ordinary C# library to
   its consumers.
3. A Rust developer can use supported .NET and NuGet libraries through safe, typed, diagnosable
   Rust APIs.

The compiler has already proved much of the hard mechanism. The remaining program is primarily
about reproducibility, integration at scale, package contracts, dependency resolution, continuous
trust, and a smaller set of hard interop gaps. New feature breadth is subordinate to making these
three journeys boring and dependable.

## 1. Product definitions of done

### Outcome A: Rust in a large C# repository

This outcome is done when a developer starting from a clean clone can:

- restore the repository using documented, non-interactive tooling;
- build Rust projects as part of the normal solution build;
- edit a Rust crate or any of its inputs and never receive a stale managed assembly;
- run Rust-backed behavior through the normal C# test workflow;
- debug failures to Rust file and line information;
- publish and deploy through the repository's existing pipeline;
- observe the module through the host's logging, metrics, tracing, configuration, cancellation, and
  exception conventions;
- roll back or switch to a C# implementation without changing callers; and
- do all of this on every supported developer and CI platform.

An isolated fixture or a manually copied DLL is evidence for a mechanism, not completion of this
outcome.

### Outcome B: Rust libraries as ordinary NuGet packages

This outcome is done when a C# consumer can discover, restore, reference, inspect, debug, update,
and publish a Rust-authored package using the same workflows and expectations as a C#-authored
package. The package must have:

- stable assembly, package, namespace, and public API identity;
- deterministic contents for identical inputs;
- explicit target-framework and runtime compatibility;
- correct dependency declarations and asset selection;
- XML documentation, nullable annotations where expressible, PDB/source-link behavior, README,
  license, repository metadata, and release notes;
- API compatibility and semantic-versioning enforcement;
- signing, checksums, provenance, and an SBOM policy;
- fresh-machine and real-feed acceptance tests; and
- actionable diagnostics for unsupported exports.

Producing a valid `.nupkg` that restores once is an important proof, but not this product contract.

### Outcome C: .NET as a Rust ecosystem

This outcome is done when a Rust developer can add a supported NuGet dependency, receive the same
resolved dependency and runtime asset graph as an equivalent C# project, call supported APIs through
typed Rust bindings, and get an early diagnostic for unsupported CLR shapes. The supported path must
handle:

- framework and package references;
- target-framework compatibility;
- transitive dependencies and version conflict resolution;
- `ref/`, `lib/`, runtime, RID, and multi-assembly assets;
- generic types and methods, delegates, interfaces, tasks, exceptions, value types, and by-reference
  parameters within the declared support matrix; and
- safe, idiomatic wrappers for the highest-value APIs while retaining an explicit raw escape hatch.

The package resolver must use NuGet's resolved asset graph rather than approximate NuGet semantics.

## 2. What exists today

The following are foundations to productize, not recreate:

- The backend produces managed assemblies and the direct PE path is the default.
- `cargo dotnet` implements setup, doctor, new, build, run, test, pack, publish, and NuGet-related
  workflows under `tools/cargo-dotnet/`.
- `msbuild/RustDotnet.targets` can build one or more Rust crates and inject their assemblies as C#
  references.
- `cargo dotnet pack` emits a NuGet package containing the managed Rust assembly and package
  metadata.
- Rust-from-C# fixtures cover primitives, strings, structures, containers, generics, interfaces,
  delegates, async, LINQ/EF-shaped paths, and other interop shapes.
- `mycorrhiza` supplies raw bindings and idiomatic wrappers for a broad BCL surface.
- `cargo dotnet add-nuget` can retrieve packages and generate bindings for a useful preview subset.
- `feasibility/e2e_matrix.sh` distinguishes native differential, managed self-check, and managed
  host acceptance cases and writes explicit derived result rows. Special product scripts run through
  `record_acceptance_result.sh`, which verifies the exact completion marker and writes one atomic
  evidence file per command. `cargo dotnet capabilities` merges repeatable `--results` inputs and
  rejects conflicts, undeclared scripted evidence, or markerless passes. Results are keyed by each
  journey's explicit runtime/profile contract: incomplete coverage is `PARTIAL`, absent journeys
  remain `NOT RUN`, and strict CI mode rejects both.

Current implementation anchors:

- [`feasibility/e2e_matrix.sh`](../feasibility/e2e_matrix.sh) defines the typed oracle classes, but
  its managed C# host inventory is still small.
- [`msbuild/RustDotnet.targets`](../msbuild/RustDotnet.targets) contains the current build,
  serialization, incremental-input, and managed-reference wiring.
- [`tools/cargo-dotnet/src/pack.rs`](../tools/cargo-dotnet/src/pack.rs) contains the deterministic
  package writer. Independent clean-tree acceptance compares every entry and the final `.nupkg`
  bytes, including the managed DLL and derived provenance.
- [`tools/cargo-dotnet/src/nuget.rs`](../tools/cargo-dotnet/src/nuget.rs) contains the preview NuGet
  importer; its dependency-group union and shallow transitive walk are intentionally not general
  NuGet resolution.
- [`tools/cargo-dotnet/src/cli.rs`](../tools/cargo-dotnet/src/cli.rs) is the command-surface source of
  truth. `new --nuget-lib` and `pack --validate` in this plan are proposed commands, not current ones.
- [`docs/BCL_COVERAGE.md`](BCL_COVERAGE.md) and
  [`docs/STATE_OF_THE_PROJECT.md`](STATE_OF_THE_PROJECT.md) summarize the broadest current capability
  evidence, with the dated-snapshot caveat described in those documents.

Capability claims remain provisional until the current rearchitecture branch has a clean,
reproducible, product-level acceptance report. In particular, exit code zero alone is not a
sufficient oracle for managed fixtures.

The repository and differential matrix both name `nightly-2026-06-17`. Phase 1 extends that existing
pin into a complete lock covering the backend, target, overlays, generated bindings, and helper
assemblies so developer, CI, and release builds share the same effective input contract.

## 3. Program invariants

These rules apply throughout the roadmap:

1. No silent stale artifacts. A missed rebuild is a correctness failure.
2. No silent interop degradation. Unsupported shapes fail during build or verification.
3. No capability claim without an executable oracle.
4. No custom approximation where the platform already exposes an authoritative result. Cargo owns
   Rust dependency resolution; NuGet/MSBuild owns .NET asset resolution.
5. Public package and ABI changes are reviewed as product contracts, not implementation details.
6. The ordinary developer path may not require knowledge of the backend repository.
7. A clean clone and a fresh package cache are first-class test environments.
8. Serial and parallel builds remain separate claims until deterministic equivalence is proved.
9. Generated capability documentation must be derived from the acceptance manifest.
10. High-risk compiler changes retain the fatal verifier and full backend regression gate.

## 4. Execution phases

### Phase 0: trustworthy baseline

Goal: establish one clean source of truth before expanding the product surface.

Work:

- Reconcile the current rearchitecture working tree into reviewable changes or explicitly preserved
  generated artifacts.
- Finish the post-Edition-2024 acceptance run.
- Observe the unified release-scope evidence merge green in CI, then retain the immutable artifact.
- Execute and retain the broader .NET 8/10 debug/release presubmit cells declared by each journey.
- Keep `acceptance/capabilities.toml` as the machine-readable inventory of journeys, explicit
  runtime/profile contracts, fixtures, evidence kinds, oracle types, and expected artifacts.
- Generate the human capability matrix only from that manifest and validated result files.
- Record hashes and metadata for the backend, toolchain, target specification, overlays, generated
  bindings, helper assemblies, and produced artifacts.
- Run every acceptance case from a clean target directory and, where relevant, an empty NuGet cache.
- Separate fast presubmit, extended presubmit, nightly, and release gates.

Required CI oracle classes:

| Oracle | Meaning | Minimum examples |
|---|---|---|
| Native differential | stdout, stderr where stable, and exit behavior match native Rust | pure Rust, std, ecosystem crates |
| Managed self-check | managed execution reaches explicit assertions and a completion marker | BCL, async, collections |
| C# host | the real C# consumer compiles and calls the rebuilt Rust artifact | export, interfaces, EF/DI-shaped paths |
| NuGet consumer | a fresh project restores from a feed, compiles, runs, and inspects metadata | pack and dependency round-trip |
| Rust NuGet consumer | NuGet restores assets; generated Rust calls the selected assembly | direct and transitive packages |
| Artifact inspection | PE/PDB/package/API metadata match the contract | deterministic pack and symbols |

Exit gate:

- clean-clone debug and release acceptance passes on the supported reference platform;
- each green row contains its decisive oracle marker;
- rerunning from identical inputs yields identical distributable artifacts, excluding documented and
  justified signatures; and
- the generated capability report contains no manually asserted pass state.

### Phase 1: hermetic build and install architecture

Goal: make the toolchain safe to invoke from large solutions.

Work:

- Stop mutating a shared `rust-src` during ordinary builds. Provision immutable, content-addressed
  toolchain inputs during setup.
- Give every invocation isolated intermediate and output directories while allowing safe shared,
  immutable caches.
- Define a toolchain lock containing the rustc commit/nightly, backend build, target specification,
  PAL/overlay registry, helper assembly, and `cargo-dotnet` version.
- Teach `doctor` to validate the lock and report drift.
- Make setup resumable, non-interactive, checksum-verified, and usable without the source checkout.
- Produce a prebuilt-toolchain distribution for each supported host.
- Emit a build receipt beside every managed Rust artifact describing all effective inputs.
- Define the runtime-architecture contract. Until architecture-correct target artifacts and NuGet
  asset selection exist, x86_64 Rust assemblies must carry an enforceable 64-bit process/build guard
  despite being managed PE files. Add a negative x86-host test so an AnyCPU consumer cannot silently
  load code whose Rust layout assumes 64-bit pointers.
- Make debug information portable and consumer-ready: deterministic/path-mapped source paths,
  correct Portable PDB sequence points in debug and optimized builds, exception-break behavior,
  Source Link policy, and IDE launch/attach instructions.

Current-tree evidence: `cargo dotnet bundle create|verify|install` now produces a deterministic,
host-bound, checksummed SDK home that installs atomically outside the checkout and remains
tamper-checked by every build and `doctor`; `install_bundle_acceptance.sh` exercises that path. This
is not yet the whole-machine/offline toolchain distribution: rustup, .NET, and restored registry
caches remain prerequisites. `pdb_consumer_acceptance.sh` now proves a normal C# host consumes the
public Rust library PDB and resolves a Rust `file.rs:line` frame in debug and release. Remaining
debug work is an actual IDE breakpoint, remote fetch, exception-break, optimized stepping, and
locals-window oracle. Cargo-dotnet already remaps machine paths to stable logical roots and can now
embed a standard, fingerprinted Source Link map for `/_/consumer/*`; the C# host parses it in both
profiles. Whole-method LocalScope and named LocalVariable metadata are present and consumer-parsed;
debug retains a known user local, while optimized builds do not promise every source local survives.
Nested lexical scopes and multi-repository dependency/source maps are not modeled.

Exit gate:

- two Rust projects build concurrently without shared mutation;
- a fresh developer machine can install and build without cloning this repository;
- changing any locked component invalidates the correct artifact; and
- an offline build succeeds after an explicit restore/setup step;
- supported processes are proven architecture-compatible and unsupported process architectures fail
  before loading the Rust assembly; and
- a clean consumer can resolve a Rust stack frame to the intended source file and line.

### Phase 2: dependency-aware MSBuild integration

Goal: make normal solution builds correct, incremental, diagnosable, and fast.

Work:

- Replace the current hand-maintained input approximation with an input fingerprint derived from
  Cargo metadata, build-script outputs, configuration, toolchain lock, overlays, generated inputs,
  and relevant environment.
- Track workspace members and transitive path dependencies.
- Model Rust artifacts as declared MSBuild inputs/outputs with a stamp or receipt that is updated only
  after a successful build.
- Preserve the last good DLL only as an explicit diagnostic artifact; never reference it after a
  failed requested rebuild.
- Support safe parallel builds after Phase 1 removes shared mutation.
- Forward structured diagnostics and timings into MSBuild and IDE logs.
- Add deterministic cleaning and cache diagnostics.
- Wire Portable PDBs and source mapping through ordinary `dotnet build`, test, publish, and IDE
  workflows; validate stepping, optimized stack traces, exception breaks, and source lookup rather
  than treating PDB existence as the debugger oracle.

Acceptance cases:

- no-op rebuild;
- leaf Rust source edit;
- transitive path-dependency edit;
- feature, profile, target, `Cargo.lock`, build script, and generated-file edits;
- backend, toolchain, overlay, and helper changes;
- deletion and rename of source files;
- multiple Rust projects built concurrently;
- failed rebuild after a previously successful build; and
- `dotnet build`, `dotnet test`, IDE build, `dotnet publish`, and NativeAOT publish.
- x64 host success plus an explicit unsupported-architecture failure; and
- debugger launch/attach, breakpoint, step, exception-break, and optimized stack-trace checks.

Exit gate:

- every meaningful input edit rebuilds exactly the affected Rust projects;
- no-op builds perform no backend work;
- failed builds cannot execute or package stale artifacts; and
- large-solution parallel builds are deterministic.

### Phase 3: production-shaped C# repository pilot

Goal: prove Outcome A in `primary-offerings` or a repository with equivalent operational demands.

Pilot selection criteria:

- deterministic and heavily fixture-tested;
- useful enough to exercise the real workflow;
- behind a narrow C# interface;
- no database ownership or irreversible side effects in the first pilot;
- easy to run in shadow or dual-execution mode; and
- cheap to fall back to C#.

Good initial shapes include calculation, validation, normalization, parsing, policy evaluation, or
document/payload transformation. Avoid authentication, money movement, migrations, scheduling,
network orchestration, and partner side effects in the first slice.

Required pilot contract:

- checked-in Rust crate and ordinary solution/project wiring;
- C# interface plus C# and Rust implementations;
- DI-controlled selection and immediate rollback;
- shared golden fixtures and property/differential tests;
- C# logging, tracing, metrics, configuration, cancellation, and exception mapping;
- clean-clone developer and CI workflows;
- debugger proof from a C# call through a Rust frame;
- container/publish/deploy proof in a non-production environment;
- shadow comparison before serving Rust results; and
- an operational runbook that assumes the responder is primarily a C# developer.

Exit gate:

- an unfamiliar developer can edit, build, test, debug, and deploy the module through normal repo
  commands;
- the deployed process architecture is enforced rather than inferred from an AnyCPU assembly;
- shadow comparison meets a predeclared equality/error budget;
- rollback requires configuration or ordinary deployment only; and
- the pilot stays green for a defined soak period before a second module is attempted.

### Phase 4: release-grade NuGet production

Goal: prove Outcome B for both local and real-feed consumers.

Work:

- Make package creation byte-for-byte deterministic for unsigned packages.
- Define package ID, assembly name, root namespace, version, and public symbol stability rules.
- Generate a reference/API baseline and enforce compatible SemVer changes.
- Publish XML docs and nullable metadata; add Source Link and symbol/source packages where supported.
- Define TFM and RID policy instead of inferring broad compatibility from one successful target.
- Model Rust crate, NuGet, helper assembly, and native/runtime dependencies explicitly.
- Publish `Mycorrhiza.Interop.Helpers` or replace bundling with a versioned, supportable dependency
  contract.
- Add package validation, signing, checksums, SBOM, provenance, and license policy.
- Add `cargo dotnet new --nuget-lib` and a consumer sample.
- Add `cargo dotnet pack --validate` and a release-oriented push workflow with dry-run support.
- Test upgrade, downgrade, cache, package-lock, and central package-management behavior.

Consumer matrix:

- SDK-style C# console and class-library consumers;
- local feed and authenticated remote feed;
- clean global package cache;
- package reference with and without central package management;
- debug symbols/source inspection;
- supported .NET versions, OSes, architectures, and publish modes; and
- direct package dependencies plus transitive NuGet and runtime assets.

Exit gate:

- a package passes platform package validation and API compatibility checks;
- a fresh out-of-tree consumer needs only a normal `PackageReference`;
- IntelliSense and debugging expose the intended managed API; and
- the same package passes the declared consumer matrix.

### Phase 5: authoritative NuGet consumption from Rust

Goal: replace the preview resolver with the platform's resolved asset graph and prove Outcome C.

Architecture:

1. Generate or host a minimal SDK-style restore project for the requested packages and target.
2. Let NuGet/MSBuild produce `project.assets.json` and restore diagnostics.
3. Read the selected compile and runtime assets, framework references, dependency graph, versions,
   and RID fallbacks from that file.
4. Generate bindings per selected reference assembly while retaining assembly identity.
5. Copy or reference runtime assets according to the resolved graph.
6. Record the resolved graph in the Rust artifact receipt and NuGet package metadata.

Dependency and supply-chain state must be explicit rather than hidden in generated side effects:

- define a checked-in declarative package/target/source specification;
- define lock-file generation, locked restore, update, downgrade, and removal semantics;
- pin the effective .NET SDK and NuGet resolver versions;
- support source mapping, authenticated/private feeds, and credential isolation;
- verify package hashes/signatures according to policy and protect caches from source substitution or
  poisoning; and
- make generated bindings and copied runtime assets reproducible products of the locked restore.

Do not independently reimplement version ranges, dependency conflict resolution, TFM reduction, RID
fallback, or `ref/` versus `lib/` selection.

Acceptance package ladder:

1. one assembly, no dependencies;
2. ordinary transitive dependency;
3. version range and conflict;
4. `ref/` plus runtime implementation;
5. multiple assemblies;
6. async and generic APIs (closed generated `Task<T>`, `ValueTask<T>`, and `IAsyncEnumerable<T>`
   returns are supported and directly consumable; arbitrary constructed and nested generics remain
   a follow-on);
7. interface/delegate callbacks;
8. runtime/RID assets; and
9. unsupported analyzer/source-generator/content-only packages with clear diagnostics.
10. private authenticated feed and source mapping;
11. locked restore, deliberate update, and package removal; and
12. tampered package, conflicting source, or cache-substitution rejection.

Exit gate:

- the resolved and locked asset graph matches an equivalent C# project using the same SDK, sources,
  credentials policy, and target;
- supported packages compile and run through generated Rust bindings;
- unsupported package or API shapes fail before execution with an actionable reason; and
- repacking a Rust library records correct NuGet dependencies rather than flattening assemblies.

### Continuous workstream: product-blocking interop gaps

Goal: close only backend gaps that block an accepted product journey. This workstream begins during
Phase 0 discovery and gates the affected Phase 3, 4, or 5 exit condition; it is not postponed until
after those journeys. Non-blocking breadth remains deferred until the core journeys are supported.

Candidate gaps include:

- by-reference generic `out` parameters;
- carrying the managed exception object across the catch boundary;
- async-stream production and coroutine-held managed handles (consumer-side `IAsyncEnumerable<T>`
  iteration is shipped and matrix-proven);
- remaining delegate arities and event shapes;
- virtual/base-class behavior needed by framework integration;
- nested generic return and value-type shapes; and
- nullable metadata and richer managed signature projection.

Each candidate starts with a minimal real API, an ABI/type-system design, a negative test, and a
product acceptance case. Pure-library wrappers may proceed independently; changes to `cilly`, the
verifier, marshalling, exception handling, layout, or exporter metadata require a written design and
the full backend gate.

Exit gate for each gap: the accepted API works through both a focused regression and its real
consumer journey, without weakening existing verifier invariants.

### Phase 6: scale, support, and release governance

Goal: operate the platform instead of treating it as a sequence of demonstrations.

Work:

- publish supported-version and deprecation policies;
- create compatibility dashboards generated from CI;
- add nightly rustc drift, .NET servicing, and NuGet ecosystem canaries;
- define security response, package revocation, and toolchain rollback procedures;
- maintain a small set of flagship applications rather than hundreds of unowned demos;
- collect build-time, artifact-size, runtime, allocation, startup, and failure metrics;
- establish release notes and migration tooling; and
- upstream generally useful fixes where practical.

Exit gate: two consecutive releases follow the documented release process, and at least two
independent real consumers remain green across an upgrade.

## 5. Workstream dependency graph

```text
Phase 0: trustworthy baseline
  -> Phase 1: hermetic toolchain
      -> Phase 2: correct MSBuild scale
          -> Phase 3: primary-offerings pilot
  -> Phase 4: release-grade NuGet publishing
  -> Phase 5: authoritative NuGet consumption

Continuous: product-blocking interop gaps
  -> may gate any Phase 3, 4, or 5 exit

Phases 3, 4, and 5
  -> Phase 6: support and release governance
```

Phase 0 gates every other implementation phase. Read-only pilot discovery begins during Phase 0 so
the real consumer constrains the infrastructure design; checked-in pilot integration begins only
after the hermetic and stale-artifact gates are green. Phase 4 and Phase 5 can run in parallel once
their shared package identity and artifact contracts are approved. A product-blocking interop gap is
inserted before the exit gate of the journey that needs it.

## 6. Multi-agent operating model

The objective is to reserve frontier reasoning for decisions with high blast radius while using
cheaper agents for evidence-heavy work.

### Roles

| Role | Default model class | Responsibilities |
|---|---|---|
| Program/root agent | balanced Terra by default | scope, dependency ordering, evidence tracking, final integration |
| Architecture reviewer | big Sol/frontier | ABI, verifier, hermetic builds, package contracts, NuGet architecture, security review |
| Implementation agent | balanced Terra or focused Sol worker | bounded implementation against an approved contract |
| Evidence scout | Luna | inventories, fixture discovery, CI logs, compatibility sweeps, generated consumers |
| Adversarial reviewer | big Sol/frontier | correctness, stale artifacts, supply chain, ABI/API compatibility, regression gaps |

### Dispatch policy

Use Luna for:

- cataloging fixtures and mapping them to capability rows;
- reconciling documentation against executable evidence;
- classifying CI failures and minimizing reproductions;
- generating repetitive consumer/restore/build matrices;
- inspecting package contents and comparing artifact manifests; and
- broad compatibility surveys whose failures are escalated with evidence.

Use balanced implementation agents for:

- bounded `cargo-dotnet`, MSBuild, scaffolding, documentation-generation, and `mycorrhiza` changes;
- focused acceptance fixtures; and
- implementation of an already-approved design with localized blast radius.

Use big Sol/frontier reasoning for:

- assembly identity, ABI, layout, marshalling, exceptions, and verifier rules;
- hermetic build and cache architecture;
- NuGet restore/asset selection and public packaging contracts;
- reproducibility, signing, provenance, and threat modeling;
- selection and boundary design for the real repository pilot; and
- review of any change that can silently alter generated code or public behavior.

### Four-slot topology

For the current four-agent configuration, use these waves:

| Wave | Root slot | Slot 2 | Slot 3 | Slot 4 |
|---|---|---|---|---|
| Discovery | Terra orchestrator | Luna evidence scout A | Luna evidence scout B | idle or bounded Sol design question |
| Design | Terra orchestrator | bounded Terra prototype | Luna reproduction/fixture work | Sol architect |
| Implementation | Terra orchestrator | Terra implementation A | Terra implementation B only if file scopes do not overlap | Luna focused validation |
| Review | Terra orchestrator | Luna matrix/log partition A | Luna matrix/log partition B | Sol adversarial reviewer |

Do not keep a Sol slot running throughout a workstream. Invoke it with the evidence packet and a
named decision, then release it after the design or review is returned. If implementation scopes
overlap, use one implementer and spend the free slot on validation rather than parallel editing.

### Per-work-item loop

1. A Luna scout produces an evidence packet: current behavior, exact files, tests, and failure.
2. The root agent classifies the item as library/tooling, product contract, or compiler/runtime.
3. A frontier architect writes or approves the contract for high-risk items.
4. A bounded implementation agent changes one independently verifiable slice.
5. CI scripts run focused and matrix validation and preserve raw, machine-readable results; agents
   may trigger or summarize them but never serve as the acceptance authority.
6. A frontier reviewer examines only high-risk diffs, failed oracles, and contract changes.
7. The root agent integrates only when the declared acceptance row is green.

No agent may mark a journey complete from exit status alone. Every task must name its decisive
oracle before implementation begins.

### Cost controls

- Prefer one frontier design/review pass over several frontier implementation agents exploring the
  same space independently.
- Default to one Terra orchestrator, up to two bounded implementation/scout agents, and one Sol
  reviewer only when a named high-risk seam is active. Raise concurrency only for independent,
  repetitive matrix partitions.
- Give scouts narrow file lists and machine-readable output formats.
- Give every delegated workstream an explicit file scope, deliverable, test budget, and stop
  condition. Terminate the scout after its evidence packet is delivered.
- Cache clean-clone setup and immutable toolchains, but never cache the decisive product artifact
  across a rebuild oracle.
- Run focused tests on every slice; run expensive matrices at merge boundaries, nightly, and release.
- Escalate to frontier review only for failed evidence, cross-boundary designs, or high-blast-radius
  files.
- Escalation triggers are: public ABI/API change, verifier or layout change, package identity or
  resolver change, hermeticity/cache change, security boundary change, conflicting evidence, or a
  repeated failure without a minimized reproduction.
- Stop parallel work when two agents would edit the same contract or compiler seam.

### High-risk file ownership

Changes in these areas require frontier design or review:

- `cilly/src/ir/`, linker, exporters, PE/PDB metadata, and type verification;
- root lowering of ABI, layout, calls, exceptions, generics, and comptime interop;
- `msbuild/RustDotnet.targets` dependency and stale-artifact semantics;
- `tools/cargo-dotnet` artifact identity, package resolution, setup, and reproducibility;
- `dotnet_macros` public signature projection and marshalling; and
- package signing, provenance, API compatibility, and release workflows.

## 7. Initial issue sequence

The first implementation sequence should be:

1. **P0 — Freeze and reconcile the source baseline.** Reconcile the active tree into reviewable
   changes and identify the exact source revision whose behavior is being claimed.
2. **P0 — Read-only pilot discovery.** Select candidate `primary-offerings` seams and derive their
   platform, architecture, observability, deployment, security, and rollback constraints before
   generic infrastructure contracts are finalized.
3. **P0 — Acceptance manifest and truthful CI.** Inventory fixtures from the frozen baseline, assign
   oracle classes, wire a representative product matrix into presubmit, and publish raw results.
4. **P0 — Clean rearchitecture acceptance.** Produce the first clean baseline report.
5. **P0 — Toolchain/artifact receipt design.** Approve the content-addressed input and build receipt
   contract.
6. **P0 — Architecture and debugging contract.** Enforce the supported process architecture and
   define the source/PDB/IDE debugger oracle.
7. **P0 — Hermetic setup/build.** Remove shared `rust-src` mutation from ordinary builds.
8. **P0 — MSBuild invalidation suite.** Add failing tests for transitive inputs and stale-artifact
   execution before changing the targets implementation.
9. **P0 — MSBuild dependency fingerprints and parallel safety.** Implement against those tests.
10. **P1 — Pilot boundary approval.** Choose one of the discovered slices with the owning team and
   approve its operational contract.
11. **P1 — Pilot skeleton and shadow oracle.** Add the checked-in Rust/C# dual implementation without
   serving Rust results by default.
12. **P1 — Deterministic NuGet and package identity.** Establish the public artifact contract.
13. **P1 — NuGet restore delegation and supply-chain state.** Replace custom asset selection with a
    locked `project.assets.json` workflow and explicit source/security policy.
14. **P1 — Fresh-consumer matrices.** Prove publishing and consumption on clean machines/caches.
15. **Continuous — Product-blocking interop gaps.** Insert each approved gap before the journey exit
    it blocks; do not batch these as late capability work.

## 8. Evidence and reporting

Each milestone produces:

- a machine-readable result file;
- the exact source, toolchain, backend, package, and environment identities;
- build and execution logs;
- artifact hashes and inspection results;
- a list of skipped rows with explicit reasons;
- a generated human summary; and
- a rollback or containment note for changes that affect consumers.

Use these states consistently:

- **implemented** — code exists;
- **focused-proven** — a focused executable oracle passes;
- **matrix-proven** — the declared platform/profile matrix passes;
- **pilot-proven** — a real repository journey passes;
- **release-supported** — documented, packaged, governed, and continuously tested; and
- **deferred/unsupported** — rejected early with an actionable diagnostic.

Only `release-supported` satisfies the three outcomes in this plan.

### 2026-07-11 execution snapshot

- **focused-proven:** private sysroot/private Cargo-home builds with artifact receipts; MSBuild
  forced/no-op/missing-receipt/failed-rebuild/x86 gates; deterministic custom-ID NuGet packaging
  consumed and executed by a fresh C# project; SDK-based Newtonsoft.Json restore/bindgen.
- **pilot-proven (contained):** the solution-excluded Primary Offerings AIP 052 pilot builds its Rust
  crate through MSBuild and passes differential C#-authority tests for representative string,
  nullable date, currency, and exact implied-decimal fields. Production DI remains unchanged.
- **not release-supported:** unique managed export namespaces/types, full AIP DTO projection and
  full-field shadow corpus, RID/native NuGet assets, automatic transitive path-input fingerprints,
  Windows MSBuild, Alpine deployment proof, clean-worktree reproducibility, signing/SBOM/API
  compatibility, and release governance remain explicit gates.

## 9. Immediate decision record

- Productization, not additional capability breadth, is the current priority.
- The first real repository integration uses a dual C#/Rust interface and shadow comparison.
- The first pilot does not own side effects or persistence.
- NuGet dependency selection will be delegated to NuGet/MSBuild.
- Large-solution support requires hermetic builds before parallelism is enabled.
- Product claims are generated from executable acceptance evidence.
- Frontier reasoning is concentrated on contracts and high-blast-radius reviews; evidence gathering
  and repetitive matrices use cheaper agents.
