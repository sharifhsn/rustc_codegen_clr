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

sha="$(git -C "$repo" rev-parse HEAD)"
if [[ -n "$(git -C "$repo" status --porcelain --untracked-files=all)" ]]; then
    dirty=true
else
    dirty=false
fi
if [[ "$scope" == release && "$dirty" == true ]]; then
    echo 'release capability evidence requires a clean source tree' >&2
    exit 2
fi

mkdir -p "$(dirname "$receipt")"
receipt_dir="$(cd "$(dirname "$receipt")" && pwd -P)"
receipt="$receipt_dir/$(basename "$receipt")"
manifest_snapshot="$receipt_dir/capabilities.manifest.toml"
report_snapshot="$receipt_dir/capability-report.md"
cp "$manifest" "$manifest_snapshot"
cp "$report" "$report_snapshot"

relative_evidence_file() {
    local path="$1" directory absolute
    [[ -f "$path" && ! -L "$path" ]] || {
        echo "evidence file is missing, not regular, or a symlink: $path" >&2
        return 2
    }
    directory="$(cd "$(dirname "$path")" && pwd -P)"
    absolute="$directory/$(basename "$path")"
    case "$absolute" in
        "$receipt_dir"/*) printf '%s\n' "${absolute#"$receipt_dir"/}" ;;
        *)
            echo "evidence file is outside the evidence-owned directory: $absolute" >&2
            return 2
            ;;
    esac
}

require_normalized_relative_path() {
    local path="$1" label="$2"
    case "$path" in
        ''|/*|*\\*|.|./*|*/./*|*/.|..|../*|*/../*|*/..|*//* )
            echo "$label must be normalized and relative: $path" >&2
            return 2
            ;;
    esac
}

evidence='[]'
artifact_receipts='[]'
artifact_files='[]'
seen_artifact_receipts='|'
for path in "$@"; do
    result_relative="$(relative_evidence_file "$path")"
    IFS= read -r header < "$path" || true
    case "$header" in
        'kind|dotnet|profile|case|'*) ;;
        *) echo "capability evidence is not runtime/profile-aware: $path" >&2; exit 2 ;;
    esac
    evidence="$(jq -cn \
        --argjson current "$evidence" \
        --arg path "$result_relative" \
        --arg sha256 "$(hash_file "$path")" \
        '$current + [{path: $path, sha256: $sha256}]')"

    IFS='|' read -r -a columns <<< "$header"
    receipt_index=-1
    for index in "${!columns[@]}"; do
        if [[ "${columns[$index]}" == receipt ]]; then
            receipt_index="$index"
            break
        fi
    done
    if ((receipt_index >= 0)); then
        while IFS='|' read -r -a fields; do
            artifact_receipt="${fields[$receipt_index]:-}"
            [[ -n "$artifact_receipt" ]] || continue
            require_normalized_relative_path "$artifact_receipt" \
                'acceptance artifact receipt path' || exit 2
            artifact_receipt="$(dirname "$path")/$artifact_receipt"
            receipt_relative="$(relative_evidence_file "$artifact_receipt")"
            case "$seen_artifact_receipts" in
                *"|$receipt_relative|"*) continue ;;
            esac
            jq -e '.schema == 1 and (.artifacts | type == "object")' \
                "$artifact_receipt" >/dev/null
            artifact_receipts="$(jq -cn \
                --argjson current "$artifact_receipts" \
                --arg path "$receipt_relative" \
                --arg sha256 "$(hash_file "$artifact_receipt")" \
                '$current + [{path: $path, sha256: $sha256}]')"
            while IFS=$'\t' read -r name artifact_path expected_sha256; do
                [[ -n "$name" && -n "$artifact_path" && -n "$expected_sha256" ]] || {
                    echo "malformed artifact entry in $artifact_receipt" >&2
                    exit 2
                }
                require_normalized_relative_path "$artifact_path" \
                    'artifact receipt path' || exit 2
                artifact_file="$(dirname "$artifact_receipt")/$artifact_path"
                artifact_relative="$(relative_evidence_file "$artifact_file")"
                actual_sha256="$(hash_file "$artifact_file")"
                if [[ "$actual_sha256" != "$expected_sha256" ]]; then
                    echo "artifact hash changed after its receipt was written: $artifact_relative" >&2
                    exit 2
                fi
                artifact_files="$(jq -cn \
                    --argjson current "$artifact_files" \
                    --arg receipt "$receipt_relative" \
                    --arg name "$name" \
                    --arg path "$artifact_relative" \
                    --arg sha256 "$actual_sha256" \
                    '$current + [{receipt: $receipt, name: $name, path: $path, sha256: $sha256}]')"
            done < <(jq -r '.artifacts | to_entries[] | [.key, .value.path, .value.sha256] | @tsv' \
                "$artifact_receipt")
            seen_artifact_receipts="$seen_artifact_receipts$receipt_relative|"
        done < <(tail -n +2 "$path")
    fi
done

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
    --arg manifest "$(basename "$manifest_snapshot")" \
    --arg manifest_sha256 "$(hash_file "$manifest_snapshot")" \
    --arg report "$(basename "$report_snapshot")" \
    --arg report_sha256 "$(hash_file "$report_snapshot")" \
    --argjson result_files "$evidence" \
    --argjson artifact_receipts "$artifact_receipts" \
    --argjson artifact_files "$artifact_files" \
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
        result_files: $result_files,
        artifact_receipts: $artifact_receipts,
        artifact_files: $artifact_files
    }' > "$tmp"
mv -f "$tmp" "$receipt"
trap - EXIT

echo "capability evidence receipt: $receipt"
if [[ "$dirty" == true ]]; then
    echo 'warning: dirty source receipt is forensic only; it is not baseline evidence' >&2
fi
