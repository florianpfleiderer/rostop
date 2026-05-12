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
