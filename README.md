# rostop

[![CI](https://github.com/florianpfleiderer/rostop/actions/workflows/ci.yml/badge.svg)](https://github.com/florianpfleiderer/rostop/actions/workflows/ci.yml)
[![Release](https://github.com/florianpfleiderer/rostop/actions/workflows/release.yml/badge.svg)](https://github.com/florianpfleiderer/rostop/actions/workflows/release.yml)
[![Rust 1.88](https://img.shields.io/badge/Rust-1.88-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![ROS 2 Humble](https://img.shields.io/badge/ROS%202-Humble-22314E?logo=ros&logoColor=white)](https://docs.ros.org/en/humble/)
[![ROS 2 Jazzy](https://img.shields.io/badge/ROS%202-Jazzy-22314E?logo=ros&logoColor=white)](https://docs.ros.org/en/jazzy/)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](#license)

> Interactive TUI for inspecting and debugging ROS 2 topics — like `htop`, for robots.

`rostop` is a fast, terminal-native tool for inspecting a running ROS 2 system: live topic list with rate / bandwidth / jitter, drill-in message inspector with decoded fields and sparklines, filter, sort, search. Built in Rust with [`ratatui`](https://github.com/ratatui-org/ratatui) and a swappable backend trait.

```text
┌ rostop ─ demo ─ 6 topics ──────────────────────────────────────────────────────────────────────────────────┐
│ TOPIC                         HZ       BW          JIT(ms)  TYPE                                     P/S   │
│▸ /cmd_vel                       99.5    4.7 KB/s     12.1   geometry_msgs/msg/Twist                  1/1   │
│  /tf                            49.8   10.8 KB/s     12.4   tf2_msgs/msg/TFMessage                   3/4   │
│  /odom                          49.8   35.0 KB/s     12.4   nav_msgs/msg/Odometry                    1/2   │
│  /scan                          40.0  113.3 KB/s     19.3   sensor_msgs/msg/LaserScan                1/2   │
│  /camera/image_raw              29.9   78.7 MB/s     31.9   sensor_msgs/msg/Image                    1/1   │
│  /diagnostics                    1.0    1.4 KB/s      0.0   diagnostic_msgs/msg/DiagnosticArray      5/1   │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
┌ inspector ─ /cmd_vel ──────────────────────────────────────────┐┌ rates ─ /cmd_vel ────────────────────────┐
│▾ linear                                                        ││Hz       99.5            ▂▃▄▅▅▆▇▇▇█▇█████│
│  · x: 0.4994679123311691                                       ││BW      4.7 KB/s         ▂▃▄▅▆▇▇▇▇████▇██│
│  · y: 0                                                        ││JIT     12.1 ms                           │
│  · z: 0                                                        ││PUB/SUB 1/1                               │
│▾ angular                                                       ││                                          │
│  · x: 0                                                        ││(sparklines auto-scale to the highest samp│
│  · y: 0                                                        ││                                          │
│  · z: 0.2968074739870145                                       ││                                          │
└────────────────────────────────────────────────────────────────┘└──────────────────────────────────────────┘
[LIVE]  sort:Hz Descending   j/k:move  /:filter  s:sort  r:reverse  p:pause  g/G:top/bot  q:quit
```

## Why

`rqt` is heavy and Qt-bound, Foxglove is Electron, `ros2 topic` is one-shot and slow. None of them give the "system at a glance" experience of `htop` over plain SSH. `rostop` aims to fill that gap:

- Open it on a robot via SSH, see the entire DDS graph at 60 FPS.
- Hit `j/k` to scroll, watch sparklines for the selected topic fill in.
- Sort by Hz or bandwidth to find the slowpokes and the firehoses.
- Filter on type or name to focus on a subsystem.
- Inspect message contents with a decoded field tree (dynamic introspection — no `.msg` codegen required).

## Quick start

The repo doesn't pick a default ROS distro — every recipe is suffixed with the target distro (`-jazzy`, `-humble`, ...). Pick the one that matches the robot you're testing against.

```bash
git clone git@github.com:florianpfleiderer/rostop.git
cd rostop

# Jazzy + CycloneDDS (Ubuntu 24.04)
just image-jazzy          # build the Jazzy dev env (ROS 2 Jazzy + Rust 1.88)
just test-jazzy           # cargo test --workspace, all green
just run-jazzy -- --demo  # launches the TUI with a fabricated 6-topic system

# Humble + Fast DDS (Ubuntu 22.04)
just image-humble         # build the Humble dev env (ROS 2 Humble + Rust 1.88)
just test-humble          # cargo test --workspace, all green (Humble container)
just run-humble           # connect to a real Humble robot — see "Running against a real ROS 2 system" below
```

Adding a new distro means dropping a `Dockerfile.<distro>` next to the existing ones and mirroring the recipe block in the `Justfile` — no other code changes required.

If `cargo` + ROS 2 are already installed locally, plain `cargo run -- --demo` works too — Docker is just for reproducibility.

### Install (prebuilt artifacts)

Every tagged release publishes `.deb` and `.tar.gz` artifacts for each supported distro. Pick the one that matches your robot — currently `humble` (Ubuntu 22.04) or `jazzy` (Ubuntu 24.04).

```bash
TAG=v0.1.0                           # pick a release tag
VERSION="${TAG#v}"
DISTRO=humble                        # or jazzy

# Option A — .deb (recommended; apt pulls missing ros-<distro>-* message packages)
curl -L -o rostop-${DISTRO}.deb \
  https://github.com/florianpfleiderer/rostop/releases/download/${TAG}/rostop-${DISTRO}_${VERSION}-1_amd64.deb
sudo apt install ./rostop-${DISTRO}.deb

# Option B — tarball (assumes the ros-<distro>-* message packages are already installed)
curl -L https://github.com/florianpfleiderer/rostop/releases/download/${TAG}/rostop-${VERSION}-${DISTRO}-x86_64.tar.gz \
  | tar -xz
install -m755 rostop-${VERSION}-${DISTRO}-x86_64/rostop ~/.local/bin/

# Either way:
source /opt/ros/${DISTRO}/setup.bash
rostop
```

`rostop-humble` and `rostop-jazzy` both ship `/usr/bin/rostop` and declare a mutual `Conflicts:`, so only one can be installed on a given host.

### Building artifacts locally

If you need to produce artifacts off-CI (e.g. an unreleased branch), the `just` recipes wrap the same `cargo-deb` invocation:

```bash
just package-humble        # → dist/rostop-humble_<ver>-1_amd64.deb
just package-jazzy         # → dist/rostop-jazzy_<ver>-1_amd64.deb
```

### All Just recipes

Every distro has the same recipe block; replace `<distro>` with `jazzy` or `humble`.

| Recipe                       | What it does                                                                  |
| ---------------------------- | ----------------------------------------------------------------------------- |
| `just image-<distro>`        | Build the `<distro>` dev image (idempotent).                                  |
| `just shell-<distro>`        | Drop into an interactive shell inside the dev container with ROS 2 sourced.   |
| `just test-<distro>`         | `cargo test --workspace` inside the `<distro>` container.                     |
| `just build-<distro>`        | `cargo build --workspace` inside the `<distro>` container.                    |
| `just run-<distro>`          | Connect to a real ROS 2 system on the host (see below).                       |
| `just run-<distro> -- --demo` | Launch the TUI with the fabricated demo backend (no ROS traffic needed).     |
| `just fmt-<distro>` / `just clippy-<distro>` | Format + lint inside the `<distro>` container.                |
| `just clean-<distro>`        | `cargo clean` inside the `<distro>` container.                                |
| `just package-<distro>`      | Build the `<distro>` `.deb` into `dist/`.                                     |
| `just test-core-<distro>`    | Run only the `rostop-core` unit tests (no ROS link, fastest feedback).        |

All recipes route through `scripts/dev.sh`, which picks the Dockerfile, image tag (`rostop-dev:<distro>`), `target/` volume, and `setup.bash` based on `$ROSTOP_DISTRO`. There is no default — set it explicitly when calling the script directly: `ROSTOP_DISTRO=humble ./scripts/dev.sh "cargo <whatever>"`.

### Which build do I need?

`r2r` (rostop's ROS 2 client library) links against the system `rcl`/`rmw` headers at build time, so the binary is locked to one ROS 2 distro and one RMW. To talk to a robot you need a rostop build that matches it:

| Robot runs                       | Use this                                          |
| -------------------------------- | ------------------------------------------------- |
| Jazzy + CycloneDDS               | `just run-jazzy` (`Dockerfile.jazzy`)             |
| Humble + Fast DDS                | `just run-humble` (`Dockerfile.humble`)           |
| something else                   | add a `Dockerfile.<distro>` and a matching recipe |

Same source tree, different build container. `scripts/dev.sh` picks the Dockerfile / image tag / `setup.bash` based on `ROSTOP_DISTRO`; each distro gets its own `target/` volume so cached artifacts don't collide. The compiled binary stamps `ROS_DISTRO` and `RMW_IMPLEMENTATION` from the build env (`build.rs`) and quotes them back in error messages, so you can tell which build you're holding without running `--version`.

### Running against a real ROS 2 system

The `just run-<distro>` recipes launch the container with `--network=host` and `--ipc=host`, so DDS discovery reaches the topics on your robot or workstation just like a native install would. Append `-- --demo` to swap in the fabricated demo backend without rebuilding.

```bash
# Jazzy (CycloneDDS default)
just run-jazzy              # uses host's ROS_DOMAIN_ID + RMW
just run-jazzy -- --demo    # fabricated demo backend (no ROS traffic)
just run-jazzy --some-flag  # extra args forwarded to the rostop binary

# Humble (Fast DDS default)
just run-humble
```

Environment variables (read from the calling shell, forwarded into the container):

| Variable                 | Default (Jazzy)       | Default (Humble)        | Notes                                                                                       |
| ------------------------ | --------------------- | ----------------------- | ------------------------------------------------------------------------------------------- |
| `ROS_DOMAIN_ID`          | `0`                   | `0`                     | Must match the system you want to observe.                                                  |
| `RMW_IMPLEMENTATION`     | `rmw_cyclonedds_cpp`  | `rmw_fastrtps_cpp`      | Set to match the host's DDS vendor.                                                         |
| `CYCLONEDDS_URI`         | unset                 | n/a                     | Optional. Path/inline XML for a CycloneDDS config — needed only if you require unicast peers or non-default interfaces. |
| `ROS_LOCALHOST_ONLY`     | `0`                   | `0`                     | Set to `1` to restrict discovery to localhost (useful for testing on the same machine).     |

Caveats:

- `--network=host` is Linux-only. On macOS / Windows Docker Desktop, host networking does not bridge to the LAN; use a native install or run the container inside a Linux VM that's on the robot's network.
- Cross-distro / cross-RMW peers don't work — `r2r` is linked against one specific stack and CDR decode will fail on samples from peers built against another. rostop will still open and show the topic graph, and the status bar will surface a one-shot `INFO: possible distro/RMW mismatch …` hint once it sees a sample fail to decode. Pick the matching recipe instead of overriding `RMW_IMPLEMENTATION`.
- Multicast must reach between host and target. Different subnets / restrictive switches break discovery — fall back to `CYCLONEDDS_URI` with explicit unicast peers (Jazzy) or a similar Fast DDS peer-list XML (Humble).

Sanity check from inside the container (`just shell-jazzy` / `just shell-humble`, then):

```bash
ros2 topic list   # should show the topics your robot is publishing
```

If that's empty, rostop will be empty too — fix discovery first.

## Keybindings

| Key            | Action                            |
| -------------- | --------------------------------- |
| `j` / `↓`      | move selection down (in focused pane) |
| `k` / `↑`      | move selection up (in focused pane)   |
| `g` / `G`      | jump to top / bottom              |
| `l` / `→`      | step **into** the selected item: from the topic table moves focus to the inspector; from the inspector descends into the selected struct/array field |
| `h` / `←`      | step **out**: pop one inspector level, or return focus to the topic table when already at the message root |
| `Enter`        | from the topic table, open a **fullscreen** single-topic panel (bigger metrics, wider sparklines, full-width message tree). `Esc` returns to the split-pane layout |
| `/`            | edit filter (Esc clears, Enter confirms) |
| `s`            | cycle sort key (Hz → BW → Type → Name) |
| `r`            | reverse sort order                |
| `p`            | pause / resume sample ingestion   |
| `q` / `Ctrl-C` | quit                              |

The inspector pane only ever shows one level of the message tree at a time, so even very large structures (e.g. a `tf2_msgs/msg/TFMessage` with hundreds of transforms) stay readable. Drill in to a single `transforms[i]` to see just its fields; `h` pops back to the list. The currently focused pane is highlighted with a yellow border; the inactive pane keeps its cursor visible but dimmed.

## Architecture

```
                ┌────────────────────────────────────┐
                │            ratatui UI              │
                │ (app loop, layout, key handling)   │
                └──────────────────┬─────────────────┘
                                   │ reads
                ┌──────────────────▼─────────────────┐
                │             rostop-core            │
                │  TopicRegistry · TopicStats        │
                │  Sparkline · MessageTree           │
                │  (pure logic, no ROS dependency)   │
                └──────────────────▲─────────────────┘
                                   │ feeds events
            ┌──────────────────────┴──────────────────────┐
            │                                             │
   ┌────────▼─────────┐                       ┌───────────▼──────────┐
   │   DemoBackend    │                       │     LiveBackend      │
   │ (always works,   │                       │ (r2r, runs next to a │
   │  no ROS install) │                       │  real ROS 2 system,  │
   │                  │                       │  `--features live`)  │
   └──────────────────┘                       └──────────────────────┘
```

- `crates/rostop-core` — pure-logic primitives. No ROS dependency. 25 unit tests cover Hz / BW / jitter computation, sample eviction, registry CRUD + sort + filter, sparkline rendering, and dynamic message tree flattening.
- `crates/rostop-cli` — the binary. ratatui rendering, key handling, demo backend, and (gated behind the `live` cargo feature) the r2r-backed `LiveBackend` plus integration tests that drive `ros2 topic pub` against it.

## Test summary

```
crates/rostop-core   25 unit tests   stats, registry, sparkline, message
crates/rostop-cli     8 unit tests   demo backend, table row builder, fmt helpers
crates/rostop-cli     2 integration  full app + render → TestBackend buffer
                                     ───
                                     35 tests, all green
crates/rostop-cli   + 3 live tests   ros2 topic pub → LiveBackend (--features live)
```

Run them yourself with `just test-jazzy` / `just test-humble` (Docker) or `cargo test --workspace` (local).

## Roadmap

Shipped:

- [x] **Topic list with Hz / BW / jitter** + sparklines, filter, sort, search.
- [x] **Demo backend** — fabricated 6-topic system, runs anywhere with no ROS install.
- [x] **Live backend** — r2r-based, accurate wire-byte counts via `subscribe_raw`.
- [x] **Message inspector** — dynamic field-tree decoding, no `.msg` codegen required; large arrays elided so `Image` / `PointCloud2` stay responsive.
- [x] **Humble + Jazzy support** — per-distro Dockerfiles, mirrored `just` recipes, no default distro.
- [x] **Release artifacts** — `.deb` and `.tar.gz` for each distro published on every `v*` tag, CI-gated.

Planned:

- [ ] **Recording / replay** — `:rec <topic>` writes a small `.mcap` from selected topics.
- [ ] **Service caller & param editor** panes (`F2` / `F3`).
- [ ] **Node-graph mini-map** showing the live pub→sub graph for the selected topic, inspired by `rqt_graph` but live and animated.
- [ ] **`htop`-style colour theme + config file** (`~/.config/rostop/config.toml`).

## License

Apache-2.0
