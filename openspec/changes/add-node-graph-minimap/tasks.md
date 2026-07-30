## 1. Topology model

- [x] 1.1 Add a backend-neutral selected-topic topology model with fully-qualified node identity, endpoint deduplication, counts, and stable ordering
- [x] 1.2 Add unit tests for duplicate endpoints, namespaces, empty sides, and unavailable sides

## 2. Application interaction

- [x] 2.1 Track per-topic sample activity needed for honest topic-level animation
- [x] 2.2 Add graph view state and `g`/`Esc` navigation from the topic table and focus view
- [x] 2.3 Clear graph activity and handle a disappearing selected topic safely

## 3. Terminal visualization

- [x] 3.1 Render the full-screen publisher→topic→subscriber topology with deterministic node cards and directional connectors
- [x] 3.2 Add recent-traffic animation, idle styling, unavailable/empty states, and bounded overflow summaries
- [x] 3.3 Update status labels and help text for graph mode

## 4. Verification and documentation

- [x] 4.1 Add TestBackend render coverage for active, idle, unavailable, deduplicated, overflow, and disappeared-topic graph states
- [x] 4.2 Update README controls/roadmap and changelog
- [x] 4.3 Run formatting, Clippy, default/live tests, and OpenSpec validation
