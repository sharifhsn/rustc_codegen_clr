#!/usr/bin/env bash
# Product-shaped SDK bundle gate: create twice, prove byte determinism, restore without using the
# checkout layout, run a scaffolded app from the restored home, and reject post-install tampering.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="${RCL_BUNDLE_DRIVER:-$repo/target/release/cargo-dotnet}"
dotnet_version="${DOTNET_VERSION:-10}"
work="${RCL_BUNDLE_WORK_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-bundle.XXXXXX")}"
keep="${RCL_BUNDLE_KEEP_WORK:-0}"
if [[ "$keep" != 1 ]]; then trap 'rm -rf "$work"' EXIT; fi
[[ -n "$work" && "$work" != "/" ]]
rm -rf "$work"
mkdir -p "$work"

[[ -x "$driver" ]] || {
    echo "cargo-dotnet release driver missing: $driver" >&2
    exit 2
}
[[ -f "$repo/target/release/librustc_codegen_clr.so" || \
   -f "$repo/target/release/librustc_codegen_clr.dylib" || \
   -f "$repo/target/release/rustc_codegen_clr.dll" ]] || {
    echo "release backend missing; run cargo build --release first" >&2
    exit 2
}
[[ -x "$repo/target/release/linker" || -f "$repo/target/release/linker.exe" ]] || {
    echo "release linker missing; run cargo build --release first" >&2
    exit 2
}

source_home="$work/producer-home"
restore_home="$work/consumer-home"
consumer_cargo_home="$work/consumer-cargo-home"
if [[ -n "$(git -C "$repo" status --porcelain=v1 --untracked-files=all)" ]]; then
    echo "install bundle acceptance requires a clean source tree so setup can seal honest provenance" >&2
    exit 2
fi

producer_cargo_home="$work/producer-cargo-home"
CARGO_HOME="$producer_cargo_home" CARGO_DOTNET_HOME="$source_home" \
    "$driver" setup --from-repo "$repo" --home "$source_home" \
    --skip-toolchain --skip-dotnet --force > "$work/setup.log" 2>&1
source_driver="$source_home/bin/cargo-dotnet"
[[ -f "$source_home/bin/cargo-dotnet.exe" ]] \
    && source_driver="$source_home/bin/cargo-dotnet.exe"
[[ -x "$source_driver" || -f "$source_driver" ]]

mkdir -p "$work/artifacts"
"$source_driver" bundle create --home "$source_home" --out "$work/artifacts/sdk-a.zip"
"$source_driver" bundle create --home "$source_home" --out "$work/artifacts/sdk-b.zip"
cmp "$work/artifacts/sdk-a.zip" "$work/artifacts/sdk-b.zip"
"$source_driver" bundle verify "$work/artifacts/sdk-a.zip"
printf 'corrupt' >> "$work/artifacts/sdk-b.zip"
if "$source_driver" bundle verify "$work/artifacts/sdk-b.zip" \
    > "$work/artifacts/archive-tamper.log" 2>&1; then
    echo "corrupted bundle archive unexpectedly verified" >&2
    exit 1
fi
grep -F 'bundle archive SHA-256 mismatch' "$work/artifacts/archive-tamper.log"
mkdir -p "$consumer_cargo_home"
CARGO_HOME="$consumer_cargo_home" \
    "$driver" bundle install "$work/artifacts/sdk-a.zip" --home "$restore_home"

installed_driver="$consumer_cargo_home/bin/cargo-dotnet"
[[ -f "$consumer_cargo_home/bin/cargo-dotnet.exe" ]] \
    && installed_driver="$consumer_cargo_home/bin/cargo-dotnet.exe"
[[ -x "$installed_driver" || -f "$installed_driver" ]]

# Model the documented checkout-independent new shell. The command must be discovered through
# Cargo's subcommand convention, not by reaching into the restored SDK home with an absolute path.
cargo_bin_dir="$(dirname "$(command -v cargo)")"
dotnet_bin_dir="$(dirname "$(command -v dotnet)")"
fresh_path="$consumer_cargo_home/bin:$cargo_bin_dir:$dotnet_bin_dir:/usr/bin:/bin:/usr/sbin:/sbin"
fresh_shell() {
    env -i \
        HOME="$HOME" \
        PATH="$fresh_path" \
        CARGO_HOME="$consumer_cargo_home" \
        CARGO_DOTNET_HOME="$restore_home" \
        CARGO_DOTNET_BACKEND=native \
        DOTNET_VERSION="$dotnet_version" \
        TMPDIR="${TMPDIR:-/tmp}" \
        cargo dotnet "$@"
}
resolved_driver="$(env -i HOME="$HOME" PATH="$fresh_path" /bin/sh -c 'command -v cargo-dotnet')"
[[ "$resolved_driver" == "$installed_driver" ]]
fresh_shell --version > "$work/artifacts/version.log"
grep -F 'cargo-dotnet ' "$work/artifacts/version.log"

mkdir -p "$work/empty-workspace"
fresh_shell doctor \
    --dotnet "$dotnet_version" --workspace "$work/empty-workspace" --json \
    > "$work/artifacts/doctor.json"
grep -F '"label": "install bundle integrity"' "$work/artifacts/doctor.json"
grep -F '"ok": true' "$work/artifacts/doctor.json"

fresh_shell new "$work/hello" \
    --app --dotnet "$dotnet_version" > "$work/artifacts/new.log"
fresh_shell run "$work/hello" --dotnet "$dotnet_version" \
    > "$work/artifacts/run.log" 2>&1
grep -Fx 'hello from Rust on .NET' "$work/artifacts/run.log"

# Exercise the installed binary's reflection build path. The source checkout remains elsewhere on
# disk but is not an ancestor of either the PATH-discovered front-end or this consumer, so mode
# detection and the generated bindgen manifest must resolve mycorrhiza from the restored schema-2
# SDK inventory rather than from cargo-dotnet's compile-time CARGO_MANIFEST_DIR.
fresh_shell new "$work/installed-nuget" \
    --app --dotnet "$dotnet_version" > "$work/artifacts/nuget-new.log"
mkdir -p "$work/installed-nuget-package" "$work/installed-nuget/local-feed"
cat > "$work/installed-nuget-package/InstalledFixture.csproj" <<EOF
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net${dotnet_version}.0</TargetFramework>
    <PackageId>RustcCodegenClr.InstalledFixture</PackageId>
    <Version>1.0.0</Version>
  </PropertyGroup>
</Project>
EOF
cat > "$work/installed-nuget-package/InstalledFixture.cs" <<'EOF'
namespace InstalledFixture;

public sealed class Probe
{
    public int Twice(int value) => value * 2;
}
EOF
dotnet pack "$work/installed-nuget-package/InstalledFixture.csproj" \
    -c Release -o "$work/installed-nuget/local-feed" --nologo \
    > "$work/artifacts/nuget-pack.log" 2>&1
fresh_shell add-nuget RustcCodegenClr.InstalledFixture 1.0.0 "$work/installed-nuget" \
    --source "$work/installed-nuget/local-feed" --force --dotnet "$dotnet_version" \
    > "$work/artifacts/installed-bindgen.log" 2>&1
installed_bindings="$work/installed-nuget/src/nuget/rustccodegenclr_installedfixture.rs"
[[ -s "$installed_bindings" ]]
grep -F 'InstalledFixture' "$installed_bindings" > /dev/null

printf 'fn injected() {}\n' > "$restore_home/dotnet_pal/injected.rs"
if fresh_shell doctor \
    --dotnet "$dotnet_version" --workspace "$work/empty-workspace" --json \
    > "$work/artifacts/extra-file-tamper.json" 2>&1; then
    echo "bundle home with an injected file unexpectedly passed doctor" >&2
    exit 1
fi
grep -F 'undeclared file' "$work/artifacts/extra-file-tamper.json"
rm "$restore_home/dotnet_pal/injected.rs"

printf '\ntampered\n' >> "$restore_home/target/x86_64-unknown-dotnet.json"
if fresh_shell doctor \
    --dotnet "$dotnet_version" --workspace "$work/empty-workspace" --json \
    > "$work/artifacts/tamper.json" 2>&1; then
    echo "tampered bundle home unexpectedly passed doctor" >&2
    exit 1
fi
grep -F '"label": "install bundle integrity"' "$work/artifacts/tamper.json"
grep -F '"ok": false' "$work/artifacts/tamper.json"

echo "== install_bundle_acceptance done: deterministic, PATH-discovered repo-less run, tamper rejected =="
