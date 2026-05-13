#!/usr/bin/env bash
# Run a command inside the rostop dev container with cached cargo + target volumes.
#
# The dev image is parameterised by ROS distro via the ROSTOP_DISTRO env var
# (default: jazzy). Each distro gets its own image tag, target volume, and
# sourced setup.bash so cached artifacts don't collide.
set -euo pipefail

DISTRO="${ROSTOP_DISTRO:-jazzy}"
case "$DISTRO" in
  jazzy)  DOCKERFILE="Dockerfile" ;;
  humble) DOCKERFILE="Dockerfile.humble" ;;
  *)
    echo "[dev.sh] unsupported ROSTOP_DISTRO=$DISTRO (supported: jazzy, humble)" >&2
    exit 2
    ;;
esac

IMAGE="rostop-dev:${DISTRO}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "[dev.sh] building $IMAGE from $DOCKERFILE ..." >&2
  docker build -t "$IMAGE" -f "$ROOT/$DOCKERFILE" "$ROOT"
fi

# TTY only if stdin is a tty (so CI / non-interactive callers still work)
TTY_FLAGS=""
if [ -t 0 ]; then TTY_FLAGS="-it"; fi

exec docker run --rm ${TTY_FLAGS} \
  -v "$ROOT":/work \
  -v rostop-cargo-registry:/opt/cargo/registry \
  -v rostop-cargo-git:/opt/cargo/git \
  -v "rostop-target-${DISTRO}:/work/target" \
  -w /work \
  "$IMAGE" \
  bash -lc "source /opt/ros/${DISTRO}/setup.bash && $*"
