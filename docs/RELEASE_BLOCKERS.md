# Release status

The `rust-dotnet-v0.0.1` GitHub prerelease was published on July 13, 2026. Its tag is immutable;
later productization work on `main` belongs in a new version rather than silently replacing 0.0.1
assets.

## Release contract

- .NET 10 only
- Linux x64, macOS Apple Silicon, and Windows x64
- pinned Rust `nightly-2026-06-17`
- host-specific SDK ZIP plus checksum
- standalone host `cargo-dotnet` executable
- `install.sh` and `install.ps1` bootstrap installers

The release is an experimental compiler preview. It does not claim production support, a stable
compiler ABI, or complete Rust/.NET semantic parity.

## Published 0.0.1 evidence

- [x] The release tree landed on `main`.
- [x] Compiler, cilly, cargo-dotnet, and product smoke CI passed on the release commit.
- [x] Linux x64, macOS Apple Silicon, and Windows x64 bundles were built on matching runners.
- [x] Each bundle was installed into isolated SDK/Cargo homes and its installed CLI executed.
- [x] All three bundles, checksums, standalone CLIs, and installers were published together.
- [x] The release workflow completed successfully for tag `rust-dotnet-v0.0.1`.

## Required for the next prerelease

- [ ] Current `main` passes the Linux, macOS, and Windows compiler/product gates.
- [ ] A new immutable version/tag is selected; do not move or overwrite `rust-dotnet-v0.0.1`.
- [ ] Matching host bundles pass the same isolated install and first-run acceptance as 0.0.1.
- [ ] Release notes distinguish shipped behavior from planned Excel, Unity, MAUI, and NativeAOT
  work.
- [ ] The documented installer and a managed-Rust plus P/Invoke attached-host journey pass from the
  published assets, outside this checkout.

NuGet trusted publishing, signed tags, and platform package signing are useful hardening but are not
claims made by this experimental GitHub prerelease.

## Later expansion

- Add Linux arm64 and macOS Intel only after matching build-and-install CI exists.
- Add interactive Windows Excel/WinUI/MAUI evidence without inheriting support from compile-only CI.
- Improve NativeAOT coverage without implying that all product hosts are trim/AOT safe.
- Turn the first real user failures into small regression fixtures and actionable diagnostics.
- Publish a later version only after its bundles pass the same three-host install test.
