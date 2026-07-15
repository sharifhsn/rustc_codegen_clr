#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
root="$repo/feasibility/api_compat"
work="${RCL_API_COMPAT_WORK_DIR:-/tmp/rustc_codegen_clr-api-compat}"
rm -rf "$work"
mkdir -p "$work"

dotnet build "$root/ApiSnapshot/ApiSnapshot.csproj" -c Release --nologo > "$work/tool-build.log"
dotnet build "$root/fixture/TypedDto.csproj" -c Release --nologo > "$work/fixture-build.log"

tool="$root/ApiSnapshot/bin/Release/net8.0/ApiSnapshot.dll"
assembly="$root/fixture/bin/Release/net8.0/TypedDto.dll"
baseline="$root/typed-dto.public-api.txt"
current="$work/current.public-api.txt"
dotnet "$tool" "$assembly" > "$current"
diff -u "$baseline" "$current"
bash "$repo/feasibility/api_compat_gate.sh" "$baseline" "$current" 1.0.0 1.0.1

# Snapshot the real backend-generated public contract, not only a Roslyn control fixture. The
# selected types are the comprehensive API-docs fixture's intended managed surface; ordinary Rust
# implementation types are deliberately not part of the contract.
driver="$repo/target/release/cargo-dotnet"
if [[ ! -x "$driver" ]]; then
    cargo build --manifest-path "$repo/Cargo.toml" --release --workspace \
        > "$work/release-build.log" 2>&1
fi
CARGO_DOTNET_BACKEND=native "$driver" build "$repo/cargo_tests/cd_export/rustlib" \
    > "$work/generated-build.log" 2>&1
generated_assembly="$repo/cargo_tests/cd_export/rustlib/target/x86_64-unknown-dotnet/release/cd_export.dll"
generated_baseline="$root/cd-export.public-api.txt"
generated_current="$work/cd-export.public-api.txt"
generated_types=(MainModule RiskQuote NullableProfile DocumentationCalculator 'IDocumentedBox`1')
dotnet "$tool" "$generated_assembly" "${generated_types[@]}" > "$generated_current"
diff -u "$generated_baseline" "$generated_current"
bash "$repo/feasibility/api_compat_gate.sh" \
    "$generated_baseline" "$generated_current" 1.0.0 1.0.1

# Prove that the oracle rejects a representative binary-breaking DTO change without
# modifying the checked-in fixture or baseline.
cp -R "$root/fixture" "$work/breaking-fixture"
perl -pi -e 's/public decimal Amount \{ get; \}/public long Amount { get; }/' "$work/breaking-fixture/TypedDto.cs"
perl -pi -e 's/Amount = amount;/Amount = (long)amount;/' "$work/breaking-fixture/TypedDto.cs"
dotnet build "$work/breaking-fixture/TypedDto.csproj" -c Release --nologo > "$work/breaking-build.log"
dotnet "$tool" "$work/breaking-fixture/bin/Release/net8.0/TypedDto.dll" > "$work/breaking.public-api.txt"
if diff -u "$baseline" "$work/breaking.public-api.txt" > "$work/breaking.diff"; then
    echo "breaking fixture unexpectedly matched the public API baseline" >&2
    exit 1
fi
if ! rg -q 'System\.Int64' "$work/breaking.diff"; then
    echo "breaking fixture failed for an unexpected reason" >&2
    cat "$work/breaking.diff" >&2
    exit 1
fi
if bash "$repo/feasibility/api_compat_gate.sh" \
    "$baseline" "$work/breaking.public-api.txt" 1.0.0 1.1.0 \
    > "$work/non-major.stdout" 2> "$work/non-major.stderr"; then
    echo "breaking API change was accepted without a major-version increase" >&2
    exit 1
fi
rg -q 'requires a major-version increase' "$work/non-major.stderr"
bash "$repo/feasibility/api_compat_gate.sh" \
    "$baseline" "$work/breaking.public-api.txt" 1.0.0 2.0.0 \
    > "$work/major.stdout"
rg -q 'accepted breaking managed API change' "$work/major.stdout"

echo '== api_compat_acceptance passed (unchanged accepted; breaking change requires major SemVer) =='
