//! Abstraction over a ROS-graph data source.
//!
//! The TUI talks to a `RosBackend` — never directly to rclrs / ros2 / etc.
//! The default backend is `DemoBackend`, which fabricates a realistic stream
//! of topics, message rates, and field values so `rostop --demo` is useful
//! on any machine (no ROS installation required). A live backend that
//! shells out to `ros2 topic` is provided behind the `live` mode and can be
//! swapped out for a native r2r/rclrs implementation later without touching
//! the UI layer.

use std::time::{Duration, Instant};

use rostop_core::endpoint::EndpointInfo;
use rostop_core::message::DynamicValue;

/// Events flowing from the backend into the application state.
#[derive(Debug, Clone)]
pub enum BackendEvent {
    /// A topic was discovered or its type changed.
    Topic {
        name: String,
        type_name: String,
        publishers: u32,
        subscribers: u32,
    },
    /// A topic disappeared from the graph.
    TopicRemoved(String),
    /// A message arrived on `name`. `bytes` is the serialized size.
    Sample {
        name: String,
        bytes: u32,
        value: DynamicValue,
        at: Instant,
    },
    /// A sample arrived but the CDR payload failed to decode against the
    /// type-support compiled into this build. Signature of a ROS 2
    /// distro / RMW mismatch (e.g. a Jazzy-built rostop subscribed to a
    /// Humble peer): the bytes come through, but `from_serialized_bytes`
    /// fails. Emitted at most once per (topic, type_name) so a torrent
    /// of foreign samples doesn't drown the channel.
    DecodeFailure { topic: String, type_name: String },
    /// Refreshed publisher and subscriber endpoint lists for a topic.
    /// Emitted on the graph-poll cadence (~500 ms); each event replaces
    /// the previously known set for `topic`.
    Endpoints {
        topic: String,
        publishers: Vec<EndpointInfo>,
        subscribers: Vec<EndpointInfo>,
    },
}

/// Backend trait — sources of `BackendEvent`s.
pub trait RosBackend: Send {
    /// Poll for any new events, returning what is currently available.
    /// Implementations may block up to `budget` if they have nothing to emit.
    fn poll(&mut self, budget: Duration) -> Vec<BackendEvent>;

    /// Human-readable name (shown in the title bar).
    fn label(&self) -> &'static str;
}

pub mod demo;

#[cfg(feature = "live")]
pub mod live;
