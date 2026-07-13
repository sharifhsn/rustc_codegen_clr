#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${RCL_API_DOCS_DIR:-/tmp/rustc_codegen_clr-api-documentation}"
dotnet_version="${DOTNET_VERSION:-10}"
tfm="net${dotnet_version}.0"
driver="$repo/tools/cargo-dotnet/target/release/cargo-dotnet"
work="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-api-docs.XXXXXX")"
trap 'rm -rf "$work"' EXIT

fail() {
    echo "api docs acceptance: $*" >&2
    exit 1
}

[[ -x "$driver" ]] || fail "release cargo-dotnet driver missing: $driver"
mkdir -p "$out/logs" "$out/rust" "$out/csharp" "$out/packages"
find "$out/rust" "$out/csharp" "$out/packages" -mindepth 1 -maxdepth 1 \
    -exec rm -rf -- {} +

echo '== generate Rust API documentation =='
RUSTDOCFLAGS="-D warnings" CARGO_TARGET_DIR="$work/rustdoc-target" \
    cargo doc --manifest-path "$repo/Cargo.toml" \
    --workspace --no-deps --all-features \
    >"$out/logs/rustdoc-workspace.log" 2>&1
RUSTDOCFLAGS="-D warnings" CARGO_TARGET_DIR="$work/rustdoc-target" cargo doc \
    --manifest-path "$repo/tools/cargo-dotnet/Cargo.toml" --no-deps \
    >"$out/logs/rustdoc-cargo-dotnet.log" 2>&1

for crate in rustc_codegen_clr cilly mycorrhiza dotnet_aot dotnet_macros cargo_dotnet; do
    [[ -f "$work/rustdoc-target/doc/$crate/index.html" ]] \
        || fail "missing generated Rust API index for $crate"
done
cp -R "$work/rustdoc-target/doc/." "$out/rust/"
{
    printf '%s\n' '<!doctype html>' '<meta charset="utf-8">' \
        '<title>rustc_codegen_clr API documentation</title>' \
        '<h1>rustc_codegen_clr API documentation</h1>' \
        '<p>Generated from the exact repository revision under acceptance.</p>' '<ul>' \
        '<li><a href="rustc_codegen_clr/index.html">rustc_codegen_clr backend</a></li>' \
        '<li><a href="cilly/index.html">cilly IR and exporters</a></li>' \
        '<li><a href="mycorrhiza/index.html">mycorrhiza interop API</a></li>' \
        '<li><a href="cargo_dotnet/index.html">cargo-dotnet driver</a></li>' \
        '<li><a href="dotnet_macros/index.html">dotnet macros</a></li>' \
        '<li><a href="dotnet_aot/index.html">NativeAOT helpers</a></li>' '</ul>' \
        >"$out/rust/index.html"
}
tar -czf "$out/rust-api-docs.tar.gz" -C "$out/rust" .

echo '== generate packaged C# XML documentation =='
CARGO_DOTNET_BACKEND=native CARGO_DOTNET_HOME="$(dirname "$driver")" \
    "$driver" pack "$repo/cargo_tests/cd_export/rustlib" \
    --id Rcl.ApiDocs.Probe --version 0.0.0 --out "$work/package" \
    --dotnet "$dotnet_version" --validate \
    >"$out/logs/csharp-xmldoc-pack.log" 2>&1

package="$work/package/Rcl.ApiDocs.Probe.0.0.0.nupkg"
xml_path="lib/$tfm/cd_export.xml"
[[ -f "$package" ]] || fail "documentation probe package was not produced"
unzip -Z1 "$package" >"$work/package.entries"
grep -Fxq "$xml_path" "$work/package.entries" || fail "NuGet package is missing $xml_path"
unzip -p "$package" "$xml_path" >"$out/csharp/cd_export.xml"
if command -v xmllint >/dev/null 2>&1; then
    xmllint --noout "$out/csharp/cd_export.xml"
else
    grep -Fq '<doc>' "$out/csharp/cd_export.xml" \
        || fail "generated C# documentation is malformed"
fi
grep -Fq '<member name="M:MainModule.greet(System.String)">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the greet API member"
grep -Fq 'inbound `&amp;str`, outbound `String`' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the Rust source summary"

cp "$package" "$out/packages/"
cp "$package.sha256" "$out/packages/"
cp "$package.rustdotnet.receipt.json" "$out/packages/"
cp "$out/csharp/cd_export.xml" "$out/packages/"
tar -czf "$out/csharp-api-docs.tar.gz" -C "$out/csharp" .

workspace_warnings="$(grep -c '^warning:' "$out/logs/rustdoc-workspace.log" || true)"
driver_warnings="$(grep -c '^warning:' "$out/logs/rustdoc-cargo-dotnet.log" || true)"
[[ "$workspace_warnings" == 0 ]] || fail "workspace rustdoc emitted $workspace_warnings warnings"
[[ "$driver_warnings" == 0 ]] || fail "cargo-dotnet rustdoc emitted $driver_warnings warnings"
{
    printf 'schema=1\n'
    printf 'dotnet_tfm=%s\n' "$tfm"
    printf 'rustdoc_workspace_warnings=%s\n' "$workspace_warnings"
    printf 'rustdoc_cargo_dotnet_warnings=%s\n' "$driver_warnings"
    printf 'rust_indexes=%s\n' 'rustc_codegen_clr,cilly,mycorrhiza,dotnet_aot,dotnet_macros,cargo_dotnet'
    printf 'csharp_xml=%s\n' "$xml_path"
    printf 'csharp_probe_member=%s\n' 'M:MainModule.greet(System.String)'
    printf 'publication_performed=false\n'
} >"$out/SUMMARY.txt"

echo "== API documentation acceptance done: $out =="
