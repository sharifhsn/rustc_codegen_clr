#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/rust-dotnet-excel-dna.XXXXXX")"
trap 'rm -rf "$work"' EXIT

dotnet_bin="${DOTNET:-}"
if [[ -z "$dotnet_bin" && -x "$HOME/.dotnet/dotnet" ]]; then
    dotnet_bin="$HOME/.dotnet/dotnet"
fi
if [[ -z "$dotnet_bin" ]]; then
    dotnet_bin="$(command -v dotnet)"
fi

cd "$repo"
cargo build -p cargo-dotnet
cargo_dotnet="$repo/target/debug/cargo-dotnet"

sdk_home="${CARGO_DOTNET_HOME:-$HOME/.cargo-dotnet}"
if [[ ! -f "$sdk_home/msbuild/RustDotnet.targets" ]]; then
    sdk_home="$work/sdk"
    "$cargo_dotnet" setup \
        --from-repo "$repo" \
        --home "$sdk_home" \
        --skip-toolchain \
        --skip-dotnet \
        --skip-ilasm \
        --force
fi

sample="$work/excel-risk-engine"
"$cargo_dotnet" new "$sample" --excel
project="$sample/excel/excel-risk-engine_excel.csproj"
functions="$sample/excel/Functions.cs"
rust_source="$sample/rustlib/src/lib.rs"

rg -q 'Task<object> PortfolioStressAsync' "$functions"
rg -q 'CancellationToken cancellationToken' "$functions"
rg -q 'Task.Run\(' "$functions"
rg -q 'catch \(OperationCanceledException\)' "$functions"
rg -q 'IsThreadSafe = true' "$functions"
rg -q 'PortfolioStressScore' "$rust_source"
rg -q 'throw_if_cancellation_requested' "$rust_source"
if rg -q 'ExcelDnaUtil\.(Application|DynamicApplication)' "$functions"; then
    echo "Excel async UDF illegally captures the Excel COM application surface" >&2
    exit 1
fi

CARGO_DOTNET_BACKEND=native \
CARGO_DOTNET_HOME="$sdk_home" \
PATH="$(dirname "$dotnet_bin"):$HOME/.cargo/bin:$PATH" \
    "$dotnet_bin" build "$project" -c Release -p:CargoDotnet="$cargo_dotnet"

packed="$sample/excel/bin/Release/net10.0-windows/publish/excel-risk-engine_excel-AddIn64-packed.xll"
test -s "$packed"
test -s "$sample/rustlib/target/x86_64-unknown-dotnet/release/excel_risk_engine.dll"

echo "Excel-DNA managed Rust acceptance OK: $packed"
