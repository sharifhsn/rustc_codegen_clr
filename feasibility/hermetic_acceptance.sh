#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="$repo/tools/cargo-dotnet/target/release/cargo-dotnet"
log_dir="${RCL_HERMETIC_LOG_DIR:-/tmp/rustc_codegen_clr-hermetic-acceptance}"
mkdir -p "$log_dir"
run_root="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-parallel.XXXXXX")"
trap 'rm -rf "$run_root"' EXIT
trace="$run_root/parallel.jsonl"
barrier="$run_root/barrier"
cache="$run_root/cache"
home="$run_root/home"

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

ambient_library="$(rustc +nightly-2026-06-17 --print sysroot)/lib/rustlib/src/rust/library"
rust_src_probe="$ambient_library/std/src/lib.rs"
registry_probe="$(find "$HOME/.cargo/registry/src" -path '*/libc-*/src/lib.rs' -type f 2>/dev/null | LC_ALL=C sort | tail -1)"
rust_before="$(hash_file "$rust_src_probe")"
registry_before=""
if [[ -n "$registry_probe" ]]; then
    registry_before="$(hash_file "$registry_probe")"
fi

# Launch independent consumers against one SDK cache. The acceptance-only barrier is entered while
# each build-stage guard is live: a global lock would make the first process time out, so success
# proves actual overlap rather than merely proving that two serialized commands eventually finish.
CARGO_DOTNET_HOME="$home" CARGO_DOTNET_CACHE_HOME="$cache" \
    CARGO_DOTNET_PARALLEL_TRACE="$trace" CARGO_DOTNET_PARALLEL_BARRIER="$barrier" \
    CARGO_DOTNET_BACKEND=native "$driver" build "$repo/cargo_tests/cd_pure" --debug --clean > "$log_dir/cd-pure.log" 2>&1 &
pure_pid=$!
CARGO_DOTNET_HOME="$home" CARGO_DOTNET_CACHE_HOME="$cache" \
    CARGO_DOTNET_PARALLEL_TRACE="$trace" CARGO_DOTNET_PARALLEL_BARRIER="$barrier" \
    CARGO_DOTNET_BACKEND=native "$driver" build "$repo/cargo_tests/cd_interop/rustlib" --release --clean > "$log_dir/cd-interop.log" 2>&1 &
interop_pid=$!
wait "$pure_pid"
wait "$interop_pid"

rust_after="$(hash_file "$rust_src_probe")"
[[ "$rust_before" == "$rust_after" ]]
if [[ -n "$registry_probe" ]]; then
    registry_after="$(hash_file "$registry_probe")"
    [[ "$registry_before" == "$registry_after" ]]
fi

rg -q '/sysroots/.*/lib/rustlib/src/rust/library/(core|std|alloc)' "$log_dir/cd-pure.log" "$log_dir/cd-interop.log"
pure_receipt="$repo/cargo_tests/cd_pure/target/x86_64-unknown-dotnet/debug/cd_pure.rustdotnet.receipt.json"
interop_receipt="$repo/cargo_tests/cd_interop/rustlib/target/x86_64-unknown-dotnet/release/cd_interop.dll.rustdotnet.receipt.json"
[[ -f "$pure_receipt" ]]
[[ -f "$interop_receipt" ]]
[[ ! -f "$repo/cargo_tests/cd_pure/.cargo/config.toml" ]]
[[ "$(jq -s '[.[] | select(.stage == "build" and .event == "enter")] | length' "$trace")" == 2 ]]
[[ "$(jq -s '[.[] | select(.stage == "build" and .event == "exit")] | length' "$trace")" == 2 ]]
[[ "$(rg -o '"crate_key":"[0-9a-f]+"' "$trace" | sort -u | wc -l | tr -d ' ')" == 2 ]]
! rg -q 'shared-toolchain build lock' "$log_dir/cd-pure.log" "$log_dir/cd-interop.log"
pure_home="$(jq -r '.cargo_home' "$pure_receipt")"
interop_home="$(jq -r '.cargo_home' "$interop_receipt")"
[[ "$pure_home" != "$interop_home" ]]
[[ "$pure_home" == "$cache"/crates/*/cargo-home ]]
[[ "$interop_home" == "$cache"/crates/*/cargo-home ]]
cp "$trace" "$log_dir/parallel-trace.jsonl"

echo '== hermetic_acceptance done: distinct builds overlapped with isolated mutable caches =='
