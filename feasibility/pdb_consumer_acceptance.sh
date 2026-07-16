#!/usr/bin/env bash
# C#-hosted Portable-PDB gate: in debug and release, enter Rust through a managed export, resolve
# the Rust frame to file:line, and parse the adjacent PDB's Document table with .NET metadata APIs.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="${RCL_PDB_DRIVER:-$repo/target/release/cargo-dotnet}"
dotnet_version="${DOTNET_VERSION:-10}"
fixture="$repo/cargo_tests/cd_export_ergonomics"
log_dir="${RCL_PDB_LOG_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-pdb.XXXXXX")}"
mkdir -p "$log_dir"

[[ -x "$driver" ]] || {
    echo "cargo-dotnet release driver missing: $driver" >&2
    exit 2
}

for profile in debug release; do
    profile_flag="--release"
    [[ "$profile" == debug ]] && profile_flag="--debug"
    DIRECT_PE=1 CARGO_DOTNET_BACKEND=native "$driver" build "$fixture" \
        "$profile_flag" --dotnet "$dotnet_version" \
        --source-link-url 'https://example.invalid/rust-dotnet-fixture/*' \
        > "$log_dir/rust-$profile.log" 2>&1
    pdb="$fixture/target/x86_64-unknown-dotnet/$profile/cd_export_ergonomics.pdb"
    receipt="$fixture/target/x86_64-unknown-dotnet/$profile/cd_export_ergonomics.dll.rustdotnet.receipt.json"
    [[ -s "$pdb" ]] || {
        echo "missing Rust Portable PDB for $profile: $pdb" >&2
        exit 1
    }
    grep -F '"source_link_url": "https://example.invalid/rust-dotnet-fixture/*"' "$receipt"
    RustDotnetVersion="$dotnet_version" RustProfile="$profile" RustCheckoutPath="$repo" \
        dotnet run -c Release --project "$fixture/csharp/cd_export_ergonomics_cs.csproj" \
        > "$log_dir/csharp-$profile.log" 2>&1
    grep -F 'Rust PDB trace names lib.rs = True (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'Rust PDB trace has file:line = True (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'Rust PDB trace names leaf = True (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'invoke_action1 callback = 7 (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'invoke_action2 callback = 30 (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'invoke_func1 = 21 (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'invoke_func2 = 42 (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'invoke_comparison = -1 (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'g.Apply(Func<int,int>) = 42 (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'Rust sidecar PDB exists = True (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'Rust Portable PDB has documents = True (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'Rust PDB uses logical consumer path = True (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'Rust PDB hides checkout path = True (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'Rust Portable PDB has local scopes = True (ok)' "$log_dir/csharp-$profile.log"
    if [[ "$profile" == debug ]]; then
        grep -F 'Rust Portable PDB names debugger local = True (ok)' "$log_dir/csharp-$profile.log"
    else
        grep -F 'Rust release PDB retains named locals = True (ok)' "$log_dir/csharp-$profile.log"
    fi
    grep -F 'Rust Portable PDB has Source Link = True (ok)' "$log_dir/csharp-$profile.log"
    grep -F 'Rust Source Link maps logical consumer documents = https://example.invalid/rust-dotnet-fixture/* (ok)' "$log_dir/csharp-$profile.log"
    grep -Fx 'PASS' "$log_dir/csharp-$profile.log"
done

echo "== pdb_consumer_acceptance done: C# -> Rust delegates + file:line in debug + release =="
