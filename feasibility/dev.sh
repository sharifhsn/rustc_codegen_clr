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
#   dev.sh gate                  Force-rebuild, run ::stable (CI skips), diff vs baseline (416/22),
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
    -e DEV_CRATE -e DEV_CLEAN -e DEV_SYM -e DEV_RUN \
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
echo "==> clean-rebuild backend (defeat host-mount mtime skew across ALL backend crates, not just cilly)"
# Future-date every backend source so cargo never skips a recompile on a host edit that looks "old".
find cilly/src rustc_codgen_clr_operand/src rustc_codegen_clr_type/src rustc_codegen_clr_call/src \
     rustc_codegen_clr_place/src rustc_codegen_clr_ctx/src src -name '*.rs' \
     -exec touch -d 2099-01-01 {} + 2>/dev/null || true
# Drop the cilly + operand + LINKER artifacts AND the ROOT dylib + its fingerprint/deps copy. The root
# nuke is critical: otherwise cargo "freshly" re-hardlinks a STALE deps dylib and you test old code.
rm -f  target/release/linker target/release/deps/linker-* target/release/deps/libcilly-* \
       target/release/deps/*operand* target/release/librustc_codegen_clr.so \
       target/release/deps/librustc_codegen_clr-* 2>/dev/null || true
rm -rf target/release/.fingerprint/cilly-* target/release/.fingerprint/rustc_codgen_clr_operand-* \
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
echo "==> force-rebuild linker + backend so the gate tests current code (ALL backend crates)"
find cilly/src rustc_codgen_clr_operand/src rustc_codegen_clr_type/src rustc_codegen_clr_call/src \
     rustc_codegen_clr_place/src rustc_codegen_clr_ctx/src src -name '*.rs' \
     -exec touch -d 2099-01-01 {} + 2>/dev/null || true
rm -f  target/release/linker target/release/deps/linker-* target/release/deps/libcilly-* \
       target/release/deps/*operand* target/release/librustc_codegen_clr.so \
       target/release/deps/librustc_codegen_clr-* 2>/dev/null || true
rm -rf target/release/.fingerprint/cilly-* target/release/.fingerprint/rustc_codgen_clr_operand-* \
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
  crate=pal_hello; run_native=0
  for a in "$@"; do case "$a" in --run) run_native=1;; *) crate="$a";; esac; done
  export DEV_RUN="$run_native" DEV_CRATE="$crate"
  _in <<'C'
set -e
SRC="$(rustc --print sysroot)/lib/rustlib/src/rust/library/std/src/sys"
PAL=/work/dotnet_pal/sys
[ -d "$PAL" ] || { echo "!! no /work/dotnet_pal/sys"; exit 1; }
echo "==> injecting dotnet PAL into rust-src ($SRC)"
# Each --rm container starts from a pristine rust-src baked into the image, so we
# always inject onto a clean base. Mirror every file under dotnet_pal/sys/** to $SRC/**.
( cd "$PAL" && find . -type f ) | while read -r f; do
  mkdir -p "$SRC/$(dirname "$f")"
  cp "$PAL/$f" "$SRC/$f"
done
# Insert the `target_os = "dotnet"` arm as the FIRST arm of each cascade's cfg_select!
# (dotnet is only true for our target, so order is irrelevant to correctness). Idempotent.
inject_arm() { # $1 = cascade file under $SRC ; $2 = arm body (one line) ; $3 = which cfg_select! block (1-based, default 1)
  local file="$SRC/$1"; local nth="${3:-1}"
  [ -f "$file" ] || return 0
  # Idempotency is per-block: a file with several cfg_select!s (thread_local/mod.rs)
  # gets a dotnet arm injected into each of blocks 1,2,3 across calls, so we key the
  # marker on the arm body, not just on the presence of `target_os = "dotnet"`.
  grep -qF "$2" "$file" && return 0
  awk -v arm="$2" -v nth="$nth" '
    /cfg_select! \{/ { blk++ }
    { print }
    /cfg_select! \{/ && blk==nth && !ins {
      print "    target_os = \"dotnet\" => {"
      print "        " arm
      print "    }"
      ins=1
    }' "$file" > "$file.__t" && mv "$file.__t" "$file"
}
# Anchor-based variant: insert the dotnet arm as the FIRST arm of the *next*
# `cfg_select! {` that appears at or after a line matching $2 (a fixed-string
# anchor). Use this for files whose cfg_select! count drifts across nightlies
# (e.g. thread_local/mod.rs, where a `destructors` cfg_select block was added,
# shifting every ordinal). Idempotent on the arm body. The arm body may be a
# multi-line block: pass it with literal newlines.
inject_arm_anchor() { # $1 = cascade file under $SRC ; $2 = fixed-string anchor ; $3 = arm body (may be multi-line)
  local file="$SRC/$1"
  [ -f "$file" ] || return 0
  grep -qF "$3" "$file" && return 0
  grep -qF "$2" "$file" || { echo "!! inject_arm_anchor: anchor '$2' not found in $1"; return 1; }
  awk -v anchor="$2" -v arm="$3" '
    index($0, anchor) { armed=1 }
    { print }
    armed && !ins && /cfg_select! \{/ {
      print "    target_os = \"dotnet\" => {"
      print arm
      print "    }"
      ins=1
    }' "$file" > "$file.__t" && mv "$file.__t" "$file"
}
[ -f "$PAL/pal/dotnet/mod.rs" ]      && inject_arm pal/mod.rs          'mod dotnet; pub use self::dotnet::*;'
[ -f "$PAL/alloc/dotnet.rs" ]        && inject_arm alloc/mod.rs        'mod dotnet;'
[ -f "$PAL/stdio/dotnet.rs" ]        && inject_arm stdio/mod.rs        'mod dotnet; pub use dotnet::*;'
[ -f "$PAL/args/dotnet.rs" ]         && inject_arm args/mod.rs         'mod dotnet; pub use dotnet::*;'
[ -f "$PAL/env/dotnet.rs" ]          && inject_arm env/mod.rs          'mod dotnet; pub use dotnet::*;'
[ -f "$PAL/random/dotnet.rs" ]       && inject_arm random/mod.rs       'mod dotnet; pub use dotnet::*;'
if [ -f "$PAL/thread_local/dotnet.rs" ]; then
  # thread_local/mod.rs has FOUR cfg_select!s, in source order:
  #   (1) the storage layer (top of file),
  #   (2) `pub(crate) mod destructors { cfg_select! { … } }`  <- added upstream;
  #       it is `#[cfg(all(target_thread_local, …))]`-gated, so it is compiled
  #       OUT for os=dotnet (not target_thread_local) and needs no arm,
  #   (3) `pub(crate) mod guard { cfg_select! { … } }`  -> supplies `enable`,
  #   (4) `pub(crate) mod key   { cfg_select! { … } }`  -> `_ => {}` (empty).
  # The (2) `destructors` block shifted every ordinal vs the older 3-block layout
  # this script was first written against, which is why the guard arm must be
  # anchored to `pub(crate) mod guard {` instead of injected by ordinal (the old
  # nth=2 landed in `destructors`, leaving `guard::enable` undefined — the very
  # E0425 we are fixing).
  #
  # Storage arm (block 1, still the first cfg_select): declares `mod dotnet` at
  # thread_local level and re-exports ITS STORAGE ITEMS ONLY (mirroring the
  # `no_threads` arm — a glob `pub use dotnet::*` instead leaks the PAL's own
  # `key`/`guard` items into thread_local scope and trips `hidden_glob_reexports`).
  inject_arm thread_local/mod.rs 'pub use dotnet::{EagerStorage, LazyStorage, thread_local_inner}; pub(crate) use dotnet::{LocalPointer, local_pointer}; mod dotnet;' 1
  # Guard arm: reach `enable` via `super::dotnet` (super of `guard` is
  # thread_local, where `mod dotnet` was declared above). `current.rs` calls
  # `crate::sys::thread_local::guard::enable()` from two sites; this is what they
  # resolve to. dotnet is modelled on `no_threads` (single managed thread), whose
  # guard `enable` is a leak-everything no-op.
  inject_arm_anchor thread_local/mod.rs 'pub(crate) mod guard {' '        pub(crate) use super::dotnet::enable;'
  # No `key` arm: nothing in std imports from `sys::thread_local::key` for
  # os=dotnet (the storage layer re-exports from `dotnet` directly, not from
  # os.rs), so the upstream `_ => {}` empty key arm compiles as-is. The PAL's
  # `dotnet::key` module stays available for when real .NET threading lands.
fi
[ -f "$PAL/io/error/dotnet.rs" ]     && inject_arm io/error/mod.rs     'mod dotnet; pub use dotnet::*;'
# time/mod.rs uses the `mod X; use X as imp;` cascade shape (it re-exports
# `imp::{Instant, SystemTime, UNIX_EPOCH}` at the bottom), unlike the
# `pub use dotnet::*` arms above. The dotnet arm backs Instant/SystemTime with
# Stopwatch/DateTime via the rcl_dotnet_{instant_ticks,instant_freq,unix_ticks}
# hooks (see cilly/src/ir/builtins/dotnet.rs).
[ -f "$PAL/time/dotnet.rs" ]         && inject_arm time/mod.rs         'mod dotnet; use dotnet as imp;'
# thread/mod.rs is a plain `pub use dotnet::*` cascade (like the `_`/unsupported
# arm): the dotnet arm provides Thread{new,join} + sleep/yield_now/set_name/
# current_os_id/available_parallelism/DEFAULT_MIN_STACK_SIZE, backed by
# System.Threading.Thread / System.Environment via the rcl_dotnet_thread_* and
# rcl_dotnet_available_parallelism hooks (see cilly/src/ir/builtins/dotnet.rs).
[ -f "$PAL/thread/dotnet.rs" ]       && inject_arm thread/mod.rs       'mod dotnet; pub use dotnet::*;'
# Teach std's build.rs that os=dotnet is a *supported* platform, otherwise std
# marks itself `restricted_std` (E0658 on use + "unwinding panics are not
# supported without std"). The allow-list is the long `if target_os == "linux"
# || ... {` block; inject our os as the first disjunct. Idempotent.
BUILD_RS="$SRC/../../build.rs"
if [ -f "$BUILD_RS" ] && ! grep -q 'target_os == "dotnet"' "$BUILD_RS"; then
  echo "==> teaching std/build.rs that os=dotnet is supported (un-restricted_std)"
  # Inject `target_os == "dotnet" ||` just before the first `target_os == "linux"`
  # disjunct of the supported-platform allow-list. awk (no perl dependency).
  awk '!ins && /target_os == "linux"/ {sub(/target_os == "linux"/, "target_os == \"dotnet\"\n        || target_os == \"linux\""); ins=1} {print}' "$BUILD_RS" > "$BUILD_RS.__t" && mv "$BUILD_RS.__t" "$BUILD_RS"
fi
echo "==> build-std cargo_tests/$DEV_CRATE for os=dotnet"
cd "/work/cargo_tests/$DEV_CRATE" 2>/dev/null || { echo "!! no cargo_tests/$DEV_CRATE"; exit 1; }
export RUSTFLAGS="-Z codegen-backend=/work/target/release/librustc_codegen_clr.so -C linker=/work/target/release/linker -C link-args=--cargo-support"
set +e
cargo -Zjson-target-spec build --release 2>&1 | grep -vE 'discirminant' | grep -E '^error|error\[|could not compile|warning: unused|Compiling (std|core|alloc) |Finished' | head -60
rc=${PIPESTATUS[0]}
echo "== build exit: $rc =="
out="target/x86_64-unknown-dotnet/release/$DEV_CRATE"
[ ! -f "$out" ] && out="target/dotnet/release/$DEV_CRATE"
if [ "$rc" = 0 ] && [ "$DEV_RUN" = 1 ] && [ -f "$out" ]; then echo "== RUN =="; "./$out"; echo "run exit: $?"; fi
C
  ;;

help|-h|--help) usage ;;
*) die "unknown command '$cmd' (try: dev.sh help)" ;;
esac
