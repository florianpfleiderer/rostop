# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build, test, run

All developer workflows route through `just` recipes that run inside per-distro Docker dev containers. **There is no default distro** — every recipe is suffixed with `-jazzy` or `-humble`, and `scripts/dev.sh` refuses to run unless `ROSTOP_DISTRO` is set. Pick whichever distro matches the robot you're testing against; for pure-core work either is fine.

```bash
just image-jazzy            # build the Jazzy dev image (idempotent, ~10 min cold)
just test-jazzy             # cargo test --workspace inside the container
just test-core-jazzy        # only rostop-core (no ROS link — fastest feedback)
just build-jazzy
just run-jazzy -- --demo    # fabricated 6-topic demo, no ROS traffic needed
just run-jazzy              # talk to a real ROS 2 system (--network=host)
just fmt-jazzy
just clippy-jazzy           # cargo clippy --workspace --all-targets -- -D warnings (must pass)
just package-jazzy          # build the .deb into dist/
```

Swap `-jazzy` for `-humble` to use the Humble container. Each distro has its own `target/` volume (`rostop-target-jazzy` vs `rostop-target-humble`) so caches don't collide; switching distros doesn't trigger a full rebuild.

Running a single test:

```bash
just test-jazzy stats::tests::hz_window_evicts_old_samples   # pass cargo test args after the recipe name
just test-core-jazzy registry::                              # whole module
```

Local `cargo` works too if ROS 2 + Rust 1.88 are installed on the host (`cargo run -p rostop-cli --features live -- --demo`); Docker is for reproducibility, not a requirement.

### CI parity

CI runs the test matrix on both distros with `-D warnings` clippy. Before pushing, run at minimum `just test-core-<distro>` and `just clippy-<distro>` for the distro you have built — full `just test-<distro>` is much slower because the `live` integration tests spin up `ros2 topic pub` inside the container.

## Architecture

Two-crate Cargo workspace with a hard separation between pure logic and ROS-linked code:

- **`crates/rostop-core`** — `TopicRegistry`, `TopicStats`, `Sparkline`, `MessageTree` (dynamic field decoding). **No ROS dependency, ever.** All Hz / BW / jitter math, registry CRUD + sort + filter, sparkline rendering, and message-tree flattening live here and are unit-tested without a ROS install. If you find yourself wanting to add `r2r` or any `ros-*` crate to `rostop-core`, the design is telling you the code belongs in `rostop-cli` instead.

- **`crates/rostop-cli`** — the `rostop` binary (`src/main.rs`), the ratatui app loop and key handling (`src/app.rs`), rendering (`src/ui/`), and backend implementations (`src/backend/`). Exposes a small `test_support` shim from `src/lib.rs` so integration tests (`tests/render.rs`, `tests/drilldown.rs`) can drive the app against a `TestBackend` without taking over a real terminal.

### The backend trait is the seam

`crates/rostop-cli/src/backend.rs` defines:

```rust
pub trait RosBackend: Send {
    fn poll(&mut self, budget: Duration) -> Vec<BackendEvent>;
    fn label(&self) -> &'static str;
}
```

The UI only ever sees `BackendEvent` (`Topic`, `TopicRemoved`, `Sample { name, bytes, value, at }`). Two implementations exist:

- `backend/demo.rs` — `DemoBackend`, always compiled, fabricates a realistic 6-topic stream. This is what powers `--demo` and the integration tests.
- `backend/live.rs` — `LiveBackend`, **gated behind `#[cfg(feature = "live")]`**, uses `r2r` and `subscribe_raw` for accurate wire-byte counts. The `live` feature pulls in `r2r`, `futures`, `serde_json` and requires the system `rcl`/`rmw`/`ros-<distro>-*` packages at build time.

When extending, put new sources of events behind the same trait — the UI must not learn about ROS, DDS, or recording formats directly.

### Distro coupling

`r2r` links the ROS 2 C client library at build time, so a given `rostop` binary is locked to one ROS distro **and** one RMW implementation. `build.rs` stamps `ROS_DISTRO` and `RMW_IMPLEMENTATION` from the build env and quotes them back in error messages and `--version`. Cross-distro / cross-RMW peers will be refused by the startup peer probe — don't try to override `RMW_IMPLEMENTATION` to make them "work."

This is why every dev recipe is distro-suffixed, why there are two Dockerfiles, and why `.deb` packaging uses `cargo-deb` variants (`rostop-humble` and `rostop-jazzy`, with a mutual `Conflicts:`). Adding a new distro = drop in `Dockerfile.<distro>` + mirror the recipe block in `Justfile` + add a `[package.metadata.deb.variants.<distro>]` block in `crates/rostop-cli/Cargo.toml`.

### Tests

- `rostop-core` unit tests live in `src/<module>/tests.rs` (e.g. `src/stats/tests.rs`). Prefer adding tests here over an integration test if the logic can be exercised without ROS — they are an order of magnitude faster than the live-backend integration tests.
- `rostop-cli` integration tests (`tests/render.rs`, `tests/drilldown.rs`) drive the full app against `TestBackend` via the `test_support` shim.
- `rostop-cli` live tests run real `ros2 topic pub` against `LiveBackend` and only compile under `--features live`.

## Conventions enforced by review

- **Conventional Commits** with optional scope: `feat(cli):`, `fix(core):`, `docs:`, `build:`, `ci:`, `chore:`. The crate scope is informative — use it when the change is crate-specific.
- **Branch naming** off `main`: `feat/<slug>`, `fix/<slug>`, `docs/<slug>`, `build/<slug>`, `hotfix/<slug>`.
- **`CHANGELOG.md`** — update `## [Unreleased]` for any user-visible change (categories: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`). The CLI surface (flags, keybindings, visible output, exit codes) is the stability boundary; the `rostop-core` Rust API is internal and may change at will pre-1.0.
- **No emojis** in code, comments, or commit messages.
- **No "what" comments** — only write a comment if the *why* is non-obvious (a hidden constraint, a workaround for a specific bug, behavior that would surprise a reader).
- PRs that touch the live backend get extra scrutiny — include reproduction steps and ideally a screenshot/asciicast.

## Files worth knowing

- `Justfile` — single source of truth for dev commands; mirror new recipes across both distros.
- `scripts/dev.sh` — picks Dockerfile, image tag, target volume, and `setup.bash` based on `$ROSTOP_DISTRO`. Refuses to run if it's unset.
- `Dockerfile.jazzy` / `Dockerfile.humble` — dev images. Adding `Dockerfile.<distro>` is the supported way to add distro support; no other code changes required for builds.
- `crates/rostop-cli/build.rs` — stamps `ROS_DISTRO` / `RMW_IMPLEMENTATION` into the binary so error messages can name the build target.
- `CLAUDE.local.md` — gitignored; use for personal scratch notes you don't want to commit.
