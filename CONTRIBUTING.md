# Contributing to rostop

Thanks for your interest. rostop is currently maintained by a single person; expect a few days of response time on issues and PRs. The bar for getting changes merged is "passes CI, fits the project's scope, and doesn't break the demo or live backends."

## Scope

rostop is a terminal UI for inspecting a running ROS 2 graph — like `htop`, for robots. Changes that move toward that goal are welcome:

- Better metrics (rate, bandwidth, jitter, latency, QoS health).
- Better inspector decoding (more message types, more compact rendering of large arrays).
- Better keybindings, layout, and ergonomics for live debugging.
- Support for additional ROS 2 distros (drop a `Dockerfile.<distro>` + mirror the recipe block).

Out of scope (for now): recording/replay (planned, but see roadmap), service callers, parameter editors, and anything that turns rostop into a general-purpose ROS GUI. The principle is "passive inspector, htop-shaped."

## Reporting bugs and suggesting features

Open an issue. For bugs, please include:

- rostop version (`rostop --version`) and which build (Humble / Jazzy / local cargo).
- ROS 2 distro and `RMW_IMPLEMENTATION` on the system you're inspecting.
- Output of `ros2 topic list` (or at least the topic that triggered the bug).
- Steps to reproduce and what you expected to see.

For features, open an issue first to discuss before writing code — it avoids wasted effort if the idea is out of scope.

## Development environment

The repo doesn't pick a default ROS distro. Every `just` recipe is suffixed with the target distro. Pick the one that matches the robot you're testing against:

```bash
just image-jazzy          # build the Jazzy dev image
just test-jazzy           # cargo test --workspace inside the Jazzy container
just run-jazzy -- --demo  # launch the TUI with the fabricated 6-topic demo

just image-humble         # same for Humble
just test-humble
just run-humble
```

A plain `cargo run -- --demo` also works if you have ROS 2 and Rust 1.88+ installed locally. Docker is for reproducibility, not a requirement.

Architecture rule worth knowing before you start: **`crates/rostop-core` has no ROS dependency.** Pure-logic primitives (stats, registry, sparkline, message tree) live there. Anything that touches `r2r` or DDS goes in `crates/rostop-cli` behind the `live` cargo feature. Don't pull ROS into `core`.

## Pull requests

1. **Fork the repo**, create a feature branch off `main`:
   - `feat/<short-slug>` for new features.
   - `fix/<short-slug>` for bug fixes.
   - `docs/<short-slug>` for documentation-only changes.
   - `build/<short-slug>` for Dockerfiles, Justfile, CI changes.
   - `hotfix/<short-slug>` for urgent fixes against a release.
2. **Make focused commits.** rostop uses [Conventional Commits](https://www.conventionalcommits.org/) — `feat(cli):`, `fix(core):`, `docs:`, `build:`, `ci:`, `style:`, `chore:`. Scoped variants are encouraged when the change is crate-specific.
3. **Run the tests** that apply to your change:
   - `just test-jazzy` and `just test-humble` for cross-distro changes.
   - `just test-core-<distro>` for `rostop-core` changes (fastest feedback, no ROS link required).
   - The `--features live` integration tests run inside the test recipes automatically.
4. **Update `CHANGELOG.md`** if your change is user-visible. Add an entry under `## [Unreleased]` in the appropriate category (`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`).
5. **Open a PR** against `main`. CI must be green before merge. The maintainer (only) merges via squash.

PRs that touch the live backend are reviewed extra carefully because they affect real ROS systems — please include reproduction steps and, ideally, a short asciicast or screenshot.

## Coding conventions

- `cargo fmt --all` before commit. `just fmt-<distro>` does it inside the container.
- `cargo clippy --workspace --all-targets -- -D warnings` must pass. `just clippy-<distro>` checks this.
- No emojis in code or commit messages.
- No comments that just describe what the code does — only add a comment if the *why* is non-obvious.
- Prefer adding a unit test in `rostop-core` over an integration test if the logic can be exercised without ROS.

## Stability boundary

rostop is at 0.x. Per [SemVer](https://semver.org/spec/v2.0.0.html), anything may change at any time before 1.0. In practice:

- The **CLI surface** (flags, keybindings, visible output format, exit codes) is the user-facing API. Breaking changes here go in a minor version bump and a `### Changed` CHANGELOG entry.
- The **`rostop-core` Rust API** is internal. It is not published to crates.io and may change in any release.
- The **on-wire interaction with ROS** uses standard `r2r` / DDS calls and is not a stability surface we control.

## License of contributions

By submitting a PR, you agree that your contribution is licensed under the [Apache License 2.0](LICENSE), the same license as the rest of the project. rostop does not currently require a CLA.
