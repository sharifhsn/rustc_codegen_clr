#!/usr/bin/env bash
# Current public validation entrypoint.  Specialized scripts remain available
# for focused debugging; this is the small release-readiness matrix.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dotnet_version="${DOTNET_VERSION:-10}"
unity_bin="${UNITY_BIN:-}"

steps=(
    "e2e managed/native matrix|feasibility/e2e_matrix.sh"
    "P/Invoke synchronous|feasibility/pinvoke_acceptance.sh"
    "P/Invoke asynchronous callbacks|feasibility/pinvoke_async_callback_acceptance.sh"
    "P/Invoke policy diagnostics|feasibility/pinvoke_policy_diagnostics_acceptance.sh"
)
if [[ -n "$unity_bin" ]]; then
    steps+=("Unity clean project|feasibility/unity_clean_acceptance.sh $unity_bin")
fi

if [[ "${1:-}" == "--list" ]]; then
    printf '%s\n' "${steps[@]%%|*}"
    [[ -n "$unity_bin" ]] || echo "Unity clean project (set UNITY_BIN to enable)"
    exit 0
fi
if [[ "${1:-}" != "" ]]; then
    echo "usage: DOTNET_VERSION=10 [UNITY_BIN=/path/to/Unity] $0 [--list]" >&2
    exit 2
fi

for entry in "${steps[@]}"; do
    label="${entry%%|*}"
    command="${entry#*|}"
    echo "== $label =="
    (cd "$repo" && DOTNET_VERSION="$dotnet_version" bash -c "$command")
done
echo "== primetime acceptance passed (Net10 managed, P/Invoke, and optional Unity) =="
