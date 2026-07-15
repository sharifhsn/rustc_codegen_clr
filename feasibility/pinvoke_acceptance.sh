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

echo "==> P/Invoke acceptance passed"
