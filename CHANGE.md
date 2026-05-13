# Proposed changes — distro / RMW compatibility

Triggered by running `rostop --features live` (via `just run-live`) against a
Humble + `rmw_fastrtps_cpp` robot, which produced `Failed to parse type hash …
from USER_DATA '(null)'` on the rostop side and a flood of `sequence size
exceeds remaining buffer` on the robot side. Root cause: rostop is hard-pinned
to Jazzy + CycloneDDS via the Dockerfile and Justfile, so any cross-distro /
cross-RMW peer triggers CDR-level discovery mismatches.

## Done

### Peer probe at startup
File: `crates/rostop-cli/src/backend/live.rs`

The first attempt was a local env check (`ROS_DISTRO` / `RMW_IMPLEMENTATION`),
but that only catches "user forgot to source the right setup.bash". For the
real scenario (`just run-live` connecting to a foreign-distro robot), the
Dockerfile already exports the expected env, so the check passed and the
problem was unchanged.

Replaced with a 2-second **peer probe** that runs inside `spin_loop` before
`LiveBackend::new()` returns success:

- Creates the r2r context + node as before.
- For `PROBE_DURATION` (2s), polls the ROS graph and subscribes (`subscribe_raw`)
  to every discovered topic, exactly as the main loop does. Discovered topics
  whose publisher list contains any node other than rostop's own
  (`SELF_NODE_NAME`) are recorded as "foreign".
- Every `BackendEvent::Sample` forwarded from a subscription increments a
  shared `AtomicUsize` counter.
- At the probe deadline:
  - **No foreign topics** → empty graph (or rostop-only): probe passes.
  - **Foreign topics + samples > 0** → wire format works: probe passes.
  - **Foreign topics + zero samples** → discovery succeeded but no CDR
    payload decoded in 2s. This is the exact signature of cross-distro /
    cross-RMW peers. Return error via `init_tx`, surfacing in the CLI before
    the TUI takes over.
- Escape hatch: `ROSTOP_SKIP_PEER_PROBE=1` skips the probe and sends `init_tx
  Ok` immediately. Useful for transient-local-only setups or starting rostop
  before any peers are up.
- Events emitted during the probe are not thrown away — they buffer in the
  existing `mpsc::Sender<BackendEvent>` and are drained by the UI on first
  `poll()` after `LiveBackend::new()` returns.

Error message shown on mismatch:

> Discovered &lt;N&gt; foreign-published topics but received zero samples in 2s.
> This is the signature of a ROS 2 distro or RMW mismatch: rostop is a
> jazzy + rmw_cyclonedds_cpp participant, and peers on a different distro
> (Humble, Iron) or RMW (rmw_fastrtps_cpp) trigger CDR decode failures
> ("sequence size exceeds remaining buffer" on the robot side). Topics seen:
> &lt;up to 5, sorted&gt; (+M more). Rebuild rostop against the target distro /
> RMW, or set ROSTOP_SKIP_PEER_PROBE=1 to bypass.

No new crate dependencies; uses `std::sync::Arc` + `AtomicUsize` and the
existing `r2r` / `futures` stack.

### Known limitations of the heuristic
- 2 seconds added to startup. Acceptable for a diagnostic TUI.
- False positive possible if every peer publishes only `TRANSIENT_LOCAL`
  topics with no recent writes during the probe window. Mitigated by the
  `ROSTOP_SKIP_PEER_PROBE` escape hatch.
- Detects the symptom (zero samples from foreign publishers) rather than
  the cause (peer distro / RMW). That's deliberate — r2r 0.9 doesn't expose
  participant USER_DATA, and the symptom is what actually matters to the user.

## Proposed (not yet implemented)

### Humble build variant
- Add `Dockerfile.humble` mirroring `Dockerfile` but based on
  `ros:humble-ros-base`, with `ros-humble-*` package equivalents and no
  `RMW_IMPLEMENTATION` override (default `rmw_fastrtps_cpp` matches the robot).
- Parameterise the expected target via `build.rs` (`ROSTOP_TARGET_DISTRO`,
  `ROSTOP_TARGET_RMW`) so the error message names the actual build target
  rather than hard-coding "jazzy + rmw_cyclonedds_cpp".
- Add `just run-live-humble` recipe that mounts the Humble image and forwards
  the caller's env.
- Extend the GitHub Releases workflow to emit both
  `rostop-<version>-jazzy-x86_64.tar.gz` and
  `rostop-<version>-humble-x86_64.tar.gz`.

### CI coverage
- The existing `cargo test -p rostop-cli --features live` jobs already exercise
  the probe's pass-through path (publishers run for the duration of the test).
  Add one negative test: start `LiveBackend::new()` with `ROSTOP_SKIP_PEER_PROBE`
  unset and no publishers — should succeed (empty graph, no foreign topics).
- Add a Humble matrix entry once the build variant lands.

### Documentation
- README: short "Which build do I need?" section pointing users at the
  matching tarball / Docker image for their robot's distro.
- CHANGELOG: roll the peer probe into the next entry (0.1.1 or 0.2.0). The
  user-visible change worth recording is "rostop refuses to start when peers
  on the wire don't speak its CDR format, instead of silently producing garbage
  on the robot side."

## Open questions

- Probe duration: 2s feels right for typical robots, but configurable via env
  (`ROSTOP_PROBE_SECS`) might be worth adding once we see real usage.
- Per-topic QoS matching (currently `QosProfile::default()` for every
  subscription) is a separate v0.2 item already noted in `CHANGELOG.md`. Not
  addressed here, but a BEST_EFFORT-only probe would catch more peers (probe
  with permissive QoS, then switch to default for the main loop). Left for
  later because the current symptom — Humble peers — fails QoS-independent
  CDR decode anyway.
