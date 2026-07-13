# Product acceptance

`capabilities.toml` is the source inventory for product-level acceptance journeys. It describes
what must be proved; it never stores a mutable pass/fail claim.

The inventory includes representative cases for each supported oracle, plus product-shaped release,
installation, interop, and hermeticity journeys. Its smallest core examples are:

- `soak_ahash`: native Rust versus managed differential output and exit behavior;
- `cd_pure`: managed assertions followed by an exact completion marker; and
- `cd_export_ergonomics`: a real C# host builds, loads, and calls the Rust assembly.

## Supported hosts

The published host matrix is Linux and macOS. Windows x64 has an experimental, explicit-opt-in
native cargo-dotnet execution lane, but MSBuild integration, packaging, publishing, and public host
support remain outside the release matrix. Help, version, scaffolding, diagnostics, and read-only
metadata commands remain available without that opt-in.

`feasibility/e2e_matrix.sh` writes the result TSV and logs. It then invokes
`feasibility/write_acceptance_receipt.sh`, which binds those results to the exact Git state,
toolchain, host, command, and content hashes. A receipt with `dirty: true` is forensic evidence only
and cannot establish a baseline.

`cargo dotnet capabilities` validates this manifest and generates the human report. Repeat
`--results <evidence.tsv>` to merge independently owned matrix and special-script result files;
every observed value is derived from their explicit `kind`, `dotnet`, `profile`, marker, and
`result` columns. Conflicting duplicate cells, undeclared scripted cases/evidence kinds, and a
`PASS` without its required marker are rejected. A presubmit journey is `PASS` only when all cells
explicitly declared by that journey for `--evidence-scope presubmit|release` are present and
passing. Missing cells are
`PARTIAL`, absent journeys are `NOT RUN`, and any failing row is `FAIL`. `--strict` retains the
report but exits nonzero unless every presubmit journey is complete and passing. The manifest itself
cannot store a mutable green claim. Runtime fields distinguish the accepted CLI profiles (8/9/10),
the default (10), presubmit coverage (8 and 10), and immutable release evidence profile (10).

`feasibility/record_acceptance_result.sh` runs one special acceptance command, verifies its exact
completion marker and diagnostics, and atomically writes one owned TSV. CI never concurrently
appends to a shared ledger: the final capability command deterministically merges those files with
the e2e matrix and enforces the release scope in one serialized step.
`feasibility/write_capability_evidence_receipt.sh` then binds the manifest, strict report, and every
input TSV hash to the exact Git SHA, dirty state, toolchain, host, and evidence scope. A receipt from
a dirty checkout remains forensic-only, just like the original matrix receipt.

Acceptance receipts record the distinct runtime and build-profile dimensions found in the bound
matrix alongside its content hash, so a receipt cannot leave its claimed coverage implicit.

`feasibility/nats_managed_array_acceptance.sh` is the focused real-service oracle for generated
rank-1 managed arrays and interface calls. It regenerates NATS.Client bindings in a temporary
consumer, proves the `byte[]` signatures, starts an isolated NATS server when `NATS_URL` is absent,
and round-trips two payloads in debug and release with no C# helper assembly.

## Baseline eligibility

A product acceptance baseline requires all of the following:

1. `dirty` is false.
2. The committed SHA contains every source-affecting input.
3. The toolchain and target match the declared capability inventory.
4. Every required row satisfies its oracle, not merely exit code zero.
5. Logs, summary, and declared artifacts are retained with their hashes.
6. Repeating the run from clean caches produces the same declared distributable artifacts.

`feasibility/reproducibility_acceptance.sh` is the release-grade oracle for item 6. It refuses a
dirty checkout, creates two detached worktrees at the same commit, gives each build independent
empty Cargo, cargo-dotnet, NuGet, home, and temporary directories, then rebuilds the driver,
backend, and linker before packing the fixture. It compares package and receipt bytes plus sorted
ZIP-entry hashes, and verifies the packaged XML, provenance, SBOM, license inventory, and recorded
artifact hashes. Set `RCL_REPRO_EVIDENCE_DIR` to retain the evidence bundle at a chosen path.

The current dirty development checkout cannot produce release evidence. Shell syntax and focused
unit validation are useful while developing the gate, but only a successful clean-HEAD run may be
reported as reproducibility evidence.

The existing files under `feasibility/results/` predate this receipt contract and remain forensic
logs only.
