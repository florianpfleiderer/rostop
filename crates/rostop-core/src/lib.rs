//! rostop-core: pure-logic primitives for the rostop TUI.
//!
//! This crate is intentionally free of any ROS2 / rclrs dependency so that the
//! statistics, registry, and rendering math can be exercised on any platform
//! without a ROS2 install.

pub mod registry;
pub mod stats;
