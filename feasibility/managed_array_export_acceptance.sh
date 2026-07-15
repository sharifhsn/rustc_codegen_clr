#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$repo/cargo_tests/cd_export"
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

CARGO_DOTNET_BACKEND=native \
    "$cargo_dotnet" build "$fixture/rustlib" --release --dotnet 10

CARGO_DOTNET_BACKEND=native \
    "$dotnet_bin" run -c Release \
    --project "$fixture/csharp/cd_export_cs.csproj" \
    -p:CargoDotnet="$cargo_dotnet" \
    -p:RustDotnetVersion=10
