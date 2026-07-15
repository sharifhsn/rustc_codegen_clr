#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dotnet_version="${DOTNET_VERSION:-10}"
driver="$repo/target/release/cargo-dotnet"
work="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-nativeaot.XXXXXX")"
source_root="$work/source"
host="$source_root/cargo_tests/cd_interop/csharp"
output="${RCL_NATIVEAOT_OUTPUT_DIR:-$work/publish}"
logs="${RCL_NATIVEAOT_LOG_DIR:-$work/logs}"
cleanup() {
    status=$?
    if [[ "$status" -eq 0 ]]; then
        rm -rf "$work"
    else
        echo "NativeAOT acceptance evidence preserved at $work" >&2
    fi
}
trap cleanup EXIT

fail() {
    echo "NativeAOT acceptance: $*" >&2
    [[ ! -f "$logs/publish.log" ]] || tail -80 "$logs/publish.log" >&2
    [[ ! -f "$logs/runtime.log" ]] || tail -80 "$logs/runtime.log" >&2
    exit 1
}

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) host_rid=linux-x64 ;;
    Linux-aarch64|Linux-arm64) host_rid=linux-arm64 ;;
    Darwin-x86_64) host_rid=osx-x64 ;;
    Darwin-arm64) host_rid=osx-arm64 ;;
    *) fail "unsupported host for the published matrix: $(uname -s)-$(uname -m)" ;;
esac
rid="${RCL_NATIVEAOT_RID:-$host_rid}"

command -v git >/dev/null || fail "git is required"
command -v tar >/dev/null || fail "tar is required"
[[ ! -e "$output" || -z "$(find "$output" -mindepth 1 -maxdepth 1 -print -quit)" ]] ||
    fail "output directory is not empty: $output"
[[ ! -e "$logs" || -z "$(find "$logs" -mindepth 1 -maxdepth 1 -print -quit)" ]] ||
    fail "log directory is not empty: $logs"
mkdir -p "$output" "$logs"
mkdir -p "$source_root"
# msbuild/ must ride along: when no installed home has RustDotnet.targets, the csproj's last
# fallback import resolves repo-relative ($(MSBuildThisFileDirectory)../../../msbuild/...), i.e.
# inside this staged tree.
git -C "$repo" archive HEAD cargo_tests/cd_interop/csharp cargo_tests/cd_interop/rustlib msbuild \
    | tar -x -C "$source_root"
cargo build --manifest-path "$repo/tools/cargo-dotnet/Cargo.toml" --release
[[ -x "$driver" ]] || fail "cargo-dotnet release driver was not built"

if ! "$driver" dotnet publish "$host" --dotnet "$dotnet_version" --rid "$rid" \
    --output "$output" >"$logs/publish.log" 2>&1; then
    fail "cargo dotnet publish failed"
fi

binary="$output/cd_interop_cs"
[[ "$rid" == win-* ]] && binary="$binary.exe"
[[ -f "$binary" ]] || fail "published native binary is missing: $binary"
[[ -x "$binary" ]] || fail "published native binary is not executable: $binary"
if ! "$binary" >"$logs/runtime.log" 2>&1; then
    fail "published native binary failed"
fi
grep -Fxq PASS "$logs/runtime.log" || fail "published binary did not report PASS"

echo "NativeAOT binary: $binary"
echo '== nativeaot_acceptance done =='
