#!/usr/bin/env bash
# 3-way performance harness: native Rust  vs  Rust-via-rustc_codegen_clr (.NET)  vs  C# (.NET).
# Builds + runs the same workloads three ways, then prints one comparison table with ratios and
# allocation profiling. Native = the upper bound; C# = the .NET peer ceiling; backend = us.
#
# Usage:
#   feasibility/perf/run.sh                 # the 3-way table
#   feasibility/perf/run.sh --knobs         # + re-run the backend with OPTIMIZE_CIL=0 and NO_UNWIND=1
#
# Requires the NATIVE backend toolchain (see the cargo-dotnet native setup): the pinned nightly on
# PATH + $HOME/.dotnet. Assumes the backend dylib + linker are already built
# (target/release/librustc_codegen_clr.dylib + target/release/linker).
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RUST="$HERE/rust"; CS="$HERE/csharp"
OUT="${PERF_OUT:-$HERE/results}"; mkdir -p "$OUT"
KNOBS=0; [ "${1:-}" = "--knobs" ] && KNOBS=1
DOTNET_VERSION="${DOTNET_VERSION:-10}"
case "$DOTNET_VERSION" in
  8|9|10) ;;
  *) echo "DOTNET_VERSION must be 8, 9, or 10" >&2; exit 2 ;;
esac

export PATH="$HOME/.rustup/toolchains/nightly-2026-06-17-aarch64-apple-darwin/bin:/opt/homebrew/opt/rustup/bin:$HOME/.dotnet:$PATH"
export DOTNET_ROOT="$HOME/.dotnet"
export CARGO_DOTNET_BACKEND=native
export CD_LINKER="$REPO/target/release/linker"
export CD_BACKEND_DYLIB="$REPO/target/release/librustc_codegen_clr.dylib"

run_backend() { # $1=outfile ; extra env via caller
  ( cd "$REPO" && feasibility/cargo-dotnet run "$RUST" --release --dotnet "$DOTNET_VERSION" 2>/dev/null ) | grep '^RESULT' > "$1"
}

echo "== [1/3] native Rust (host, upper bound) =="
rm -f "$RUST/.cargo/config.toml"          # ensure host target (cargo-dotnet regenerates this below)
( cd "$RUST" && cargo build --release >/dev/null 2>&1 \
  && ./target/release/perf_rs 2>/dev/null ) | grep '^RESULT' > "$OUT/native.txt"
echo "   $(wc -l < "$OUT/native.txt") workloads"

echo "== [2/3] Rust via rustc_codegen_clr (.NET $DOTNET_VERSION) =="
run_backend "$OUT/backend.txt"
echo "   $(wc -l < "$OUT/backend.txt") workloads"

echo "== [3/3] C# (.NET $DOTNET_VERSION, peer ceiling) =="
( cd "$CS" && dotnet run -c Release -f "net${DOTNET_VERSION}.0" 2>/dev/null ) | grep '^RESULT' > "$OUT/csharp.txt"
echo "   $(wc -l < "$OUT/csharp.txt") workloads"

if [ "$KNOBS" = 1 ]; then
  echo "== [knobs] backend with OPTIMIZE_CIL=0 =="
  ( export OPTIMIZE_CIL=0; run_backend "$OUT/backend_noopt.txt" )
  echo "== [knobs] backend with NO_UNWIND=1 =="
  ( export NO_UNWIND=1; run_backend "$OUT/backend_nounwind.txt" )
fi

echo ""
echo "================================ 3-WAY COMPARISON ================================"
awk -v nf="$OUT/native.txt" -v bf="$OUT/backend.txt" -v cf="$OUT/csharp.txt" \
    -v kf="$OUT/backend_noopt.txt" -v uf="$OUT/backend_nounwind.txt" -v knobs="$KNOBS" '
BEGIN{
  while((getline l < nf)>0){split(l,a," "); nat[a[2]]=a[3]; ral[a[2]]=a[4]; rby[a[2]]=a[5]; ord[++n]=a[2]}
  while((getline l < bf)>0){split(l,a," "); bk[a[2]]=a[3]}
  while((getline l < cf)>0){split(l,a," "); cs[a[2]]=a[3]; csby[a[2]]=a[4]; csg[a[2]]=a[5]}
  while((getline l < kf)>0){split(l,a," "); noopt[a[2]]=a[3]}
  while((getline l < uf)>0){split(l,a," "); nounw[a[2]]=a[3]}
  printf "%-14s %10s %10s %10s  %8s %8s   %9s %11s\n",\
         "workload","native","backend","C#","bk/nat","bk/C#","rs-allocs","cs-gen0"
  printf "%-14s %10s %10s %10s  %8s %8s   %9s %11s\n",\
         "--------","(ms)","(ms)","(ms)","x","x","(count)","(count)"
  for(i=1;i<=n;i++){ k=ord[i];
    nm=nat[k]/1e6; bm=bk[k]/1e6; cm=cs[k]/1e6;
    rn=(nat[k]>0)?bk[k]/nat[k]:0; rc=(cs[k]>0)?bk[k]/cs[k]:0;
    printf "%-14s %10.2f %10.2f %10.2f  %7.1fx %7.1fx   %9d %11d\n", k, nm, bm, cm, rn, rc, ral[k]+0, csg[k]+0;
  }
  if(knobs=="1"){
    printf "\n---- optimizer-knob deltas on the backend (ms; lower is better) ----\n";
    printf "%-14s %10s %12s %12s\n","workload","backend","OPT_CIL=0","NO_UNWIND=1";
    for(i=1;i<=n;i++){ k=ord[i];
      printf "%-14s %10.2f %12.2f %12.2f\n", k, bk[k]/1e6, noopt[k]/1e6, nounw[k]/1e6;
    }
  }
}'
echo "================================================================================="
echo "raw results in $OUT/{native,backend,csharp}.txt"
