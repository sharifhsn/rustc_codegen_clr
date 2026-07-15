#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="$repo/target/release/cargo-dotnet"
log_dir="${RCL_NUGET_LOG_DIR:-/tmp/rustc_codegen_clr-nuget-acceptance}"
dotnet_version="${DOTNET_VERSION:-10}"
tfm="net${dotnet_version}.0"
work="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-nuget-acceptance.XXXXXX")"
trap 'rm -rf "$work"' EXIT

if [[ ! -x "$driver" ]]; then
    echo "release cargo-dotnet driver missing: $driver" >&2
    exit 2
fi
mkdir -p "$log_dir"

# Identical source/package inputs must produce byte-identical NuGet packages.
for side in a b; do
    CARGO_DOTNET_BACKEND=native "$driver" pack "$repo/cargo_tests/cd_interop/rustlib" \
        --id Rcl.Determinism.Probe --version 1.0.0 --out "$work/pack-$side" \
        --dotnet "$dotnet_version" --validate \
        > "$log_dir/pack-$side.log" 2>&1
done
first="$work/pack-a/Rcl.Determinism.Probe.1.0.0.nupkg"
second="$work/pack-b/Rcl.Determinism.Probe.1.0.0.nupkg"
cmp "$first" "$second"
[[ -f "$first.sha256" ]]
[[ -f "$second.sha256" ]]

# A custom package ID must retain the CLR assembly's real filename/identity and execute when
# consumed through ordinary PackageReference restore.
mkdir -p "$work/package-consumer"
cp "$repo/feasibility/fixtures/nuget_consumer/Consumer.csproj" "$work/package-consumer/"
cp "$repo/feasibility/fixtures/nuget_consumer/Program.cs" "$work/package-consumer/"
NUGET_PACKAGES="$work/nuget-packages" dotnet run \
    --project "$work/package-consumer/Consumer.csproj" \
    -p:RustDotnetVersion="$dotnet_version" \
    -p:RestoreSources="$work/pack-a;https://api.nuget.org/v3/index.json" \
    > "$log_dir/package-consumer.log" 2>&1
grep -qx '42' "$log_dir/package-consumer.log"

# A fresh external crate must restore through the SDK assets graph and receive generated bindings
# plus runtime assets without relying on this repository's rustup directory override.
"$driver" new --lib "$work/consumer" --name rcl_nuget_acceptance \
    > "$log_dir/scaffold.log" 2>&1
"$driver" add-nuget Newtonsoft.Json 13.0.3 "$work/consumer/rustlib" --force \
    > "$log_dir/add-nuget.log" 2>&1
[[ -f "$work/consumer/rustlib/src/nuget/newtonsoft_json.rs" ]]
[[ -f "$work/consumer/rustlib/.cargo-dotnet-nuget-deps.json" ]]
[[ -f "$work/consumer/rustlib/.cargo-dotnet-nuget-assets/manifest.json" ]]

# A package must retain the SDK graph's NuGet paths, not the flattened executable-directory
# paths used by `cargo dotnet build`.  Make a small owned fixture for the current host RID,
# package it, then restore and run the ordinary C# consumer from the local feed.  The two opaque
# native/resource payloads are deliberately not invoked: this is a NuGet layout/selection proof,
# not a fake P/Invoke success claim.
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) rid='osx-arm64' ;;
    Darwin-x86_64) rid='osx-x64' ;;
    Linux-x86_64) rid='linux-x64' ;;
    Linux-aarch64) rid='linux-arm64' ;;
    *) echo "unsupported host RID for NuGet layout acceptance: $(uname -s)-$(uname -m)" >&2; exit 2 ;;
esac
rid_crate="$work/rid-crate"
cp -R "$repo/cargo_tests/cd_interop/rustlib" "$rid_crate"
assets="$rid_crate/.cargo-dotnet-nuget-assets"
runtime_path="runtimes/$rid/lib/$tfm/cd_interop.dll"
native_path="runtimes/$rid/native/librcl_rid_asset.$([[ "$rid" == osx-* ]] && echo dylib || echo so)"
resource_path="runtimes/$rid/lib/$tfm/fr/Rcl.Rid.Asset.resources.dll"
mkdir -p "$assets/owned/rid-fixture/$(dirname "$runtime_path")" \
    "$assets/owned/rid-fixture/$(dirname "$native_path")" \
    "$assets/owned/rid-fixture/$(dirname "$resource_path")"
cp "$repo/cargo_tests/cd_interop/rustlib/target/x86_64-unknown-dotnet/release/cd_interop.dll" \
    "$assets/owned/rid-fixture/$runtime_path"
printf 'native RID fixture\n' > "$assets/owned/rid-fixture/$native_path"
printf 'resource RID fixture\n' > "$assets/owned/rid-fixture/$resource_path"
printf '{\n  "version": 1,\n  "roots": {\n    "Rcl.Rid.Fixture": {\n      "assets": [\n        {"owner":"Rcl.Rid.Fixture/1.0.0","kind":"runtime","logical_path":"%s","rid":"%s","fallback":false,"staged_path":"owned/rid-fixture/%s"},\n        {"owner":"Rcl.Rid.Fixture/1.0.0","kind":"native","logical_path":"%s","rid":"%s","fallback":false,"staged_path":"owned/rid-fixture/%s"},\n        {"owner":"Rcl.Rid.Fixture/1.0.0","kind":"resource","logical_path":"%s","rid":"%s","fallback":false,"staged_path":"owned/rid-fixture/%s"}\n      ]\n    }\n  }\n}\n' \
    "$runtime_path" "$rid" "$runtime_path" \
    "$native_path" "$rid" "$native_path" \
    "$resource_path" "$rid" "$resource_path" \
    > "$assets/manifest.json"
CARGO_DOTNET_BACKEND=native "$driver" pack "$rid_crate" \
    --id Rcl.Rid.Assets.Probe --version 1.0.0 --out "$work/rid-pack" \
    --dotnet "$dotnet_version" --validate \
    > "$log_dir/rid-pack.log" 2>&1
rid_package="$work/rid-pack/Rcl.Rid.Assets.Probe.1.0.0.nupkg"
unzip -Z1 "$rid_package" > "$work/rid-package.entries"
grep -Fx "$runtime_path" "$work/rid-package.entries"
grep -Fx "$native_path" "$work/rid-package.entries"
grep -Fx "$resource_path" "$work/rid-package.entries"
mkdir -p "$work/rid-package-consumer"
sed 's/Rcl.Determinism.Probe/Rcl.Rid.Assets.Probe/' \
    "$repo/feasibility/fixtures/nuget_consumer/Consumer.csproj" \
    > "$work/rid-package-consumer/Consumer.csproj"
cp "$repo/feasibility/fixtures/nuget_consumer/Program.cs" "$work/rid-package-consumer/Program.cs"
NUGET_PACKAGES="$work/rid-nuget-packages" dotnet restore \
    "$work/rid-package-consumer/Consumer.csproj" \
    -p:RustDotnetVersion="$dotnet_version" \
    --runtime "$rid" \
    --source "$work/rid-pack" --source https://api.nuget.org/v3/index.json \
    > "$log_dir/rid-package-consumer.log" 2>&1
NUGET_PACKAGES="$work/rid-nuget-packages" dotnet build \
    "$work/rid-package-consumer/Consumer.csproj" --no-restore \
    -p:RustDotnetVersion="$dotnet_version" \
    -p:RuntimeIdentifier="$rid" -p:SelfContained=false \
    >> "$log_dir/rid-package-consumer.log" 2>&1
dotnet "$work/rid-package-consumer/bin/Debug/$tfm/$rid/Consumer.dll" \
    >> "$log_dir/rid-package-consumer.log" 2>&1
grep -qx '42' "$log_dir/rid-package-consumer.log"

echo '== nuget_acceptance done =='
