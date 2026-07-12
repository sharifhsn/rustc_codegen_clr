#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
log_dir="${RCL_WAVE0_BLOCKER_LOG_DIR:-/tmp/rustc_codegen_clr-wave0-blockers}"
mkdir -p "$log_dir"

# Focused-closed blockers run as ordinary green gates. Release closure still requires the broader
# shadow/deployment/governance matrix documented in docs/RELEASE_BLOCKERS.md.
bash "$repo/feasibility/managed_identity_acceptance.sh" \
    > "$log_dir/managed-identity.log" 2>&1

bash "$repo/feasibility/transitive_input_invalidation_acceptance.sh" \
    > "$log_dir/transitive-inputs.log" 2>&1

(cd "$repo/tools/cargo-dotnet" && cargo test) > "$log_dir/cargo-dotnet-tests.log" 2>&1

(cd "$repo/tools/cargo-dotnet" && \
    cargo test rid_asset_graph_snapshots_are_preserved) \
    > "$log_dir/rid-assets.log" 2>&1

(cd "$repo/dotnet_macros" && cargo test --test negative_ui) \
    > "$log_dir/typed-dto.log" 2>&1

CARGO_DOTNET_BACKEND=native \
    "$repo/tools/cargo-dotnet/target/release/cargo-dotnet" \
    build "$repo/cargo_tests/cd_typed_dto/rustlib" \
    >> "$log_dir/typed-dto.log" 2>&1
dotnet run --project "$repo/cargo_tests/cd_typed_dto/csharp/cd_typed_dto_cs.csproj" \
    -c Release >> "$log_dir/typed-dto.log" 2>&1

echo '== wave0_blocker_acceptance focused-closed gates passed =='
