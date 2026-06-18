#!/usr/bin/env bash
# Host-side driver: build the dev image and run a harness step with the repo
# mounted. Run from anywhere.
#
#   feasibility/run.sh build      # build cilly + the codegen backend
#   feasibility/run.sh smoke      # compile+run a Rust program on .NET
#   feasibility/run.sh test       # run cargo test ::stable subset
#   feasibility/run.sh demo       # Rust -> C# interop demo
#   feasibility/run.sh shell      # drop into a shell in the container
#
# Env:
#   PLATFORM=linux/amd64   # force the on-path arch (needed for running tests on
#                          # Apple Silicon; slower under emulation). Default: host.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="rcc-dev"
PLATFORM_ARG=()
[ -n "${PLATFORM:-}" ] && PLATFORM_ARG=(--platform "$PLATFORM")

echo "==> Building image $IMAGE…"
# `${arr[@]+"${arr[@]}"}` guards against "unbound variable" when expanding an empty
# array under `set -u` on bash 3.2 (the version macOS ships).
docker build ${PLATFORM_ARG[@]+"${PLATFORM_ARG[@]}"} -t "$IMAGE" "$REPO_ROOT/feasibility"

TTY_ARG=()
[ -t 1 ] && TTY_ARG=(-it)

echo "==> Running harness: ${*:-all}"
# The named volume `rcc-target` masks /work/target so the container's Linux build
# artifacts never collide with the host's target/ (and persist across runs for caching).
docker run --rm ${TTY_ARG[@]+"${TTY_ARG[@]}"} ${PLATFORM_ARG[@]+"${PLATFORM_ARG[@]}"} \
  -v "$REPO_ROOT":/work -v rcc-target:/work/target -w /work \
  "$IMAGE" bash feasibility/harness.sh "${@:-all}"
