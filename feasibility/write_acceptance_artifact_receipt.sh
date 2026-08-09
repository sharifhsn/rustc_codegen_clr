#!/usr/bin/env bash
# Write a machine-verifiable inventory of the exact files supporting one acceptance result.
set -euo pipefail

receipt="${1:?usage: write_acceptance_artifact_receipt.sh RECEIPT CASE KIND DOTNET PROFILES [NAME=PATH ...]}"
case_name="${2:?missing case}"
kind="${3:?missing evidence kind}"
dotnet="${4:?missing runtime}"
profiles_text="${5:?missing profiles}"
shift 5

command -v jq >/dev/null 2>&1 || {
    echo 'jq is required to write acceptance artifact receipts' >&2
    exit 2
}

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

artifacts='{}'
seen='|'
receipt_parent="$(dirname "$receipt")"
mkdir -p "$receipt_parent"
receipt_parent="$(cd "$receipt_parent" && pwd -P)"
receipt="$receipt_parent/$(basename "$receipt")"
artifact_dir="$receipt.files"
artifact_dir_name="$(basename "$artifact_dir")"
if [[ -e "$receipt" || -e "$artifact_dir" ]]; then
    echo "refusing to replace existing acceptance evidence: $receipt" >&2
    exit 2
fi
stage="$(mktemp -d "$receipt_parent/.acceptance-artifacts.XXXXXX")"
stage_files="$stage/files"
mkdir -p "$stage_files"
receipt_tmp="$(mktemp "$receipt_parent/.acceptance-receipt.XXXXXX")"
cleanup() {
    rm -rf "$stage"
    rm -f "$receipt_tmp"
}
trap cleanup EXIT
for spec in "$@"; do
    name="${spec%%=*}"
    path="${spec#*=}"
    if [[ "$spec" != *=* || ! "$name" =~ ^[a-z][a-z0-9_]*$ ]]; then
        echo "invalid acceptance artifact declaration: $spec" >&2
        exit 2
    fi
    if [[ "$seen" == *"|$name|"* ]]; then
        echo "duplicate acceptance artifact name: $name" >&2
        exit 2
    fi
    [[ -f "$path" ]] || {
        echo "acceptance artifact does not exist or is not a file: $name=$path" >&2
        exit 2
    }
    directory="$(cd "$(dirname "$path")" && pwd -P)"
    path="$directory/$(basename "$path")"
    [[ "$path" != *'|'* ]] || {
        echo "acceptance artifact path contains unsupported '|': $path" >&2
        exit 2
    }
    copied="$stage_files/$name"
    cp "$path" "$copied"
    artifacts="$(jq -cn \
        --argjson current "$artifacts" \
        --arg name "$name" \
        --arg path "$artifact_dir_name/$name" \
        --arg sha256 "$(hash_file "$copied")" \
        '$current + {($name): {path: $path, sha256: $sha256}}')"
    seen="$seen$name|"
done

profiles="$(jq -cn --arg profiles "$profiles_text" \
    '$profiles | split(" ") | map(select(length > 0))')"
jq -n \
    --argjson schema 1 \
    --arg case "$case_name" \
    --arg kind "$kind" \
    --arg dotnet "$dotnet" \
    --argjson profiles "$profiles" \
    --argjson artifacts "$artifacts" \
    '{schema: $schema, case: $case, kind: $kind, dotnet: $dotnet, profiles: $profiles, artifacts: $artifacts}' \
    > "$receipt_tmp"
mv "$stage_files" "$artifact_dir"
if ! mv "$receipt_tmp" "$receipt"; then
    mv "$artifact_dir" "$stage_files" || true
    exit 1
fi
trap - EXIT
rm -rf "$stage"

echo "acceptance artifact receipt: $receipt"
