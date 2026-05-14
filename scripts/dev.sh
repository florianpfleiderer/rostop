#!/usr/bin/env bash
# Run a command inside the rostop dev container with cached cargo + target volumes.
#
# The dev image is parameterised by ROS distro via the ROSTOP_DISTRO env var.
# There is intentionally no default — set it explicitly (e.g. `ROSTOP_DISTRO=jazzy`)
# or call one of the per-distro `just` recipes (`just <recipe>-jazzy`, `-humble`, ...).
# Each distro gets its own Dockerfile, image tag, target volume, and sourced
# setup.bash so cached artifacts don't collide.
set -euo pipefail

if [ -z "${ROSTOP_DISTRO:-}" ]; then
  echo "[dev.sh] ROSTOP_DISTRO is not set." >&2
  echo "[dev.sh] Set it explicitly (e.g. ROSTOP_DISTRO=jazzy) or use a per-distro just recipe (\`just test-jazzy\`, \`just test-humble\`, ...)." >&2
  exit 2
fi

DISTRO="$ROSTOP_DISTRO"
DOCKERFILE="Dockerfile.${DISTRO}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ ! -f "$ROOT/$DOCKERFILE" ]; then
  echo "[dev.sh] no $DOCKERFILE in repo root — add one to support ROSTOP_DISTRO=$DISTRO." >&2
  exit 2
fi

IMAGE="rostop-dev:${DISTRO}"

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
