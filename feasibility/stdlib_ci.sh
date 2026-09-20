#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
suite="${1:?usage: stdlib_ci.sh SUITE PROFILE OUT_DIR}"
profile="${2:?usage: stdlib_ci.sh SUITE PROFILE OUT_DIR}"
out="${3:?usage: stdlib_ci.sh SUITE PROFILE OUT_DIR}"
build_timeout="${RCL_STDLIB_BUILD_TIMEOUT:-1800}"

case "$suite" in
    coretests|alloctests) ;;
    *)
        echo "stdlib CI only gates coretests or alloctests, not '$suite'" >&2
        exit 2
        ;;
esac

case "$profile" in
    debug|release) ;;
    *)
        echo "stdlib CI profile must be debug or release, not '$profile'" >&2
        exit 2
        ;;
esac

args=("$repo/feasibility/stdlib_suite.py" "$suite" "--out" "$out" "--build-timeout" "$build_timeout")
if [[ "$profile" == debug ]]; then
    args+=(--debug)
fi

# Keep the report validation below reachable when the suite records a terminal failed result.
# The Python runner's exit status remains the CI gate status.
set +e
python3 "${args[@]}"
runner_status=$?
set -e

canonical="$out/reports/$suite/$profile/latest-full.json"
if [[ ! -s "$canonical" ]]; then
    echo "stdlib suite did not produce a full-run receipt: $canonical" >&2
    exit 1
fi

report="$(jq -er '.run_report' "$canonical")"
if [[ ! -s "$report" ]]; then
    echo "stdlib suite receipt points to a missing report: $report" >&2
    exit 1
fi

# A successful process is not enough: require a complete report with one terminal result for
# every selected name and a denominator that accounts for every recorded status.
jq -e \
    --arg suite "$suite" \
    --arg profile "$profile" \
    '
      .suite == $suite and
      .profile == $profile and
      .terminal_status == "complete" and
      .terminal_completion.complete == true and
      (.terminal_completion.selected == (.selected_tests | length)) and
      (.terminal_completion.recorded_results == (.results | length)) and
      ([
        .counts.passed,
        .counts.failed,
        .counts.crashed,
        .counts.timeout,
        .counts.unsupported,
        .counts["not-applicable"],
        .counts["upstream-ignored"],
        .counts["backend-policy-skip"]
      ] | all(.[]; type == "number" and . >= 0 and floor == .)) and
      ([
        .counts.passed,
        .counts.failed,
        .counts.crashed,
        .counts.timeout,
        .counts.unsupported,
        .counts["not-applicable"],
        .counts["upstream-ignored"],
        .counts["backend-policy-skip"]
      ] | add) == (.selected_tests | length)
    ' "$report"

jq -r '
    "stdlib suite=\(.suite) profile=\(.profile) passed=\(.counts.passed) selected=\(.selected_tests | length) failed=\(.counts.failed) crashed=\(.counts.crashed) timeout=\(.counts.timeout) upstream_ignored=\(.counts["upstream-ignored"]) backend_policy_skip=\(.counts["backend-policy-skip"])"
  ' "$report"

exit "$runner_status"
