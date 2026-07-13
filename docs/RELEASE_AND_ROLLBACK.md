# Release and rollback

GitHub prereleases are the distribution channel for the experimental SDK.

## Release

1. Update the root and `tools/cargo-dotnet` package versions.
2. Land the intended tree on `main` and wait for the normal CI workflows.
3. Create an annotated `rust-dotnet-v<semver>` tag at that commit and push it.
4. `.github/workflows/release.yml` builds the compiler and installer on Linux x64, macOS Apple
   Silicon, and Windows x64.
5. Each job stages a host SDK, creates and verifies its checksummed bundle, installs it into empty
   SDK/Cargo homes, and executes the installed CLI.
6. Only after all host jobs pass does the workflow create the GitHub prerelease and attach the
   bundles, checksum sidecars, standalone CLIs, and bootstrap installers.
7. Copy the published install command on a clean host and run `doctor`, `new`, and `run` before
   announcing the release.

Tags and published release assets are immutable. Never move a tag or replace an asset under an
existing version.

## Rollback

GitHub prereleases are explicitly versioned, so rollback means directing users to the previous
known-good version and removing the bad version from the recommended README command. Preserve the
bad tag and release for diagnosis; mark it clearly in the release notes instead of replacing its
bytes.

Fix forward on a new commit and version, run the same three-host workflow, and publish a new tag.
