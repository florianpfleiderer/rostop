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

# Format
fmt:
    ./scripts/dev.sh "cargo fmt --all"

# Lint
clippy:
    ./scripts/dev.sh "cargo clippy --workspace --all-targets -- -D warnings"

# Clean
clean:
    ./scripts/dev.sh "cargo clean"
