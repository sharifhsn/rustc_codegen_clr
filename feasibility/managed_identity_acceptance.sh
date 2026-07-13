#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
project="$repo/cargo_tests/cd_multi_library_collision/consumer/Consumer.csproj"
driver="$repo/tools/cargo-dotnet/target/release/cargo-dotnet"
log_dir="${RCL_MANAGED_IDENTITY_LOG_DIR:-/tmp/rustc_codegen_clr-managed-identity}"
dotnet_version="${DOTNET_VERSION:-10}"
tfm="net${dotnet_version}.0"
mkdir -p "$log_dir"
rm -rf "$repo/cargo_tests/cd_multi_library_collision/consumer/bin" \
    "$repo/cargo_tests/cd_multi_library_collision/consumer/obj"

if [[ ! -x "$driver" ]]; then
    echo "release cargo-dotnet driver missing: $driver" >&2
    exit 2
fi

CARGO_DOTNET_BACKEND=native CARGO_DOTNET_HOME="$repo" dotnet build "$project" \
    -p:CargoDotnet="$driver" -p:RustDotnetForceBuild=true \
    --nologo > "$log_dir/current-collision.log" 2>&1
if ! rg -q 'collision_alpha.dll' "$log_dir/current-collision.log" \
    || ! rg -q 'collision_beta.dll' "$log_dir/current-collision.log" \
    || ! rg -q 'Different.Assembly.dll' "$log_dir/current-collision.log"; then
    echo "multi-library build did not reference Cargo-named and custom-named assemblies" >&2
    tail -80 "$log_dir/current-collision.log" >&2
    exit 1
fi

output="$(CARGO_DOTNET_BACKEND=native CARGO_DOTNET_HOME="$repo" dotnet run --project "$project" --no-build)"
if [[ "$output" != $'alpha\nbeta\ncustom' ]]; then
    echo "unexpected multi-library output: $output" >&2
    exit 1
fi

expect_identity_failure() {
    local name="$1"
    local expected="$2"
    shift 2
    local log="$log_dir/${name}.log"
    if CARGO_DOTNET_BACKEND=native CARGO_DOTNET_HOME="$repo" "$driver" \
        validate-managed-identities "$@" > "$log" 2>&1; then
        echo "managed identity negative case unexpectedly succeeded: $name" >&2
        exit 1
    fi
    if ! rg -q -- "$expected" "$log"; then
        echo "managed identity negative case lacked diagnostic $expected: $name" >&2
        tail -80 "$log" >&2
        exit 1
    fi
}

expect_identity_failure duplicate-assembly 'duplicate managed assembly name' \
    "$repo/cargo_tests/cd_multi_library_collision/alpha" \
    "$repo/cargo_tests/cd_multi_library_collision/duplicate_assembly"
expect_identity_failure duplicate-public-fqn 'duplicate managed public type' \
    "$repo/cargo_tests/cd_multi_library_collision/alpha" \
    "$repo/cargo_tests/cd_multi_library_collision/duplicate_fqn"
custom_dll="$repo/cargo_tests/cd_multi_library_collision/invalid_custom_assembly/target/x86_64-unknown-dotnet/release/Different.Assembly.dll"
custom_receipt="$custom_dll.rustdotnet.receipt.json"
[[ -f "$custom_dll" && -f "$custom_receipt" ]]
jq -e '.managed_identity.assembly_name == "Different.Assembly"' "$custom_receipt" >/dev/null

pack_dir="$log_dir/custom-pack"
rm -rf "$pack_dir"
CARGO_DOTNET_BACKEND=native CARGO_DOTNET_HOME="$repo" "$driver" pack \
    "$repo/cargo_tests/cd_multi_library_collision/invalid_custom_assembly" \
    --out "$pack_dir" --dotnet "$dotnet_version" --validate > "$log_dir/custom-pack.log" 2>&1
custom_package="$pack_dir/Collision.InvalidCustomAssembly.0.1.0.nupkg"
[[ -f "$custom_package" ]]
unzip -Z1 "$custom_package" | rg -q "^lib/$tfm/Different\\.Assembly\\.dll$"
unzip -Z1 "$custom_package" | rg -q "^lib/$tfm/Different\\.Assembly\\.xml$"
consumer="$repo/cargo_tests/cd_multi_library_collision/custom_package_consumer/Consumer.csproj"
rm -rf "$repo/cargo_tests/cd_multi_library_collision/custom_package_consumer/bin" \
    "$repo/cargo_tests/cd_multi_library_collision/custom_package_consumer/obj" \
    "$log_dir/custom-packages"
custom_output="$(dotnet run --project "$consumer" \
    -p:RustDotnetVersion="$dotnet_version" \
    -p:RestoreSources="$pack_dir" \
    -p:RestorePackagesPath="$log_dir/custom-packages")"
[[ "$custom_output" == "custom" ]]

echo '== managed_identity_acceptance passed =='
