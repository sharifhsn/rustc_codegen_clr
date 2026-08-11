#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
host="${RCL_RELEASE_HOST:?set RCL_RELEASE_HOST}"
version="${RCL_RELEASE_VERSION:?set RCL_RELEASE_VERSION}"
work="${RCL_RELEASE_WORK_DIR:-${RUNNER_TEMP:-/tmp}/rust-dotnet-release}"

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

hash_git_tree() {
    if command -v sha256sum >/dev/null 2>&1; then
        git archive --format=tar HEAD | sha256sum | awk '{print $1}'
    else
        git archive --format=tar HEAD | shasum -a 256 | awk '{print $1}'
    fi
}

if [[ "${RUNNER_OS:-}" == Windows ]] && command -v cygpath >/dev/null 2>&1; then
    work="$(cygpath -u "$work")"
fi

case "$host" in
    linux-x64)
        backend="target/release/librustc_codegen_clr.so"
        backend_name="librustc_codegen_clr.so"
        linker="target/release/linker"
        driver="target/release/cargo-dotnet"
        asset_driver="cargo-dotnet-linux-x64"
        runtime_rid="linux-x64"
        ;;
    macos-arm64)
        backend="target/release/librustc_codegen_clr.dylib"
        backend_name="librustc_codegen_clr.dylib"
        linker="target/release/linker"
        driver="target/release/cargo-dotnet"
        asset_driver="cargo-dotnet-macos-arm64"
        runtime_rid="osx-arm64"
        ;;
    windows-x64)
        backend="target/release/rustc_codegen_clr.dll"
        backend_name="rustc_codegen_clr.dll"
        linker="target/release/linker.exe"
        driver="target/release/cargo-dotnet.exe"
        asset_driver="cargo-dotnet-windows-x64.exe"
        runtime_rid="win-x64"
        ;;
    *)
        echo "unsupported release host: $host" >&2
        exit 2
        ;;
esac

cd "$repo"
[[ -z "$(git status --porcelain=v1 --untracked-files=all)" ]] || {
    echo "release bundle refuses a dirty source tree" >&2
    exit 2
}
head_revision="$(git rev-parse --verify HEAD)"
source_tree_sha256="$(hash_git_tree)"
driver_build_id="source-sha256:$source_tree_sha256"
# Build the packaged driver here with the identity this script is about to record. Relying on a
# prior workflow step would let a stale/default-ID target artifact be mislabeled by VERSION.
CARGO_DOTNET_BUILD_ID="$driver_build_id" \
    cargo +stable build --release --frozen --manifest-path tools/cargo-dotnet/Cargo.toml
for required in "$backend" "$linker" "$driver"; do
    [[ -f "$required" ]] || {
        echo "release artifact is missing: $required" >&2
        echo "build the compiler workspace and cargo-dotnet in release mode first" >&2
        exit 2
    }
done

home="$work/sdk-home"
out="$work/release-assets"
install_home="$work/install-home"
cargo_home="$work/cargo-home"
setup_cargo_home="$work/setup-cargo-home"
rm -rf "$work"
mkdir -p "$out"
CARGO_HOME="$setup_cargo_home" CARGO_DOTNET_HOME="$home" \
    "$driver" setup --from-repo "$repo" --home "$home" \
    --skip-toolchain --skip-dotnet --force
home_driver="$home/bin/cargo-dotnet"
[[ "$host" == windows-x64 ]] && home_driver="$home/bin/cargo-dotnet.exe"
[[ -f "$home_driver" ]] || {
    echo "sealed SDK home driver is missing: $home_driver" >&2
    exit 2
}

cp "$home_driver" "$out/$asset_driver"
chmod +x "$out/$asset_driver" 2>/dev/null || true
printf '%s  %s\n' "$(hash_file "$out/$asset_driver")" "$asset_driver" \
    > "$out/$asset_driver.sha256"
bundle="$out/cargo-dotnet-sdk-$host-$version.zip"
"$home_driver" bundle create --home "$home" --out "$bundle"
"$home_driver" bundle verify "$bundle"

CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$driver" bundle install "$bundle"
installed="$cargo_home/bin/cargo-dotnet"
[[ "$host" == windows-x64 ]] && installed="$cargo_home/bin/cargo-dotnet.exe"
"$installed" --version

hello="$work/hello-dotnet"
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$installed" dotnet new "$hello" --app
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$installed" dotnet doctor --workspace "$hello"
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$installed" dotnet run "$hello" --release

# Prove the installed, repo-independent SDK carries the helper crate and can restore, stage, and
# execute a host-native P/Invoke dependency from only the checked-in project record.
pinvoke="$work/pinvoke-sqlite"
mkdir -p "$pinvoke/src"
cp cargo_tests/pinvoke_sqlite/Cargo.toml "$pinvoke/Cargo.toml"
cp cargo_tests/pinvoke_sqlite/.cargo-dotnet-nuget-deps.json \
    "$pinvoke/.cargo-dotnet-nuget-deps.json"
cp cargo_tests/pinvoke_sqlite/src/main.rs "$pinvoke/src/main.rs"
cp cargo_tests/pinvoke_sqlite/src/native.rs "$pinvoke/src/native.rs"
cp cargo_tests/pinvoke_sqlite/src/sqlite.rs "$pinvoke/src/sqlite.rs"
cp cargo_tests/pinvoke_sqlite/sqlite3_api.h "$pinvoke/sqlite3_api.h"
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$installed" dotnet bindgen sqlite3_api.h \
    --library e_sqlite3 \
    --path "$pinvoke" \
    --allowlist-function 'sqlite3_(open|close|exec|errmsg|free|libversion_number)' \
    --allowlist-type 'sqlite3.*' \
    --check
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$installed" dotnet run "$pinvoke" --release

# Prove the installed SDK's MSBuild integration from an unrelated C# project, including the hard
# deployment case that motivated the runtime-asset manifest: managed Rust calls a vendored native
# Rust library through P/Invoke, while the C# host receives and loads the sidecar automatically.
native_crate="$repo/cargo_tests/pinvoke_async_callback_native"
cargo build --manifest-path "$native_crate/Cargo.toml" --release
case "$host" in
    linux-x64) native_rid="linux-x64"; native_library="$native_crate/target/release/libasync_callback.so" ;;
    macos-arm64) native_rid="osx-arm64"; native_library="$native_crate/target/release/libasync_callback.dylib" ;;
    windows-x64) native_rid="win-x64"; native_library="$native_crate/target/release/async_callback.dll" ;;
esac
native_filename="$(basename "$native_library")"

product="$work/webapi-demo"
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$installed" dotnet new "$product" --webapi
cat "$repo/feasibility/fixtures/attach/native_probe.rs" >> "$product/rustlib/src/lib.rs"
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$installed" dotnet add-native-file "$native_library" \
    --library async_callback --path "$product/rustlib" --rid "$native_rid"

consumer="$work/attached-consumer"
dotnet new console --name AttachedConsumer --output "$consumer" --framework net10.0
cp "$repo/feasibility/fixtures/attach/Program.cs" "$consumer/Program.cs"
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$installed" dotnet attach "$consumer/AttachedConsumer.csproj" \
    --rust-crate "$product/rustlib"
cargo_dotnet_msbuild="$installed"
if command -v cygpath >/dev/null 2>&1; then
    cargo_dotnet_msbuild="$(cygpath -w "$installed")"
fi
attached_log="$work/attached-consumer.log"
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    dotnet run --project "$consumer/AttachedConsumer.csproj" -c Release \
    -p:CargoDotnet="$cargo_dotnet_msbuild" > "$attached_log" 2>&1
grep -Fq 'managed Rust processed 21 into 42' "$attached_log"
grep -Fq 'native Rust probe=0' "$attached_log"
test -s "$consumer/bin/Release/net10.0/$native_filename"

echo "== release bundle ready: $bundle =="
