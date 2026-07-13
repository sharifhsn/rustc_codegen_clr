#!/usr/bin/env bash
# Product-shaped first-user gate: scaffold every template outside the checkout and execute the
# Rust-on-.NET app plus both C# consumer journeys with one explicit runtime profile.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
driver="${RCL_ONBOARDING_DRIVER:-$repo/tools/cargo-dotnet/target/release/cargo-dotnet}"
dotnet_version="${DOTNET_VERSION:-10}"
work="$(mktemp -d "${TMPDIR:-/tmp}/rustdotnet-onboarding.XXXXXX")"
log_dir="${RCL_ONBOARDING_LOG_DIR:-$work/logs}"
install_home="$work/install-home"
cargo_home="$work/cargo-home"
trap 'rm -rf "$work"' EXIT

if [[ ! -x "$driver" ]]; then
    echo "cargo-dotnet driver missing: $driver" >&2
    echo "build it with: cargo build --manifest-path tools/cargo-dotnet/Cargo.toml --release" >&2
    exit 2
fi
if ! dotnet --list-runtimes | grep -q "^Microsoft.NETCore.App ${dotnet_version}\."; then
    echo ".NET $dotnet_version runtime is not installed (set DOTNET_VERSION to 8, 9, or 10)" >&2
    exit 2
fi
mkdir -p "$log_dir"

# A bare cargo install (or copied standalone binary) contains the command but not the backend SDK.
# Copying the built driver outside the checkout forces installed-mode detection and proves first
# use explains both complete recovery paths instead of emitting a generic missing-dir failure.
mkdir -p "$work/bare-bin"
cp "$driver" "$work/bare-bin/cargo-dotnet"
if CARGO_DOTNET_HOME="$work/missing-sdk" CARGO_DOTNET_BACKEND=native \
  "$work/bare-bin/cargo-dotnet" build "$repo/cargo_tests/cd_pure" \
  > "$log_dir/bare-install.log" 2>&1; then
  echo 'bare cargo-dotnet unexpectedly built without an installed SDK home' >&2
  exit 1
fi
grep -F 'A bare `cargo install` installs only the command.' "$log_dir/bare-install.log"
grep -F 'cargo dotnet setup --from-repo /path/to/rustc_codegen_clr' \
  "$log_dir/bare-install.log"
grep -F 'cargo dotnet bundle install /path/to/cargo-dotnet-sdk-<host>.zip' \
  "$log_dir/bare-install.log"

# Keep the copy-paste entry points on the one-build checkout bootstrap. Setup promotes the running
# release executable into CARGO_HOME/bin; a preceding `cargo install` would compile it twice.
for guide in "$repo/README.md" "$repo/QUICKSTART.md" "$repo/docs/QUICKSTART_INTEROP.md"; do
    grep -F 'cargo run --release --manifest-path tools/cargo-dotnet/Cargo.toml --' "$guide"
    if head -n 40 "$guide" | grep -F 'cargo install --path tools/cargo-dotnet'; then
        echo "newcomer guide redundantly installs cargo-dotnet before setup: $guide" >&2
        exit 1
    fi
done

# Exercise the documented checkout bootstrap into isolated install/Cargo homes. The native setup
# caller must reuse `driver` instead of compiling cargo-dotnet a second time in the legacy
# provisioner. Keep the log: the explicit delegated message is the regression assertion.
CARGO_HOME="$cargo_home" CARGO_DOTNET_HOME="$install_home" \
  "$driver" setup --from-repo "$repo" --home "$install_home" \
  --skip-toolchain --skip-dotnet --skip-ilasm --force > "$log_dir/setup.log" 2>&1
grep -F 'front-end install delegated to the native setup caller' "$log_dir/setup.log"
grep -F 'PAL warm delegated to the native private-sysroot setup caller' "$log_dir/setup.log"
grep -F "installed the already-built cargo-dotnet -> $cargo_home/bin/cargo-dotnet" \
  "$log_dir/setup.log"
if grep -F 'injecting dotnet PAL into rust-src' "$log_dir/setup.log"; then
  echo 'native setup unexpectedly mutated ambient rust-src through the legacy warm path' >&2
  exit 1
fi
installed_driver="$cargo_home/bin/cargo-dotnet"
[[ -x "$installed_driver" ]]

# Model a new shell instead of inheriting this script's command lookup. Only documented host
# executables plus the isolated Cargo bin are visible; `cargo dotnet` must discover the installed
# subcommand, self-locate the isolated SDK home, and report installed mode.
cargo_bin_dir="$(dirname "$(command -v cargo)")"
dotnet_bin_dir="$(dirname "$(command -v dotnet)")"
fresh_path="$cargo_home/bin:$cargo_bin_dir:$dotnet_bin_dir:/usr/bin:/bin:/usr/sbin:/sbin"
fresh_shell() {
  env -i \
    HOME="$HOME" \
    PATH="$fresh_path" \
    CARGO_HOME="$cargo_home" \
    CARGO_DOTNET_HOME="$install_home" \
    CARGO_DOTNET_BACKEND=native \
    DOTNET_VERSION="$dotnet_version" \
    TMPDIR="${TMPDIR:-/tmp}" \
    cargo dotnet "$@"
}
fresh_dotnet() {
  env -i \
    HOME="$HOME" \
    PATH="$fresh_path" \
    CARGO_HOME="$cargo_home" \
    CARGO_DOTNET_HOME="$install_home" \
    CARGO_DOTNET_BACKEND=native \
    DOTNET_VERSION="$dotnet_version" \
    TMPDIR="${TMPDIR:-/tmp}" \
    dotnet "$@"
}

fresh_shell doctor --workspace "$work" --dotnet "$dotnet_version" \
  > "$log_dir/fresh-shell-doctor.log"
grep -F "installed home $install_home" "$log_dir/fresh-shell-doctor.log"

fresh_shell new "$work/app" --app --dotnet "$dotnet_version" > "$log_dir/new-app.log"
fresh_shell new "$work/lib" --lib --dotnet "$dotnet_version" > "$log_dir/new-lib.log"
fresh_shell new "$work/plugin" --plugin --dotnet "$dotnet_version" > "$log_dir/new-plugin.log"

grep -F "cargo dotnet run --dotnet $dotnet_version" "$log_dir/new-app.log"
grep -F "targets net$dotnet_version.0" "$log_dir/new-lib.log"
grep -F "targets net$dotnet_version.0" "$log_dir/new-plugin.log"

for manifest in "$work/app/Cargo.toml" "$work/lib/rustlib/Cargo.toml" \
    "$work/plugin/rustlib/Cargo.toml"; do
    cargo metadata --no-deps --format-version 1 --manifest-path "$manifest" > /dev/null
done

fresh_shell doctor --workspace "$work/app" --dotnet "$dotnet_version" \
    > "$log_dir/doctor.log"
fresh_shell doctor --workspace "$work/app" --dotnet "$dotnet_version" --json \
    > "$log_dir/doctor.json"
grep -F '"schema": 1' "$log_dir/doctor.json"
grep -F '"mode": "environment"' "$log_dir/doctor.json"
grep -F '"ok": true' "$log_dir/doctor.json"
fresh_shell doctor 'EntryPointNotFoundException: onboarding_probe' --json \
    > "$log_dir/doctor-failure.json"
grep -F '"mode": "failure"' "$log_dir/doctor-failure.json"
grep -F '"matched": true' "$log_dir/doctor-failure.json"
fresh_shell run "$work/app" --dotnet "$dotnet_version" \
    > "$log_dir/app.log" 2>&1
grep -Fx 'hello from Rust on .NET' "$log_dir/app.log"
[[ "$(grep -xc '6' "$log_dir/app.log")" -eq 2 ]]

fresh_dotnet run -c Release --project "$work/lib/csharp/lib_cs.csproj" \
    -p:CargoDotnet="$installed_driver" > "$log_dir/lib.log" 2>&1
grep -Fx 'lib: 3/3 checks passed' "$log_dir/lib.log"

fresh_dotnet run -c Release --project "$work/plugin/csharp/plugin_cs.csproj" \
    -p:CargoDotnet="$installed_driver" > "$log_dir/plugin.log" 2>&1
grep -Fx 'plugin: 2/2 checks passed' "$log_dir/plugin.log"

echo "== onboarding_acceptance done: isolated setup + fresh-shell app/lib/plugin on net${dotnet_version}.0 =="
