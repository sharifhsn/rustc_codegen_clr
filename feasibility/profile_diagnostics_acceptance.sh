#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="$repo/target/release/cargo-dotnet"
[[ -f "$driver.exe" ]] && driver="$driver.exe"
[[ -x "$driver" || -f "$driver" ]] || {
  echo "release cargo-dotnet driver missing: $driver" >&2
  exit 2
}

work="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-profile-diagnostics.XXXXXX")"
trap 'rm -rf "$work"' EXIT

"$driver" new --webapi "$work/web" --name profile_web >/dev/null
"$driver" doctor --workspace "$work/web" --json >"$work/web.json"
grep -Fq '"label": "host/profile:' "$work/web.json"
grep -Fq 'net10-coreclr [supported] matches net10.0' "$work/web.json"

"$driver" new --maui "$work/maui" --name profile_maui >/dev/null
if "$driver" doctor --workspace "$work/maui" --json >"$work/maui.json"; then
  echo "planned MAUI profile unexpectedly passed cargo dotnet doctor" >&2
  exit 1
fi
grep -Fq '"hard": true' "$work/maui.json"
grep -Fq 'compatibility profile `maui-windows-net10` is planned' "$work/maui.json" || \
  grep -Fq 'compatibility profile \"maui-windows-net10\" is planned' "$work/maui.json"

echo '== profile diagnostics acceptance passed: supported Web API; planned MAUI rejected =='
