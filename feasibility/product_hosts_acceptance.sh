#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/rust-dotnet-product-hosts.XXXXXX")"
web_pid=""
cleanup() {
    if [[ -n "$web_pid" ]]; then
        kill "$web_pid" 2>/dev/null || true
        wait "$web_pid" 2>/dev/null || true
    fi
    rm -rf "$work"
}
trap cleanup EXIT

cd "$repo"
cargo build -p cargo-dotnet
cargo_dotnet="$repo/target/debug/cargo-dotnet"
native="$repo/cargo_tests/pinvoke_async_callback_native"
cargo build --manifest-path "$native/Cargo.toml" --release

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) rid="osx-arm64"; native_library="$native/target/release/libasync_callback.dylib" ;;
    Linux-x86_64) rid="linux-x64"; native_library="$native/target/release/libasync_callback.so" ;;
    MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) rid="win-x64"; native_library="$native/target/release/async_callback.dll" ;;
    *) echo "unsupported product-host acceptance: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
native_filename="$(basename "$native_library")"

for template in webapi worker winui maui; do
    "$cargo_dotnet" new "$work/$template-demo" "--$template"
done

# Windows-only projects are contract-checked here. Runtime support remains planned until the
# Windows CI jobs build and launch them with their actual workloads installed.
winui_project="$work/winui-demo/winui/WinuiDemo.WinUI.csproj"
maui_project="$work/maui-demo/maui/MauiDemo.Maui.csproj"
rg -q '<RustDotnetCompatibilityProfile>winui3-net10-windows</RustDotnetCompatibilityProfile>' "$winui_project"
rg -q '<UseWinUI>true</UseWinUI>' "$winui_project"
rg -q '<RustDotnetCompatibilityProfile>maui-windows-net10</RustDotnetCompatibilityProfile>' "$maui_project"
rg -q '<UseMaui>true</UseMaui>' "$maui_project"
if rg -q 'net10\.0-(android|ios|maccatalyst)' "$maui_project"; then
    echo "MAUI scaffold advertises an unproven mobile target" >&2
    exit 1
fi

export CARGO_DOTNET_BACKEND=native
sdk_home="$repo"
cargo_dotnet_msbuild="$cargo_dotnet"
if command -v cygpath >/dev/null 2>&1; then
    sdk_home="$(cygpath -w "$repo")"
    cargo_dotnet_msbuild="$(cygpath -w "$cargo_dotnet")"
fi
export CARGO_DOTNET_HOME="$sdk_home"

# Extend the shared managed backend with one real P/Invoke call and vendor its host-RID native
# Rust library. Attached/MSBuild hosts must receive this sidecar without hand-copying it.
cat "$repo/feasibility/fixtures/attach/native_probe.rs" >> "$work/webapi-demo/rustlib/src/lib.rs"
"$cargo_dotnet" add-native-file "$native_library" --library async_callback \
    --path "$work/webapi-demo/rustlib" --rid "$rid"

web_project="$work/webapi-demo/webapi/WebapiDemo.WebApi.csproj"
worker_project="$work/worker-demo/worker/WorkerDemo.Worker.csproj"
attached_dir="$work/attached-consumer"
dotnet new console --name AttachedConsumer --output "$attached_dir" --framework net10.0
cp "$repo/feasibility/fixtures/attach/Program.cs" "$attached_dir/Program.cs"
"$cargo_dotnet" attach "$attached_dir/AttachedConsumer.csproj" \
    --rust-crate "$work/webapi-demo/rustlib"
cp "$attached_dir/AttachedConsumer.csproj" "$work/attached-once.csproj"
"$cargo_dotnet" attach "$attached_dir/AttachedConsumer.csproj" \
    --rust-crate "$work/webapi-demo/rustlib"
cmp "$work/attached-once.csproj" "$attached_dir/AttachedConsumer.csproj"

if ! attached_output="$(dotnet run --project "$attached_dir/AttachedConsumer.csproj" -c Release \
    -p:CargoDotnet="$cargo_dotnet_msbuild" 2>&1)"; then
    echo "$attached_output" >&2
    exit 1
fi
[[ "$attached_output" == *'managed Rust processed 21 into 42'* ]]
[[ "$attached_output" == *'native Rust probe=0'* ]]
test -s "$attached_dir/bin/Release/net10.0/$native_filename"

dotnet build "$web_project" -c Release -p:CargoDotnet="$cargo_dotnet_msbuild"
dotnet build "$worker_project" -c Release -p:CargoDotnet="$cargo_dotnet_msbuild"

web_log="$work/webapi.log"
port=$((40000 + RANDOM % 20000))
web_url="http://127.0.0.1:$port"
dotnet run --project "$web_project" -c Release --no-build --urls "$web_url" >"$web_log" 2>&1 &
web_pid=$!

for _ in {1..100}; do
    if response="$(curl --fail --silent "$web_url/health" 2>/dev/null)"; then
        break
    fi
    if ! kill -0 "$web_pid" 2>/dev/null; then
        echo "generated Web API exited before listening" >&2
        head -200 "$web_log" >&2
        exit 1
    fi
    sleep 0.1
done
[[ -n "${response:-}" ]] || { echo "generated Web API did not answer /health" >&2; exit 1; }
[[ "$response" == *'"engine":"managed Rust processed 21 into 42"'* ]]
[[ "$response" == *'"answer":42'* ]]
kill "$web_pid"
wait "$web_pid" 2>/dev/null || true
web_pid=""

worker_output="$(dotnet run --project "$worker_project" -c Release --no-build 2>&1)"
[[ "$worker_output" == *'managed Rust processed 21 into 42; answer=42'* ]]

echo "Product host acceptance OK: attached console copied and executed a vendored native Rust sidecar; Web API and Worker executed managed Rust; WinUI/MAUI contracts generated without unsupported mobile claims"
