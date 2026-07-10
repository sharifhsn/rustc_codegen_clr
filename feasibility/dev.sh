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
#   * Re-running the ::stable gate and eyeballing 22 failures is error-prone. -> `gate` diffs the
#     known baseline and reports only NEW failures.
#
# Runs INSIDE the existing `rcc-dev` image (built once; this script never rebuilds it).
#
# Usage:
#   dev.sh sh '<bash>'           Run bash in the container (repo at /work, color off, no cwd-drift).
#   dev.sh backend               Force clean-rebuild of cilly + linker + backend (defeat mtime skew).
#   dev.sh run <crate> [--clean] Build (forced relink) + run cargo_tests/<crate>; prints stdout+exit.
#                                --clean does a full `cargo clean` first (rebuilds std; bulletproof).
#   dev.sh buildstd [--clean]    Shorthand for `run build_std`.
#   dev.sh il <crate> <symbol>   Disassemble method(s) whose (mangled) name contains <symbol> from
#                                the crate's built .dll (ikdasm). e.g. `il build_std rust_alloc`.
#   dev.sh gate                  Force-rebuild, run ::stable (CI skips), diff vs baseline (425/13),
#                                report PASS/FAIL + any NEW failures (outside the known-22 set).
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
    -e DEV_CRATE -e DEV_CLEAN -e DEV_SYM -e DEV_RUN -e OPTIMIZE_CIL -e DIRECT_PE \
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
  echo "==> cargo clean (full, bulletproof)"; cargo clean
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
set +e
echo "==> cargo test ::stable (CI skip set)"
out="$(cargo test --release ::stable -- --skip f128 --skip num_test --skip simd --skip fuzz87 2>&1)"
echo "$out" | grep -E 'test result:' || echo "(no result line — build error?)"
# Known-22 baseline groups (see rcc-dev-harness-gotchas memory). Report only NEW failures, and
# re-run each once — `failN`/env tests are flaky, so only a failure that reproduces is a regression.
known=' any atomics catch f16 fastrand_test futex_test hello_world once_lock_test std_hello_world type_id uninit_fill '
new_tests=()
while read -r t; do
  tn="$(echo "$t" | sed -E 's/^ *//')"; [ -z "$tn" ] && continue
  g="$(echo "$tn" | sed -E 's/compile_test::([a-z0-9_]+)::.*/\1/')"
  case "$known" in *" $g "*) : ;; *) new_tests+=("$tn") ;; esac
done < <(echo "$out" | awk '/^failures:/{f=1} f' | grep -E '^    compile_test' | sort -u)
if [ ${#new_tests[@]} -eq 0 ]; then echo "OK: only known-22 baseline failures (no regressions)"; exit 0; fi
echo "==> ${#new_tests[@]} failure(s) outside baseline; re-running each to filter flakiness"
real=""; flaky=""
for tn in "${new_tests[@]}"; do
  if cargo test --release "$tn" -- --exact 2>&1 | grep -q 'test result: ok'; then
    flaky="$flaky  $tn"$'\n'; else real="$real  $tn"$'\n'; fi
done
[ -n "$flaky" ] && { echo "~~ flaky (passed on retry, ignore):"; printf '%s' "$flaky"; }
if [ -n "$real" ]; then echo "!! REAL REGRESSIONS (failed twice):"; printf '%s' "$real"; exit 1; else
  echo "OK: no real regressions (out-of-baseline failures were all flaky)"; fi
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
