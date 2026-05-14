# Changelog

All notable changes to rostop are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Inspector pane now shows an "(idle — no messages in Ns · P pub / S sub)"
  indicator for topics that have been known to the graph for ≥ 3 s without
  producing a message (e.g. `/parameter_events`), replacing the ambiguous
  "(no message yet)" placeholder once the subscription has clearly settled.
- New `DynamicValue::ArrayElided(len)` variant in `rostop-core::message`,
  rendered as `[N items, elided]`. The live backend's `json_to_dynamic` uses
  it to summarise large primitive arrays (`Image::data`, `PointCloud2::data`,
  `LaserScan::ranges` over 4096 elements, etc.) instead of materialising
  millions of `DynamicValue::U64`s per frame. Small drillable arrays
  (`TFMessage::transforms`, `JointState::position`, anything ≤ 4096) are
  unchanged.
- GitHub Releases workflow now builds Humble (Ubuntu 22.04) **and** Jazzy
  (Ubuntu 24.04) artifacts on every `v*` tag — each as both a `.tar.gz` and
  a `.deb`, all built with the `live` feature enabled. The release notes
  embed copy-pasteable install snippets and a combined `SHA256SUMS`.
- CI now gates both Humble and Jazzy via a build matrix.

### Changed

- Live backend now decodes message payloads via `r2r::WrappedNativeMsgUntyped`,
  so the inspector pane shows the actual field tree (header, arrays, nested
  structs) for live topics instead of a single `DynamicValue::Bytes(len)`
  scalar. Wire-byte counts still come from `subscribe_raw`, so Hz / BW /
  jitter remain accurate. Topics whose message type was not linked in at
  build time fall back to the previous `Bytes(len)` placeholder.
- Live-feature integration tests now run with `--test-threads=1` so multiple
  `LiveBackend` instances do not race on the shared `ROS_DOMAIN_ID`.

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
