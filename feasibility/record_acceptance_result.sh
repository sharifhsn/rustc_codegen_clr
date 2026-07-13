#!/usr/bin/env bash
# Run one product acceptance command and atomically write its runtime/profile evidence rows.
# Each invocation owns one result file; CI merges files later instead of concurrently appending.
set -euo pipefail

results="${RCL_EVIDENCE_RESULTS:?set RCL_EVIDENCE_RESULTS to an owned TSV path}"
kind="${RCL_EVIDENCE_KIND:?set RCL_EVIDENCE_KIND to the manifest evidence_kind}"
case_name="${RCL_EVIDENCE_CASE:?set RCL_EVIDENCE_CASE to the manifest case}"
dotnet_version="${RCL_EVIDENCE_DOTNET:?set RCL_EVIDENCE_DOTNET to a runtime or independent}"
profiles_text="${RCL_EVIDENCE_PROFILES:?set RCL_EVIDENCE_PROFILES to one or more profiles}"
marker_text="${RCL_EVIDENCE_MARKER:?set RCL_EVIDENCE_MARKER to the exact completion line}"
command_log="${RCL_EVIDENCE_LOG:-${results%.*}.command.log}"

if (($# == 0)); then
    echo 'record_acceptance_result: expected an acceptance command' >&2
    exit 2
fi

read -r -a profiles <<< "$profiles_text"
if ((${#profiles[@]} == 0)); then
    echo 'record_acceptance_result: no evidence profiles were supplied' >&2
    exit 2
fi

mkdir -p "$(dirname "$results")" "$(dirname "$command_log")"
tmp="$(mktemp "${results}.tmp.XXXXXX")"
cleanup() {
    rm -f "$tmp"
}
trap cleanup EXIT

set +e
"$@" > "$command_log" 2>&1
command_exit=$?
set -e
cat "$command_log"

if grep -Fqx -- "$marker_text" "$command_log"; then
    marker=yes
else
    marker=no
fi
diagnostics='(^error(\[|:)|compiler unexpectedly panicked|could not compile item|panicked at|final post-link verification failed|verification failed|miscompilation|fatal error|^unhandled exception|^process terminated)'
diagnostic_hits="$( { rg -n -i "$diagnostics" "$command_log" 2>/dev/null || true; } \
    | wc -l | tr -d ' ')"
if ((command_exit == 0 && diagnostic_hits == 0)) && [[ "$marker" == yes ]]; then
    result=PASS
else
    result=FAIL
fi

printf 'kind|dotnet|profile|case|dotnet_exit|native_exit|stdout_match|diagnostic_hits|marker|required|result\n' > "$tmp"
for profile in "${profiles[@]}"; do
    case "$profile" in
        debug|release|independent) ;;
        *) echo "record_acceptance_result: unsupported profile $profile" >&2; exit 2 ;;
    esac
    printf '%s|%s|%s|%s|%d|na|na|%s|%s|yes|%s\n' \
        "$kind" "$dotnet_version" "$profile" "$case_name" "$command_exit" \
        "$diagnostic_hits" "$marker" "$result" >> "$tmp"
done
mv -f "$tmp" "$results"
trap - EXIT

echo "acceptance evidence: $results ($result)"
if [[ "$result" != PASS ]]; then
    exit 1
fi
