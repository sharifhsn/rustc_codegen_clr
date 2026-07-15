#!/usr/bin/env bash
# Release gate for Cargo-derived MSBuild invalidation.
#
# The fixture leaves the manual escape hatch absent: Cargo metadata must contribute the nested
# workspace/path-dependency/build.rs-owned input closure automatically.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture="$repo/feasibility/fixtures/msbuild_transitive_inputs"
project="$fixture/csharp/FixtureConsumer.csproj"
crate="$fixture/rustlib"
dep_source="$crate/deps/fixture_dep/src/lib.rs"
build_input="$crate/build-input.txt"
driver="$repo/target/release/cargo-dotnet"
log_dir="${RCL_TRANSITIVE_INPUT_LOG_DIR:-/tmp/rustc_codegen_clr-transitive-inputs}"

if [[ ! -x "$driver" ]]; then
    echo "release cargo-dotnet driver missing: $driver" >&2
    exit 2
fi

mkdir -p "$log_dir"

# The fixture is intentionally standalone, so its SDK restore assets may not exist in a fresh clone.
dotnet restore "$project" --nologo > "$log_dir/restore.log" 2>&1

restore_fixture() {
    # Keep the checked-in fixture deterministic even if a regression assertion or cargo fails.
    perl -0pi -e 's/    50\n/    40\n/' "$dep_source"
    printf '1\n' > "$build_input"
}
trap restore_fixture EXIT
restore_fixture

run_build() {
    CARGO_DOTNET_BACKEND=native CARGO_DOTNET_HOME="$repo" \
        dotnet build "$project" -p:CargoDotnet="$driver" --no-restore --nologo "$@"
}

run_consumer() {
    dotnet run --project "$project" --no-build --no-restore
}

expect_value() {
    local expected="$1"
    local actual
    actual="$(run_consumer)"
    if [[ "$actual" != "$expected" ]]; then
        echo "expected C# to observe fixture_value=$expected, got $actual" >&2
        exit 1
    fi
}

expect_rust_rebuild() {
    local log="$1"
    if ! rg -q 'RustDotnet: building Rust crate' "$log"; then
        printf '%s\n' \
            'KNOWN RELEASE BLOCKER: a Cargo transitive input changed but MSBuild skipped cargo dotnet.' \
            "Expected a Rust rebuild after changing $2." \
            'The automatic input set currently omits nested workspace/path dependencies and build.rs inputs.' \
            'Implement Cargo-metadata-derived input fingerprints, then keep this acceptance test green.' >&2
        exit 1
    fi
}

# Establish a managed DLL from a forced build; the original inputs calculate 40 + 1.
run_build -p:RustDotnetForceBuild=true > "$log_dir/forced-build.log" 2>&1
expect_value 41

# The unchanged run is the non-negotiable counterpart: full dependency tracking must still no-op.
run_build > "$log_dir/noop-build.log" 2>&1
if rg -q 'RustDotnet: building Rust crate' "$log_dir/noop-build.log"; then
    echo 'unchanged nested-workspace fixture rebuilt Rust instead of no-oping' >&2
    exit 1
fi

# Failing-first assertion: this source belongs to a nested path dependency, not `rustlib/src/**`.
perl -0pi -e 's/    40\n/    50\n/' "$dep_source"
run_build > "$log_dir/path-dependency-change.log" 2>&1
expect_rust_rebuild "$log_dir/path-dependency-change.log" "$dep_source"
expect_value 51

# This is reached after the path-dependency fingerprint exists. Cargo itself correctly marks the
# build script dirty, but MSBuild must invoke Cargo first for that knowledge to matter.
printf '2\n' > "$build_input"
run_build > "$log_dir/build-script-input-change.log" 2>&1
expect_rust_rebuild "$log_dir/build-script-input-change.log" "$build_input"
expect_value 52

# Updating the input manifest must stabilize. A fingerprint implementation that rewrites its
# manifest or stamp on every evaluation would rebuild forever and is not an acceptable fix.
run_build > "$log_dir/final-noop-build.log" 2>&1
if rg -q 'RustDotnet: building Rust crate' "$log_dir/final-noop-build.log"; then
    echo 'transitive input fingerprint did not stabilize after the rebuild' >&2
    exit 1
fi
expect_value 52

echo '== transitive_input_invalidation_acceptance done =='
