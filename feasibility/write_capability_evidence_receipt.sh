#!/usr/bin/env bash
# Bind a merged strict capability report and every contributing result file to source/toolchain state.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="${1:?usage: write_capability_evidence_receipt.sh MANIFEST REPORT SCOPE RECEIPT RESULTS...}"
report="${2:?missing strict capability report}"
scope="${3:?missing evidence scope}"
receipt="${4:?missing receipt path}"
shift 4

if (($# == 0)); then
    echo 'capability evidence receipt requires at least one result file' >&2
    exit 2
fi
if ! command -v jq >/dev/null 2>&1; then
    echo 'jq is required to write a capability evidence receipt' >&2
    exit 2
fi
case "$scope" in
    presubmit|release) ;;
    *) echo "unsupported capability evidence scope: $scope" >&2; exit 2 ;;
esac

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

for path in "$manifest" "$report" "$@"; do
    [[ -f "$path" ]] || { echo "capability evidence input does not exist: $path" >&2; exit 2; }
done

evidence='[]'
for path in "$@"; do
    IFS= read -r header < "$path" || true
    case "$header" in
        'kind|dotnet|profile|case|'*) ;;
        *) echo "capability evidence is not runtime/profile-aware: $path" >&2; exit 2 ;;
    esac
    evidence="$(jq -cn \
        --argjson current "$evidence" \
        --arg path "$path" \
        --arg sha256 "$(hash_file "$path")" \
        '$current + [{path: $path, sha256: $sha256}]')"
done

sha="$(git -C "$repo" rev-parse HEAD)"
if [[ -n "$(git -C "$repo" status --porcelain --untracked-files=all)" ]]; then
    dirty=true
else
    dirty=false
fi
mkdir -p "$(dirname "$receipt")"
tmp="$(mktemp "${receipt}.tmp.XXXXXX")"
trap 'rm -f "$tmp"' EXIT
jq -n \
    --argjson schema 1 \
    --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg sha "$sha" \
    --argjson dirty "$dirty" \
    --arg rustc "$(rustc +nightly-2026-06-17 --version 2>&1 || true)" \
    --arg dotnet "$(dotnet --version 2>&1 || true)" \
    --arg host_os "$(uname -s)" \
    --arg host_arch "$(uname -m)" \
    --arg scope "$scope" \
    --arg command "${RCL_CAPABILITY_COMMAND:-cargo dotnet capabilities --strict}" \
    --arg manifest "$manifest" \
    --arg manifest_sha256 "$(hash_file "$manifest")" \
    --arg report "$report" \
    --arg report_sha256 "$(hash_file "$report")" \
    --argjson result_files "$evidence" \
    '{
        schema: $schema,
        generated_at: $generated_at,
        source: {sha: $sha, dirty: $dirty},
        toolchain: {rustc: $rustc, dotnet: $dotnet},
        host: {os: $host_os, arch: $host_arch},
        evidence_scope: $scope,
        command: $command,
        manifest: {path: $manifest, sha256: $manifest_sha256},
        report: {path: $report, sha256: $report_sha256},
        result_files: $result_files
    }' > "$tmp"
mv -f "$tmp" "$receipt"
trap - EXIT

echo "capability evidence receipt: $receipt"
if [[ "$dirty" == true ]]; then
    echo 'warning: dirty source receipt is forensic only; it is not baseline evidence' >&2
fi
