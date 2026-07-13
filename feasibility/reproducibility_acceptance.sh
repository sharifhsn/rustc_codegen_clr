#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
evidence="${RCL_REPRO_EVIDENCE_DIR:-${TMPDIR:-/tmp}/rustc_codegen_clr-reproducibility-evidence}"
fixture="cargo_tests/cd_interop/rustlib"
package_id="${RCL_REPRO_PACKAGE_ID:-Rcl.Reproducibility.Probe}"
package_version="${RCL_REPRO_PACKAGE_VERSION:-1.0.0}"
release_tag="${RCL_REPRO_RELEASE_TAG:-}"
semver='(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?'
[[ "$package_version" =~ ^${semver}$ ]] || {
    echo "reproducibility acceptance: invalid exact package version: $package_version" >&2
    exit 2
}
if [[ -n "$release_tag" ]]; then
    [[ "$release_tag" == "rust-dotnet-v$package_version" ]] || {
        echo "reproducibility acceptance: release tag $release_tag does not match package version $package_version" >&2
        exit 2
    }
fi
package="$package_id.$package_version.nupkg"
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
dotnet_version="${DOTNET_VERSION:-10}"
tfm="net${dotnet_version}.0"
case "$dotnet_version" in
    8) ilasm_tool='ilasm-tool' ;;
    9|10) ilasm_tool="ilasm${dotnet_version}-tool" ;;
    *) echo "reproducibility acceptance: unsupported DOTNET_VERSION=$dotnet_version" >&2; exit 2 ;;
esac
ilasm_path="${ILASM_PATH:-$HOME/.dotnet/$ilasm_tool/ilasm}"

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
if [[ "${RCL_REPRO_ALLOW_DIRTY_CALLER:-0}" != 1 ]]; then
    [[ -z "$(git -C "$repo" status --porcelain --untracked-files=all)" ]] ||
        fail "HEAD is dirty; commit every source-affecting input before collecting release evidence"
fi

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
    package_dll="lib/net${dotnet_version}.0/cd_interop.dll"
    package_xml="lib/net${dotnet_version}.0/cd_interop.xml"
    root="$run_root/private-$side"
    tree_canonical="$(cd "$tree" && pwd -P)"
    root_canonical="$(mkdir -p "$root" && cd "$root" && pwd -P)"
    tree_alias="${tree_canonical#/private}"
    root_alias="${root_canonical#/private}"
    # The raw remap arguments necessarily contain each side's absolute paths. rustc includes
    # codegen flags in its crate disambiguator, so pin metadata as well as remapping debug paths;
    # otherwise symbol identities differ even when every source byte is identical.
    remap_flags="--remap-path-prefix=$tree=/_/rustc_codegen_clr --remap-path-prefix=$tree_canonical=/_/rustc_codegen_clr --remap-path-prefix=$tree_alias=/_/rustc_codegen_clr --remap-path-prefix=$root=/_/build-root --remap-path-prefix=$root_canonical=/_/build-root --remap-path-prefix=$root_alias=/_/build-root -C metadata=rustdotnet-reproducible -C codegen-units=1"
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
        --id "$package_id" --version "$package_version" --out "$out/package" \
        --dotnet "$dotnet_version" --validate \
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
    [[ "$(jq -r '.package' "$receipt")" == "$package" ]] || fail "$side receipt package identity mismatch"
    [[ "$(cut -d ' ' -f 1 "$pkg.sha256")" == "$package_hash" ]] || fail "$side checksum mismatch"
    jq -e '.schema == 1 and (.entries | type == "object" and length > 0)' "$receipt" >/dev/null
    while IFS= read -r line; do
        actual="${line%%  *}"
        entry="${line#*  }"
        [[ "$(jq -r --arg entry "$entry" '.entries[$entry] // empty' "$receipt")" == "$actual" ]] ||
            fail "$side receipt entry hash mismatch: $entry"
    done <"$out/zip-entry-hashes.txt"

    nuspec_entry="$(awk '/\.nuspec$/ { print; exit }' "$out/zip-entries.txt")"
    [[ -n "$nuspec_entry" ]] || fail "$side package has no nuspec"
    unzip -p "$pkg" "$nuspec_entry" >"$out/package.nuspec"
    grep -Fq "<id>$package_id</id>" "$out/package.nuspec" || fail "$side nuspec package id mismatch"
    grep -Fq "<version>$package_version</version>" "$out/package.nuspec" || fail "$side nuspec version mismatch"

    unzip -p "$pkg" "$package_xml" >"$out/api.xml"
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
       "$(jq -r --arg path "$package_dll" '.entries[$path]' "$receipt")" ]] ||
        fail "$side provenance DLL hash does not match the packaged DLL"
    [[ "$(jq -r '.xml_docs.sha256' "$out/artifact-provenance.json")" == \
       "$(jq -r --arg path "$package_xml" '.entries[$path]' "$receipt")" ]] ||
        fail "$side provenance XML hash does not match the packaged XML"
}

build_side a "$tree_a"
build_side b "$tree_b"

# Reproducibility is a byte-for-byte release property, not merely an equivalent-envelope property.
# Compare both the unpacked entry hashes (for actionable failure evidence) and the final NuGet bytes.
cmp "$evidence/a/zip-entry-hashes.txt" "$evidence/b/zip-entry-hashes.txt"
cmp "$evidence/a/package/$package" "$evidence/b/package/$package"
dll_hash_a="$(grep "  lib/$tfm/cd_interop.dll$" "$evidence/a/zip-entry-hashes.txt" | cut -d ' ' -f 1)"
dll_hash_b="$(grep "  lib/$tfm/cd_interop.dll$" "$evidence/b/zip-entry-hashes.txt" | cut -d ' ' -f 1)"
package_hash="$(hash_file "$evidence/a/package/$package")"
printf 'commit=%s\nrelease_tag=%s\npackage_id=%s\npackage_version=%s\npackage_name=%s\npackage_reproducible=true\nenvelope_reproducible=true\nmanaged_dll_reproducible=true\npackage_sha256=%s\nmanaged_dll_sha256_a=%s\nmanaged_dll_sha256_b=%s\n' \
    "$commit" "$release_tag" "$package_id" "$package_version" "$package" "$package_hash" \
    "$dll_hash_a" "$dll_hash_b" >"$evidence/SUMMARY.txt"

echo '== reproducibility_acceptance done =='
