#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "==> verify generated SQLite declarations and run the complete P/Invoke fixture"
driver="$repo/target/release/cargo-dotnet"
if [[ -f "$driver.exe" ]]; then driver="$driver.exe"; fi
[[ -x "$driver" || -f "$driver" ]] || {
  echo "missing release cargo-dotnet driver: $driver" >&2
  exit 1
}
CARGO_DOTNET_BACKEND=native "$driver" \
  bindgen sqlite3_api.h \
  --library e_sqlite3 \
  --path "$repo/cargo_tests/pinvoke_sqlite" \
  --allowlist-function 'sqlite3_(open|close|exec|errmsg|free|libversion_number)' \
  --allowlist-type 'sqlite3.*' \
  --check
CARGO_DOTNET_BACKEND=native "$driver" \
  run "$repo/cargo_tests/pinvoke_sqlite"

doctor_json="$(CARGO_DOTNET_BACKEND=native "$driver" \
  doctor --workspace "$repo/cargo_tests/pinvoke_sqlite" --json)"
rg -q '"label": "native import e_sqlite3"' <<<"$doctor_json"
rg -q '"ok": true' <<<"$doctor_json"
rg -q 'exports \[sqlite3_close, sqlite3_errmsg, sqlite3_exec, sqlite3_free, sqlite3_libversion_number, sqlite3_open\]' <<<"$doctor_json"

diagnostic_log="${TMPDIR:-/tmp}/rust-dotnet-invalid-pinvoke.$$.log"
trap 'rm -f "$diagnostic_log"' EXIT
if CARGO_DOTNET_BACKEND=native "$driver" \
  build "$repo/cargo_tests/pinvoke_invalid_signature" >"$diagnostic_log" 2>&1; then
  echo "unsupported P/Invoke signature unexpectedly compiled" >&2
  exit 1
fi
rg -q 'library `unsupported_native`' "$diagnostic_log"
rg -q 'Rust symbol `complex_operation`' "$diagnostic_log"
rg -q 'native entry point `native_complex_operation`' "$diagnostic_log"
rg -q 'parameter 1 has unsupported type `&str`' "$diagnostic_log"
rg -q 'use \*const T or \*mut T' "$diagnostic_log"

if CARGO_DOTNET_BACKEND=native "$driver" \
  build "$repo/cargo_tests/pinvoke_invalid_signature" \
  --no-default-features --features callback-abi >"$diagnostic_log" 2>&1; then
  echo "unsupported P/Invoke callback ABI unexpectedly compiled" >&2
  exit 1
fi
rg -q 'Rust symbol `register_callback`' "$diagnostic_log"
rg -q 'native entry point `native_register_callback`' "$diagnostic_log"
rg -q 'callback calling convention `extern "Rust"` is unsupported' "$diagnostic_log"
rg -q 'use `extern "C"`' "$diagnostic_log"

echo "==> P/Invoke acceptance passed, including native library/RID/architecture/export diagnostics"
