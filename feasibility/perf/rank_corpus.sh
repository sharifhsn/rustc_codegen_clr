#!/usr/bin/env bash
# Bottleneck map from the EXISTING library #[bench] corpus — zero new runs.
# Joins the backend bench results (latest_benchmarks.txt) against native (native_benchmark.txt) by
# bench name and ranks by the backend/native slowdown ratio. The top of this list is the hotspot
# list to attack; the per-category aggregate shows which *kinds* of code are slow.
#
# Regenerate the inputs with the core/alloc #[bench] suites (see bin/success_corebenches.txt); this
# script only analyzes whatever pair is currently on disk.
#
# Usage: feasibility/perf/rank_corpus.sh [TOP_N]   (default 25)
set -uo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
TOP="${1:-25}"
BK="$REPO/latest_benchmarks.txt"
NAT="$REPO/native_benchmark.txt"
[ -f "$BK" ] && [ -f "$NAT" ] || { echo "missing $BK or $NAT"; exit 1; }

norm() { # "test NAME ... bench: 1,234.5 ns/iter ..." -> "NAME<TAB>1234.5"
  grep -E "^test .* bench:" "$1" \
    | sed -E 's/^test +([^ ]+) +\.\.\. bench: +([0-9,]+(\.[0-9]+)?).*/\1\t\2/' | tr -d ','
}
# FLOOR=min native ns to count: sub-ns native benches are work the native optimizer fully elided
# (dead code), which makes the ratio meaningless. Default 10ns keeps real, measurable workloads.
FLOOR="${PERF_FLOOR:-10}"
join -t$'\t' <(norm "$BK" | sort) <(norm "$NAT" | sort) \
  | awk -F'\t' -v fl="$FLOOR" '$3>=fl{printf "%.1f\t%s\t%.0f\t%.0f\n",$2/$3,$1,$2,$3}' > /tmp/_perf_ranked.tsv

echo "==== TOP $TOP slowest (backend/native ratio; native >= ${FLOOR}ns) ===="
printf "%8s  %-52s %10s %10s\n" "ratio" "bench" "bk(ns)" "nat(ns)"
sort -rn /tmp/_perf_ranked.tsv | head -"$TOP" \
  | awk -F'\t' '{printf "%7.1fx  %-52s %10d %10d\n",$1,$2,$3,$4}'

echo ""
echo "==== by category (prefix before '::'), mean ratio + count ===="
awk -F'\t' '{n=$2; sub(/::.*/,"",n); sum[n]+=$1; cnt[n]++}
  END{ for(c in sum){ printf "%7.1fx  %-22s (%d benches)\n", sum[c]/cnt[c], c, cnt[c] } }' \
  /tmp/_perf_ranked.tsv | sort -rn | head -20
echo ""
echo "(joined $(wc -l < /tmp/_perf_ranked.tsv) benches with native >= ${FLOOR}ns)"
