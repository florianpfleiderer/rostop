//! Pure data preparation for the topic table.
//!
//! Building rendering rows is a deterministic function of (registry state,
//! sort/filter settings, now timestamp), so it's tested without spinning up a
//! terminal.

use rostop_core::registry::{SortKey, SortOrder, TopicRegistry};

/// One displayed row in the topic table.
#[derive(Debug, Clone, PartialEq)]
pub struct TopicTableRow {
    pub name: String,
    pub type_name: String,
    pub hz: f64,
    pub bps: f64,
    pub jitter_ms: f64,
    pub publishers: u32,
    pub subscribers: u32,
    /// Whole seconds since the topic was first observed in the graph. 0 when
    /// no `first_seen_ns` has been stamped yet (e.g. test fixtures that
    /// bypass `App::ingest`). The inspector uses this to decide whether to
    /// show an "idle" indicator instead of "no message yet".
    pub idle_secs: u64,
}

/// Build the list of topic rows for display.
///
/// `filter` is a case-insensitive substring matched against the topic name or
/// type. `now_ns` is used to evaluate rate/bandwidth in the registry's rolling
/// window.
pub fn build_rows(
    registry: &TopicRegistry,
    sort_key: SortKey,
    sort_order: SortOrder,
    filter: &str,
    now_ns: u64,
) -> Vec<TopicTableRow> {
    let sorted = registry.sorted_by(sort_key, sort_order, now_ns);
    let q = filter.to_lowercase();
    sorted
        .into_iter()
        .filter(|e| {
            q.is_empty()
                || e.name.to_lowercase().contains(&q)
                || e.type_name.to_lowercase().contains(&q)
        })
        .map(|e| TopicTableRow {
            name: e.name.clone(),
            type_name: e.type_name.clone(),
            hz: e.stats.hz(now_ns),
            bps: e.stats.bps(now_ns),
            jitter_ms: e.stats.jitter_ms(now_ns),
            publishers: e.publishers,
            subscribers: e.subscribers,
            idle_secs: e
                .first_seen_ns
                .map(|t| now_ns.saturating_sub(t) / 1_000_000_000)
                .unwrap_or(0),
        })
        .collect()
}

/// Format a byte-per-second value as a human readable string ("9.1 MB/s").
pub fn fmt_bps(bps: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    if bps >= GB {
        format!("{:.1} GB/s", bps / GB)
    } else if bps >= MB {
        format!("{:.1} MB/s", bps / MB)
    } else if bps >= KB {
        format!("{:.1} KB/s", bps / KB)
    } else {
        format!("{bps:.0} B/s")
    }
}

#[cfg(test)]
mod tests;
