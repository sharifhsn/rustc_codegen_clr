#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
release="$repo/.github/workflows/release.yml"
bundle="$repo/feasibility/release_bundle.sh"
workflows="$repo/.github/workflows"

fail() {
    echo "release workflow acceptance: $*" >&2
    exit 1
}

[[ -f "$release" ]] || fail "missing .github/workflows/release.yml"

grep -Fq "tags: ['rust-dotnet-v*']" "$release" || fail "release trigger is not tag-only"
! grep -Fq 'workflow_dispatch:' "$release" || fail "manual branch dispatch could bypass tag identity"
grep -Fq 'contents: write' "$release" || fail "release workflow cannot create a GitHub release"
grep -Fq 'manifest_version' "$release" || fail "release tag is not matched to the CLI version"

for host in linux-x64 macos-arm64 windows-x64; do
    grep -Fq "host: $host" "$release" || fail "release matrix is missing $host"
    grep -Fq "$host" "$repo/install.sh" "$repo/install.ps1" \
        || fail "bootstrap installers are missing $host"
done

grep -Fq 'cargo build --release --workspace' "$release" \
    || fail "release does not build the compiler workspace"
grep -Fq 'feasibility/release_bundle.sh' "$release" || fail "release does not run the bundle builder"
grep -Fq 'bundle create' "$bundle" || fail "release does not create SDK bundles"
grep -Fq 'bundle verify' "$bundle" || fail "release does not verify SDK bundles"
grep -Fq 'bundle install' "$bundle" || fail "release does not clean-install SDK bundles"
grep -Fq 'actions/upload-artifact@' "$release" || fail "release does not archive host assets"
grep -Fq 'actions/download-artifact@' "$release" || fail "release does not collect host assets"
grep -Fq 'gh release create' "$release" || fail "release does not publish a GitHub release"
grep -Fq -- '--prerelease' "$release" || fail "0.x compiler release must remain a prerelease"
grep -Fq 'install.sh install.ps1' "$release" || fail "release does not attach bootstrap installers"

bad_actions="$(rg -n 'uses:[[:space:]]+[^[:space:]]+@' "$workflows" \
    | rg -v 'uses:[[:space:]]+[^[:space:]]+@[0-9a-f]{40}([[:space:]]+#.*)?$' || true)"
[[ -z "$bad_actions" ]] || {
    printf '%s\n' "$bad_actions" >&2
    fail "every workflow action must be pinned to a full commit SHA"
}

ruby -e 'require "yaml"; ARGV.each { |path| YAML.safe_load(File.read(path), aliases: true) }' \
    "$workflows"/*.yml

echo '== release_workflow_acceptance done =='
