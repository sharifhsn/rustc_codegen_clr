# Product acceptance

`capabilities.toml` is the source inventory for product-level acceptance journeys. It describes
what must be proved; it never stores a mutable pass/fail claim.

The initial slice intentionally contains one case for each supported oracle:

- `soak_ahash`: native Rust versus managed differential output and exit behavior;
- `cd_pure`: managed assertions followed by an exact completion marker; and
- `cd_export_ergonomics`: a real C# host builds, loads, and calls the Rust assembly.

## Supported hosts

The current release supports Linux and macOS hosts. Windows hosts are explicitly unsupported for
setup, build, test, packaging, publishing, and MSBuild integration until Windows acceptance exists.
This is a host-tooling boundary, not a claim about .NET target or runtime portability. Help, version,
scaffolding, diagnostics, and read-only metadata commands remain available on unsupported hosts.

`feasibility/e2e_matrix.sh` writes the result TSV and logs. It then invokes
`feasibility/write_acceptance_receipt.sh`, which binds those results to the exact Git state,
toolchain, host, command, and content hashes. A receipt with `dirty: true` is forensic evidence only
and cannot establish a baseline.

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
