#!/usr/bin/env bash
# The ::stable gate, ported from feasibility/dev.sh `gate` (Docker) to run directly on a CI
# runner. Passes iff every failing test group is in the known baseline, or any new failure
# passes on an isolated re-run (the fail*/env tests are flaky under parallel load).
set -u

echo "==> cargo test ::stable (CI skip set)"
out="$(cargo test --release ::stable -- --skip f128 --skip num_test --skip simd --skip fuzz87 2>&1)"
echo "$out" | grep -E 'test result:' || { echo "(no result line — build error?)"; echo "$out" | tail -50; exit 1; }

# Known baseline failure groups — keep in sync with feasibility/dev.sh `gate`.
known=' any atomics catch f16 fastrand_test futex_test hello_world once_lock_test std_hello_world type_id uninit_fill '
new_tests=()
while read -r t; do
  tn="$(echo "$t" | sed -E 's/^ *//')"; [ -z "$tn" ] && continue
  g="$(echo "$tn" | sed -E 's/compile_test::([a-z0-9_]+)::.*/\1/')"
  case "$known" in *" $g "*) : ;; *) new_tests+=("$tn") ;; esac
done < <(echo "$out" | awk '/^failures:/{f=1} f' | grep -E '^    compile_test' | sort -u)

if [ ${#new_tests[@]} -eq 0 ]; then
  echo "OK: only known-baseline failures (no regressions)"
  exit 0
fi

echo "==> ${#new_tests[@]} failure(s) outside baseline; re-running each to filter flakiness"
real=""; flaky=""
for tn in "${new_tests[@]}"; do
  if cargo test --release "$tn" -- --exact 2>&1 | grep -q 'test result: ok'; then
    flaky="$flaky  $tn"$'\n'
  else
    real="$real  $tn"$'\n'
  fi
done
[ -n "$flaky" ] && { echo "~~ flaky (passed on retry, ignore):"; printf '%s' "$flaky"; }
if [ -n "$real" ]; then
  echo "!! REAL REGRESSIONS (failed twice):"
  printf '%s' "$real"
  exit 1
fi
echo "OK: no real regressions (out-of-baseline failures were all flaky)"
