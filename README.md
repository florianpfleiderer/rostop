# rostop

> Interactive TUI for inspecting and debugging ROS 2 topics — like `htop`, for robots.

`rostop` is a fast, terminal-native, SSH-friendly tool to inspect a running ROS 2 system: live topic list with rate / bandwidth / jitter, drill-in message inspector with decoded fields and sparklines, filter / search / record. Built in Rust with [`ratatui`](https://github.com/ratatui-org/ratatui) and [`rclrs`](https://github.com/ros2-rust/ros2_rust).

> Status: **early development** — core stats and registry are in place; UI and rclrs adapter coming next.

## Why

`rqt` is heavy and Qt-bound. Foxglove is great but is an Electron app and overkill for "what's actually publishing right now?". `ros2 topic` is slow and one-shot. `rostop` aims to fill the gap: open it, see the whole system, hit `Enter` on a topic, get instantly readable diagnostics — all over plain SSH.

## Build & Run

A `Dockerfile` is included with ROS 2 Jazzy + Rust toolchain pre-installed so the dev environment is reproducible across macOS / Linux / Windows.

```bash
just image     # build the dev container (first time only)
just test      # cargo test --workspace, inside the container
just run --demo  # run the TUI in demo mode (no ROS install needed)
```

If you have `cargo` and ROS 2 Jazzy installed locally, plain `cargo` works too.

## Layout

- `crates/rostop-core` — pure-logic primitives (stats, registry, rendering helpers). No ROS 2 dependency. Fully unit-tested.
- `crates/rostop-cli` — the binary: `ratatui` UI + `rclrs` adapter.

## License

Apache-2.0
