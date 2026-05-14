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
    /// Monotonic nanoseconds (from `App::elapsed_ns`) when this topic was first
    /// observed via the backend. `None` until the first ingest event sets it;
    /// used to compute idle time for healthy-but-quiet topics.
    pub first_seen_ns: Option<u64>,
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

    /// Insert a topic, or update its type if it already exists.
    pub fn upsert(&mut self, name: &str, type_name: &str) {
        self.entries
            .entry(name.to_string())
            .and_modify(|e| e.type_name = type_name.to_string())
            .or_insert_with(|| TopicEntry {
                name: name.to_string(),
                type_name: type_name.to_string(),
                publishers: 0,
                subscribers: 0,
                stats: TopicStats::new(DEFAULT_WINDOW_NS),
                first_seen_ns: None,
            });
    }

    /// Drop a topic (e.g. it disappeared from the ROS graph).
    pub fn remove(&mut self, name: &str) {
        self.entries.remove(name);
    }

    /// Feed a message sample for a known topic. Unknown topics are silently
    /// ignored — discovery is the registry's job, not the sample stream's.
    pub fn record(&mut self, name: &str, t_ns: u64, bytes: u32) {
        if let Some(e) = self.entries.get_mut(name) {
            e.stats.record(t_ns, bytes);
        }
    }

    /// Update publisher and subscriber counts for a topic.
    pub fn set_endpoints(&mut self, name: &str, publishers: u32, subscribers: u32) {
        if let Some(e) = self.entries.get_mut(name) {
            e.publishers = publishers;
            e.subscribers = subscribers;
        }
    }

    /// Stamp the time this topic was first seen, if not already stamped.
    /// Idempotent — later calls do not overwrite the original timestamp.
    pub fn mark_seen(&mut self, name: &str, t_ns: u64) {
        if let Some(e) = self.entries.get_mut(name) {
            if e.first_seen_ns.is_none() {
                e.first_seen_ns = Some(t_ns);
            }
        }
    }

    /// Returns entries whose name or type contains `query` (case-insensitive).
    pub fn filtered(&self, query: &str) -> Vec<&TopicEntry> {
        let q = query.to_lowercase();
        self.entries
            .values()
            .filter(|e| {
                e.name.to_lowercase().contains(&q) || e.type_name.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Entries sorted by `key`. Rate-based sorts use the rolling window ending at `now_ns`.
    pub fn sorted_by(&self, key: SortKey, order: SortOrder, now_ns: u64) -> Vec<&TopicEntry> {
        let mut out: Vec<&TopicEntry> = self.entries.values().collect();
        out.sort_by(|a, b| match key {
            SortKey::Name => a.name.cmp(&b.name),
            SortKey::Type => a.type_name.cmp(&b.type_name),
            SortKey::Hz => a
                .stats
                .hz(now_ns)
                .partial_cmp(&b.stats.hz(now_ns))
                .unwrap_or(std::cmp::Ordering::Equal),
            SortKey::Bandwidth => a
                .stats
                .bps(now_ns)
                .partial_cmp(&b.stats.bps(now_ns))
                .unwrap_or(std::cmp::Ordering::Equal),
        });
        if order == SortOrder::Descending {
            out.reverse();
        }
        out
    }
}

#[cfg(test)]
mod tests;
