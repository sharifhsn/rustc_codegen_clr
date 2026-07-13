#!/usr/bin/env bash
# Product-shaped gate for class and interface events on the default direct-PE path.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="${RCL_EVENT_DRIVER:-$repo/tools/cargo-dotnet/target/release/cargo-dotnet}"
dotnet_version="${DOTNET_VERSION:-10}"
log_dir="${RCL_EVENT_LOG_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-events.XXXXXX")}"
sentinel="$log_dir/ilasm-must-not-run"
marker="$log_dir/ilasm-was-invoked"
mkdir -p "$log_dir"

[[ -x "$driver" ]] || { echo "cargo-dotnet release driver missing: $driver" >&2; exit 2; }

# Context resolution still accepts an ILAsm escape hatch, but DIRECT_PE=1 must never execute it.
{
    printf '%s\n' '#!/usr/bin/env bash'
    printf 'marker=%q\n' "$marker"
    printf '%s\n' \
        'printf "args:" >> "$marker"' \
        'printf " <%s>" "$@" >> "$marker"' \
        'printf "\n" >> "$marker"' \
        'ps -o pid=,ppid=,command= -p "$$" -p "$PPID" >> "$marker" 2>/dev/null || true' \
        'exit 91'
} > "$sentinel"
chmod +x "$sentinel"

run_fixture() {
    local name="$1"
    local expected="$2"
    local root="$repo/cargo_tests/$name"
    local project="$root/csharp/${name}_cs.csproj"
    local log="$log_dir/$name.log"

    rm -rf "$root/rustlib/target" "$root/csharp/bin" "$root/csharp/obj"
    DIRECT_PE=1 ILASM_PATH="$sentinel" CARGO_DOTNET_BACKEND=native \
        CARGO_DOTNET_HOME="$repo" RustDotnetVersion="$dotnet_version" \
        dotnet run -c Release --project "$project" \
        -p:CargoDotnet="$driver" -p:RustDotnetForceBuild=true \
        > "$log" 2>&1
    grep -F "$expected" "$log"
    if grep -F '[FAIL]' "$log"; then
        echo "$name reported a failed event check" >&2
        exit 1
    fi
}

run_fixture cd_event 'cd_event: 4/4 checks passed'
run_fixture cd_iface_event 'cd_iface_event: 6/6 checks passed'

consumer="$repo/cargo_tests/cd_event_subscription"
rm -rf "$consumer/target"
DIRECT_PE=1 ILASM_PATH="$sentinel" CARGO_DOTNET_BACKEND=native \
    "$driver" run "$consumer" --dotnet "$dotnet_version" --clean \
    > "$log_dir/cd_event_subscription.log" 2>&1
grep -F 'cd_event_subscription: 4/4 checks passed' "$log_dir/cd_event_subscription.log"

if [[ -e "$marker" ]]; then
    echo "direct-PE event acceptance unexpectedly invoked ILAsm" >&2
    exit 1
fi

echo '== event_acceptance passed: class/interface metadata + Rust-side subscription, zero ILAsm =='
