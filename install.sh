#!/usr/bin/env sh
set -eu

version="${RUST_DOTNET_VERSION:-0.0.1}"
repository="${RUST_DOTNET_REPOSITORY:-sharifhsn/rustc_codegen_clr}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) host="linux-x64" ;;
  Darwin-arm64) host="macos-arm64" ;;
  *)
    echo "rust-dotnet $version has no SDK bundle for $(uname -s)-$(uname -m)." >&2
    echo "Supported release hosts: Linux x64, macOS Apple Silicon, Windows x64." >&2
    exit 2
    ;;
esac

command -v curl >/dev/null 2>&1 || {
  echo "curl is required to install rust-dotnet." >&2
  exit 2
}

base="${RUST_DOTNET_BASE_URL:-https://github.com/$repository/releases/download/rust-dotnet-v$version}"
work="$(mktemp -d "${TMPDIR:-/tmp}/rust-dotnet-install.XXXXXX")"
trap 'rm -rf "$work"' EXIT HUP INT TERM

driver="$work/cargo-dotnet-$host"
bundle="$work/cargo-dotnet-sdk-$host-$version.zip"

echo "Downloading rust-dotnet $version for $host..."
curl -fsSL "$base/cargo-dotnet-$host" -o "$driver"
curl -fsSL "$base/cargo-dotnet-sdk-$host-$version.zip" -o "$bundle"
curl -fsSL "$base/cargo-dotnet-sdk-$host-$version.zip.sha256" -o "$bundle.sha256"
chmod +x "$driver"

"$driver" bundle install "$bundle"

echo
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
echo "rust-dotnet $version is installed. Ensure $cargo_bin is on PATH, then run:"
echo "  cargo dotnet doctor"
echo "  cargo dotnet new hello-dotnet --app"
echo "  cargo dotnet run hello-dotnet"
