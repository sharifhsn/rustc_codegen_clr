#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$repo/cargo_tests/pinvoke_policy_diagnostics/Cargo.toml"
work="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-policy-diagnostics.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# Raw bindgen declarations are deliberately independent of the safe facade.
cargo check --manifest-path "$manifest" --no-default-features --features raw \
  >"$work/raw.log" 2>&1

for case in incomplete-function contradictory-string incomplete-retained incomplete-scoped incomplete-handle; do
  if cargo check --manifest-path "$manifest" --no-default-features --features "$case" \
      >"$work/$case.log" 2>&1; then
    echo "invalid native_api policy unexpectedly compiled: $case" >&2
    exit 1
  fi
  grep -Fq 'incomplete or contradictory' "$work/$case.log"
  grep -Eiq 'raw (native handles|callback declarations|declaration|`extern`/bindgen declarations)' \
    "$work/$case.log"
  grep -Fq 'Observed polic' "$work/$case.log"
done

grep -Fq 'function `incomplete_function`' "$work/incomplete-function.log"
grep -Fq 'function `contradictory_string`' "$work/contradictory-string.log"
grep -Fq 'retained callback `Registration`' "$work/incomplete-retained.log"
grep -Fq 'scoped callback `CallbackStorage`' "$work/incomplete-scoped.log"
grep -Fq 'handle `NativeHandle`' "$work/incomplete-handle.log"

echo '== P/Invoke policy diagnostics acceptance passed; raw escape hatch remains available =='
