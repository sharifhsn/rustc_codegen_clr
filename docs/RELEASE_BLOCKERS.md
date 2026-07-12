# Release blockers for transparent Rust/.NET integration

This is the go/no-go list for the three product outcomes in
[`PRODUCTIZATION_PLAN.md`](PRODUCTIZATION_PLAN.md). A focused proof or pilot is not a release claim.
The dependency-ordered implementation program is in
[`RELEASE_EXECUTION_PLAN.md`](RELEASE_EXECUTION_PLAN.md).

## Closed in the current productization working tree

- Ordinary native builds use a content-keyed private sysroot and private Cargo home; ambient
  `rust-src` and registry sources are not patched. The source key includes the complete ambient
  rust-src tree, and snapshot files are copied rather than hardlinked.
- Every successful Rust assembly has a SHA-256 identity receipt. MSBuild treats a missing receipt as
  stale, deletes all prior evidence before a requested rebuild, and cannot execute a stale DLL after
  a failed rebuild.
- MSBuild rejects 32-bit consumers and invalidates on tool binary, target, profile, .NET version,
  integration logic, crate sources/manifests, and explicit external inputs.
- Cargo-dotnet and MSBuild enforce the current Linux/macOS host boundary before operational work;
  Windows hosts receive a stable diagnostic until Windows acceptance exists.
- MSBuild derives the reachable local Cargo package/file closure automatically. Path/workspace
  dependency and build-script-owned input mutations rebuild once and then return to a stable no-op;
  the closure hash is included in the artifact receipt when MSBuild generated it.
- NuGet ZIP output is deterministic for identical inputs, emits a checksum, validates package
  structure, preserves CLR assembly filename/identity when package ID differs, and is executed by a
  fresh custom-ID `PackageReference` consumer.
- NuGet binding generation delegates version/dependency selection to SDK restore and
  `project.assets.json`, including requested RID runtime/native/resource assets. Owned staging is
  atomic, removes stale graphs, rejects ambiguous paths, and preserves RID-relative paths when
  packing and restoring a fresh C# consumer.
- Native builds and MSBuild's Cargo metadata discovery both layer alternate source configuration
  with copied credentials into an empty private Cargo home. The authenticated local sparse-registry
  acceptance proves source resolution, leaves the ambient home untouched, and rejects token leaks
  from logs, receipts, and build artifacts.
- The contained Primary Offerings pilot compiles a checked-in Rust crate through MSBuild and runs
  representative differential tests without changing production DI or the solution build.
- Package metadata selects distinct managed module identities; collision preflight rejects duplicate
  assemblies/types, the two-library C# fixture executes both, and focused PE metadata coverage proves
  direct-PE self references retain the internal `MainModule` sentinel while emitting the public type.
- `#[dotnet_dto]` emits real CLR constructors/properties. The managed consumer proves exact
  `System.Decimal` scale, nullable `System.DateOnly`, and `System.String` roundtrips. Export names can
  be idiomatic C# identifiers, and opt-in `Result<T, E>` errors become catchable managed exceptions
  for GC-safe value payloads. `Result<managed handle, E>` is rejected at compile time.
- The isolated Primary Offerings AIP pilot projects all 82 fields to a typed CLR DTO and compares
  names, CLR types, values, decimal scale, null mutations, field-end truncations, invalid
  date/decimal/enum values, suffixes, and Unicode across 169 generated corpus records. Production
  DI and solution membership remain unchanged.
- The solution-excluded pilot has a compiler-free `PackageReference` mode: a unique immutable
  package restores and passes all nine local parity tests without Rust SDK targets. A focused .NET
  8 Alpine publish/runtime harness restores the immutable package, publishes the real PO dependency
  closure, and executes the DTO marker under musl. Portable `libc`/`libm` P/Invoke names prevent the
  build host's macOS libraries from leaking into the assembly.
- Distinct consumer builds use canonical-crate-keyed Cargo homes and locks; registry PAL mutation,
  logs, overlay config, XML scratch, targets, and receipts are no longer shared across crates. The
  private-sysroot publisher and shared C# helper retain narrow locks. The hermetic acceptance uses a
  two-process barrier inside the build-stage lifetime, proving real overlap while also proving
  distinct caches/receipts and unchanged ambient rust-src/registry files.

## Generic ABI P0

- [x] Freeze managed ABI schema 1 to the behavior the toolchain actually emits: independent Cargo,
      NuGet, CLR assembly, namespace, and type identities; explicit Rust-snake/C#-Pascal member
      naming; mutable typed DTO properties plus arbitrary-arity construction; structural nullable
      value types; reference-nullability-oblivious metadata; author-owned enum/domain-error evolution;
      and conservative major-SemVer enforcement for every reflected public API change.

## Reference integration evidence (not product-specific release requirements)

Primary Offerings is retained as a demanding external-style fixture. Further convenience or product
integration in that repository is not required for the general SDK release.
- [x] Freeze the AIP 052 ASCII/BMP, surrogate-rejection, truncation, suffix, field-layout, and value
      rules as `monark.aip.position.052/v1`, backed by the typed 82-field, 169-record corpus.
- [x] Add a production shadow adapter that always serves the C# result, catches every Rust
      exception, bounds concurrent timed-out calls, emits value-free OpenTelemetry metrics, and uses
      a rotated keyed HMAC for log correlation. Both composition roots bind a default-off kill switch;
      focused DI tests prove disabled authority and enabled decorator resolution.
- [x] Prove the selected delivery model in Primary Offerings' real Alpine build/publish/runtime
      container. Prefer feed-first immutable NuGet consumption over installing a compiler in every
      application image.

## P1 before public package release

- [ ] Reproduce packages from independent clean worktrees and empty caches, not one reused target.
- [x] Include XML docs, symbols, artifact receipt/provenance, SBOM, license inventory, and source
      revision; define signing and trusted publishing.
- [x] Add metadata-only API compatibility baselines and enforce a major SemVer increase for any
      reflected public-surface change (conservative until additive-change classification exists).
- [x] Keep Windows outside the published host matrix until Windows build/MSBuild/package acceptance
      exists. Cargo-dotnet and MSBuild enforce the current Linux/macOS operational boundary before
      mutation; metadata/help remain available for diagnosis.
- [x] Remove the conservative global build lock after a parallel shared-cache corruption matrix.
- [ ] Pin the compiler/backend/tool crates to an immutable release tag and publish a documented
      rollback procedure.

Only when all applicable P0/P1 checks are continuously green may documentation call the integration
"just like importing a C# library" or `release-supported`.
