#!/usr/bin/env bash
set -euo pipefail

if (( $# != 4 )); then
    echo 'usage: api_compat_gate.sh <baseline-api> <current-api> <baseline-version> <current-version>' >&2
    exit 2
fi

baseline="$1"
current="$2"
baseline_version="$3"
current_version="$4"
semver='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)([-+][0-9A-Za-z.-]+)?$'
[[ "$baseline_version" =~ $semver ]] || { echo "invalid baseline SemVer: $baseline_version" >&2; exit 2; }
baseline_major="${BASH_REMATCH[1]}"
[[ "$current_version" =~ $semver ]] || { echo "invalid current SemVer: $current_version" >&2; exit 2; }
current_major="${BASH_REMATCH[1]}"

if cmp -s "$baseline" "$current"; then
    exit 0
fi
if (( current_major <= baseline_major )); then
    echo "breaking managed API change requires a major-version increase: $baseline_version -> $current_version" >&2
    diff -u "$baseline" "$current" >&2 || true
    exit 1
fi
echo "accepted breaking managed API change with major-version increase: $baseline_version -> $current_version"
