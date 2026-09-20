#!/usr/bin/env bash
# feasibility/dev.sh — deterministic dev tooling for rustc_codegen_clr on the rcc-dev container.
#
# Works around recurring footguns (the things that waste hours):
#   * Docker host-mount mtime skew silently defeats cargo's incremental cache, so edits to `cilly`
#     never reach the linker and you debug a STALE binary. -> commands force rebuilds by removing
#     artifacts, and `run` verifies the produced binary is fresh (fails loudly otherwise).
#   * `cd` in a host shell changes $PWD and breaks `docker run -v "$PWD":/work`. -> the repo root is
#     resolved from THIS script's location, never from the caller's cwd.
#   * Disassembling one (mangled) method from a build-std .dll is fiddly. -> `il`.
#   * A stale direct-host test baseline can hide verifier failures. -> `gate` runs the current
#     workspace, CIL, and managed-storage invariants without suppressing historical failures.
#
# Runs INSIDE the existing `rcc-dev` image (built once; this script never rebuilds it).
#
# Usage:
#   dev.sh sh '<bash>'           Run bash in the container (repo at /work, color off, no cwd-drift).
#   dev.sh backend               Force clean-rebuild of cilly + linker + backend (defeat mtime skew).
#   dev.sh run <crate> [--clean] Build (forced relink) + run cargo_tests/<crate>; prints stdout+exit.
#                                --clean does a full `cargo clean` first (rebuilds std).
#   dev.sh buildstd [--clean]    Shorthand for `run build_std`.
#   dev.sh il <crate> <symbol>   Disassemble method(s) whose (mangled) name contains <symbol> from
#                                the crate's built .dll (ikdasm). e.g. `il build_std rust_alloc`.
#   dev.sh gate                  Force-rebuild, then run workspace, CIL, and managed-storage gates.
#   dev.sh pal-build             Inject the in-repo dotnet PAL (dotnet_pal/sys/**) into the
#                                container's rust-src (mirror files + insert the target_os="dotnet"
#                                cascade arms), then build-std cargo_tests/pal_hello for os=dotnet.
#                                Used to iterate the std::sys::pal::dotnet work (WF-2).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${RCC_IMAGE:-rcc-dev}"

die(){ echo "dev.sh: $*" >&2; exit 1; }
usage(){ sed -n '2,33p' "${BASH_SOURCE[0]}"; }

# Run stdin as bash inside the container. Repo mounted at /work (resolved from script location, so
# the caller's cwd is irrelevant), persistent build cache on the rcc-target volume, color off.
# Forwards the DEV_* parameter vars. NOTE: no `set -e` — `cargo test` and running a program both
# return non-zero on legitimate outcomes (test failures, panicking programs); the command bodies use
# explicit guards instead.
_in(){
  docker run --rm -i -e CARGO_TERM_COLOR=never \
    -e DEV_CRATE -e DEV_CLEAN -e DEV_SYM -e DEV_RUN -e OPTIMIZE_CIL \
    -v "$REPO_ROOT":/work -v rcc-target:/work/target -w /work \
    "$IMAGE" bash -o pipefail -s
}

cmd="${1:-help}"; shift || true
case "$cmd" in

sh)
  [ $# -ge 1 ] || die "usage: dev.sh sh '<bash>'"
  printf '%s' "$*" | _in
  ;;

backend)
  _in <<'C'
set -e
cd /work
echo "==> clean-rebuild backend (defeat host-mount mtime skew across cilly + the root backend)"
# Future-date every backend source so cargo never skips a recompile on a host edit that looks "old".
find cilly/src src -name '*.rs' \
     -exec touch -d 2099-01-01 {} + 2>/dev/null || true
# Drop the cilly + LINKER artifacts AND the ROOT dylib + its fingerprint/deps copy. The root
# nuke is critical: otherwise cargo "freshly" re-hardlinks a STALE deps dylib and you test old code.
rm -f  target/release/linker target/release/deps/linker-* target/release/deps/libcilly-* \
       target/release/librustc_codegen_clr.so \
       target/release/deps/librustc_codegen_clr-* 2>/dev/null || true
rm -rf target/release/.fingerprint/cilly-* \
       target/release/.fingerprint/rustc_codegen_clr-* target/release/.fingerprint/linker-* 2>/dev/null || true
( cd cilly && cargo build --release )
echo "==> backend dylib"
cargo build --release -p rustc_codegen_clr
ls -la target/release/librustc_codegen_clr.so target/release/linker
C
  ;;

run|buildstd)
  if [ "$cmd" = buildstd ]; then crate="build_std"; else crate="${1:-}"; shift || true; fi
  [ -n "${crate:-}" ] || die "usage: dev.sh run <crate> [--clean]"
  clean=0; [ "${1:-}" = --clean ] && clean=1
  export DEV_CRATE="$crate" DEV_CLEAN="$clean"
  _in <<'C'
cd "/work/cargo_tests/$DEV_CRATE" 2>/dev/null || { echo "!! no cargo_tests/$DEV_CRATE"; exit 1; }
export RUSTFLAGS='-Z codegen-backend=/work/target/release/librustc_codegen_clr.so -C linker=/work/target/release/linker -C link-args=--cargo-support'
TT=x86_64-unknown-linux-gnu
out="target/$TT/release/$DEV_CRATE"
start=$(date +%s)
if [ "$DEV_CLEAN" = 1 ]; then
  echo "==> cargo clean"; cargo clean
else
  # Force a relink despite mtime skew: future-date the sources (always newer than any cached
  # artifact) and drop the stale outputs, so cargo recompiles main + re-invokes the linker.
  echo "==> forcing relink (future-mtime sources + rm outputs)"
  find src -name '*.rs' -exec touch -d 2099-01-01 {} + 2>/dev/null || true
  rm -f "$out" "$out.dll" 2>/dev/null || true
fi
cargo build --release 2>&1 | grep -viE 'discirminant|warning: unused|note:' | tail -8
# Determinism guard: refuse to run a stale/absent binary.
[ -f "$out" ] || { echo "!! BUILD PRODUCED NO BINARY at $out"; exit 1; }
if [ "$(stat -c %Y "$out")" -lt "$start" ]; then echo "!! WARNING: $out was not rebuilt (mtime older than build start) — result may be stale"; fi
echo "==> run ./$out"
"./$out"; echo "exit: $?"
C
  ;;

il)
  crate="${1:-}"; sym="${2:-}"
  [ -n "$crate" ] && [ -n "$sym" ] || die "usage: dev.sh il <crate> <symbol-substr>   (e.g. il build_std rust_alloc)"
  export DEV_CRATE="$crate" DEV_SYM="$sym"
  _in <<'C'
TT=x86_64-unknown-linux-gnu
dll="/work/cargo_tests/$DEV_CRATE/target/$TT/release/$DEV_CRATE.dll"
[ -f "$dll" ] || { echo "!! no $dll — build it first: dev.sh run $DEV_CRATE"; exit 1; }
# Print every .method whose body (header through 'end of method') mentions the symbol substring.
ikdasm "$dll" 2>/dev/null | awk -v pat="$DEV_SYM" '
  /^[[:space:]]*\.method/ { inm=1; buf=""; hit=0 }
  inm { buf = buf $0 "\n"; if (index($0, pat)) hit=1 }
  /end of method/ { if (inm && hit) printf "%s\n", buf; inm=0 }
'
C
  ;;

gate)
  _in <<'C'
set -e
cd /work
echo "==> force-rebuild linker + backend so the gate tests current code"
find cilly/src src -name '*.rs' \
     -exec touch -d 2099-01-01 {} + 2>/dev/null || true
rm -f  target/release/linker target/release/deps/linker-* target/release/deps/libcilly-* \
       target/release/librustc_codegen_clr.so \
       target/release/deps/librustc_codegen_clr-* 2>/dev/null || true
rm -rf target/release/.fingerprint/cilly-* \
       target/release/.fingerprint/rustc_codegen_clr-* target/release/.fingerprint/linker-* 2>/dev/null || true
( cd cilly && cargo build --release ) >/dev/null
cargo build --release -p rustc_codegen_clr >/dev/null
echo "==> workspace/compiler/linker invariant gates"
cargo check --workspace --all-targets --locked
cargo test -p cilly --locked
cargo test -p rustc_codegen_clr --lib \
  managed_references_are_rejected_from_rust_byte_storage -- --nocapture
C
  ;;

pal-build)
  # PHASE D: pal-build now DELEGATES to the user-facing `cargo dotnet` command, so
  # the probe regression path exercises the IDENTICAL pipeline CORE the one-command
  # DX runs (feasibility/_cargo_dotnet_core.sh) — no second implementation to drift.
  # Arg parsing is unchanged (regression-safe): crate name default pal_hello + the
  # --run flag => `cargo dotnet run` (else `build`), on cargo_tests/<crate>.
  crate=pal_hello; run_native=0
  for a in "$@"; do case "$a" in --run) run_native=1;; *) crate="$a";; esac; done
  [ "$run_native" = 1 ] && sub=run || sub=build
  exec "$REPO_ROOT/feasibility/cargo-dotnet" "$sub" "cargo_tests/$crate"
  ;;
help|-h|--help) usage ;;
*) die "unknown command '$cmd' (try: dev.sh help)" ;;
esac
