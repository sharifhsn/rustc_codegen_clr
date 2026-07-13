#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
summary="${1:?usage: write_acceptance_receipt.sh SUMMARY [RECEIPT] [LOG_DIR]}"
receipt="${2:-${summary%.*}.receipt.json}"
log_dir="${3:-}"

if ! command -v jq >/dev/null 2>&1; then
    echo "jq is required to write an acceptance receipt" >&2
    exit 2
fi

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

if [[ ! -f "$summary" ]]; then
    echo "acceptance summary does not exist: $summary" >&2
    exit 2
fi
IFS= read -r summary_header < "$summary" || true
case "$summary_header" in
    'kind|dotnet|profile|case|'*) ;;
    *)
        echo "acceptance summary is not runtime/profile-aware; regenerate it with e2e_matrix.sh" >&2
        exit 2
        ;;
esac

sha="$(git -C "$repo" rev-parse HEAD)"
if [[ -n "$(git -C "$repo" status --porcelain --untracked-files=all)" ]]; then
    dirty=true
else
    dirty=false
fi

rustc_version="$(rustc +nightly-2026-06-17 --version 2>&1 || true)"
dotnet_version="$(dotnet --version 2>&1 || true)"
host_os="$(uname -s)"
host_arch="$(uname -m)"
summary_hash="$(hash_file "$summary")"
matrix_dotnet="$(awk -F'|' 'NR > 1 && $2 != "" { print $2 }' "$summary" | LC_ALL=C sort -u | paste -sd, -)"
matrix_profiles="$(awk -F'|' 'NR > 1 && $3 != "" { print $3 }' "$summary" | LC_ALL=C sort -u | paste -sd, -)"
capability_report="${summary%.*}.capabilities.md"
capability_report_hash=""
if [[ -f "$capability_report" ]]; then
    capability_report_hash="$(hash_file "$capability_report")"
fi
manifest_hash="$(hash_file "$repo/acceptance/capabilities.toml")"
generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
logs_hash=""
if [[ -n "$log_dir" && -d "$log_dir" ]]; then
    hash_manifest="$(mktemp)"
    while IFS= read -r path; do
        printf '%s  %s\n' "$(hash_file "$path")" "${path#"$log_dir"/}"
    done < <(find "$log_dir" -type f | LC_ALL=C sort) > "$hash_manifest"
    logs_hash="$(hash_file "$hash_manifest")"
    rm -f "$hash_manifest"
fi

jq -n \
    --argjson schema 1 \
    --arg generated_at "$generated_at" \
    --arg sha "$sha" \
    --argjson dirty "$dirty" \
    --arg rustc "$rustc_version" \
    --arg dotnet "$dotnet_version" \
    --arg host_os "$host_os" \
    --arg host_arch "$host_arch" \
    --arg command "${RCL_MATRIX_COMMAND:-feasibility/e2e_matrix.sh}" \
    --arg summary "$summary" \
    --arg summary_sha256 "$summary_hash" \
    --arg matrix_dotnet "$matrix_dotnet" \
    --arg matrix_profiles "$matrix_profiles" \
    --arg capability_report "$capability_report" \
    --arg capability_report_sha256 "$capability_report_hash" \
    --arg manifest "acceptance/capabilities.toml" \
    --arg manifest_sha256 "$manifest_hash" \
    --arg log_dir "$log_dir" \
    --arg logs_sha256 "$logs_hash" \
    '{
        schema: $schema,
        generated_at: $generated_at,
        source: {sha: $sha, dirty: $dirty},
        toolchain: {rustc: $rustc, dotnet: $dotnet},
        host: {os: $host_os, arch: $host_arch},
        command: $command,
        manifest: {path: $manifest, sha256: $manifest_sha256},
        summary: {path: $summary, sha256: $summary_sha256},
        matrix: {
            dotnet: ($matrix_dotnet | split(",") | map(select(length > 0))),
            profiles: ($matrix_profiles | split(",") | map(select(length > 0)))
        },
        capability_report: {path: $capability_report, sha256: $capability_report_sha256},
        logs: {path: $log_dir, manifest_sha256: $logs_sha256}
    }' > "$receipt"

echo "receipt: $receipt"
if [[ "$dirty" == true ]]; then
    echo "warning: dirty source receipt is forensic only; it is not baseline evidence" >&2
fi
