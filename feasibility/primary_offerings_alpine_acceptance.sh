#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
pilot="${PRIMARY_OFFERINGS_PILOT_ROOT:-/Users/sharif/Code/monark/worktrees/primary-offerings/rust-dotnet-aip-pilot}"
crate="$pilot/rust/aip-position-parser"
project="$pilot/rust-dotnet-aip-pilot/RustDotnet.AipPositionParser.Pilot.csproj"
dockerfile="$pilot/rust-dotnet-aip-pilot/Dockerfile.alpine-smoke"
feed="$pilot/.rust-dotnet-aip-feed"
driver="$repo/tools/cargo-dotnet/target/release/cargo-dotnet"
work="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-po-alpine.XXXXXX")"
log_dir="${RCL_PO_ALPINE_LOG_DIR:-/tmp/rustc_codegen_clr-po-alpine-acceptance}"
version="0.1.0-alpine.$(date -u +%Y%m%d%H%M%S).$(git -C "$repo" rev-parse --short HEAD)"
image="rustdotnet-aip-alpine-smoke:$version"
nuget_config="${NUGET_CONFIG_FILE:-$HOME/.nuget/NuGet/NuGet.Config}"

cleanup() {
    rm -rf "$feed" "$work"
    if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
        docker image rm --force "$image" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

for required in "$crate/Cargo.toml" "$project" "$dockerfile" "$nuget_config"; do
    if [[ ! -f "$required" ]]; then
        echo "required pilot input missing: $required" >&2
        exit 2
    fi
done

mkdir -p "$feed" "$log_dir"

# The pilot intentionally names unpublished SDK crates like a future registry consumer. For this
# checkout proof, point a temporary copy at the exact SDK sources being packed and leave the pilot
# manifest unchanged.
pack_crate="$work/aip-position-parser"
mkdir -p "$pack_crate"
rsync -a --exclude target/ "$crate/" "$pack_crate/"
sed -i.bak \
    -e "s#dotnet_macros = \"0.1.0\"#dotnet_macros = { path = \"$repo/dotnet_macros\" }#" \
    -e "s#mycorrhiza = \"0.0.0\"#mycorrhiza = { path = \"$repo/mycorrhiza\" }#" \
    "$pack_crate/Cargo.toml"
rm -f "$pack_crate/Cargo.toml.bak"

# Build the driver from this checkout so the package proof cannot silently use an older install.
cargo build --manifest-path "$repo/tools/cargo-dotnet/Cargo.toml" --release \
    >"$log_dir/cargo-dotnet-build.log" 2>&1
CARGO_DOTNET_BACKEND=native "$driver" pack "$pack_crate" \
    --id Monark.RustDotnet.AipPositionParser \
    --version "$version" \
    --out "$work/pack" \
    --validate \
    >"$log_dir/pack.log" 2>&1
package="$work/pack/Monark.RustDotnet.AipPositionParser.$version.nupkg"
test -f "$package"
cp "$package" "$feed/"

# Package mode must work with no SDK targets import and no RustCrate build in the consumer.
NUGET_PACKAGES="$work/nuget-packages" dotnet restore "$project" \
    -p:EnableRustDotnetAipPilot=true \
    -p:UseRustDotnetAipPackage=true \
    -p:RustDotnetAipPackageVersion="$version" \
    -p:RustDotnetSdkRoot="$work/missing-sdk" \
    --configfile "$nuget_config" \
    --source "$feed" \
    --source https://api.nuget.org/v3/index.json \
    --source https://nuget.pkg.github.com/Monark-Markets/index.json \
    --source https://nuget.pkg.github.com/sharifhsn/index.json \
    >"$log_dir/package-mode-test.log" 2>&1
NUGET_PACKAGES="$work/nuget-packages" dotnet test "$project" \
    --configuration Release \
    --no-restore \
    -p:EnableRustDotnetAipPilot=true \
    -p:UseRustDotnetAipPackage=true \
    -p:RustDotnetAipPackageVersion="$version" \
    -p:RustDotnetSdkRoot="$work/missing-sdk" \
    >>"$log_dir/package-mode-test.log" 2>&1

if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then
    echo "SKIP: Docker is unavailable; package-mode project validation passed."
    echo "Logs: $log_dir"
    exit 0
fi

docker build \
    --file "$dockerfile" \
    --secret "id=nuget_config,src=$nuget_config" \
    --build-arg "RUST_DOTNET_AIP_PACKAGE_VERSION=$version" \
    --tag "$image" \
    "$pilot" \
    >"$log_dir/docker-build.log" 2>&1
docker run --rm "$image" >"$log_dir/docker-run.log" 2>&1
grep -qx 'RUST_DOTNET_AIP_ALPINE_OK' "$log_dir/docker-run.log"
grep -qx 'DTO_PARSE_OK FirmNumber=F1' "$log_dir/docker-run.log"

echo "PASS: immutable feed package $version restored, published, and ran on .NET 8 Alpine."
echo "Logs: $log_dir"
