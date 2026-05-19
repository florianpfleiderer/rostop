# Changelog

All notable changes to rostop are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Status-bar mismatch hint.** When a `subscribe_raw` payload fails to decode
  against the type-support compiled into this build (the signature of a ROS 2
  distro / RMW mismatch — e.g. a Jazzy-built rostop subscribed to a Humble
  peer), the status bar shows a single sticky `INFO: possible distro/RMW
  mismatch — built against {target_distro}+{target_rmw}, some samples failed
  to decode` line. Emitted at most once per process so a torrent of foreign
  samples doesn't churn the message.
- **Fullscreen single-topic view.** Pressing `Enter` on a row in the topic
  table now hides the split-pane layout and dedicates the whole terminal to
  the selected topic: large metrics block (Hz, BW, jitter, pub/sub counts,
  idle seconds), wider Hz and BW sparklines, and a full-width decoded
  message tree with the same `j`/`k`/`l`/`h` drill keys as the inspector
  pane. `Esc` returns to the split-pane layout, preserving the drill path.
  Publisher / subscriber lists and per-topic QoS are not surfaced yet —
  follow-ups to issue #15.
- **Publisher list with QoS in the fullscreen panel.** The fullscreen
  single-topic view now lists each publisher with node name, namespace,
  reliability + durability + history policy, liveliness setting, and a
  short hex prefix of the endpoint GID. Subscribers render as
  `(not available)` for now — r2r 0.9.5 only exposes the publisher side
  of the graph, so the subscriber lookup will follow once upstream r2r
  gains the symmetric API.

### Changed

- **Startup is now non-blocking.** Removed the 2 s peer-mismatch probe that
  refused to open the TUI when foreign publishers were seen with zero decoded
  samples. The check misfired on idle systems (e.g. the ROS 2 daemon's
  `/rosout` with no traffic) and was unhelpful even when correct — rostop is
  a topic viewer, not a diagnostics gate. The `ROSTOP_SKIP_PEER_PROBE` env
  var is gone with it (no probe means no skip needed). Distro/RMW mismatches
  are now surfaced passively via the new INFO hint above.
- **`.deb` release-artifact filenames** now put the version before the
  distro, matching the tarball convention. The new pattern is
  `rostop-<version>-<distro>_1_amd64.deb` (was
  `rostop-<distro>_<version>-1_amd64.deb`). The internal apt package name
  is still `rostop-humble` / `rostop-jazzy` — only the filename changes,
  so `apt install ./<file>.deb` and `apt list --installed` work
  identically. Install scripts that pin the old filename will need a
  one-line update; the new URL is in the release notes and README.

### Fixed

- **Topic table auto-scrolls to keep the selected row visible.** Previously,
  on a terminal too short to display every discovered topic, pressing `j`/`G`
  past the last visible row moved `app.selected` off-screen and the
  highlight disappeared. The table is now rendered via
  `render_stateful_widget` with a `TableState` that tracks the selected
  index, so ratatui scrolls the viewport to follow the cursor.

## [0.1.0] - 2026-05-14

First public release. Inspect a live ROS 2 graph from the terminal.

### Added

- **`LiveBackend`** (cargo feature `live`) — backed by [r2r 0.9](https://github.com/sequenceplanner/r2r).
  Discovers topics via `get_topic_names_and_types`, subscribes with
  `subscribe_raw` for accurate wire-byte counts, and decodes payloads via
  `r2r::WrappedNativeMsgUntyped` so the inspector pane shows the actual field
  tree (header, arrays, nested structs). Events flow to the UI thread over an
  `std::sync::mpsc` channel; a single dedicated OS thread owns the node and
  runs `spin_once` + a `futures::executor::LocalPool` for stream draining.
- **ROS 2 Humble and Jazzy support** — per-distro Dockerfiles, mirrored `just`
  recipes (`just <verb>-humble` / `just <verb>-jazzy`), and no default distro:
  every recipe is suffixed with the target. Adding a new distro is dropping a
  `Dockerfile.<distro>` and mirroring the recipe block.
- **`just run-<distro>`** recipes — run the dev image with `--network=host
  --ipc=host` and forward `ROS_DOMAIN_ID` / `RMW_IMPLEMENTATION` /
  `CYCLONEDDS_URI` from the caller's shell, so DDS discovery reaches the
  host's topics. Append `-- --demo` to swap in the fabricated backend.
- **Inspector field tree** with `DynamicValue::ArrayElided(len)` rendered as
  `[N items, elided]` for large primitive arrays (`Image::data`,
  `PointCloud2::data`, `LaserScan::ranges` over 4096 elements), so big sensor
  topics stay responsive. Small drillable arrays (`TFMessage::transforms`,
  `JointState::position`, anything ≤ 4096) remain expandable.
- **Inspector idle indicator** — shows `(idle — no messages in Ns · P pub /
  S sub)` for topics known to the graph for ≥ 3 s without producing a
  message (e.g. `/parameter_events`), replacing the ambiguous
  "(no message yet)" placeholder once the subscription has clearly settled.
- **GitHub Releases workflow** — fires on `v*` tags, builds Humble
  (Ubuntu 22.04) and Jazzy (Ubuntu 24.04) artifacts as both `.tar.gz` and
  `.deb`, all with the `live` feature enabled. Release notes embed
  copy-pasteable install snippets and a combined `SHA256SUMS`.
- **CI gate for the live feature** — `cargo test -p rostop-cli --features
  live` runs three integration tests against `ros2 topic pub` inside both
  Humble and Jazzy containers via a build matrix. Live-feature tests run
  with `--test-threads=1` so multiple `LiveBackend` instances do not race on
  the shared `ROS_DOMAIN_ID`.

### Known limitations

- The live backend uses fixed `QosProfile::default()` for all subscriptions.
  Sensor topics published with `BEST_EFFORT` reliability still appear in the
  graph but may not deliver samples until QoS negotiation is exposed.
- Topic removal cleans up registry state but does not eagerly cancel the
  per-topic forwarder task; it exits on the next stream error or process
  shutdown.
- Topics whose message type was not linked in at build time fall back to a
  `DynamicValue::Bytes(len)` placeholder in the inspector pane.

[Unreleased]: https://github.com/florianpfleiderer/rostop/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/florianpfleiderer/rostop/releases/tag/v0.1.0
