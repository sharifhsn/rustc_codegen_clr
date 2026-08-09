#!/usr/bin/env bash
set -euo pipefail

# End-to-end proof that a managed-Rust caller can use slices, UTF-8 strings,
# owned results, and Result while generated code owns the raw C ABI. The
# fixture works on the public RIDs whenever its host can build the native cdylib.

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture="$repo/cargo_tests/pinvoke_safe_rust"
native_crate="$fixture/native-lib"
managed_crate="$fixture/managed-lib"
facade_crate="$fixture/facade"
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
  echo ".NET $dotnet_version SDK is required for safe-Rust P/Invoke acceptance" >&2
  return 1
}

dotnet_cmd="$(select_dotnet)"
dotnet_root="$(cd "$(dirname "$dotnet_cmd")" && pwd)"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) rid="osx-arm64"; library="$native_crate/target/release/libsafe_rust_native.dylib" ;;
  Linux-x86_64) rid="linux-x64"; library="$native_crate/target/release/libsafe_rust_native.so" ;;
  MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) rid="win-x64"; library="$native_crate/target/release/safe_rust_native.dll" ;;
  *) echo "unsupported safe-Rust P/Invoke host: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

cleanup() {
  rm -rf "$fixture/native" "$fixture/.cargo-dotnet-native-files.json"
  rm -rf "$managed_crate/native" "$managed_crate/.cargo-dotnet-native-files.json"
}
trap cleanup EXIT
cleanup

driver="$repo/target/release/cargo-dotnet"
if [[ -f "$driver.exe" ]]; then driver="$driver.exe"; fi
[[ -x "$driver" || -f "$driver" ]] || {
  echo "missing release cargo-dotnet driver: $driver" >&2
  exit 1
}

echo "==> build native safe-Rust contract library for $rid"
cargo +stable build --release --manifest-path "$native_crate/Cargo.toml"

if rg -n 'unsafe[[:space:]]*\{|unsafe[[:space:]]+(extern|fn)|\*(const|mut)[[:space:]]|\.as_(mut_)?ptr\(' \
  "$fixture/src" "$facade_crate/src" "$managed_crate/src" "$native_crate/src"; then
  echo "safe-Rust P/Invoke application source contains raw ABI code" >&2
  exit 1
fi

echo "==> stage generated native-contract library"
"$driver" add-native-file "$library" \
  --library safe_rust_native \
  --path "$fixture" \
  --rid "$rid"
"$driver" add-native-file "$library" \
  --library safe_rust_native \
  --path "$managed_crate" \
  --rid "$rid"

echo "==> run managed Rust through safe borrowed and owned APIs"
CARGO_DOTNET_BACKEND=native "$driver" run "$fixture"

echo "==> call the same safe facade from ordinary C#"
CARGO_DOTNET_BACKEND=native "$driver" build "$managed_crate"
DOTNET_ROOT="$dotnet_root" "$dotnet_cmd" build \
  "$fixture/csharp/SafePInvoke.csproj" -c Release
csharp_output="$fixture/csharp/bin/Release/net10.0"
cp "$library" "$csharp_output/"
DOTNET_ROOT="$dotnet_root" "$dotnet_cmd" "$csharp_output/SafePInvoke.dll"

echo "==> safe managed-Rust to native-Rust P/Invoke acceptance passed"
