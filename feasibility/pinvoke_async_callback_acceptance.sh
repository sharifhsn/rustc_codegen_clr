#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
native="$repo/cargo_tests/pinvoke_async_callback_native"
managed="$repo/cargo_tests/pinvoke_async_callback"
managed_job="$repo/cargo_tests/cd_native_job"
dotnet_version="${DOTNET_VERSION:-10}"

select_dotnet() {
  local candidate
  for candidate in \
    "${DOTNET_ROOT:+$DOTNET_ROOT/dotnet}" \
    "$(command -v dotnet 2>/dev/null || true)" \
    "$HOME/.dotnet/dotnet" \
    "$HOME/.dotnet/dotnet.exe"; do
    [[ -n "$candidate" && -f "$candidate" ]] || continue
    if "$candidate" --list-sdks | grep -q "^${dotnet_version}\."; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  echo ".NET $dotnet_version SDK is required for managed native-job acceptance" >&2
  return 1
}

dotnet_cmd="$(select_dotnet)"
dotnet_root="$(cd "$(dirname "$dotnet_cmd")" && pwd)"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) rid="osx-arm64"; library="$native/target/release/libasync_callback.dylib" ;;
  Linux-x86_64) rid="linux-x64"; library="$native/target/release/libasync_callback.so" ;;
  MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) rid="win-x64"; library="$native/target/release/async_callback.dll" ;;
  *) echo "unsupported retained-callback acceptance host: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

cleanup() {
  rm -rf "$managed/native" "$managed/.cargo-dotnet-native-files.json"
  rm -rf "$managed_job/rustlib/native" "$managed_job/rustlib/.cargo-dotnet-native-files.json"
}
trap cleanup EXIT
cleanup

driver="$repo/target/release/cargo-dotnet"
if [[ -f "$driver.exe" ]]; then driver="$driver.exe"; fi
[[ -x "$driver" || -f "$driver" ]] || {
  echo "missing release cargo-dotnet driver: $driver" >&2
  exit 1
}

echo "==> build native retained-callback worker for $rid"
cargo +stable build --release --manifest-path "$native/Cargo.toml"

echo "==> verify declarations and stage the native worker"
CARGO_DOTNET_BACKEND=native "$driver" \
  bindgen async_callback.h \
  --library async_callback \
  --path "$managed" \
  --allowlist-function 'ac_.*' \
  --allowlist-type 'ac_.*' \
  --check
"$driver" add-native-file "$library" \
  --library async_callback \
  --path "$managed" \
  --rid "$rid"
"$driver" add-native-file "$library" \
  --library async_callback \
  --path "$managed_job/rustlib" \
  --rid "$rid"

echo "==> run retained asynchronous callback through CoreCLR"
CARGO_DOTNET_BACKEND=native "$driver" run "$managed" -- all

echo "==> build and run C#-natural managed native job"
CARGO_DOTNET_BACKEND=native "$driver" build "$managed_job/rustlib"
DOTNET_ROOT="$dotnet_root" "$dotnet_cmd" build \
  "$managed_job/csharp/cd_native_job_cs.csproj" -c Release
job_output="$managed_job/csharp/bin/Release/net10.0"
cp "$library" "$job_output/"
DOTNET_ROOT="$dotnet_root" "$dotnet_cmd" "$job_output/cd_native_job_cs.dll"

echo "==> retained asynchronous P/Invoke callback acceptance passed"
