# rostop

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

```bash
git clone git@github.com:florianpfleiderer/rostop.git
cd rostop
just image                # build the Docker dev env (ROS 2 Jazzy + Rust 1.88)
just test                 # cargo test --workspace, all green
just run --demo           # launches the TUI with a fabricated 6-topic system
```

If `cargo` + ROS 2 Jazzy are already installed locally, plain `cargo run -- --demo` works too — Docker is just for reproducibility.

### Which build do I need?

`r2r` (rostop's ROS 2 client library) links against the system `rcl`/`rmw` headers at build time, so the binary is locked to one ROS 2 distro and one RMW. To talk to a robot you need a rostop build that matches it:

| Robot runs                       | Use this                                  |
| -------------------------------- | ----------------------------------------- |
| Jazzy + CycloneDDS               | `just run-live` (default — `Dockerfile`)        |
| Humble + Fast DDS                | `just run-live-humble` (`Dockerfile.humble`)    |
| something else                   | add a `Dockerfile.<distro>` and a matching recipe |

Same source tree, different build container. `scripts/dev.sh` picks the right Dockerfile / image tag / `setup.bash` based on `ROSTOP_DISTRO` (default `jazzy`); each distro gets its own `target/` volume so cached artifacts don't collide. The compiled binary stamps `ROS_DISTRO` and `RMW_IMPLEMENTATION` from the build env (`build.rs`) and quotes them back in error messages, so you can tell which build you're holding without running `--version`.

### Running against a real ROS 2 system

The `just run-live` / `just run-live-humble` recipes launch the container with `--network=host` and `--ipc=host`, so DDS discovery reaches the topics on your robot or workstation just like a native install would.

```bash
# Jazzy (CycloneDDS) — default
just run-live              # uses host's ROS_DOMAIN_ID + RMW
just run-live --some-flag  # extra args forwarded to the rostop binary

# Humble (Fast DDS)
just run-live-humble
```

Environment variables (read from the calling shell, forwarded into the container):

| Variable                 | Default (Jazzy)       | Default (Humble)        | Notes                                                                                       |
| ------------------------ | --------------------- | ----------------------- | ------------------------------------------------------------------------------------------- |
| `ROS_DOMAIN_ID`          | `0`                   | `0`                     | Must match the system you want to observe.                                                  |
| `RMW_IMPLEMENTATION`     | `rmw_cyclonedds_cpp`  | `rmw_fastrtps_cpp`      | Set to match the host's DDS vendor.                                                         |
| `CYCLONEDDS_URI`         | unset                 | n/a                     | Optional. Path/inline XML for a CycloneDDS config — needed only if you require unicast peers or non-default interfaces. |
| `ROS_LOCALHOST_ONLY`     | `0`                   | `0`                     | Set to `1` to restrict discovery to localhost (useful for testing on the same machine).     |
| `ROSTOP_SKIP_PEER_PROBE` | unset                 | unset                   | Set to `1` to skip the 2 s startup peer probe (useful when peers are slow to come up).      |

Caveats:

- `--network=host` is Linux-only. On macOS / Windows Docker Desktop, host networking does not bridge to the LAN; use a native install or run the container inside a Linux VM that's on the robot's network.
- Cross-distro / cross-RMW peers don't work — the peer probe will refuse to start with a diagnostic naming the build target. Pick the matching recipe instead of overriding `RMW_IMPLEMENTATION`.
- Multicast must reach between host and target. Different subnets / restrictive switches break discovery — fall back to `CYCLONEDDS_URI` with explicit unicast peers (Jazzy) or a similar Fast DDS peer-list XML (Humble).

Sanity check from inside the container (`just shell` / `just shell-humble`, then):

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
| `l` / `→` / `Enter` | step **into** the selected item: from the topic table moves focus to the inspector; from the inspector descends into the selected struct/array field |
| `h` / `←`      | step **out**: pop one inspector level, or return focus to the topic table when already at the message root |
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

Run them yourself with `just test` (Docker) or `cargo test --workspace` (local).

## Roadmap

- **Field-level inspector for live topics** — v0.1.0 uses `subscribe_raw` for accurate Hz/BW/jitter without per-message decode cost. The inspector pane shows `DynamicValue::Bytes(len)` for live topics; on-demand decoded subscription for the currently selected topic is the next step.
- **Recording / replay** — `:rec <topic>` writes a small `.mcap` from selected topics.
- **Service caller & param editor** panes (`F2` / `F3`).
- **Node-graph mini-map** showing the live pub→sub graph for the selected topic, inspired by `rqt_graph` but live and animated.
- **`htop`-style colour theme + config file** (`~/.config/rostop/config.toml`).

## License

Apache-2.0
