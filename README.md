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
   │ (always works,   │                       │ (r2r / rclrs, runs   │
   │  no ROS install) │                       │  next to a real      │
   │                  │                       │  ROS 2 system)       │
   └──────────────────┘                       └──────────────────────┘
```

- `crates/rostop-core` — pure-logic primitives. No ROS dependency. 25 unit tests cover Hz / BW / jitter computation, sample eviction, registry CRUD + sort + filter, sparkline rendering, and dynamic message tree flattening.
- `crates/rostop-cli` — the binary. ratatui rendering, key handling, demo backend, and the integration test that renders to a `TestBackend` buffer and asserts the topic table contains expected strings.

## Test summary

```
crates/rostop-core   25 unit tests   stats, registry, sparkline, message
crates/rostop-cli     8 unit tests   demo backend, table row builder, fmt helpers
crates/rostop-cli     2 integration  full app + render → TestBackend buffer
                                     ───
                                     35 tests, all green
```

Run them yourself with `just test` (Docker) or `cargo test --workspace` (local).

## Roadmap

- **Live ROS 2 backend** via [`r2r`](https://github.com/sequenceplanner/r2r) — graph discovery, dynamic subscription with runtime introspection, no message codegen needed. The `RosBackend` trait is already in place; only the live impl is missing.
- **Recording / replay** — `:rec <topic>` writes a small `.mcap` from selected topics.
- **Service caller & param editor** panes (`F2` / `F3`).
- **Node-graph mini-map** showing the live pub→sub graph for the selected topic, inspired by `rqt_graph` but live and animated.
- **`htop`-style colour theme + config file** (`~/.config/rostop/config.toml`).

## License

Apache-2.0
