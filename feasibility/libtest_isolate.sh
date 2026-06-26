#!/usr/bin/env bash
# libtest_isolate.sh — run a (backend-compiled) libtest binary to completion despite process-killing
# tests, by the run-till-death / skip / repeat method.
#
# A monolithic libtest binary runs tests sequentially; an ordinary `panic!` is caught by libtest
# (recorded FAILED, run continues), but an ABORT (panic across a `nounwind` boundary, an
# AccessViolation, or a hang) kills the whole process — so a single bad test hides the pass/fail
# status of every test after it. This script runs the binary, finds the test that killed the run
# (the last `test NAME ...` line with no `ok`/`FAILED`), adds it to a `--skip` set, and re-runs,
# until the suite reaches a clean `test result:` summary. The killers it collects are the
# crash/hang/abort tests = candidate backend bugs (each one passes natively in the official suite).
#
# Usage: libtest_isolate.sh <test-binary> [per_run_timeout_secs] [max_rounds]
# Output: a classification of every killer (ABORT / HANG / CRASH) + the final aggregate pass/fail.
set -u
BIN="${1:?usage: libtest_isolate.sh <test-binary> [timeout] [max_rounds]}"
TIMEOUT="${2:-240}"
MAX_ROUNDS="${3:-60}"
export PATH="$HOME/.dotnet:$PATH"
export DOTNET_ROOT="${DOTNET_ROOT:-$HOME/.dotnet}"

WORK="$(mktemp -d)"
SKIP_ARGS=()
KILLERS_FILE="$WORK/killers.txt"
: > "$KILLERS_FILE"
total_pass=0
total_fail=0

round=0
while [ "$round" -lt "$MAX_ROUNDS" ]; do
  round=$((round + 1))
  out="$WORK/run_$round.log"
  timeout "$TIMEOUT" "$BIN" --test-threads=1 ${SKIP_ARGS[@]+"${SKIP_ARGS[@]}"} > "$out" 2>&1
  ec=$?

  passes=$(grep -cE '^test .* \.\.\. ok$' "$out")
  total_pass=$((total_pass + passes))

  if grep -qE '^test result:' "$out"; then
    # Clean completion: collect this round's FAILED (libtest-caught panics) and stop.
    grep -E '^test .* \.\.\. FAILED$' "$out" | sed -E 's/^test (.*) \.\.\. FAILED$/\1/' >> "$WORK/failed.txt"
    echo "[round $round] reached clean summary (+$passes ok)"
    break
  fi

  # No summary => the process was killed. The killer is the last started-but-unfinished test.
  killer=$(grep -E '^test ' "$out" | tail -1 | sed -E 's/^test ([^ ]+) \.\.\..*/\1/')
  if [ -z "$killer" ]; then
    echo "[round $round] died with no identifiable test (ec=$ec); stopping. See $out"
    break
  fi
  # Classify by how it died.
  if [ "$ec" = 124 ]; then
    kind=HANG
  elif grep -qiE 'nounwind|FailFast|aborted' "$out"; then
    kind=ABORT
  elif grep -qiE 'AccessViolation|SIGSEGV|segmentation' "$out"; then
    kind=CRASH
  else
    kind="DIED(ec=$ec)"
  fi
  reason=$(grep -E '^test ' "$out" | tail -1 | sed -E 's/^test [^ ]+ \.\.\. ?//' | cut -c1-90)
  echo "[round $round] +$passes ok, KILLER: $kind  $killer  | $reason"
  echo "$kind	$killer	$reason" >> "$KILLERS_FILE"
  SKIP_ARGS+=(--skip "$killer")
done

echo
echo "==================== SUMMARY ===================="
echo "rounds:        $round"
echo "killers:       $(wc -l < "$KILLERS_FILE" | tr -d ' ')  (crash/hang/abort = candidate backend bugs)"
echo "total ok:      $total_pass"
echo "libtest FAILED (caught panics): $(sort -u "$WORK/failed.txt" 2>/dev/null | wc -l | tr -d ' ')"
echo
echo "---- KILLERS (kind / test / reason) ----"
column -t -s$'\t' "$KILLERS_FILE" 2>/dev/null || cat "$KILLERS_FILE"
echo
echo "---- libtest FAILED (need native diff) ----"
sort -u "$WORK/failed.txt" 2>/dev/null | head -40
echo
echo "(logs in $WORK)"
