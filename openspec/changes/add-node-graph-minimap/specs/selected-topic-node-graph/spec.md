## ADDED Requirements

### Requirement: Selected-topic topology
The system SHALL display the selected topic between its publishing nodes and
subscribing nodes, with directional publisher-to-topic and topic-to-subscriber
connections.

#### Scenario: Topic has publishers and subscribers
- **WHEN** the user opens the graph for a topic with known publisher and subscriber endpoints
- **THEN** the graph shows publishing nodes on the left, the topic in the center, subscribing nodes on the right, and directional connections between them

#### Scenario: Graph side is empty
- **WHEN** one endpoint side is known and contains no nodes
- **THEN** the graph identifies that side as having no publishers or no subscribers

#### Scenario: Graph side is unavailable
- **WHEN** the backend cannot provide publisher or subscriber endpoint information
- **THEN** the graph labels that side as unavailable rather than empty

### Requirement: Stable node identity and bounded layout
The system SHALL deduplicate endpoints by fully-qualified node identity, sort
nodes deterministically, and bound visible content to the terminal area.

#### Scenario: Node owns multiple endpoints
- **WHEN** multiple endpoints on one side share a namespace and node name
- **THEN** the graph renders one node card with its endpoint count

#### Scenario: Nodes exceed available height
- **WHEN** the number of nodes cannot fit in the graph column
- **THEN** the graph renders a stable subset and a summary of the hidden node count

### Requirement: Live activity animation
The system SHALL animate selected-topic connections following recent samples and
SHALL retain idle topology when no recent sample exists.

#### Scenario: Recent sample arrives
- **WHEN** a sample for the graphed topic arrived within the activity window
- **THEN** moving flow markers and an active traffic status are rendered on its candidate connections

#### Scenario: Topic is idle
- **WHEN** no sample for the graphed topic arrived within the activity window
- **THEN** connections remain visible in an idle style without moving flow markers

### Requirement: Graph navigation
The system SHALL provide keyboard controls to enter and leave the selected-topic
graph without changing the selected topic.

#### Scenario: Open from topic table
- **WHEN** a topic is selected in the topic table and the user presses `g`
- **THEN** the system opens its full-screen node graph

#### Scenario: Open from focus view
- **WHEN** a topic is displayed in focus view and the user presses `g`
- **THEN** the system opens its full-screen node graph

#### Scenario: Close graph
- **WHEN** the graph is open and the user presses `g` or `Esc`
- **THEN** the system returns to the view from which the graph was opened

#### Scenario: Selected topic disappears
- **WHEN** the graphed topic disappears from the live graph
- **THEN** the graph shows a non-panicking disappeared-topic state with a way to return

