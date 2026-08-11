#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
release="$repo/.github/workflows/release.yml"
fork_gate="$repo/.github/workflows/fork-gate.yml"
bundle="$repo/feasibility/release_bundle.sh"
compat_launcher="$repo/feasibility/cargo-dotnet"
workflows="$repo/.github/workflows"

fail() {
    echo "release workflow acceptance: $*" >&2
    exit 1
}

[[ -f "$release" ]] || fail "missing .github/workflows/release.yml"

grep -Fq "tags: ['rust-dotnet-v*']" "$release" || fail "release trigger is not tag-only"
! grep -Fq 'workflow_dispatch:' "$release" || fail "manual branch dispatch could bypass tag identity"
grep -Fq 'contents: read' "$release" || fail "release workflow does not default to read-only contents"
grep -Fq '    permissions:' "$release" || fail "publish job does not declare scoped permissions"
grep -Fq '      contents: write' "$release" || fail "publish job cannot create a GitHub release"
[[ "$(rg -c 'bash feasibility/verify_release_tag.sh' "$release")" == 2 ]] \
    || fail "signed tag is not verified before both build and publish"
tag_gate="$repo/feasibility/verify_release_tag.sh"
[[ -x "$tag_gate" ]] || fail "release tag verification helper is missing or not executable"
grep -Fq 'git cat-file -t' "$tag_gate" || fail "release tag gate does not require an annotated tag"
grep -Fq '$tag_ref^{commit}' "$tag_gate" || fail "release tag gate does not bind the tag to HEAD"
grep -Fq '.verification.verified == true' "$tag_gate" \
    || fail "release tag gate does not require GitHub signature verification"
grep -Fq 'manifest_version' "$release" || fail "release tag is not matched to the CLI version"
grep -Fq 'cargo fetch --locked' "$release" || fail "release does not fetch its locked graph before frozen builds"
grep -Fq -- '--frozen' "$release" || fail "release builds are not frozen after the explicit fetch"
fetch_line="$(rg -n 'cargo fetch --locked' "$release" | cut -d: -f1 | head -1)"
frozen_line="$(rg -n -- '--frozen' "$release" | cut -d: -f1 | head -1)"
((fetch_line < frozen_line)) || fail "release frozen build appears before the locked dependency fetch"

for host in linux-x64 macos-arm64 windows-x64; do
    grep -Fq "host: $host" "$release" || fail "release matrix is missing $host"
    grep -Fq "$host" "$repo/install.sh" "$repo/install.ps1" \
        || fail "bootstrap installers are missing $host"
done
grep -Fq 'runner: macos-15' "$release" \
    || fail "release macOS bundle is not built on the supported Apple-Silicon runner"

grep -Fq 'cargo build --release --workspace' "$release" \
    || fail "release does not build the compiler workspace"
grep -Fq 'cargo test -p rust-dotnet-sdk-core --frozen' "$release" \
    || fail "Windows release does not execute SDK filesystem capability tests"
grep -Fq "if: matrix.host == 'windows-x64'" "$release" \
    || fail "release SDK filesystem tests are not scoped to the Windows host"
grep -Fq 'cargo test -p rust-dotnet-sdk-core --locked' "$fork_gate" \
    || fail "Windows fork gate does not execute SDK filesystem capability tests"
grep -Fq 'feasibility/release_bundle.sh' "$release" || fail "release does not run the bundle builder"
grep -Fq 'CARGO_DOTNET_BUILD_ID="source-sha256:' "$release" \
    || fail "release driver is not bound to the captured source-tree digest"
grep -Fq 'release bundle refuses a dirty source tree' "$bundle" \
    || fail "release bundle does not fail closed on dirty sources"
grep -Fq 'source_tree_sha256' "$bundle" \
    || fail "release VERSION does not record its source-tree digest"
grep -Fq 'CARGO_DOTNET_BUILD_ID="$driver_build_id"' "$bundle" \
    || fail "release bundle does not rebuild the packaged driver with its recorded identity"
grep -Fq 'release_tag="${CARGO_DOTNET_SOURCE_RELEASE_TAG:-untagged}"' "$compat_launcher" \
    || fail "local setup can still mistake an unverified Git tag for release attestation"
grep -Fq 'git_tag="${CARGO_DOTNET_SOURCE_GIT_TAG:-' "$compat_launcher" \
    || fail "local setup does not retain descriptive exact-tag provenance separately"
grep -Fq 'bundle create' "$bundle" || fail "release does not create SDK bundles"
grep -Fq 'bundle verify' "$bundle" || fail "release does not verify SDK bundles"
grep -Fq '"$home_driver" bundle install "$bundle"' "$bundle" \
    || fail "release does not install with the exact sealed bundle driver"
grep -Fq 'dotnet attach' "$bundle" || fail "release does not exercise installed MSBuild attachment"
grep -Fq 'native Rust probe=0' "$bundle" \
    || fail "release does not exercise installed attached-host native sidecars"
grep -Fq 'actions/upload-artifact@' "$release" || fail "release does not archive host assets"
grep -Fq 'actions/download-artifact@' "$release" || fail "release does not collect host assets"
grep -Fq 'gh release create' "$release" || fail "release does not publish a GitHub release"
grep -Fq 'RELEASE_NOTES_${version}.md' "$release" \
    || fail "release notes are not selected from the immutable tag version"
! grep -Fq -- '--notes-file docs/RELEASE_NOTES_0.0.1.md' "$release" \
    || fail "release notes are still hardcoded to 0.0.1"
grep -Fq -- '--prerelease' "$release" || fail "0.x compiler release must remain a prerelease"
grep -Fq 'install.sh install.ps1' "$release" || fail "release does not attach bootstrap installers"
grep -Fq 'cargo-dotnet-$host.sha256' "$repo/install.sh" \
    || fail "Unix installer does not verify a standalone driver checksum"
grep -Fq 'cargo-dotnet-$HostId.exe.sha256' "$repo/install.ps1" \
    || fail "PowerShell installer does not verify a standalone driver checksum"

bad_actions="$(rg -n 'uses:[[:space:]]+[^[:space:]]+@' "$workflows" \
    | rg -v 'uses:[[:space:]]+[^[:space:]]+@[0-9a-f]{40}([[:space:]]+#.*)?$' || true)"
[[ -z "$bad_actions" ]] || {
    printf '%s\n' "$bad_actions" >&2
    fail "every workflow action must be pinned to a full commit SHA"
}

ruby -e 'require "yaml"; ARGV.each { |path| document = YAML.safe_load(File.read(path), aliases: true); raise "#{path}: default contents permission is not read" unless document.dig("permissions", "contents") == "read" }' \
    "$workflows"/*.yml

echo '== release_workflow_acceptance done =='
