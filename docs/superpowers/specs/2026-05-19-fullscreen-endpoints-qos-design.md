# Fullscreen view: publisher / subscriber lists + per-endpoint QoS

Design doc for [#24](https://github.com/florianpfleiderer/rostop/issues/24).

## Goal

Surface, in the existing fullscreen single-topic panel, the live ROS 2 graph endpoints attached to the selected topic — one row per publisher and subscriber — with each row carrying the node identity (name, namespace), the endpoint GID, and the negotiated QoS profile (reliability, durability, history kind + depth, deadline, lifespan, liveliness).

This is the second half of issue #15 (the first half — the fullscreen layout — shipped in #22).

## Non-goals

- Changing or republishing QoS profiles (rostop is passive).
- Showing services or parameters in the fullscreen panel.
- Cross-domain endpoint discovery (see #26).
- A non-fullscreen surface for this data.

## Key finding from exploration

**`r2r` 0.9.5 only wraps `rcl_get_publishers_info_by_topic`, not the symmetric subscription variant.** The issue body in #24 says r2r exposes both — it does not.

A first plan was to write a small unsafe shim calling `rcl_get_subscriptions_info_by_topic` directly, since the C symbol IS in `r2r_rcl::bindings`. That requires the `*const rcl_node_t` raw pointer, which r2r exposes as `Node::node_handle` — but the field is **`pub(crate)`**, not public. There is no public accessor either. So we cannot call the FFI ourselves from outside the r2r crate.

**Revised scope for this PR**: publishers + QoS only. The subscribers section is still rendered, but with a clear "(not available — see #X)" placeholder. A follow-up issue tracks either upstreaming `get_subscriptions_info_by_topic` to r2r or vendoring a small fork.

This is still a meaningful slice of the original issue — the publisher list is the more useful half (you're usually trying to find *who is publishing this topic*; subscribers are easier to enumerate by inspection).

## Architecture

The change adds one event variant, one core type, one backend FFI shim, and one rendering section. The architectural seam (UI does not import r2r) stays intact.

### 1. `rostop-core` — new `EndpointInfo` type

A pure-data carrier with no r2r dependency:

```rust
// crates/rostop-core/src/endpoint.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointInfo {
    pub node_name: String,
    pub node_namespace: String,
    pub topic_type: String,
    /// 24-byte GID, hex-formatted for display.
    pub endpoint_gid: [u8; 24],
    pub qos: QosSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QosSnapshot {
    pub reliability: ReliabilityKind,
    pub durability: DurabilityKind,
    pub history: HistoryKind,
    pub depth: usize,
    /// `None` means "unspecified / default / infinite" — render as "—".
    pub deadline: Option<Duration>,
    pub lifespan: Option<Duration>,
    pub liveliness: LivelinessKind,
    pub liveliness_lease: Option<Duration>,
}
```

Enums (`ReliabilityKind`, `DurabilityKind`, `HistoryKind`, `LivelinessKind`) mirror the DDS vocabulary including an `Unknown` variant — endpoints discovered before their QoS settles can show up with policy values the RMW reports as "unknown" / "system default".

`endpoint_gid` is `[u8; 24]` because `RMW_GID_STORAGE_SIZE` is 24 in every RMW shipping today; we hard-code that rather than dragging in `r2r_rcl` constants. We assert the size at the boundary (the backend shim panics if r2r ever changes it — better to fail loudly than silently truncate).

The 0 / `Duration::MAX` / 0xFFFFFFFFFFFFFFFF "infinite" / "default" sentinels coming from DDS are normalised to `Option::None` at construction so the renderer doesn't need to know DDS magic numbers.

### 2. `BackendEvent::Endpoints { topic, publishers, subscribers }`

New variant on `BackendEvent` in `crates/rostop-cli/src/backend.rs`:

```rust
Endpoints {
    topic: String,
    publishers: Vec<EndpointInfo>,
    subscribers: Vec<EndpointInfo>,
},
```

**Push vs pull decision: push, piggybacked on the existing 500 ms graph poll.** Reasoning:
- Endpoint info is more stable than sample rates — re-polling every 500 ms is fine in practice.
- On-demand polling (only when the user enters fullscreen) would require a request channel back to the spin thread, which is more plumbing for marginal benefit.
- The existing graph-poll loop is already iterating per topic; adding two FFI calls per known topic is cheap relative to the discovery work that already happens.
- Demo backend already drives discovery this way; symmetry beats cleverness.

**Refresh cadence: every graph-poll tick (~500 ms) for now.** If endpoint churn is visible in the UI we add a per-topic debounce (only emit when the set differs from last emission). Start without the debounce; add it if tests / live use show flicker.

**Channel volume**: one event per known topic per poll tick. With ~50 topics that's 100 events / s — well within mpsc capacity. Each event allocates a Vec<EndpointInfo> with usually 1–3 entries. Acceptable.

### 3. Backend mapping — `live.rs` only

One function in `backend/live.rs`:

```rust
fn get_publishers_endpoint_info(node: &Node, topic: &str)
    -> Vec<EndpointInfo>;
```

Wraps `node.get_publishers_info_by_topic(name, false)` and maps `r2r::TopicEndpointInfo` → `EndpointInfo`. Subscribers can't be fetched without access to `Node::node_handle` (see "Key finding"); we emit an empty subscriber list in the live backend and the renderer shows "(not available)" rather than "(none)" so the user knows there's a known limitation, not just an empty graph.

QoS mapping converts `r2r::qos::{HistoryPolicy, ReliabilityPolicy, DurabilityPolicy, LivelinessPolicy}` to the corresponding core enums. r2r's `BestAvailable` variant is feature-gated on Iron / Jazzy / Rolling; we fold it to `Unknown` in the mapping so Humble builds keep compiling. `SystemDefault` also folds to `Unknown` since we have no way to know what the system default actually resolved to.

Sentinel detection folds "infinite" / `Duration::ZERO` `Duration` fields to `None`.

The spin loop is extended: after the existing publisher count probe, it computes the publisher list and emits `BackendEvent::Endpoints` per topic.

### 4. App state — store latest endpoints per topic

In `app.rs`:

```rust
pub endpoints: HashMap<String, (Vec<EndpointInfo>, Vec<EndpointInfo>)>,
```

`ingest()` handles `BackendEvent::Endpoints` by replacing the entry. `TopicRemoved` clears it. No pruning beyond that — endpoint lists are tiny.

### 5. Demo backend — graceful degrade

`DemoBackend` emits two fake endpoints per topic at discovery time:

- one publisher (`/demo_pub`, `/`, BestEffort, Volatile, KeepLast(10))
- one subscriber (`/demo_sub`, `/`, Reliable, Volatile, KeepLast(10))

Just enough to drive the rendering and integration tests without being elaborate.

### 6. Rendering — two new sections in fullscreen

Layout adjustment in `render_fullscreen_topic` (`ui/view.rs`):

```
┌─ fullscreen ─ /topic ─ type ─ distro+rmw ───────────────┐
│ HZ      ...                                              │
│ BW      ...                                              │
│ JIT     ...                                              │
│ PUB/SUB 2/3                                              │
│ IDLE    ...                                              │
├──────────────────────────────────────────────────────────┤
│ PUBLISHERS                                               │
│   /node_a (/ns)   Reliable/Volatile  KeepLast(10)        │
│     deadline —   lifespan —   liveliness Automatic       │
│     gid 0123456789abcdef…                                │
│   /node_b ...                                            │
│                                                          │
│ SUBSCRIBERS                                              │
│   /node_c ...                                            │
├──────────────────────────────────────────────────────────┤
│ message                                                  │
│   <message tree>                                         │
└──────────────────────────────────────────────────────────┘
```

The vertical constraint becomes `[Length(6), Length(N_endpoints + 3), Min(1)]` where `N_endpoints` is bounded — we clamp to a sane max (e.g. 12 rows total across both lists), with a "+N more" footer if exceeded. Rare in practice but defends against pathological topics.

**Sort order**: stable by node namespace + node name within each list. Deterministic, matches what `rqt_graph` users expect, and means the rendering doesn't shimmy when endpoints arrive in different orders across polls.

**GID rendering**: truncate to first 8 hex bytes by default ("0123456789abcdef…"). Full 24-byte GID shown only if the user has explicitly drilled into an endpoint (future enhancement; not in this PR).

## Test plan

### `rostop-core` unit tests (preferred, fast)

In `crates/rostop-core/src/endpoint/tests.rs`:

- QoS sentinel normalisation: `Duration::MAX` → `None`, `Duration::ZERO` for deadline → `None`, etc.
- `QosSnapshot` equality / hashing as expected.
- GID hex rendering: 24-byte buffer formats stably.

### `rostop-cli` integration tests (slow, but already exist)

In `crates/rostop-cli/tests/render.rs`:

- Extend `fullscreen_mode_swaps_layout_to_a_single_topic_panel` to assert "PUBLISHERS" and "SUBSCRIBERS" sections render after a tick (DemoBackend emits fake endpoints).
- New test: fullscreen panel shows "(none)" placeholders if a topic has no endpoints of one kind.

### Live tests (`--features live`)

Existing live tests publish a real topic via `ros2 topic pub`. Extend one to assert at least one publisher entry is parsed back into the registry, with non-empty `node_name`.

## Phasing inside the PR

Atomic commits, each green:

1. `feat(core):` add `EndpointInfo`, `QosSnapshot`, enums, sentinel logic, unit tests.
2. `feat(cli):` add `BackendEvent::Endpoints`, handle in `App::ingest`, store in `endpoints` map.
3. `feat(cli):` demo backend emits fake endpoints; integration test for fullscreen rendering.
4. `feat(cli):` live backend publisher-side wrapping via `r2r::TopicEndpointInfo`; subscribers surface a "(not available)" placeholder.
5. `docs:` CHANGELOG `[Unreleased]` entry; open follow-up issue for subscriber support.

## Open risks / things to watch

- **r2r breakage**: the unsafe shim depends on r2r_rcl's bindgen output. If r2r bumps to a version with renamed FFI symbols, the shim breaks. Mitigation: keep the shim small, well-commented, and unit-test against `--features live`.
- **RMW differences**: not every RMW reports the full QoS struct meaningfully (e.g. CycloneDDS deadline reporting). Render `Unknown` / `—` rather than 0 to avoid lying.
- **`KeepAll` history**: depth is meaningless when history is `KeepAll`. Don't show "KeepAll(0)" — show "KeepAll".
- **Endpoint churn during discovery**: bursty at startup as nodes appear. If flicker is annoying in the UI, add the per-topic debounce mentioned above as a follow-up.

## References

- Issue [#24](https://github.com/florianpfleiderer/rostop/issues/24) — feature request
- Issue [#15](https://github.com/florianpfleiderer/rostop/issues/15) — original parent
- PR [#22](https://github.com/florianpfleiderer/rostop/pull/22) — fullscreen layout (merged)
- `crates/rostop-cli/src/backend/live.rs:158` — existing publisher count probe
- `crates/rostop-cli/src/ui/view.rs:57` — `render_fullscreen_topic`
- `crates/rostop-cli/src/app.rs:30` — `App` struct (state lives here)
