#!/usr/bin/env bash
# Differential project validator for rustc_codegen_clr.
#
# Builds and runs a Rust project TWICE — once with the normal (native) toolchain,
# once through rustc_codegen_clr on .NET — with identical inputs, then diffs the
# output. The native run is ground truth, so any difference is a miscompilation.
# This catches the bugs that unit tests miss (a passing test only proves *that*
# program path is right; a real project exercises far more).
#
# Runs INSIDE the dev container (repo at /work). Driver: feasibility/validate/run.sh
#
# Usage: validate.sh <project-name> [<project-name> ...]   (or `all`)
set -uo pipefail

ROOT=/work
VDIR="$ROOT/feasibility/validate"
REL="$ROOT/target/release"
BACKEND="$REL/librustc_codegen_clr.so"
LINKER="$REL/linker"

[ -f "$BACKEND" ] && [ -x "$LINKER" ] || {
    echo "!! backend not built — run: feasibility/run.sh build"; exit 1; }
HOST=$(rustc -vV | awk '/^host:/{print $2}')
RUNTIME_VER=$(dotnet --info 2>/dev/null | awk '/Host:/{h=1} h&&/Version:/{print $2; exit}')

# Run one project descriptor. Returns 0 on PASS, 1 on FAIL, 2 on infra error.
validate_one() {
    local proj="$1"
    local pdir="$VDIR/projects/$proj"
    [ -f "$pdir/project.env" ] || { echo "?? no such project: $proj"; return 2; }

    # --- descriptor (defaults, overridable in project.env) ---
    local SRC="local" GIT_REV="" SUBDIR="" BIN="" RUN_ARGS="" STDIN_FILE=""
    local PROFILE="diff" NORMALIZE="" BUILD_STD="core,alloc,std,panic_abort"
    # shellcheck disable=SC1090
    source "$pdir/project.env"

    local work; work=$(mktemp -d)
    local crate
    if [ "$SRC" = "local" ]; then
        crate="$pdir/crate"
    else
        echo "==> [$proj] fetching $SRC ${GIT_REV:+@ $GIT_REV}"
        git clone --quiet --depth 1 ${GIT_REV:+--branch "$GIT_REV"} "$SRC" "$work/src" \
            || { echo "!! clone failed"; return 2; }
        crate="$work/src${SUBDIR:+/$SUBDIR}"
    fi
    local stdin_src=/dev/null
    [ -n "$STDIN_FILE" ] && stdin_src="$pdir/$STDIN_FILE"

    norm() { if [ -n "$NORMALIZE" ]; then sed -E "$NORMALIZE"; else cat; fi; }

    # --- native baseline (ground truth) ---
    echo "==> [$proj] native: build + run"
    ( cd "$crate" && CARGO_TARGET_DIR="$work/native" cargo build --release ${BIN:+--bin "$BIN"} ) \
        >"$work/nb.log" 2>&1 || { echo "!! native build failed:"; tail -20 "$work/nb.log"; return 2; }
    local nbin
    nbin=$(find "$work/native/release" -maxdepth 1 -type f ! -name '*.*' -perm -u+x | head -1)
    ( cd "$crate" && "$nbin" $RUN_ARGS <"$stdin_src" ) >"$work/native.out" 2>&1
    echo "[exit:$?]" >>"$work/native.out"

    # --- through the backend, on .NET ---
    echo "==> [$proj] clr: build through rustc_codegen_clr + run on .NET"
    ( cd "$crate" && CARGO_TARGET_DIR="$work/clr" \
        RUSTFLAGS="-Zcodegen-backend=$BACKEND -Clinker=$LINKER -Clink-args=--cargo-support -Ctarget-feature=+x87+sse" \
        cargo build --release -Zbuild-std="$BUILD_STD" --target "$HOST" ${BIN:+--bin "$BIN"} ) \
        >"$work/cb.log" 2>&1 || { echo "!! CLR build failed:"; tail -30 "$work/cb.log"; return 1; }
    local cbin
    cbin=$(find "$work/clr/$HOST/release" -maxdepth 1 -type f ! -name '*.*' | head -1)
    printf '{ "runtimeOptions": { "tfm": "net8.0", "framework": { "name": "Microsoft.NETCore.App", "version": "%s" }, "configProperties": { "System.Threading.ThreadPool.MinThreads": 4, "System.Threading.ThreadPool.MaxThreads": 25 } } }' \
        "$RUNTIME_VER" > "$cbin.runtimeconfig.json"
    ( cd "$crate" && dotnet "$cbin" $RUN_ARGS <"$stdin_src" ) >"$work/clr.out" 2>&1
    echo "[exit:$?]" >>"$work/clr.out"

    # --- compare ---
    norm <"$work/native.out" >"$work/native.norm"
    norm <"$work/clr.out"    >"$work/clr.norm"
    if diff -u "$work/native.norm" "$work/clr.norm" >"$work/diff.txt"; then
        echo "✅ PASS [$proj]: native and .NET output identical ($(wc -l <"$work/native.norm") lines)"
        return 0
    else
        echo "❌ FAIL [$proj]: output differs (- native / + .NET) — candidate miscompilation:"
        sed 's/^/    /' "$work/diff.txt" | head -60
        return 1
    fi
}

# --- main ---
targets=("$@")
if [ "${1:-}" = "all" ] || [ $# -eq 0 ]; then
    mapfile -t targets < <(ls "$VDIR/projects" | grep -v '^_')
fi
fails=0
for t in "${targets[@]}"; do
    validate_one "$t" || fails=$((fails+1))
    echo
done
echo "==== $((${#targets[@]} - fails))/${#targets[@]} projects validated ===="
[ "$fails" -eq 0 ]
