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

### Running against a real ROS 2 system

The `just run-live` recipe launches the container with `--network=host` and `--ipc=host`, so DDS discovery reaches the topics on your robot or workstation just like a native install would.

```bash
just run-live              # uses host's ROS_DOMAIN_ID + RMW
just run-live --some-flag  # extra args forwarded to the rostop binary
```

Environment variables (read from the calling shell, forwarded into the container):

| Variable             | Default               | Notes                                                                                       |
| -------------------- | --------------------- | ------------------------------------------------------------------------------------------- |
| `ROS_DOMAIN_ID`      | `0`                   | Must match the system you want to observe.                                                  |
| `RMW_IMPLEMENTATION` | `rmw_cyclonedds_cpp`  | Set to match the host's DDS vendor. The image ships CycloneDDS; Fast DDS would need a rebuild. |
| `CYCLONEDDS_URI`     | unset                 | Optional. Path/inline XML for a CycloneDDS config — needed only if you require unicast peers or non-default interfaces. |
| `ROS_LOCALHOST_ONLY` | `0`                   | Set to `1` to restrict discovery to localhost (useful for testing on the same machine).     |

Caveats:

- `--network=host` is Linux-only. On macOS / Windows Docker Desktop, host networking does not bridge to the LAN; use a native install or run the container inside a Linux VM that's on the robot's network.
- If your host runs Fast DDS and you can't switch it to CycloneDDS, change `RMW_IMPLEMENTATION` *and* install the matching `ros-jazzy-rmw-fastrtps-cpp` package in the Dockerfile.
- Multicast must reach between host and target. Different subnets / restrictive switches break discovery — fall back to `CYCLONEDDS_URI` with explicit unicast peers.

Sanity check from inside the container (`just shell`, then):

```bash
ros2 topic list   # should show the topics your robot is publishing
```

If that's empty, rostop will be empty too — fix discovery first.

## Keybindings

| Key            | Action                            |
| -------------- | --------------------------------- |
| `j` / `↓`      | move selection down               |
| `k` / `↑`      | move selection up                 |
| `g` / `G`      | jump to top / bottom              |
| `/`            | edit filter (Esc clears, Enter confirms) |
| `s`            | cycle sort key (Hz → BW → Type → Name) |
| `r`            | reverse sort order                |
| `p`            | pause / resume sample ingestion   |
| `q` / `Ctrl-C` | quit                              |

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
