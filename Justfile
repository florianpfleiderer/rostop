# rostop developer commands. Everything runs inside the Docker dev container.
#
# The dev environment is parameterised by ROS distro. There is no default —
# every recipe that builds, tests, or runs against ROS is suffixed with the
# distro (`-jazzy`, `-humble`, ...). `scripts/dev.sh` picks the Dockerfile,
# image tag, and target volume from $ROSTOP_DISTRO. Add a new distro by
# dropping a `Dockerfile.<distro>` next to the existing ones and mirroring
# the recipe pair below.

default:
    @just --list

# --- Jazzy ----------------------------------------------------------------

# Build the Jazzy dev image (idempotent)
image-jazzy:
    docker build -t rostop-dev:jazzy -f Dockerfile.jazzy .

# Open an interactive shell in the Jazzy dev container with ROS 2 sourced
shell-jazzy:
    ROSTOP_DISTRO=jazzy ./scripts/dev.sh "exec bash"

# Run cargo test (workspace) inside the Jazzy container
test-jazzy *args:
    ROSTOP_DISTRO=jazzy ./scripts/dev.sh "cargo test --workspace {{args}}"

# Build the workspace inside the Jazzy container
build-jazzy *args:
    ROSTOP_DISTRO=jazzy ./scripts/dev.sh "cargo build --workspace {{args}}"

# Run the CLI against a real Jazzy ROS 2 system on the host (host network so DDS
# discovery works). Honours ROS_DOMAIN_ID (default 0) and RMW_IMPLEMENTATION
# (default rmw_cyclonedds_cpp) from the caller's env.
# Pass `--demo` to swap in the fabricated demo backend (no ROS traffic needed).
run-jazzy *args:
    #!/usr/bin/env bash
    set -euo pipefail
    docker image inspect rostop-dev:jazzy >/dev/null 2>&1 || docker build -t rostop-dev:jazzy -f Dockerfile.jazzy .
    TTY_FLAGS=""; [ -t 0 ] && TTY_FLAGS="-it"
    docker run --rm ${TTY_FLAGS} \
      --network=host \
      --ipc=host \
      -e ROS_DOMAIN_ID="${ROS_DOMAIN_ID:-0}" \
      -e RMW_IMPLEMENTATION="${RMW_IMPLEMENTATION:-rmw_cyclonedds_cpp}" \
      -e CYCLONEDDS_URI="${CYCLONEDDS_URI:-}" \
      -e ROS_LOCALHOST_ONLY="${ROS_LOCALHOST_ONLY:-0}" \
      -v "$(pwd)":/work \
      -v rostop-cargo-registry:/opt/cargo/registry \
      -v rostop-cargo-git:/opt/cargo/git \
      -v rostop-target-jazzy:/work/target \
      -w /work \
      rostop-dev:jazzy \
      bash -lc "source /opt/ros/jazzy/setup.bash && cargo run -p rostop-cli --features live -- {{args}}"

# Format inside the Jazzy container
fmt-jazzy:
    ROSTOP_DISTRO=jazzy ./scripts/dev.sh "cargo fmt --all"

# Lint inside the Jazzy container
clippy-jazzy:
    ROSTOP_DISTRO=jazzy ./scripts/dev.sh "cargo clippy --workspace --all-targets -- -D warnings"

# Clean cargo artifacts in the Jazzy container
clean-jazzy:
    ROSTOP_DISTRO=jazzy ./scripts/dev.sh "cargo clean"

# Build the Jazzy .deb release artifact into dist/ (needs rostop-dev:jazzy with cargo-deb — rebuild with `just image-jazzy` if you predate that).
package-jazzy:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p dist
    ROSTOP_DISTRO=jazzy ./scripts/dev.sh \
        "cargo deb --variant jazzy -p rostop-cli --features live --output /work/dist/"
    # cargo-deb produces rostop-jazzy_<ver>-<rev>_amd64.deb. Reshape it to
    # rostop-<ver>-jazzy_<rev>_amd64.deb so the version sits before the
    # distro, matching the tarball convention. The internal apt package
    # name is still rostop-jazzy (set in Cargo.toml deb variant metadata).
    cd dist
    old=$(ls rostop-jazzy_*.deb)
    new=$(echo "$old" | sed -E 's/^rostop-jazzy_(.*)-([0-9]+)_amd64\.deb$/rostop-\1-jazzy_\2_amd64.deb/')
    mv "$old" "$new"
    sha256sum rostop-*-jazzy_*_amd64.deb | tee SHA256SUMS.jazzy

# --- Humble ---------------------------------------------------------------

# Build the Humble dev image (idempotent)
image-humble:
    docker build -t rostop-dev:humble -f Dockerfile.humble .

# Open an interactive shell in the Humble dev container
shell-humble:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "exec bash"

# Run cargo test (workspace) inside the Humble container
test-humble *args:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "cargo test --workspace {{args}}"

# Build the workspace inside the Humble container
build-humble *args:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "cargo build --workspace {{args}}"

# Run the CLI against a real Humble ROS 2 system on the host (host network so DDS
# discovery works). Honours ROS_DOMAIN_ID (default 0) and RMW_IMPLEMENTATION
# (default rmw_fastrtps_cpp — Humble's default RMW).
# Pass `--demo` to swap in the fabricated demo backend (no ROS traffic needed).
run-humble *args:
    #!/usr/bin/env bash
    set -euo pipefail
    docker image inspect rostop-dev:humble >/dev/null 2>&1 || docker build -t rostop-dev:humble -f Dockerfile.humble .
    TTY_FLAGS=""; [ -t 0 ] && TTY_FLAGS="-it"
    docker run --rm ${TTY_FLAGS} \
      --network=host \
      --ipc=host \
      -e ROS_DOMAIN_ID="${ROS_DOMAIN_ID:-0}" \
      -e RMW_IMPLEMENTATION="${RMW_IMPLEMENTATION:-rmw_fastrtps_cpp}" \
      -e ROS_LOCALHOST_ONLY="${ROS_LOCALHOST_ONLY:-0}" \
      -v "$(pwd)":/work \
      -v rostop-cargo-registry:/opt/cargo/registry \
      -v rostop-cargo-git:/opt/cargo/git \
      -v rostop-target-humble:/work/target \
      -w /work \
      rostop-dev:humble \
      bash -lc "source /opt/ros/humble/setup.bash && cargo run -p rostop-cli --features live -- {{args}}"

# Format inside the Humble container
fmt-humble:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "cargo fmt --all"

# Lint inside the Humble container
clippy-humble:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "cargo clippy --workspace --all-targets -- -D warnings"

# Clean cargo artifacts in the Humble container
clean-humble:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "cargo clean"

# Build the Humble .deb release artifact into dist/ (needs rostop-dev:humble with cargo-deb — rebuild with `just image-humble` if you predate that).
package-humble:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p dist
    ROSTOP_DISTRO=humble ./scripts/dev.sh \
        "cargo deb --variant humble -p rostop-cli --features live --output /work/dist/"
    # See package-jazzy for the rename rationale.
    cd dist
    old=$(ls rostop-humble_*.deb)
    new=$(echo "$old" | sed -E 's/^rostop-humble_(.*)-([0-9]+)_amd64\.deb$/rostop-\1-humble_\2_amd64.deb/')
    mv "$old" "$new"
    sha256sum rostop-*-humble_*_amd64.deb | tee SHA256SUMS.humble

# --- Core-only (no ROS link) ----------------------------------------------
#
# `rostop-core` has no ROS dependency, so these don't really care which distro
# they run under — they're suffixed only so `scripts/dev.sh` has a container
# to use. Pick whichever you've already built.

test-core-jazzy *args:
    ROSTOP_DISTRO=jazzy ./scripts/dev.sh "cargo test -p rostop-core {{args}}"

test-core-humble *args:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "cargo test -p rostop-core {{args}}"
