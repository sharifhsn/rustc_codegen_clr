#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
example="$repo/examples/issue-dashboard"
dotnet_version="${DOTNET_VERSION:-10}"
driver="$repo/tools/cargo-dotnet/target/release/cargo-dotnet"
work="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-flagship.XXXXXX")"
trap 'rm -rf "$work"' EXIT

fail() {
    echo "flagship example acceptance: $*" >&2
    exit 1
}

cargo build --manifest-path "$repo/tools/cargo-dotnet/Cargo.toml" --release
[[ -x "$driver" ]] || fail "cargo-dotnet release driver was not built"

raw="$work/raw.txt"
actual="$work/actual.txt"
"$driver" dotnet run "$example" --backend native --dotnet "$dotnet_version" --clean \
    -- "$example/sample/issues.json" >"$raw" 2>"$work/valid.log"
# The C# helper prints its own build summary to stdout before the generated program starts.
tail -n 5 "$raw" >"$actual"
cmp "$example/expected-output.txt" "$actual" || fail "sample output changed"

"$driver" dotnet run "$example" --backend native --dotnet "$dotnet_version" \
    >"$work/default-raw.txt" 2>"$work/default.log"
tail -n 5 "$work/default-raw.txt" >"$work/default-actual.txt"
cmp "$example/expected-output.txt" "$work/default-actual.txt" ||
    fail "bundled-default output changed"

printf '{ invalid json\n' >"$work/invalid.json"
if "$driver" dotnet run "$example" --backend native --dotnet "$dotnet_version" \
    -- "$work/invalid.json" >"$work/invalid.out" 2>"$work/invalid.log"; then
    fail "malformed input unexpectedly succeeded"
fi
grep -Fq "error: invalid JSON in $work/invalid.json" "$work/invalid.out" "$work/invalid.log" ||
    fail "malformed-input diagnostic was not actionable"

echo '== flagship_example_acceptance done =='
