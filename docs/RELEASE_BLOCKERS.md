# Release status

The next public artifact is the `rust-dotnet-v0.0.1` GitHub prerelease.

## Release contract

- .NET 10 only
- Linux x64, macOS Apple Silicon, and Windows x64
- pinned Rust `nightly-2026-06-17`
- host-specific SDK ZIP plus checksum
- standalone host `cargo-dotnet` executable
- `install.sh` and `install.ps1` bootstrap installers

The release is an experimental compiler preview. It does not claim production support, a stable
compiler ABI, or complete Rust/.NET semantic parity.

## Required before publishing 0.0.1

- [ ] Land the release tree on `main`.
- [ ] Pass the normal compiler, cilly, cargo-dotnet, and product smoke CI on that commit.
- [ ] Build each host bundle on its matching GitHub runner.
- [ ] Install each bundle into an empty SDK/Cargo home and execute the installed CLI.
- [ ] Publish all three bundles, checksums, standalone CLIs, and installers in one GitHub prerelease.
- [ ] Copy the documented install command on a clean host and run the hello-world scaffold.

NuGet feed automation, signed tags, exhaustive profile matrices, and a complete evidence ledger are
useful future hardening, not blockers for this experimental GitHub preview.

## After 0.0.1

- Add Linux arm64 and macOS Intel only after matching build-and-install CI exists.
- Improve Windows MSBuild and NativeAOT coverage without hiding currently supported compiler use.
- Turn the first real user failures into small regression fixtures and actionable diagnostics.
- Publish a later version only after its bundles pass the same three-host install test.
