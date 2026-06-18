#!/usr/bin/env bash
# Compile feasibility/demo/add.rs with the CLR backend and run it on .NET.
# Expects BACKEND and LINKER env vars (set by harness.sh). Run inside the container.
set -euo pipefail

: "${BACKEND:?set BACKEND to librustc_codegen_clr.so}"
: "${LINKER:?set LINKER to the linker binary}"

SRC="feasibility/demo/add.rs"
OUT="/tmp/demo_add"

echo "==> Compiling $SRC with rustc_codegen_clr…"
rustc -O --crate-type=bin \
    -Z codegen-backend="$BACKEND" \
    -C linker="$LINKER" \
    -C link-args=--cargo-support \
    -Ctarget-feature=+x87+sse \
    "$SRC" -o "$OUT"

echo "==> Running the Rust-built .NET assembly on CoreCLR…"
if [ -f "$OUT.exe" ]; then dotnet "$OUT.exe"; else dotnet "$OUT"; fi

echo
echo "If you saw the fib output above, Rust logic just executed inside .NET 8."
