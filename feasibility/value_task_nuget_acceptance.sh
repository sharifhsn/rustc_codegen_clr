#!/usr/bin/env bash
# Product-shaped proof that add-nuget preserves closed Task<int>, ValueTask<int>, and
# IAsyncEnumerable<int> returns, then awaits/streams genuinely incomplete operations without a
# handwritten shim.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_driver="$repo/target/release/cargo-dotnet"
driver="$source_driver"
dotnet_version="${DOTNET_VERSION:-10}"
work="${RCL_VALUE_TASK_WORK_DIR:-$(mktemp -d)}"
logs="${RCL_VALUE_TASK_LOG_DIR:-$work/logs}"
consumer="$work/consumer"
package="$work/package"
feed="$consumer/local-feed"
fresh_clone="$work/fresh-clone"
cargo_home="$work/cargo-home"
install_home="$work/sdk-home"

cleanup() {
    local status=$?
    if [[ "${BASH_SUBSHELL:-0}" -ne 0 ]]; then
        return "$status"
    fi
    if [[ "$status" -ne 0 ]]; then
        echo "value_task_nuget_acceptance failed; retained work: $work" >&2
        echo "logs: $logs" >&2
        return "$status"
    fi
    if [[ -z "${RCL_VALUE_TASK_WORK_DIR:-}" && "${RCL_VALUE_TASK_KEEP_WORK:-0}" != 1 ]]; then
        rm -rf "$work"
    fi
}
trap cleanup EXIT

mkdir -p "$package" "$feed" "$consumer/src" "$logs" "$work/cache"
export CARGO_DOTNET_CACHE_HOME="$work/cache"

# Unquoted delimiter: the csproj needs $dotnet_version expanded (everything else is literal XML).
cat > "$package/AsyncFixture.csproj" <<EOF
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net${dotnet_version}.0</TargetFramework>
    <PackageId>RustcCodegenClr.AsyncFixture</PackageId>
    <Version>1.0.0</Version>
    <Authors>rustc_codegen_clr acceptance</Authors>
    <Description>Local generated async API bindgen acceptance fixture.</Description>
  </PropertyGroup>
</Project>
EOF
cat > "$package/Calculator.cs" <<'EOF'
using System.Threading.Tasks;
using System.Collections.Generic;

namespace AsyncFixture;

public sealed class Calculator
{
    public async Task<int> GetTaskAnswerAsync()
    {
        await Task.Delay(20);
        return 84;
    }

    public async ValueTask<int> GetAnswerAsync()
    {
        await Task.Delay(25);
        return 42;
    }

    public async IAsyncEnumerable<int> StreamAsync()
    {
        foreach (int value in new[] { 7, 8, 9 })
        {
            await Task.Delay(10);
            yield return value;
        }
    }
}
EOF

dotnet pack "$package/AsyncFixture.csproj" -c Release -o "$feed" --nologo \
    > "$logs/package.log" 2>&1

cat > "$consumer/Cargo.toml" <<EOF
[package]
name = "generated_value_task_probe"
version = "0.0.0"
edition = "2024"

[dependencies]
mycorrhiza = { path = "$repo/mycorrhiza" }

[workspace]
EOF
cat > "$consumer/src/main.rs" <<'EOF'
mod nuget;

use mycorrhiza::prelude::{AsyncEnumerable, await_task, await_value_task, block_on};
use nuget::rustccodegenclr_asyncfixture::AsyncFixture::{Calculator, Calculator_Methods};

fn main() -> std::process::ExitCode {
    let calculator = Calculator::new();
    let task_answer = block_on(await_task(calculator.get_task_answer_async()));
    let answer = block_on(await_value_task(calculator.get_answer_async()));
    let values = AsyncEnumerable::from_handle(calculator.stream_async())
        .get_async_enumerator()
        .collect_blocking();
    if task_answer == 84 && answer == 42 && values == [7, 8, 9] {
        println!("generated Task answer: {task_answer}");
        println!("generated ValueTask answer: {answer}");
        println!("generated async stream: {values:?}");
        println!("== generated_async_nuget_probe done ==");
        std::process::ExitCode::SUCCESS
    } else {
        eprintln!(
            "generated async API mismatch: task_answer={task_answer}, answer={answer}, stream={values:?}"
        );
        std::process::ExitCode::FAILURE
    }
}
EOF
cat > "$consumer/NuGet.Config" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="acceptance" value="$feed" />
  </packageSources>
</configuration>
EOF

cargo build --manifest-path "$repo/tools/cargo-dotnet/Cargo.toml" --release \
    > "$logs/driver-build.log" 2>&1
[[ -x "$driver" ]]
cargo build --release --workspace > "$logs/backend-build.log" 2>&1

# `restore` is an installed-SDK operation: provision the exact source snapshot into this fixture's
# isolated SDK/Cargo homes, then use that installed driver for generation, recovery, and execution.
# This also proves the generated async surface does not depend on checkout-mode path discovery.
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
    "$source_driver" setup --from-repo "$repo" --home "$install_home" \
    --skip-toolchain --skip-dotnet --force > "$logs/setup.log" 2>&1
driver="$cargo_home/bin/cargo-dotnet"
[[ -x "$driver" ]]
export CARGO_HOME="$cargo_home"
export CARGO_DOTNET_HOME="$install_home"

(
    cd "$consumer"
    "$driver" add-nuget RustcCodegenClr.AsyncFixture 1.0.0 . --force --source "$feed" \
        --dotnet "$dotnet_version" > "$logs/generate.log" 2>&1
)

bindings="$consumer/src/nuget/rustccodegenclr_asyncfixture.rs"
rg -q 'fn get_task_answer_async\(self\) -> mycorrhiza::task::TaskT<i32>' "$bindings"
rg -q 'self\.instance0::<"GetTaskAnswerAsync", mycorrhiza::task::TaskT<i32>>' "$bindings"
rg -q 'fn get_answer_async\(self\) -> mycorrhiza::task::ValueTaskT<i32>' "$bindings"
rg -q 'self\.instance0::<"GetAnswerAsync", mycorrhiza::task::ValueTaskT<i32>>' "$bindings"
rg -q 'fn stream_async\(self\) -> mycorrhiza::enumerate_async::IAsyncEnumerable<i32>' "$bindings"
rg -q 'self\.instance0::<"StreamAsync", mycorrhiza::enumerate_async::IAsyncEnumerable<i32>>' "$bindings"

# Prove the checked-in dependency record is sufficient in a real fresh-clone shape. The local
# feed lives below the project and is recorded project-relative; remove the original checkout and
# all gitignored staged/cache output before asking `restore` to reproduce the exact RID/TFM/feed.
cp -R "$consumer" "$fresh_clone"
rm -rf "$fresh_clone/.cargo-dotnet-nuget-assets" "$fresh_clone/target"
mv "$consumer" "$work/original-unavailable"
consumer="$fresh_clone"
rg -q '"schema": 1' "$consumer/.cargo-dotnet-nuget-deps.json"
rg -q '"path": "local-feed"' "$consumer/.cargo-dotnet-nuget-deps.json"
"$driver" restore "$consumer" --dotnet "$dotnet_version" \
    > "$logs/fresh-clone-restore.log" 2>&1
[[ -f "$consumer/.cargo-dotnet-nuget-assets/manifest.json" ]]

# Immutable NuGet cache objects retain selected assets only. Neither the old nor rebound local
# source path (and therefore no private-feed URL/token) may survive in their bytes.
if rg -a -F "$work/original-unavailable/local-feed" "$work/cache/nuget/v4/objects" \
    > "$logs/source-leak.log" 2>&1 \
    || rg -a -F "$fresh_clone/local-feed" "$work/cache/nuget/v4/objects" \
        >> "$logs/source-leak.log" 2>&1; then
    echo "NuGet cache leaked a source path" >&2
    exit 1
fi

for profile in release debug; do
    CARGO_DOTNET_BACKEND=native \
        "$driver" run "$consumer" "--$profile" --dotnet "$dotnet_version" \
        > "$logs/$profile.log" 2>&1
    rg -q '^generated Task answer: 84$' "$logs/$profile.log"
    rg -q '^generated ValueTask answer: 42$' "$logs/$profile.log"
    rg -q '^generated async stream: \[7, 8, 9\]$' "$logs/$profile.log"
    rg -q '^== generated_async_nuget_probe done ==$' "$logs/$profile.log"
done

echo "== value_task_nuget_acceptance done: generated Task<int>, ValueTask<int>, and IAsyncEnumerable<int>, delayed debug+release =="
echo "logs: $logs"
