#!/usr/bin/env bash
# Test harness for rustc_codegen_clr, meant to run INSIDE the dev container
# (repo mounted at /work). See feasibility/run.sh for the host-side driver.
#
# Usage: harness.sh <build|smoke|test|demo|all|shell>
#
# Linux build artifacts go to a container-only target dir so they never clobber
# the host's macOS/arm64 target/.
set -euo pipefail

# Build into the default target/ — the project's own test harness (compile_test.rs)
# hardcodes target/release paths. run.sh mounts a named volume over /work/target so
# this never collides with the host's target/.
REL="target/release"
BACKEND="$REL/librustc_codegen_clr.so"
LINKER="$REL/linker"
cd /work

build() {
  echo "==> Building cilly (IR library + linker binary)…"
  ( cd cilly && cargo build --release )
  echo "==> Building the rustc codegen backend (dylib)…"
  cargo build --release
  echo "==> Artifacts:"
  ls -la "$BACKEND" "$LINKER" 2>/dev/null \
    || { echo "!! expected artifacts missing"; return 1; }
}

# Compile a single standalone Rust program with the backend and run it on .NET.
# This is the end-to-end proof that the whole pipeline works.
smoke() {
  [ -f "$BACKEND" ] || { echo "!! build first (no backend at $BACKEND)"; return 1; }
  local src="${1:-test/hello.rs}"
  local out="/tmp/smoke_$(basename "${src%.rs}")"
  echo "==> Compiling $src with the CLR backend…"
  rustc -O --crate-type=bin \
      -Z codegen-backend="$BACKEND" \
      -C linker="$LINKER" \
      -C link-args=--cargo-support \
      -Ctarget-feature=+x87+sse \
      "$src" -o "$out"
  echo "==> Running the produced .NET assembly…"
  if [ -f "$out.exe" ]; then dotnet "$out.exe"; else dotnet "$out"; fi
}

# Run the current compiler/linker invariant gates. Product-shaped scripts outside this legacy
# container harness own build-std/runtime acceptance; the historical direct-host `::stable` corpus
# is not silently baseline-suppressed here.
test_suite() {
  echo "==> compiler/linker invariant gates…"
  cargo check --workspace --all-targets --locked
  cargo test -p cilly --locked
  cargo test -p rustc_codegen_clr --lib \
      managed_references_are_rejected_from_rust_byte_storage -- --nocapture
}

# Interop demo: a tiny Rust function compiled to a .NET assembly + a C# caller.
demo() {
  [ -f "$BACKEND" ] || { echo "!! build first"; return 1; }
  BACKEND="$BACKEND" LINKER="$LINKER" bash feasibility/demo/run_demo.sh
}

case "${1:-all}" in
  build) build ;;
  smoke) shift; smoke "${1:-test/hello.rs}" ;;
  test)  test_suite ;;
  demo)  demo ;;
  all)   build && smoke && test_suite ;;
  shell) exec bash ;;
  *) echo "usage: $0 <build|smoke|test|demo|all|shell>"; exit 2 ;;
esac
