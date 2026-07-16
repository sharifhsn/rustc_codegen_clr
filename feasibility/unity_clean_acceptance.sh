#!/usr/bin/env bash
set -euo pipefail

REPO=$(cd "$(dirname "$0")/.." && pwd)
UNITY_BIN=${1:-${UNITY_BIN:-}}
CARGO_DOTNET=${CARGO_DOTNET:-$REPO/target/release/cargo-dotnet}

if [[ -z "$UNITY_BIN" || ! -x "$UNITY_BIN" ]]; then
  echo "FAIL: pass the pinned Unity Editor executable or set UNITY_BIN" >&2
  exit 2
fi
if [[ ! -x "$CARGO_DOTNET" ]]; then
  echo "FAIL: build cargo-dotnet first: cargo build --release -p cargo-dotnet" >&2
  exit 1
fi

if [[ -n "${UNITY_ACCEPTANCE_ROOT:-}" ]]; then
  WORK=$UNITY_ACCEPTANCE_ROOT
  mkdir -p "$WORK"
else
  WORK=$(mktemp -d "${TMPDIR:-/tmp}/rust-dotnet-unity-clean.XXXXXX")
  trap 'rm -rf "$WORK"' EXIT
fi

PRODUCER=$WORK/producer/game
PACKAGE=$WORK/com.example.rustgame
CONSUMER=$WORK/consumer/game

run_player() {
  local app=$1
  local log=$2
  local executable
  executable=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")
  "$app/Contents/MacOS/$executable" -batchmode -nographics -logFile "$log"
}

"$CARGO_DOTNET" new "$PRODUCER" --unity
"$CARGO_DOTNET" unity attach "$PRODUCER" "$PRODUCER/rustlib" \
  --native-crate "$PRODUCER/native" --native-export rust_native_multiply
"$CARGO_DOTNET" unity doctor --editor "$UNITY_BIN" --project "$PRODUCER"

"$UNITY_BIN" -batchmode -nographics -quit -projectPath "$PRODUCER" \
  -executeMethod RustcCodegenClr.Unity.Editor.CargoDotnetUnityBuild.Mono \
  -logFile "$PRODUCER/mono-build.log"
run_player "$PRODUCER/Builds/Mono.app" "$PRODUCER/mono-player.log"
grep -q 'RUST_UNITY_READY=1' "$PRODUCER/mono-player.log"

"$UNITY_BIN" -batchmode -nographics -quit -projectPath "$PRODUCER" \
  -executeMethod RustcCodegenClr.Unity.Editor.CargoDotnetUnityBuild.IL2CPP \
  -logFile "$PRODUCER/il2cpp-build.log"
run_player "$PRODUCER/Builds/IL2CPP.app" "$PRODUCER/il2cpp-player.log"
grep -q 'RUST_UNITY_READY=1' "$PRODUCER/il2cpp-player.log"

"$CARGO_DOTNET" unity package "$PRODUCER" "$PACKAGE" \
  --name com.example.rustgame --version 0.0.1

# Prove the package in a second project with no Rust crate or source-checkout reference. The
# scaffold supplies only the readable Unity-side adapter/build automation; all Rust artifacts come
# from the embedded UPM package.
"$CARGO_DOTNET" new "$CONSUMER" --unity
rm -rf "$CONSUMER/rustlib" "$CONSUMER/native"
cp -R "$PACKAGE" "$CONSUMER/Packages/com.example.rustgame"
perl -0pi -e \
  's/"dependencies": \{/"dependencies": {\n    "com.example.rustgame": "file:com.example.rustgame",/' \
  "$CONSUMER/Packages/manifest.json"

"$UNITY_BIN" -batchmode -nographics -quit -projectPath "$CONSUMER" \
  -executeMethod RustcCodegenClr.Unity.Editor.CargoDotnetUnityBuild.Mono \
  -logFile "$CONSUMER/mono-build.log"
run_player "$CONSUMER/Builds/Mono.app" "$CONSUMER/mono-player.log"
grep -q 'RUST_UNITY_READY=1' "$CONSUMER/mono-player.log"

"$UNITY_BIN" -batchmode -nographics -quit -projectPath "$CONSUMER" \
  -executeMethod RustcCodegenClr.Unity.Editor.CargoDotnetUnityBuild.IL2CPP \
  -logFile "$CONSUMER/il2cpp-build.log"
run_player "$CONSUMER/Builds/IL2CPP.app" "$CONSUMER/il2cpp-player.log"
grep -q 'RUST_UNITY_READY=1' "$CONSUMER/il2cpp-player.log"

echo "PASS: clean scaffold, doctor, native staging, UPM import, Mono, and IL2CPP gates completed."
echo "Evidence root: $WORK"
