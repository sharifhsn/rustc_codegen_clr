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
mkdir -p "$source_home/bin" "$source_home/target" "$source_home/crates"

for candidate in "$repo/target/release/librustc_codegen_clr.so" \
    "$repo/target/release/librustc_codegen_clr.dylib" \
    "$repo/target/release/rustc_codegen_clr.dll"; do
    [[ -f "$candidate" ]] && cp "$candidate" "$source_home/bin/"
done
[[ -f "$repo/target/release/linker.exe" ]] \
    && cp "$repo/target/release/linker.exe" "$source_home/bin/" \
    || cp "$repo/target/release/linker" "$source_home/bin/"
cp "$repo/x86_64-unknown-dotnet.json" "$source_home/target/"
cp "$repo/feasibility/_cargo_dotnet_core.sh" "$source_home/core.sh"
cp "$repo/feasibility/cargo-dotnet" "$source_home/cargo-dotnet"
cp -R "$repo/dotnet_pal" "$source_home/dotnet_pal"
cp -R "$repo/dotnet_overlays" "$source_home/dotnet_overlays"
cp -R "$repo/msbuild" "$source_home/msbuild"
cp -R "$repo/mycorrhiza" "$source_home/crates/mycorrhiza"
cp -R "$repo/dotnet_macros" "$source_home/crates/dotnet_macros"
cp -R "$repo/crates/rust-dotnet-pinvoke" "$source_home/crates/rust-dotnet-pinvoke"
cp -R "$repo/mycorrhiza_interop_helpers" "$source_home/mycorrhiza_interop_helpers"

toolchain="$(awk -F '"' '/channel/ { print $2; exit }' "$repo/rust-toolchain.toml")"
git_rev="$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)"
printf 'schema = 1\ngit_rev = %s\nrelease_tag = untagged\nhost = %s\ntoolchain = %s\n' \
    "$git_rev" "$(uname -sm)" "$toolchain" > "$source_home/VERSION"

mkdir -p "$work/artifacts"
"$driver" bundle create --home "$source_home" --out "$work/artifacts/sdk-a.zip"
"$driver" bundle create --home "$source_home" --out "$work/artifacts/sdk-b.zip"
cmp "$work/artifacts/sdk-a.zip" "$work/artifacts/sdk-b.zip"
"$driver" bundle verify "$work/artifacts/sdk-a.zip"
printf 'corrupt' >> "$work/artifacts/sdk-b.zip"
if "$driver" bundle verify "$work/artifacts/sdk-b.zip" \
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
