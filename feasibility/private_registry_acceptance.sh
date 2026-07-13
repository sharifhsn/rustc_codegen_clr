#!/usr/bin/env bash
# Hermetic acceptance of cargo-dotnet's private Cargo-home layering for an authenticated
# sparse registry.  All credentials are throwaway; this script rejects any log/receipt leak.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="$repo/tools/cargo-dotnet/target/release/cargo-dotnet"
fixture_crate="$repo/feasibility/fixtures/private_registry_crate"
fixture_consumer="$repo/feasibility/fixtures/private_registry_consumer"
server="$repo/feasibility/private_registry_server.py"
work="${RCL_PRIVATE_REGISTRY_WORK_DIR:-$(mktemp -d /tmp/rustc_codegen_clr-private-registry.XXXXXX)}"
keep_work="${RCL_PRIVATE_REGISTRY_KEEP_WORK:-0}"
token="rcl-private-registry-acceptance-token-7f91f96a"
server_pid=""

cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    if [[ "$keep_work" != "1" ]]; then
        rm -rf "$work"
    fi
}
trap cleanup EXIT

mkdir -p "$work/ambient-cargo" "$work/logs" "$work/package-target"
cp -R "$fixture_consumer" "$work/consumer"
cp -R "$fixture_consumer" "$work/metadata-consumer"

# Build a real .crate archive from the committed fixture before measuring the ambient source.
# The cargo-dotnet invocation below is the subject under test; this setup never shares its cache.
CARGO_TARGET_DIR="$work/package-target" cargo +nightly-2026-06-17 package \
    --manifest-path "$fixture_crate/Cargo.toml" --allow-dirty --no-verify > "$work/logs/package.log" 2>&1
archive="$work/package-target/package/rcl-private-registry-probe-0.1.0.crate"
[[ -f "$archive" ]]

RCL_PRIVATE_REGISTRY_TOKEN="$token" python3 "$server" \
    --crate "$archive" \
    --crate-name rcl-private-registry-probe \
    --version 0.1.0 \
    --token-env RCL_PRIVATE_REGISTRY_TOKEN \
    --port-file "$work/port" \
    --events "$work/registry.events" \
    > "$work/logs/registry.log" 2>&1 &
server_pid=$!
for _ in $(seq 1 100); do
    [[ -f "$work/port" ]] && break
    sleep 0.05
done
[[ -f "$work/port" ]]
port="$(<"$work/port")"
index="sparse+http://127.0.0.1:$port/"

printf '%s\n' \
    '[registries.rcl-private]' \
    "index = \"$index\"" \
    '[registry]' \
    'global-credential-providers = ["cargo:token"]' \
    '[source.rcl-private]' \
    "registry = \"$index\"" \
    > "$work/ambient-cargo/config.toml"
printf '%s\n' \
    '[registries.rcl-private]' \
    "token = \"$token\"" \
    > "$work/ambient-cargo/credentials.toml"

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

credential_store() {
    cache="$1"
    matches="$(find "$cache/crates" -path '*/cargo-home/credentials.toml' -type f -print)"
    [[ -n "$matches" && "$(printf '%s\n' "$matches" | wc -l | tr -d ' ')" == 1 ]]
    printf '%s\n' "$matches"
}

ambient_config_before="$(hash_file "$work/ambient-cargo/config.toml")"
ambient_creds_before="$(hash_file "$work/ambient-cargo/credentials.toml")"

# MSBuild asks this helper to resolve Cargo's local dependency closure before invoking a native
# build. It gets a separate empty cache so a successful result proves it layers the private source
# configuration and copied credential file rather than borrowing the ordinary build cache.
CARGO_HOME="$work/ambient-cargo" \
CARGO_DOTNET_CACHE_HOME="$work/metadata-cache" \
"$driver" metadata-inputs "$work/metadata-consumer" --output "$work/metadata-consumer/inputs.txt" \
    > "$work/logs/metadata-inputs.log" 2>&1
[[ -f "$work/metadata-consumer/inputs.txt" ]]
metadata_credentials="$(credential_store "$work/metadata-cache")"
[[ -f "$metadata_credentials" ]]
[[ ! -e "$work/ambient-cargo/registry" ]]

# The consumer has no Cargo.lock: this covers both `generate-lockfile` and the real build path.
# A new cargo-dotnet cache proves copied credentials/source config are usable from an empty store.
CARGO_HOME="$work/ambient-cargo" \
CARGO_DOTNET_CACHE_HOME="$work/private-cache" \
CARGO_DOTNET_BACKEND=native \
"$driver" build "$work/consumer" --debug --clean > "$work/logs/cargo-dotnet.log" 2>&1

[[ "$(hash_file "$work/ambient-cargo/config.toml")" == "$ambient_config_before" ]]
[[ "$(hash_file "$work/ambient-cargo/credentials.toml")" == "$ambient_creds_before" ]]
private_credentials="$(credential_store "$work/private-cache")"
[[ -f "$private_credentials" ]]
[[ ! -e "$work/ambient-cargo/registry" ]]
[[ -d "$(dirname "$private_credentials")/registry" ]]
[[ -f "$work/consumer/Cargo.lock" ]]
[[ -f "$work/consumer/target/x86_64-unknown-dotnet/debug/rcl-private-registry-consumer.rustdotnet.receipt.json" ]]
rg -qx 'config' "$work/registry.events"
rg -qx 'index-authorized' "$work/registry.events"
rg -qx 'download-authorized' "$work/registry.events"

# The token must exist only in the copied private credential store.  The output assembly is
# included here as a package-adjacent artifact; receipts and all uploadable acceptance logs must
# also be clean.  `rg -a -l` makes a binary leak visible without printing the secret itself.
safe_paths=(
    "$work/logs"
    "$work/registry.events"
    "$work/metadata-consumer/inputs.txt"
    "$work/consumer/target/x86_64-unknown-dotnet/debug"
)
if rg -a -F -l -- "$token" "${safe_paths[@]}"; then
    echo 'private registry token leaked outside the private credential store' >&2
    exit 1
fi

echo '== private_registry_acceptance done =='
