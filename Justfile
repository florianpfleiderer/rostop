# rostop developer commands. Everything runs inside the Docker dev container.
#
# The dev environment is parameterised by ROS distro. The default recipes
# target Jazzy; mirror `-humble` recipes target Humble. `scripts/dev.sh`
# picks the Dockerfile, image tag, and target volume based on $ROSTOP_DISTRO.

default:
    @just --list

# Build the Jazzy dev image (idempotent)
image:
    docker build -t rostop-dev:jazzy -f Dockerfile .

# Build the Humble dev image (idempotent)
image-humble:
    docker build -t rostop-dev:humble -f Dockerfile.humble .

# Open an interactive shell in the Jazzy dev container with ROS 2 sourced
shell:
    ./scripts/dev.sh "exec bash"

# Open an interactive shell in the Humble dev container
shell-humble:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "exec bash"

# Run cargo test (workspace) inside the Jazzy container
test *args:
    ./scripts/dev.sh "cargo test --workspace {{args}}"

# Run cargo test (workspace) inside the Humble container
test-humble *args:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "cargo test --workspace {{args}}"

# Run a single core-crate test fast (no ROS link)
test-core *args:
    ./scripts/dev.sh "cargo test -p rostop-core {{args}}"

# Build the workspace inside the Jazzy container
build *args:
    ./scripts/dev.sh "cargo build --workspace {{args}}"

# Build the workspace inside the Humble container
build-humble *args:
    ROSTOP_DISTRO=humble ./scripts/dev.sh "cargo build --workspace {{args}}"

# Run the CLI (e.g. `just run --demo`)
run *args:
    ./scripts/dev.sh "cargo run -p rostop-cli -- {{args}}"

# Run the CLI against a real Jazzy ROS 2 system on the host (host network so DDS discovery works).
# Honours ROS_DOMAIN_ID (default 0) and RMW_IMPLEMENTATION (default rmw_cyclonedds_cpp) from the caller's env.
run-live *args:
    #!/usr/bin/env bash
    set -euo pipefail
    docker image inspect rostop-dev:jazzy >/dev/null 2>&1 || docker build -t rostop-dev:jazzy -f Dockerfile .
    TTY_FLAGS=""; [ -t 0 ] && TTY_FLAGS="-it"
    docker run --rm ${TTY_FLAGS} \
      --network=host \
      --ipc=host \
      -e ROS_DOMAIN_ID="${ROS_DOMAIN_ID:-0}" \
      -e RMW_IMPLEMENTATION="${RMW_IMPLEMENTATION:-rmw_cyclonedds_cpp}" \
      -e CYCLONEDDS_URI="${CYCLONEDDS_URI:-}" \
      -e ROS_LOCALHOST_ONLY="${ROS_LOCALHOST_ONLY:-0}" \
      -e ROSTOP_SKIP_PEER_PROBE="${ROSTOP_SKIP_PEER_PROBE:-}" \
      -v "$(pwd)":/work \
      -v rostop-cargo-registry:/opt/cargo/registry \
      -v rostop-cargo-git:/opt/cargo/git \
      -v rostop-target-jazzy:/work/target \
      -w /work \
      rostop-dev:jazzy \
      bash -lc "source /opt/ros/jazzy/setup.bash && cargo run -p rostop-cli --features live -- {{args}}"

# Run the CLI against a real Humble ROS 2 system on the host (host network so DDS discovery works).
# Honours ROS_DOMAIN_ID (default 0) and RMW_IMPLEMENTATION (default rmw_fastrtps_cpp — Humble's default RMW).
run-live-humble *args:
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
      -e ROSTOP_SKIP_PEER_PROBE="${ROSTOP_SKIP_PEER_PROBE:-}" \
      -v "$(pwd)":/work \
      -v rostop-cargo-registry:/opt/cargo/registry \
      -v rostop-cargo-git:/opt/cargo/git \
      -v rostop-target-humble:/work/target \
      -w /work \
      rostop-dev:humble \
      bash -lc "source /opt/ros/humble/setup.bash && cargo run -p rostop-cli --features live -- {{args}}"

# Format
fmt:
    ./scripts/dev.sh "cargo fmt --all"

# Lint
clippy:
    ./scripts/dev.sh "cargo clippy --workspace --all-targets -- -D warnings"

# Clean
clean:
    ./scripts/dev.sh "cargo clean"

# Build the Jazzy .deb release artifact into dist/ (needs rostop-dev:jazzy with cargo-deb — rebuild with `just image` if you predate that).
package-jazzy:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p dist
    ROSTOP_DISTRO=jazzy ./scripts/dev.sh \
        "cargo deb --variant jazzy -p rostop-cli --features live --output /work/dist/"
    cd dist && sha256sum rostop-jazzy_*.deb | tee SHA256SUMS.jazzy

# Build the Humble .deb release artifact into dist/ (needs rostop-dev:humble with cargo-deb — rebuild with `just image-humble` if you predate that).
package-humble:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p dist
    ROSTOP_DISTRO=humble ./scripts/dev.sh \
        "cargo deb --variant humble -p rostop-cli --features live --output /work/dist/"
    cd dist && sha256sum rostop-humble_*.deb | tee SHA256SUMS.humble
