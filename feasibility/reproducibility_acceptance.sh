#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
evidence="${RCL_REPRO_EVIDENCE_DIR:-${TMPDIR:-/tmp}/rustc_codegen_clr-reproducibility-evidence}"
fixture="cargo_tests/cd_interop/rustlib"
package="Rcl.Reproducibility.Probe.1.0.0.nupkg"
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
ilasm_path="${ILASM_PATH:-$HOME/.dotnet/ilasm-tool/ilasm}"

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

fail() {
    echo "reproducibility acceptance: $*" >&2
    exit 1
}

for tool in cargo git jq unzip; do
    command -v "$tool" >/dev/null || fail "required tool is missing: $tool"
done
[[ -x "$ilasm_path" ]] || fail "CoreCLR ilasm is missing: $ilasm_path"

# Release evidence must describe exactly HEAD. In particular, `git worktree add HEAD` must never
# silently omit a caller's modified or untracked source inputs.
[[ -z "$(git -C "$repo" status --porcelain --untracked-files=all)" ]] ||
    fail "HEAD is dirty; commit every source-affecting input before collecting release evidence"

commit="$(git -C "$repo" rev-parse HEAD)"
source_date_epoch="$(git -C "$repo" show -s --format=%ct "$commit")"
run_root="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-repro.XXXXXX")"
tree_a="$run_root/source-a"
tree_b="$run_root/source-b"
cleanup() {
    if [[ "${RCL_REPRO_KEEP_WORKTREES:-0}" == 1 ]]; then
        echo "reproducibility worktrees preserved at $run_root" >&2
        return
    fi
    git -C "$repo" worktree remove --force "$tree_a" >/dev/null 2>&1 || true
    git -C "$repo" worktree remove --force "$tree_b" >/dev/null 2>&1 || true
    rm -rf "$run_root"
}
trap cleanup EXIT

rm -rf "$evidence"
mkdir -p "$evidence/a" "$evidence/b"
git -C "$repo" worktree add --detach "$tree_a" "$commit" >"$evidence/worktree-a.log" 2>&1
git -C "$repo" worktree add --detach "$tree_b" "$commit" >"$evidence/worktree-b.log" 2>&1

build_side() {
    side="$1"
    tree="$2"
    root="$run_root/private-$side"
    tree_canonical="$(cd "$tree" && pwd -P)"
    root_canonical="$(mkdir -p "$root" && cd "$root" && pwd -P)"
    tree_alias="${tree_canonical#/private}"
    root_alias="${root_canonical#/private}"
    remap_flags="--remap-path-prefix=$tree=/_/rustc_codegen_clr --remap-path-prefix=$tree_canonical=/_/rustc_codegen_clr --remap-path-prefix=$tree_alias=/_/rustc_codegen_clr --remap-path-prefix=$root=/_/build-root --remap-path-prefix=$root_canonical=/_/build-root --remap-path-prefix=$root_alias=/_/build-root -C codegen-units=1"
    out="$evidence/$side"
    mkdir -p "$root/home" "$root/cargo" "$root/cache" "$root/nuget" "$root/tmp" "$out/package"

    (cd "$tree" && env \
        HOME="$root/home" \
        RUSTUP_HOME="$rustup_home" \
        CARGO_HOME="$root/cargo" \
        CARGO_DOTNET_CACHE_HOME="$root/cache" \
        NUGET_PACKAGES="$root/nuget" \
        TMPDIR="$root/tmp" \
        ILASM_PATH="$ilasm_path" \
        CARGO_INCREMENTAL=0 \
        SOURCE_DATE_EPOCH="$source_date_epoch" \
        RUSTFLAGS="$remap_flags" \
        cargo build --manifest-path "tools/cargo-dotnet/Cargo.toml" --release) \
        >"$out/cargo-dotnet-build.log" 2>&1
    (cd "$tree" && env \
        HOME="$root/home" \
        RUSTUP_HOME="$rustup_home" \
        CARGO_HOME="$root/cargo" \
        CARGO_DOTNET_CACHE_HOME="$root/cache" \
        NUGET_PACKAGES="$root/nuget" \
        TMPDIR="$root/tmp" \
        ILASM_PATH="$ilasm_path" \
        CARGO_INCREMENTAL=0 \
        SOURCE_DATE_EPOCH="$source_date_epoch" \
        RUSTFLAGS="$remap_flags" \
        cargo build --manifest-path "Cargo.toml" --release --workspace) \
        >"$out/backend-build.log" 2>&1
    env \
        HOME="$root/home" \
        RUSTUP_HOME="$rustup_home" \
        CARGO_HOME="$root/cargo" \
        CARGO_DOTNET_CACHE_HOME="$root/cache" \
        NUGET_PACKAGES="$root/nuget" \
        TMPDIR="$root/tmp" \
        ILASM_PATH="$ilasm_path" \
        CARGO_INCREMENTAL=0 \
        CARGO_BUILD_JOBS=1 \
        CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
        SOURCE_DATE_EPOCH="$source_date_epoch" \
        CARGO_DOTNET_BACKEND=native \
        "$tree/tools/cargo-dotnet/target/release/cargo-dotnet" pack "$tree/$fixture" \
        --id Rcl.Reproducibility.Probe --version 1.0.0 --out "$out/package" --validate \
        >"$out/pack.log" 2>&1

    pkg="$out/package/$package"
    receipt="$pkg.rustdotnet.receipt.json"
    [[ -s "$pkg" && -s "$receipt" && -s "$pkg.sha256" ]] ||
        fail "$side did not produce package, checksum, and receipt"
    unzip -Z1 "$pkg" | LC_ALL=C sort >"$out/zip-entries.txt"
    while IFS= read -r entry; do
        pattern="$entry"
        [[ "$entry" == '[Content_Types].xml' ]] && pattern='\[Content_Types\].xml'
        printf '%s  %s\n' "$(unzip -p "$pkg" "$pattern" | hash_file /dev/stdin)" "$entry"
    done <"$out/zip-entries.txt" >"$out/zip-entry-hashes.txt"

    package_hash="$(hash_file "$pkg")"
    [[ "$(jq -r '.sha256' "$receipt")" == "$package_hash" ]] || fail "$side receipt package hash mismatch"
    [[ "$(cut -d ' ' -f 1 "$pkg.sha256")" == "$package_hash" ]] || fail "$side checksum mismatch"
    jq -e '.schema == 1 and (.entries | type == "object" and length > 0)' "$receipt" >/dev/null
    while IFS= read -r line; do
        actual="${line%%  *}"
        entry="${line#*  }"
        [[ "$(jq -r --arg entry "$entry" '.entries[$entry] // empty' "$receipt")" == "$actual" ]] ||
            fail "$side receipt entry hash mismatch: $entry"
    done <"$out/zip-entry-hashes.txt"

    unzip -p "$pkg" 'lib/net8.0/cd_interop.xml' >"$out/api.xml"
    unzip -p "$pkg" 'build/rustdotnet/artifact-provenance.json' >"$out/artifact-provenance.json"
    unzip -p "$pkg" 'build/rustdotnet/sbom.cdx.json' >"$out/sbom.cdx.json"
    unzip -p "$pkg" 'build/rustdotnet/licenses.json' >"$out/licenses.json"
    if command -v xmllint >/dev/null 2>&1; then
        xmllint --noout "$out/api.xml"
    else
        grep -q '<doc>' "$out/api.xml" || fail "$side XML documentation is malformed"
    fi
    jq -e '.schema == 1 and .artifact.sha256 and .xml_docs.sha256' "$out/artifact-provenance.json" >/dev/null
    jq -e '.bom_format == "CycloneDX" and .spec_version == "1.5" and (.components | length > 0)' "$out/sbom.cdx.json" >/dev/null
    jq -e '.schema == 1 and (.components | length > 0)' "$out/licenses.json" >/dev/null
    [[ "$(jq -r '.artifact.sha256' "$out/artifact-provenance.json")" == \
       "$(jq -r '.entries["lib/net8.0/cd_interop.dll"]' "$receipt")" ]] ||
        fail "$side provenance DLL hash does not match the packaged DLL"
    [[ "$(jq -r '.xml_docs.sha256' "$out/artifact-provenance.json")" == \
       "$(jq -r '.entries["lib/net8.0/cd_interop.xml"]' "$receipt")" ]] ||
        fail "$side provenance XML hash does not match the packaged XML"
}

build_side a "$tree_a"
build_side b "$tree_b"

cmp "$evidence/a/package/$package" "$evidence/b/package/$package"
cmp "$evidence/a/package/$package.rustdotnet.receipt.json" \
    "$evidence/b/package/$package.rustdotnet.receipt.json"
cmp "$evidence/a/zip-entry-hashes.txt" "$evidence/b/zip-entry-hashes.txt"
printf 'commit=%s\npackage_sha256=%s\n' "$commit" \
    "$(hash_file "$evidence/a/package/$package")" >"$evidence/SUMMARY.txt"

echo '== reproducibility_acceptance done =='
