# rust-dotnet 0.0.2

This prerelease keeps the .NET 10 and host support boundary from 0.0.1: Linux x64, macOS Apple
Silicon, and Windows x64 on the pinned Rust nightly.

## Highlights

- Safe Rust-to-Rust native contracts now generate narrow C ABI shims for scalars, strings, slices,
  and owned scalar buffers. Contract signatures are embedded in exported symbol fingerprints so a
  mismatched importer fails to resolve instead of silently calling an incompatible ABI.
- Release and CI dependency resolution is locked, release builds are frozen after an explicit
  locked fetch, and third-party workflow actions are pinned to immutable commits.
- Installed SDK integrity rejects injected files, directories, symlinks, content changes, and
  executable-mode drift. Mutable Cargo, NuGet, lock, and private-sysroot state lives outside the
  immutable SDK home.
- NuGet IDs, versions, RIDs, local asset paths, and forced cache deletion are checked against their
  filesystem boundaries. Package filenames are derived only from validated components, and packages
  are structurally validated before transactional publication.
- Capability evidence now binds every required artifact name to a verified path and SHA-256 receipt.
- Backend rustc flags use Cargo's encoded argument channel, including checkouts and SDK paths that
  contain spaces, and host detection rejects architectures outside the published support matrix.

This is still an experimental compiler preview. It does not promise a stable compiler ABI or full
Rust/.NET semantic parity.
