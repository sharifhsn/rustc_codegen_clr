#!/usr/bin/env bash
# The ::stable gate, ported from feasibility/dev.sh `gate` (Docker) to run directly on a CI
# runner. The historical group-wide baseline was intentionally removed: it could hide a new
# regression anywhere inside an exempt group. Until an exact, reviewed per-test baseline exists,
# this gate is strict: every initial failure is retried in isolation, and every reproducing failure
# fails CI. A failure that passes in isolation is reported as load-sensitive/flaky.
set -uo pipefail

echo "==> cargo test ::stable (CI skip set)"
out="$(cargo test --release ::stable -- --skip f128 --skip num_test --skip simd --skip fuzz87 2>&1)"
cargo_status=$?
printf '%s\n' "$out" | grep -E 'test result:' || {
  echo "(no result line — build or infrastructure error; cargo exit $cargo_status)"
  printf '%s\n' "$out" | tail -50
  exit 1
}

if ((cargo_status == 0)); then
  echo "OK: ::stable passed"
  exit 0
fi

# Account for every failed test reported by libtest. If cargo failed for some other reason after
# producing an earlier result line, do not misclassify that build/infrastructure failure as an
# accepted baseline failure.
failed_tests=()
while read -r t; do
  tn="$(echo "$t" | sed -E 's/^ *//')"; [ -z "$tn" ] && continue
  failed_tests+=("$tn")
done < <(printf '%s\n' "$out" | awk '/^failures:/{f=1} f' | grep -E '^    compile_test' | sort -u)

reported_failed="$({
  printf '%s\n' "$out" \
    | sed -nE 's/.*; ([0-9]+) failed;.*/\1/p' \
    | awk '{ total += $1 } END { print total + 0 }'
})"
if ((reported_failed == 0 || ${#failed_tests[@]} != reported_failed)); then
  echo "!! cargo exited $cargo_status, but the gate could account for ${#failed_tests[@]} of $reported_failed reported failed tests"
  echo "!! refusing to treat an unparsed cargo/build failure as an accepted test baseline"
  printf '%s\n' "$out" | tail -80
  exit 1
fi

echo "==> ${#failed_tests[@]} failure(s); re-running each to filter load-sensitive flakiness"
real=""; flaky=""
for tn in "${failed_tests[@]}"; do
  retry_log="$(mktemp)"
  if cargo test --release "$tn" -- --exact >"$retry_log" 2>&1; then
    flaky="$flaky  $tn"$'\n'
  else
    real="$real  $tn"$'\n'
    echo "---- isolated retry failed: $tn ----"
    tail -40 "$retry_log"
  fi
  rm -f "$retry_log"
done
[ -n "$flaky" ] && { echo "~~ load-sensitive/flaky (cargo passed on isolated retry):"; printf '%s' "$flaky"; }
if [ -n "$real" ]; then
  echo "!! REPRODUCING FAILURES (failed in the aggregate and in isolation):"
  printf '%s' "$real"
  exit 1
fi
echo "OK: no reproducing failures (all initial failures passed isolated cargo retries)"
