## Why

Endpoint tables expose who publishes and subscribes to a selected topic, but they
do not make the data flow or live activity immediately legible. A compact,
animated topology view gives operators the rqt_graph insight they need without
leaving rostop or expanding the scope to a whole-system graph visualizer.

## What Changes

- Add a selected-topic node-graph mini-map showing publishers flowing into the
  topic and the topic flowing out to subscribers.
- Animate graph edges from recent sample activity while keeping idle topology
  visible and stable.
- Provide deterministic layout, endpoint deduplication, overflow summaries, and
  useful empty/unavailable states for narrow terminals and incomplete graphs.
- Add a dedicated key binding and help text for opening and closing the graph.
- Cover topology derivation and terminal rendering with unit and snapshot-style
  render tests.

## Capabilities

### New Capabilities

- `selected-topic-node-graph`: Live, animated publisher/topic/subscriber topology
  visualization for the currently selected topic.

### Modified Capabilities

None.

## Impact

The change affects application view state, keyboard handling, endpoint-derived
presentation logic, ratatui rendering, demo data, render tests, and user
documentation. It reuses the existing backend-neutral `BackendEvent::Endpoints`
and sample events, so no ROS-client-specific API or new dependency is required.
