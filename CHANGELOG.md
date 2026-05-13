# Changelog

All notable changes to rostop are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-05-13

First public release. Inspect a live ROS 2 graph from the terminal.

### Added

- **`LiveBackend`** (cargo feature `live`) — backed by [r2r 0.9](https://github.com/sequenceplanner/r2r).
  Discovers topics via `get_topic_names_and_types`, subscribes with `subscribe_raw`
  for accurate wire-byte counts, and forwards events to the UI thread over an
  `std::sync::mpsc` channel. A single dedicated OS thread owns the node and
  runs `spin_once` + a `futures::executor::LocalPool` for stream draining.
- **`just run-live`** recipe — runs the dev image with `--network=host --ipc=host`
  and forwards `ROS_DOMAIN_ID` / `RMW_IMPLEMENTATION` / `CYCLONEDDS_URI` from
  the caller's shell, so DDS discovery reaches the host's topics.
- **GitHub Releases workflow** — fires on `v*` tags, builds
  `rostop-<version>-jazzy-x86_64.tar.gz` inside `ros:jazzy-ros-base`, publishes
  the tarball + sha256 with copy-pasteable install snippet.
- **CI gate for the live feature** — `cargo test -p rostop-cli --features live`
  runs three integration tests against `ros2 topic pub` inside the ROS Jazzy
  container.

### Known limitations

- Live topics show `DynamicValue::Bytes(len)` in the inspector pane (Hz / BW /
  jitter are accurate; field-level decoding for the selected topic is the
  headline v0.2 item).
- The live backend uses fixed `QosProfile::default()` for all subscriptions.
  Sensor topics published with `BEST_EFFORT` reliability still appear in the
  graph but may not deliver samples until QoS negotiation is exposed.
- Topic removal cleans up registry state but does not eagerly cancel the
  per-topic forwarder task; it exits on the next stream error or process
  shutdown.
