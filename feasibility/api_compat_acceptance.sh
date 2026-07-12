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
