# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build, test, run

All developer workflows route through `just` recipes that run inside per-distro Docker dev containers. **There is no default distro** — every recipe is suffixed with `-jazzy` or `-humble`, and `scripts/dev.sh` refuses to run unless `ROSTOP_DISTRO` is set. Pick whichever distro matches the robot you're testing against; for pure-core work either is fine.

```bash
just image-jazzy            # build the Jazzy dev image (idempotent, ~10 min cold)
just test-jazzy             # cargo test --workspace inside the container
just test-core-jazzy        # only rostop-core (no ROS link — fastest feedback)
just build-jazzy
just run-jazzy --demo       # fabricated 6-topic demo, no ROS traffic needed
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

## AI-assisted workflow

The maintainer drives issue work through Claude Code with the `superpowers` skill set. Skills are auto-discovered each session (see the `using-superpowers` skill loaded at startup) — this section codifies *when* to reach for them on this repo.

### Per-issue loop

1. **One feature branch per GitHub issue.** Name it after the change type, not the issue number: `feat/<slug>`, `fix/<slug>`, `docs/<slug>`, `build/<slug>`, `hotfix/<slug>`. Use `gh issue list` to see what's open.
2. **Work in a git worktree** when the issue is non-trivial or touches files you have open elsewhere. Invoke the `using-git-worktrees` skill — it sets up an isolated checkout so the main workspace stays clean and parallel issue work doesn't cross-contaminate.
3. **Atomic commits.** Each commit should be a single coherent change that builds and passes tests on its own. Don't bundle "fix bug + reformat + rename" into one commit; reviewers (and `git bisect`) will thank you. Conventional Commits (`feat(cli):`, `fix(core):`, …) — scope to the crate when applicable.
4. **Open the PR early** (`gh pr create`) and push follow-up commits to it. Don't accumulate a 10-commit branch locally before anyone sees it.
5. **Update `CHANGELOG.md`** under `## [Unreleased]` for any user-visible change *in the same PR*, not a follow-up.

### Which skill, when

- **Starting a new feature or non-trivial change** → `brainstorming` first. Don't jump to code on a "let's add X" request; the skill exists because rostop's scope is deliberately narrow ("passive inspector, htop-shaped" — see CONTRIBUTING.md). Confirm the change fits the project's scope and the two-crate split before writing.
- **Multi-step implementation** → `writing-plans` to draft, `executing-plans` (or `subagent-driven-development` if tasks are independent) to carry it out. Plans are cheap insurance against half-finished refactors.
- **Bug investigation** → `systematic-debugging` before proposing a fix. Especially important for live-backend bugs where the bug might be in r2r, DDS, the RMW, or our code — guessing wastes time.
- **Independent searches or refactors** → `dispatching-parallel-agents` to run them concurrently. Skill choice here is purely about parallelism, not correctness.
- **Before claiming done** → `verification-before-completion`. For rostop, "done" means the *specific* commands in the next subsection have been run and shown green.
- **Before merging** → `requesting-code-review` on the branch. Catches the "looks fine to me" failure mode.
- **TDD-shaped changes in `rostop-core`** → `test-driven-development`. Pure-logic code with no ROS dependency is the ideal TDD target; prefer it over integration tests in `rostop-cli` whenever the logic can be exercised without ROS.

### Verification gates

"All tests pass" is not a thing you can claim without running these:

| Scope                      | Command                                                  |
| -------------------------- | -------------------------------------------------------- |
| Touched `rostop-core` only | `just test-core-<distro>`                                |
| Touched `rostop-cli`       | `just test-<distro>` (runs the live integration tests)   |
| Cross-distro change        | Both `just test-jazzy` *and* `just test-humble`          |
| Always                     | `just fmt-<distro>` and `just clippy-<distro>` (`-D warnings`) |

CI runs both distros on every PR. Don't skip the matching local run just because CI will catch it — the feedback loop is much faster locally, and you stay out of the "push, wait, push, wait" anti-pattern.

### What to avoid

- **Don't pull `r2r` or any `ros-*` crate into `rostop-core`.** The boundary is the point of the architecture; once broken it stays broken. If you need ROS state in core code, the design is wrong — add the data to `BackendEvent` and let the UI plumb it through.
- **Don't override `RMW_IMPLEMENTATION`** to make cross-distro / cross-RMW peers "work." The peer probe will refuse, and for good reason — the binary is linked against one specific stack. Pick the matching `just run-<distro>` recipe instead.
- **Don't add features behind a feature flag without thinking** — the only feature gate is `live`, and it exists because the entire `r2r` toolchain is heavy. New cargo features need a real reason.
- **Don't write planning / decision / "what I'm about to do" markdown files.** Work from conversation context. The exceptions are `CHANGELOG.md` (user-facing), and explicit plan documents the user asks for (saved under `docs/superpowers/plans/`).

### Ask before running — costly or side-effect-y commands

The `.claude/settings.json` allowlist exists so trivial reads (status / log / `cargo check`) don't prompt. It is **not** a license to run anything that fits the glob pattern. The commands below have outsized side effects on this repo specifically — confirm with the user before invoking them, even if a glob technically permits it.

- **`just image-<distro>`** — builds the Jazzy/Humble dev image from scratch. ~10 minutes cold, downloads several hundred MB. Never run this speculatively; ask whether the user has it built already (`docker image inspect rostop-dev:<distro>` is read-only and fine).
- **`just test-<distro>`, `just build-<distro>`, `just clippy-<distro>`, `just fmt-<distro>`** — all route through `scripts/dev.sh` and will trigger `docker build` if the image isn't present. Once the image exists they're cheap, but cold-start cost is hidden behind the recipe name. Ask before the first invocation of a session, or run `docker image inspect rostop-dev:<distro>` first to confirm the image exists.
- **`just run-<distro>`** (without `--demo`) — opens the TUI inside a `--network=host --ipc=host` container and connects to whatever real ROS 2 system the user has on their LAN. It takes over the terminal, may interfere with debugging the user is doing on the same robot, and is not something to run "to check." Use `--demo` if you genuinely need to exercise the binary; otherwise ask.
- **`just package-<distro>`** — full release build of the `.deb`. Long compile, writes to `dist/`. Only run when the user is preparing a release.
- **`cargo clean` / `just clean-<distro>`** — wipes the per-distro `rostop-target-<distro>` Docker volume. The next build will be cold and slow. Don't run this to "reset" something — diagnose the actual problem first.
- **`cargo update`** — touches `Cargo.lock` and can cascade into unintended dep bumps across the workspace. Only run when the user has explicitly asked to update dependencies.
- **`docker build`, `docker volume rm`, `docker system prune`, `docker rmi`** — all destructive to the cached dev environment. The user has spent real time building these images and volumes; don't blow them away.
- **Any `gh` command that writes** (`gh pr create`, `gh pr merge`, `gh pr comment`, `gh issue create`, `gh issue close`, `gh release create`) — visible to others. The allowlist permits `gh issue/pr/run view/list`, which are read-only; everything else needs explicit user go-ahead.
- **`git push`, `git push --force`, `git reset --hard`, `git rebase`, `git branch -D`, `git checkout -- <files>`** — none are in the allowlist, but listed here so you don't ask "why didn't this work" — they need explicit approval. Force-push and reset are off-limits unless the user names them specifically.
- **Long-running background processes** (`cargo run`, `cargo watch`, anything that doesn't terminate on its own) — start them only when the user has asked for a running process, and always tell them you've done it so they can clean it up.
- **Modifying `Cargo.lock` by hand, files in `dist/`, files in `target/`** — `Cargo.lock` is managed by cargo, `dist/` is release artifacts, `target/` is build output. None are hand-edited.
- **Editing `CHANGELOG.md` sections other than `## [Unreleased]`** — released-version entries are historical record. Only add to `[Unreleased]`.

Rule of thumb: if you're about to run something that takes more than ~30s, opens a network socket, mutates shared state (git remote, GitHub, Docker volumes), or holds the terminal, **ask first**. Reading the code, running `cargo check`, running `rg`/`fd`, and reading PR/issue state never need permission.

### Memory and `docs/superpowers/plans/`

`docs/superpowers/plans/` is where plans created via the `writing-plans` skill get committed when they outlive a single session (e.g. multi-day refactors). Most plans are session-scoped and don't go here. Personal session memory lives outside the repo (in `~/.claude/projects/`) — don't commit that.
