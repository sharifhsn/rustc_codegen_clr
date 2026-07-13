#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
host="${RCL_RELEASE_HOST:?set RCL_RELEASE_HOST}"
version="${RCL_RELEASE_VERSION:?set RCL_RELEASE_VERSION}"
work="${RCL_RELEASE_WORK_DIR:-${RUNNER_TEMP:-/tmp}/rust-dotnet-release}"

if [[ "${RUNNER_OS:-}" == Windows ]] && command -v cygpath >/dev/null 2>&1; then
    work="$(cygpath -u "$work")"
fi

case "$host" in
    linux-x64)
        backend="target/release/librustc_codegen_clr.so"
        backend_name="librustc_codegen_clr.so"
        linker="target/release/linker"
        driver="tools/cargo-dotnet/target/release/cargo-dotnet"
        asset_driver="cargo-dotnet-linux-x64"
        ;;
    macos-arm64)
        backend="target/release/librustc_codegen_clr.dylib"
        backend_name="librustc_codegen_clr.dylib"
        linker="target/release/linker"
        driver="tools/cargo-dotnet/target/release/cargo-dotnet"
        asset_driver="cargo-dotnet-macos-arm64"
        ;;
    windows-x64)
        backend="target/release/rustc_codegen_clr.dll"
        backend_name="rustc_codegen_clr.dll"
        linker="target/release/linker.exe"
        driver="tools/cargo-dotnet/target/release/cargo-dotnet.exe"
        asset_driver="cargo-dotnet-windows-x64.exe"
        ;;
    *)
        echo "unsupported release host: $host" >&2
        exit 2
        ;;
esac

cd "$repo"
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
rm -rf "$work"
mkdir -p "$home/bin" "$home/target" "$home/crates" "$out"

case "$host" in
    linux-x64) ilasm_rid="linux-x64"; ilasm_name="ilasm" ;;
    macos-arm64) ilasm_rid="osx-arm64"; ilasm_name="ilasm" ;;
    windows-x64) ilasm_rid="win-x64"; ilasm_name="ilasm.exe" ;;
esac
ilasm_package="$work/ilasm10.zip"
ilasm_extract="$work/ilasm10"
curl -fsSL -o "$ilasm_package" \
    "https://www.nuget.org/api/v2/package/runtime.$ilasm_rid.microsoft.netcore.ilasm/10.0.0"
unzip -qo "$ilasm_package" -d "$ilasm_extract"
cp "$ilasm_extract/runtimes/$ilasm_rid/native/$ilasm_name" "$home/bin/$ilasm_name"
chmod +x "$home/bin/$ilasm_name" 2>/dev/null || true

cp "$backend" "$home/bin/$backend_name"
cp "$linker" "$home/bin/$(basename "$linker")"
cp x86_64-unknown-dotnet.json "$home/target/x86_64-unknown-dotnet.json"
cp feasibility/_cargo_dotnet_core.sh "$home/core.sh"
cp feasibility/cargo-dotnet "$home/cargo-dotnet"
cp -R dotnet_pal dotnet_overlays msbuild "$home/"
cp -R mycorrhiza dotnet_macros "$home/crates/"
cp -R mycorrhiza_interop_helpers "$home/"
printf 'schema = 1\ngit_rev = %s\nrelease_tag = rust-dotnet-v%s\nhost_rid = %s\ntoolchain = nightly-2026-06-17\n' \
    "$(git rev-parse HEAD)" "$version" "$host" > "$home/VERSION"

cp "$driver" "$out/$asset_driver"
chmod +x "$out/$asset_driver" 2>/dev/null || true
bundle="$out/cargo-dotnet-sdk-$host-$version.zip"
"$driver" bundle create --home "$home" --out "$bundle"
"$driver" bundle verify "$bundle"

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

echo "== release bundle ready: $bundle =="
