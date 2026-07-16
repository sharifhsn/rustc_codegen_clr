#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$ROOT/../.." && pwd)
UNITY_BIN=${1:-${UNITY_BIN:-}}
CARGO_DOTNET=${CARGO_DOTNET:-$REPO/target/release/cargo-dotnet}
if [[ -z "$UNITY_BIN" || ! -x "$UNITY_BIN" ]]; then echo "SKIP: set UNITY_BIN to Unity 6.3 Editor" >&2; exit 2; fi
if [[ ! -x "$CARGO_DOTNET" ]]; then echo "FAIL: build cargo-dotnet first (cargo build --release -p cargo-dotnet)" >&2; exit 1; fi
rm -rf "$ROOT/Assets/Plugins/Managed" "$ROOT/Builds"; mkdir -p "$ROOT/Assets/Plugins/Managed" "$ROOT/Builds"
"$CARGO_DOTNET" unity build "$ROOT" "$ROOT/rust"
[[ -f "$ROOT/Assets/Plugins/Managed/Rust.Unity.Sample.dll" ]] || { echo "FAIL: managed DLL not staged" >&2; exit 1; }
UNITY_CONTENTS=$(cd "$(dirname "$UNITY_BIN")/.." && pwd)
UNITY_MONO="$UNITY_CONTENTS/Resources/Scripting/MonoBleedingEdge/bin/mono"
UNITY_CSC="$UNITY_CONTENTS/Resources/Scripting/MonoBleedingEdge/lib/mono/4.5/csc.exe"
run_player() {
  local app=$1
  local log=$2
  local executable
  executable=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")
  "$app/Contents/MacOS/$executable" -batchmode -nographics -logFile "$log"
}
"$UNITY_MONO" "$UNITY_CSC" -nologo -out:"$ROOT/Builds/managed-probe.exe" "$ROOT/ManagedProbe.cs"
(cd "$ROOT/Assets/Plugins/Managed" && "$UNITY_MONO" "$ROOT/Builds/managed-probe.exe") > "$ROOT/Builds/managed-probe.log"
grep -q 'UNITY_MONO_MANAGED_RUST=42' "$ROOT/Builds/managed-probe.log"
"$CARGO_DOTNET" unity native "$ROOT" "$ROOT/native" --export rust_native_multiply
[[ -f "$ROOT/Assets/Plugins/macOS/libunity_native_sample.dylib" ]] || { echo "FAIL: native plug-in not staged" >&2; exit 1; }
"$UNITY_MONO" "$UNITY_CSC" -nologo -out:"$ROOT/Builds/native-probe.exe" "$ROOT/NativeProbe.cs"
(cd "$ROOT/Assets/Plugins/macOS" && "$UNITY_MONO" "$ROOT/Builds/native-probe.exe") > "$ROOT/Builds/native-probe.log"
grep -q 'UNITY_MONO_NATIVE_RUST=42' "$ROOT/Builds/native-probe.log"
"$UNITY_BIN" -batchmode -nographics -projectPath "$ROOT" \
  -runTests -testPlatform EditMode \
  -testResults "$ROOT/Builds/editor-results.xml" \
  -logFile "$ROOT/Builds/editor-tests.log"
"$UNITY_BIN" -batchmode -nographics -projectPath "$ROOT" \
  -runTests -testPlatform PlayMode \
  -testResults "$ROOT/Builds/playmode-results.xml" \
  -logFile "$ROOT/Builds/playmode-tests.log"
"$UNITY_BIN" -batchmode -nographics -quit -projectPath "$ROOT" \
  -executeMethod BuildPlayers.Mono -logFile "$ROOT/Builds/mono-build.log"
run_player "$ROOT/Builds/Mono.app" "$ROOT/Builds/mono.log"
grep -q 'RUST_UNITY_OK=42,DOMAIN=98,NATIVE=42' "$ROOT/Builds/mono.log"
"$UNITY_BIN" -batchmode -nographics -quit -projectPath "$ROOT" \
  -executeMethod BuildPlayers.IL2CPP -logFile "$ROOT/Builds/il2cpp-build.log"
run_player "$ROOT/Builds/IL2CPP.app" "$ROOT/Builds/il2cpp.log"
grep -q 'RUST_UNITY_OK=42,DOMAIN=98,NATIVE=42' "$ROOT/Builds/il2cpp.log"
echo "PASS: EditMode, PlayMode, Mono, and IL2CPP gates completed."
