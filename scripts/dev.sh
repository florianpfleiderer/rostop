#!/usr/bin/env bash
# Run a command inside the rostop dev container with cached cargo + target volumes.
set -euo pipefail

IMAGE="rostop-dev:latest"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "[dev.sh] building $IMAGE ..." >&2
  docker build -t "$IMAGE" "$ROOT"
fi

# TTY only if stdin is a tty (so CI / non-interactive callers still work)
TTY_FLAGS=""
if [ -t 0 ]; then TTY_FLAGS="-it"; fi

exec docker run --rm ${TTY_FLAGS} \
  -v "$ROOT":/work \
  -v rostop-cargo-registry:/opt/cargo/registry \
  -v rostop-cargo-git:/opt/cargo/git \
  -v rostop-target:/work/target \
  -w /work \
  "$IMAGE" \
  bash -lc "source /opt/ros/jazzy/setup.bash && $*"
