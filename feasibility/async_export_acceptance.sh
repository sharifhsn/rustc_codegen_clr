#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$repo/cargo_tests/cd_async_export"
dotnet_bin="${DOTNET:-}"
if [[ -z "$dotnet_bin" && -x "$HOME/.dotnet/dotnet" ]]; then
    dotnet_bin="$HOME/.dotnet/dotnet"
fi
if [[ -z "$dotnet_bin" ]]; then
    dotnet_bin="$(command -v dotnet)"
fi

cd "$repo"
CARGO_DOTNET_BACKEND=native cargo run -p cargo-dotnet -- dotnet build \
    "$fixture" --release --dotnet 10

cd "$fixture/csharp"
"$dotnet_bin" run -c Release
