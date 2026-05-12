//! Topic registry: holds discovered topics, their endpoint counts, and stats.
//!
//! Pure-data layer — no ROS2 binding. A `RosBackend` adapter in `rostop-cli`
//! feeds discovery and sample events into this registry; the TUI reads from it.

use std::collections::BTreeMap;

use crate::stats::TopicStats;

const DEFAULT_WINDOW_NS: u64 = 1_000_000_000; // 1 s

/// One row's worth of state in the topic table.
#[derive(Debug)]
pub struct TopicEntry {
    pub name: String,
    pub type_name: String,
    pub publishers: u32,
    pub subscribers: u32,
    pub stats: TopicStats,
}

/// Sort keys for `TopicRegistry::sorted_by`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Hz,
    Bandwidth,
    Type,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// In-memory registry of all topics observed in the ROS graph.
#[derive(Debug, Default)]
pub struct TopicRegistry {
    entries: BTreeMap<String, TopicEntry>,
}

impl TopicRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of registered topics.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no topics are registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Look up a topic by name.
    pub fn get(&self, name: &str) -> Option<&TopicEntry> {
        self.entries.get(name)
    }
}

#[cfg(test)]
mod tests;
