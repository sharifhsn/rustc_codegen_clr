#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
project="$repo/cargo_tests/cd_interop/csharp/cd_interop_cs.csproj"
crate="$repo/cargo_tests/cd_interop/rustlib"
dll="$crate/target/x86_64-unknown-dotnet/release/cd_interop.dll"
receipt="$dll.rustdotnet.receipt.json"
driver="$repo/target/release/cargo-dotnet"
log_dir="${RCL_MSBUILD_LOG_DIR:-/tmp/rustc_codegen_clr-msbuild-acceptance}"

if [[ ! -x "$driver" ]]; then
    echo "release cargo-dotnet driver missing: $driver" >&2
    exit 2
fi
mkdir -p "$log_dir"

# Every build below is intentionally --no-restore; materialize obj/project.assets.json here so
# the script does not depend on an earlier acceptance step having restored this csproj in-place
# (the NativeAOT gate used to as a side effect, before it moved to a staged git-archive copy).
CARGO_DOTNET_BACKEND=native CARGO_DOTNET_HOME="$repo" \
    dotnet restore "$project" "-p:CargoDotnet=$driver" --nologo > "$log_dir/restore.log" 2>&1

common=(
    "$project"
    "-p:CargoDotnet=$driver"
    --no-restore
    --nologo
)

run_build() {
    CARGO_DOTNET_BACKEND=native CARGO_DOTNET_HOME="$repo" \
        dotnet build "${common[@]}" "$@"
}

# A requested build must produce the managed assembly and a runnable C# consumer.
run_build -p:RustDotnetForceBuild=true > "$log_dir/forced-build.log" 2>&1
[[ -f "$dll" ]]
[[ -f "$receipt" ]]
dotnet run --project "$project" --no-build > "$log_dir/consumer.log" 2>&1
rg -q '^PASS$' "$log_dir/consumer.log"

# An unchanged second build must be a real MSBuild no-op for Rust.
run_build > "$log_dir/noop-build.log" 2>&1
if rg -q 'RustDotnet: building Rust crate' "$log_dir/noop-build.log"; then
    echo "unchanged MSBuild invocation rebuilt the Rust crate" >&2
    stamp="$dll.rustdotnet.stamp"
    inputs="$crate/target/x86_64-unknown-dotnet/release/.rustdotnet-cargo-inputs"
    echo "incremental stamp: $(stat -c '%y %n' "$stamp" 2>/dev/null || stat -f '%Sm %N' "$stamp")" >&2
    while IFS= read -r input; do
        if [[ ! -e "$input" ]]; then
            echo "missing incremental input: $input" >&2
        elif [[ "$input" -nt "$stamp" ]]; then
            echo "newer incremental input: $(stat -c '%y %n' "$input" 2>/dev/null || stat -f '%Sm %N' "$input")" >&2
        fi
    done < "$inputs"
    exit 1
fi

# Receipt is part of the incremental output contract. Deleting it must force a rebuild; MSBuild may
# never keep referencing a DLL whose provenance evidence disappeared.
rm "$receipt"
run_build > "$log_dir/missing-receipt-rebuild.log" 2>&1
rg -q 'RustDotnet: building Rust crate' "$log_dir/missing-receipt-rebuild.log"
[[ -f "$receipt" ]]

# Once stale, a failed requested rebuild must not leave the previous DLL eligible.
set +e
CARGO_DOTNET_HOME="$repo" dotnet build "$project" \
    -p:CargoDotnet=/usr/bin/false -p:RustDotnetForceBuild=true \
    --no-restore --nologo > "$log_dir/expected-failure.log" 2>&1
failed_exit=$?
set -e
if ((failed_exit == 0)); then
    echo "the deliberate cargo-dotnet failure unexpectedly succeeded" >&2
    exit 1
fi
if [[ -e "$dll" ]]; then
    echo "a stale Rust DLL survived a failed requested rebuild" >&2
    exit 1
fi
if [[ -e "$receipt" ]]; then
    echo "a stale Rust artifact receipt survived a failed requested rebuild" >&2
    exit 1
fi

# Restore the valid artifact for subsequent checks and local developer use.
run_build -p:RustDotnetForceBuild=true > "$log_dir/restored-build.log" 2>&1
[[ -f "$dll" ]]
[[ -f "$receipt" ]]

# The managed PE is not permission to run 64-bit Rust layout in a 32-bit process.
set +e
CARGO_DOTNET_HOME="$repo" dotnet build "$project" \
    -p:CargoDotnet=/usr/bin/false -p:PlatformTarget=x86 \
    --no-restore --nologo > "$log_dir/x86-rejection.log" 2>&1
x86_exit=$?
set -e
if ((x86_exit == 0)) || ! rg -q 'x86/32-bit consumers are unsupported' "$log_dir/x86-rejection.log"; then
    echo "the explicit x86 consumer was not rejected by the architecture guard" >&2
    exit 1
fi

echo '== msbuild_acceptance done =='
