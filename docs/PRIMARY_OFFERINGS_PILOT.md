# Primary Offerings Rust pilot contract

Status: contained typed pilot and feed-first Alpine acceptance harness implemented; final Alpine
runtime execution and production integration remain gated

Source inspected: `/Users/sharif/Code/monark/primary-offerings`, clean `main` at `4ffca66e0`.

## Selected first pilot: AIP position parsing

The first production-shaped pilot should implement the existing `IAipPositionParser` contract in
Rust while retaining the current C# parser as the authority and rollback path.

Why this boundary:

- `IAipPositionParser.Parse(ReadOnlySpan<char>)` is already a narrow dependency-injection seam.
- `AipPositionParser` is a deterministic fixed-width Record Type 052 parser with no database,
  network, filesystem, clock, or partner side effects.
- Existing tests provide a fully populated roughly 988-character fixture and assert the mapped
  record fields.
- The surrounding `AipFileParsingService` already separates parsing from SFTP/database/logging
  orchestration and has a fake parser in tests.
- A mismatch can be observed without serving Rust output or changing settlement state.

Primary Offerings anchors:

- `primary-offering-web-common/Services/Aip/Parsers/IAipPositionParser.cs`
- `primary-offering-web-common/Services/Aip/Parsers/AipPositionParser.cs`
- `primary-offering-web-common/Services/Aip/AipFileParsingService.cs`
- `primary-offering-web-common/Extensions/ServiceExtensions.cs`
- `primary-offering-web-tests/Services/Aip/AipPositionParserTests.cs`
- `primary-offering-web-tests/Services/Aip/AipFileParsingServiceTests.cs`

## Pilot shape

1. Keep the C# implementation registered and authoritative.
2. Add a Rust library with a managed export that accepts the fixed-width record and returns a
   managed result representation with explicit parse diagnostics.
3. Introduce a dual parser in C# that calls both implementations for sampled or configured traffic.
4. Canonicalize both results before comparison so representation-only differences do not create
   noise.
5. Emit only counters, timings, field identifiers, and a non-reversible input correlation hash.
   Never log the position line or parsed investor/account data.
6. Serve the C# result throughout the shadow period.
7. Enable the Rust result only after the declared corpus and soak gates pass.
8. Retain an immediate configuration/DI rollback to C#.

## Acceptance corpus

- The existing fully populated golden record.
- Minimum/blank optional fields.
- Every date, decimal, integer, enum, flag, and mapping boundary.
- Short, long, malformed, and unsupported record types.
- Non-ASCII and invalid-character inputs.
- Decimal scale, sign, leading zero, and maximum-value cases.
- A production-derived sanitized corpus with no retained PII.
- Property tests for fixed-width slicing, total consumption, and no panic on arbitrary input.

The native Rust implementation, managed Rust assembly, and C# implementation must agree on the
canonical result or the same classified error for every required corpus row.

## Operational gates

- Normal Primary Offerings clean-clone build, test, container, and publish paths build the Rust
  module without a backend-repository checkout.
- The Rust project is an ordinary solution dependency and obeys no-op/invalidation/stale-artifact
  MSBuild acceptance.
- The deployed process architecture is explicitly 64-bit.
- C# logging/tracing/cancellation/error conventions remain intact at the boundary.
- Debug symbols resolve a managed call through the Rust frame to Rust source and line.
- Shadow comparison has a predeclared mismatch budget of zero for golden/sanitized fixtures and an
  explicitly approved operational budget for sampled live traffic.
- Latency and allocation budgets are measured against the C# parser before serving Rust results.
- Rollback is a configuration or ordinary deployment change and does not require data repair.

## Focused Alpine delivery proof

`feasibility/primary_offerings_alpine_acceptance.sh` is designed to pack the isolated AIP crate once with a unique
immutable NuGet version, consumes it through `PackageReference` with no compiler or Rust MSBuild
targets in the consumer, publishes a small consumer through the .NET 8 Alpine SDK, and executes its
typed DTO parse on the Primary Offerings .NET 8 Alpine runtime base shape. The staged local feed is
inside the pilot's ignored `.rust-dotnet-aip-feed/` path and is removed on exit.

When green, this closes only the focused package/load/DTO/musl delivery proof. It does not change production DI,
solution membership, application Dockerfiles, health endpoints, x64 deployment policy, shadow
metrics, or promotion readiness.

## Follow-on candidates

1. `InvestorTransliterationNormalizer.NormalizeNullable/NormalizeRequired`: valuable pure string
   normalization, but PII-sensitive; shadow only the pure function and never dual-write mutations.
2. `ShareDetailValidator`: small, side-effect-free validation contract; add golden validator cases
   before using it as a package/integration proof.

Authentication, order handling, money movement, migrations, scheduling, database ownership, SFTP
orchestration, and partner side effects are explicitly outside the first pilot.
