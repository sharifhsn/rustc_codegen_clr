#!/usr/bin/env bash
# Current public validation entrypoint.  Specialized scripts remain available
# for focused debugging; this is the small release-readiness matrix.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dotnet_version="${DOTNET_VERSION:-10}"

if [[ "${1:-}" == "--list" ]]; then
    printf '%s\n' \
        "e2e managed/native matrix" \
        "P/Invoke synchronous" \
        "P/Invoke asynchronous callbacks" \
        "P/Invoke policy diagnostics"
    exit 0
fi
if [[ "${1:-}" != "" ]]; then
    echo "usage: DOTNET_VERSION=10 $0 [--list]" >&2
    exit 2
fi

run_step() {
    local label="$1"
    shift
    echo "== $label =="
    (cd "$repo" && DOTNET_VERSION="$dotnet_version" "$@")
}

run_step "e2e managed/native matrix" feasibility/e2e_matrix.sh
run_step "P/Invoke synchronous" feasibility/pinvoke_acceptance.sh
run_step "P/Invoke asynchronous callbacks" feasibility/pinvoke_async_callback_acceptance.sh
run_step "P/Invoke policy diagnostics" feasibility/pinvoke_policy_diagnostics_acceptance.sh
echo "== primetime acceptance passed (.NET 10 managed and P/Invoke) =="
