#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${RCL_API_DOCS_DIR:-/tmp/rustc_codegen_clr-api-documentation}"
dotnet_version="${DOTNET_VERSION:-10}"
tfm="net${dotnet_version}.0"
driver="$repo/target/release/cargo-dotnet"
work="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-api-docs.XXXXXX")"
trap 'rm -rf "$work"' EXIT

select_dotnet() {
    local candidate
    for candidate in \
        "${DOTNET_ROOT:+$DOTNET_ROOT/dotnet}" \
        "$(command -v dotnet 2>/dev/null || true)" \
        "$HOME/.dotnet/dotnet" \
        "$HOME/.dotnet/dotnet.exe"; do
        [[ -n "$candidate" && -f "$candidate" ]] || continue
        if "$candidate" --list-sdks | grep -q "^${dotnet_version}\."; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    echo ".NET $dotnet_version SDK is required for API documentation acceptance" >&2
    return 1
}

dotnet_cmd="$(select_dotnet)"
dotnet_root="$(cd "$(dirname "$dotnet_cmd")" && pwd)"

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

for crate in rustc_codegen_clr cilly mycorrhiza dotnet_aot dotnet_macros cargo_dotnet \
    rust_dotnet_assets rust_dotnet_bindgen rust_dotnet_sdk_core rust_dotnet_pinvoke; do
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
        '<li><a href="rust_dotnet_assets/index.html">Rust/.NET asset resolution</a></li>' \
        '<li><a href="rust_dotnet_bindgen/index.html">C-header P/Invoke generation</a></li>' \
        '<li><a href="rust_dotnet_sdk_core/index.html">Rust/.NET SDK core</a></li>' \
        '<li><a href="rust_dotnet_pinvoke/index.html">P/Invoke helpers</a></li>' \
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
grep -Fq '<param name="name">Name to include in the greeting; `&lt;` and `&amp;` remain escaped in IntelliSense.</param>' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the greet parameter contract"
grep -Fq '<returns>A greeting produced by managed Rust.</returns>' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the greet return contract"
grep -Fq '<exception cref="T:System.Exception">Thrown when the checked answer is unavailable.</exception>' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the managed exception contract"
grep -Fq '<member name="T:RiskQuote">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the DTO type"
grep -Fq '<member name="M:RiskQuote.#ctor(System.Int32,System.Boolean)">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the DTO primary constructor"
grep -Fq '<member name="M:RiskQuote.#ctor">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the parameterless DTO constructor"
grep -Fq '<member name="P:RiskQuote.Value">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the DTO property"
grep -Fq '<member name="M:DocumentationCalculator.Project(System.Int32)">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the generated method"
grep -Fq '<param name="periods">Number of periods to project.</param>' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the generated-method parameter contract"
grep -Fq '<member name="T:IDocumentedBox`1">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the generic interface type"
grep -Fq '<typeparam name="T">Value stored by the interface.</typeparam>' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the interface type-parameter contract"
grep -Fq '<member name="M:IDocumentedBox`1.Put(`0)">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the interface method"
grep -Fq '<member name="M:IDocumentedBox`1.Echo``1(``0)">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the generic interface method"
grep -Fq '<typeparam name="U">Echoed value type.</typeparam>' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the method type-parameter contract"
grep -Fq '<member name="P:IDocumentedBox`1.Count">' "$out/csharp/cd_export.xml" \
    || fail "generated C# documentation lacks the interface property"
grep -Fq '<member name="M:MainModule.version">' "$out/csharp/cd_export.xml" \
    || fail "parameterless XML member IDs must omit parentheses"

echo '== run clean packaged documentation consumer =='
cp -R "$repo/feasibility/fixtures/api_docs_consumer" "$work/consumer"
DOTNET_ROOT="$dotnet_root" "$dotnet_cmd" restore "$work/consumer/ApiDocsConsumer.csproj" \
    -p:RclPackageSource="$work/package" \
    -p:RestorePackagesPath="$work/nuget-cache" \
    >"$out/logs/csharp-xmldoc-consumer-restore.log" 2>&1
DOTNET_ROOT="$dotnet_root" "$dotnet_cmd" run --project "$work/consumer/ApiDocsConsumer.csproj" --no-restore \
    -p:RclPackageSource="$work/package" \
    -p:RestorePackagesPath="$work/nuget-cache" \
    >"$out/logs/csharp-xmldoc-consumer-run.log" 2>&1
grep -Fq 'api docs clean consumer: PASS' "$out/logs/csharp-xmldoc-consumer-run.log" \
    || fail "clean packaged documentation consumer did not pass"

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
    printf 'rust_indexes=%s\n' 'rustc_codegen_clr,cilly,mycorrhiza,dotnet_aot,dotnet_macros,cargo_dotnet,rust_dotnet_assets,rust_dotnet_bindgen,rust_dotnet_sdk_core,rust_dotnet_pinvoke'
    printf 'csharp_xml=%s\n' "$xml_path"
    printf 'csharp_probe_members=%s\n' 'method,param,return,exception,type,constructor,property,generated-method,generic-interface,generic-method,nullable-export,nullable-method,nullable-interface,nullable-dto'
    printf 'clean_packaged_consumer=%s\n' 'passed'
    printf 'publication_performed=false\n'
} >"$out/SUMMARY.txt"

echo "== API documentation acceptance done: $out =="
