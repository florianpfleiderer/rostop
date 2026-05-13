# rostop developer commands. Everything runs inside the Docker dev container.

default:
    @just --list

# Build the dev image (idempotent)
image:
    docker build -t rostop-dev:latest .

# Open an interactive shell in the dev container with ROS2 sourced
shell:
    ./scripts/dev.sh "exec bash"

# Run cargo test (workspace) inside the container
test *args:
    ./scripts/dev.sh "cargo test --workspace {{args}}"

# Run a single core-crate test fast (no ROS link)
test-core *args:
    ./scripts/dev.sh "cargo test -p rostop-core {{args}}"

# Build the workspace
build *args:
    ./scripts/dev.sh "cargo build --workspace {{args}}"

# Run the CLI (e.g. `just run --demo`)
run *args:
    ./scripts/dev.sh "cargo run -p rostop-cli -- {{args}}"

# Run the CLI against a real ROS 2 system on the host (shares host network so DDS discovery works).
# Honours ROS_DOMAIN_ID (default 0) and RMW_IMPLEMENTATION (default rmw_cyclonedds_cpp) from the caller's env.
run-live *args:
    #!/usr/bin/env bash
    set -euo pipefail
    docker image inspect rostop-dev:latest >/dev/null 2>&1 || docker build -t rostop-dev:latest .
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
      -v rostop-target:/work/target \
      -w /work \
      rostop-dev:latest \
      bash -lc "source /opt/ros/jazzy/setup.bash && cargo run -p rostop-cli -- {{args}}"

# Format
fmt:
    ./scripts/dev.sh "cargo fmt --all"

# Lint
clippy:
    ./scripts/dev.sh "cargo clippy --workspace --all-targets -- -D warnings"

# Clean
clean:
    ./scripts/dev.sh "cargo clean"
