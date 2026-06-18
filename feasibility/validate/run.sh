#!/usr/bin/env bash
# Host driver for the differential project validator. Builds the dev image + the
# backend, then runs feasibility/validate/validate.sh inside the container.
#
#   feasibility/validate/run.sh                # validate all projects
#   feasibility/validate/run.sh kitchen_sink   # validate one
#
# PLATFORM=linux/amd64 forces the on-path arch (slower under emulation on Apple Silicon).
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="rcc-dev"
PLAT=()
[ -n "${PLATFORM:-}" ] && PLAT=(--platform "$PLATFORM")
TTY=()
[ -t 1 ] && TTY=(-it)

docker build ${PLAT[@]+"${PLAT[@]}"} -t "$IMAGE" "$REPO_ROOT/feasibility" >/dev/null
docker run --rm ${TTY[@]+"${TTY[@]}"} ${PLAT[@]+"${PLAT[@]}"} \
  -v "$REPO_ROOT":/work -v rcc-target:/work/target -w /work "$IMAGE" \
  bash -c "bash feasibility/harness.sh build >/dev/null && bash feasibility/validate/validate.sh ${*:-all}"
