# Changelog

All notable changes to rostop are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Default sort is now `Name` ascending** instead of `Hz` descending. On a
  busy system, similarly-rated topics swap places visibly every second and
  the user lands on a moving target the moment they open the TUI — alphabetical
  is a calmer starting view that lets them find a specific topic without
  fighting the sort. (Closes #18.)
- **`s` (cycle sort key) now also picks a sensible order for the new key**:
  `Name` and `Type` snap to ascending, `Hz` and `Bandwidth` snap to
  descending. The previous behaviour silently kept whatever order was
  active, which produced surprising results like "Hz ascending" (slowest
  topic first) right after cycling from Name.
- **Status-bar sort indicator uses filled triangles.** `sort:Hz▼` /
  `sort:Name▲` instead of `sort:Hz Descending` / `sort:Name Ascending`.
  Same information, ~10 columns shorter — keeps the help string readable
  in 80-column terminals. `▼` follows the htop / `top` convention (high
  values flow downward, i.e. listed first); filled triangles render
  more crisply than ↑/↓ arrows in low-quality terminal fonts.
- **Focus mode is now opened with `f`** (was `Enter`). The status-bar mode
  label is `[FOCUS]` and the panel title is `focus ─ {topic} ─ …`. Pressing
  `f` again exits focus mode (`Esc` still works too), so the user can
  toggle in and out without moving off the same key. The `Enter` key keeps
  its other meaning — descending one level inside the inspector pane.

### Removed

- **The `r` "reverse sort order" keybinding** and its help-bar hint.
  Sort direction is now derived from the sort key (see above) and no longer
  user-toggleable. If a per-key override becomes a real need, add it back
  behind a separate binding rather than re-exposing the global toggle.
- **The `/` filter feature.** The `/` keybinding, the `filter:` field on
  `App`, the `filter` parameter on `ui::rows::build_rows`, the
  `TopicRegistry::filtered` method in `rostop-core`, the `[FILTER: …]`
  status-bar mode and its trailing `filter:""` debug suffix, and the
  `build_rows_applies_filter` / `filter_by_substring_matches_name_or_type`
  tests are all gone. With sort defaulting to `Name` ascending and a
  manageable topic count on most systems, the substring filter was unused
  in practice; if a real need resurfaces, see how `r` came back as a
  potential per-key override above — keep the new binding scoped.
- **`g` / `G` jump-to-top / jump-to-bottom** in all three contexts
  (topic table, inspector pane, focus mode). The status-bar help string
  is shorter as a result; `j`/`k` still move one row at a time and the
  topic table auto-scrolls under the cursor, so the user never gets
  stranded off-screen.

### Added

- **Explicit ROS domain selection.** `rostop --domain <0-232>` overrides
  `ROS_DOMAIN_ID`, validates the DDS protocol range before startup, and shows
  the active domain in the topic-table title.
- **Waveform scope for numeric message fields.** Press `w` on a topic to open
  a full-terminal Braille line chart with live current/min/max/mean metrics,
  spike-preserving decimation, selectable nested numeric fields, 1–30 second
  windows, and switchable auto/locked Y scaling. The demo `/cmd_vel` signal
  includes both slow motion and controller ripple so the view is useful
  immediately without a ROS system.
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
- **Publisher and subscriber lists with per-endpoint QoS in the focus
  panel.** Focus mode lists every publisher and subscriber attached to
  the selected topic, with node name, namespace, reliability + durability
  + history policy, liveliness setting, and a short hex prefix of the
  endpoint GID. Subscribers depend on `Node::get_subscriptions_info_by_topic`
  — not in upstream r2r 0.9.5 yet — so a build of rostop without the
  patched r2r will continue to show subscribers as `(not available)`.
  Tracked by issue #29.

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

- **Live sample delivery is now bounded and fair to the UI.** High-rate topics
  can no longer grow the backend event queue without limit or force a frame to
  drain an arbitrary number of samples before handling terminal input.
- **Topics that change type are re-subscribed safely.** The previous
  subscription is cancelled, its UI state is removed, and a fresh subscription
  is created for the newly discovered type.
- **Publisher/subscriber counts now refresh with the ROS graph.** The topic
  table no longer retains its initial counts while endpoint details change.
- **Live backend now matches publisher QoS instead of always subscribing
  with the ROS 2 default.** Previously every subscription used
  `r2r::QosProfile::default()` (Reliable / Volatile), which silently failed
  to match BestEffort publishers — the topic still appeared via the graph
  poll, but no samples were ever delivered, so Hz / BW / jitter stayed
  blank. When the discovery layer reports publisher QoS, subscribe with a
  derived profile that drops to BestEffort if any publisher is BestEffort
  and to Volatile if any publisher is Volatile (TransientLocal is
  preserved when every publisher offers it, so latched topics like
  `/tf_static` still deliver their cached value). When per-publisher QoS
  is not yet available at first sighting, fall back to the previous
  default so the subscription still happens immediately.
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
