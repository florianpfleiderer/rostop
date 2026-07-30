## Context

rostop already receives backend-neutral endpoint snapshots for every topic and
stores the latest publisher/subscriber lists in `App::endpoints`. Sample events
also arrive with monotonic timing, but the UI currently uses that activity only
for statistics, sparklines, messages, and the waveform scope.

The mini-map must work with demo and live backends, remain useful on an SSH
terminal, tolerate unavailable graph sides, and avoid adding ROS-specific types
to rendering code.

## Goals / Non-Goals

**Goals:**

- Make the selected topic's data-flow direction understandable at a glance.
- Show live traffic without hiding idle publishers or subscribers.
- Keep node placement deterministic across graph refreshes.
- Remain legible on common 80-column terminals and bound work for topics with
  many endpoints.
- Reuse existing graph and sample events without a new backend API.

**Non-Goals:**

- Whole-system topology, graph traversal, or automatic layout.
- Representing services, actions, parameters, or hidden DDS participants.
- Claiming which individual publisher produced a received sample; ROS sample
  events currently identify the topic, not the source endpoint.
- Replacing the detailed endpoint/QoS tables in focus mode.

## Decisions

### Use a dedicated selected-topic graph mode

The `g` key opens a full-screen graph for the selected topic from the topic
table or focus view; `g` and `Esc` return. This gives the graph enough space to
be useful while retaining the existing focus panel for detailed QoS and message
inspection. Embedding a tiny graph into the split pane was rejected because it
would collapse quickly at 80 columns and duplicate endpoint content.

### Derive topology from existing endpoint snapshots

A pure `rostop-core` topology helper will normalize node identity, deduplicate
multiple endpoints belonging to the same node, and sort nodes by namespace/name.
The UI therefore renders nodes rather than DDS endpoint rows, while counts still
communicate collapsed endpoints. No new live-backend calls are required.

### Animate topic-level flow honestly

Each sample updates a per-topic activity sequence and timestamp. During a short
activity window, a moving glyph advances along every publisher→topic and
topic→subscriber edge. Outside that window, edges remain visible in a dim idle
style. Because the received sample cannot be attributed to an individual
publisher, all candidate paths pulse together rather than implying false source
provenance.

### Use a bounded deterministic terminal layout

Publishers occupy the left column, the topic is centered, and subscribers occupy
the right column. Nodes are stable-sorted. The renderer shows as many nodes as
fit and uses a `+N more` summary for overflow. Unicode box and line glyphs are
drawn directly into the ratatui buffer, with ASCII-compatible labels and no new
rendering dependency.

## Risks / Trade-offs

- **Activity can be mistaken for per-node attribution** → label the animation
  as topic traffic and document that all candidate paths pulse together.
- **Large endpoint sets overwhelm the viewport** → deduplicate by node, cap
  visible cards from available height, and display overflow counts.
- **Duplicate node names in unusual graphs** → use fully-qualified
  namespace/name identity and retain endpoint counts.
- **Unicode line glyphs render poorly in some fonts** → keep topology meaningful
  through column headings, arrows, and text even if line art is imperfect.
- **Rapid graph churn causes visual movement** → stable sorting and snapshot
  replacement keep positions deterministic.

## Migration Plan

The feature is additive and defaults off. It can be rolled back by removing the
graph state, key binding, renderer, and pure topology helper without changing
the backend event contract or persisted data.

