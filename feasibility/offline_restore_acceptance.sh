#!/usr/bin/env bash
# Prove explicit online restore followed by a Cargo-semantic offline build with no ambient cache.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="${RCL_OFFLINE_RESTORE_DRIVER:-$repo/target/release/cargo-dotnet}"
dotnet_version="${DOTNET_VERSION:-10}"
work="${RCL_OFFLINE_RESTORE_WORK_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-offline.XXXXXX")}"
keep="${RCL_OFFLINE_RESTORE_KEEP_WORK:-0}"
ambient_home="$HOME"
rustup_home="${RUSTUP_HOME:-$ambient_home/.rustup}"
ilasm="${ILASM_PATH:-$ambient_home/.dotnet/ilasm${dotnet_version}-tool/ilasm}"

if [[ "$keep" != 1 ]]; then trap 'rm -rf "$work"' EXIT; fi
[[ -n "$work" && "$work" != "/" ]]
rm -rf "$work"
mkdir -p "$work/home/.cargo" "$work/cache" "$work/sdk" "$work/logs"

[[ -x "$driver" ]] || { echo "cargo-dotnet release driver missing: $driver" >&2; exit 2; }
[[ -d "$rustup_home" ]] || { echo "rustup home missing: $rustup_home" >&2; exit 2; }
[[ -x "$ilasm" ]] || { echo "CoreCLR ilasm missing: $ilasm" >&2; exit 2; }

common_env=(
    HOME="$work/home"
    CARGO_HOME="$work/home/.cargo"
    RUSTUP_HOME="$rustup_home"
    CARGO_DOTNET_HOME="$work/sdk"
    CARGO_DOTNET_CACHE_HOME="$work/cache"
    CARGO_DOTNET_BACKEND=native
    ILASM_PATH="$ilasm"
    DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1
    DOTNET_CLI_TELEMETRY_OPTOUT=1
    NUGET_XMLDOC_MODE=skip
)

env "${common_env[@]}" "$driver" new "$work/hello" --app --dotnet "$dotnet_version" \
    > "$work/logs/new.log" 2>&1
env "${common_env[@]}" "$driver" restore "$work/hello" --dotnet "$dotnet_version" \
    > "$work/logs/restore.log" 2>&1

receipt="$(find "$work/cache" -type f -name restore-receipt.json -print -quit)"
[[ -n "$receipt" && -f "$receipt" ]]
jq -e '.schema == 1 and (.inputs | length > 0) and (.cache | length > 0)' "$receipt" >/dev/null
find "$work/sdk/sysroots" -type f -name READY -print -quit | grep -q .
find "$work/cache" -type f -path '*/registry/cache/*' -print -quit | grep -q .

# Ordinary source edits must not invalidate dependency restore state.
printf '\nfn offline_acceptance_marker() {}\n' >> "$work/hello/src/main.rs"

offline_env=(
    "${common_env[@]}"
    CARGO_NET_OFFLINE=true
    HTTP_PROXY=http://127.0.0.1:9
    HTTPS_PROXY=http://127.0.0.1:9
    ALL_PROXY=http://127.0.0.1:9
    http_proxy=http://127.0.0.1:9
    https_proxy=http://127.0.0.1:9
    all_proxy=http://127.0.0.1:9
)
env -u GIT_HTTP_PROXY -u GIT_HTTPS_PROXY "${offline_env[@]}" \
    "$driver" run "$work/hello" --dotnet "$dotnet_version" --offline --frozen \
    > "$work/logs/offline-run.log" 2>&1
grep -Fx 'hello from Rust on .NET' "$work/logs/offline-run.log"
grep -F 'verified offline restore receipt' "$work/logs/offline-run.log"

# A successful offline build must not mutate the recorded dependency cache contract.
env -u GIT_HTTP_PROXY -u GIT_HTTPS_PROXY "${offline_env[@]}" \
    "$driver" build "$work/hello" --dotnet "$dotnet_version" --offline --frozen \
    > "$work/logs/offline-rebuild.log" 2>&1
grep -F 'verified offline restore receipt' "$work/logs/offline-rebuild.log"

# Receipt verification must fail before Cargo can use a damaged private cache.
cache_file="$(find "$work/cache" -type f -path '*/registry/cache/*' -print -quit)"
printf '\ntampered\n' >> "$cache_file"
if env -u GIT_HTTP_PROXY -u GIT_HTTPS_PROXY "${offline_env[@]}" \
    "$driver" build "$work/hello" --dotnet "$dotnet_version" --offline --frozen \
    > "$work/logs/cache-tamper.log" 2>&1; then
    echo "tampered private Cargo cache unexpectedly passed offline preflight" >&2
    exit 1
fi
grep -F 'offline restore receipt is stale' "$work/logs/cache-tamper.log"
grep -F 'private Cargo cache changed' "$work/logs/cache-tamper.log"

echo "== offline_restore_acceptance done: isolated restore, offline run, cache tamper rejected =="
