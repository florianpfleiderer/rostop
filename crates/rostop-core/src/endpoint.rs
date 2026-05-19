//! Topic endpoint metadata for the fullscreen single-topic panel.
//!
//! `EndpointInfo` and `QosSnapshot` are pure-data carriers — no ROS / DDS
//! dependency. The live backend converts r2r / rcl FFI types into these and
//! forwards them on `BackendEvent::Endpoints`; the UI consumes them as-is.

use std::time::Duration;

/// Per-topic endpoint snapshot: publishers and subscribers, each optionally
/// `None` if the backend cannot determine that side (e.g. r2r 0.9.5 does
/// not expose subscriber info). The UI renders `None` as "(not available)"
/// to distinguish from a confirmed empty list.
pub type EndpointSets = (Option<Vec<EndpointInfo>>, Option<Vec<EndpointInfo>>);

/// One ROS 2 graph endpoint (publisher or subscriber) attached to a topic.
///
/// `endpoint_gid` is stored as a `Vec<u8>` rather than a fixed array because
/// `RMW_GID_STORAGE_SIZE` is set at compile time by r2r's bindgen against the
/// active rcl headers and varies across distros: Humble reports 24 bytes,
/// Jazzy reports a smaller buffer. The TUI only displays a short hex prefix,
/// so the actual length doesn't matter beyond "non-empty".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointInfo {
    pub node_name: String,
    pub node_namespace: String,
    pub topic_type: String,
    pub endpoint_gid: Vec<u8>,
    pub qos: QosSnapshot,
}

/// Negotiated QoS profile, normalised so the renderer doesn't have to know
/// DDS sentinel values for "infinite" / "system default".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QosSnapshot {
    pub reliability: ReliabilityKind,
    pub durability: DurabilityKind,
    pub history: HistoryKind,
    /// Only meaningful when `history == KeepLast`; ignored for `KeepAll`.
    pub depth: usize,
    pub deadline: Option<Duration>,
    pub lifespan: Option<Duration>,
    pub liveliness: LivelinessKind,
    pub liveliness_lease: Option<Duration>,
}

impl QosSnapshot {
    /// Display string for the history policy — collapses `(depth)` for
    /// `KeepAll` since depth is meaningless there.
    pub fn history_display(&self) -> String {
        match self.history {
            HistoryKind::KeepAll => "KeepAll".into(),
            HistoryKind::KeepLast => format!("KeepLast({})", self.depth),
            HistoryKind::Unknown => "—".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReliabilityKind {
    Reliable,
    BestEffort,
    Unknown,
}

impl ReliabilityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ReliabilityKind::Reliable => "Reliable",
            ReliabilityKind::BestEffort => "BestEffort",
            ReliabilityKind::Unknown => "—",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurabilityKind {
    Volatile,
    TransientLocal,
    Unknown,
}

impl DurabilityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DurabilityKind::Volatile => "Volatile",
            DurabilityKind::TransientLocal => "TransientLocal",
            DurabilityKind::Unknown => "—",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    KeepLast,
    KeepAll,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivelinessKind {
    Automatic,
    ManualByTopic,
    Unknown,
}

impl LivelinessKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LivelinessKind::Automatic => "Automatic",
            LivelinessKind::ManualByTopic => "ManualByTopic",
            LivelinessKind::Unknown => "—",
        }
    }
}

/// Fold DDS sentinel durations (`MAX` = infinite, `ZERO` = system default) to
/// `None`. The TUI renders `None` as an em-dash.
pub fn normalise_duration(d: Duration) -> Option<Duration> {
    if d == Duration::MAX || d.is_zero() {
        None
    } else {
        Some(d)
    }
}

/// Hex-render up to the first 8 bytes of an endpoint GID — enough to
/// disambiguate in practice without making rows unreadable. Tolerates GIDs
/// shorter than 8 bytes (older RMW configs / synthesised values).
pub fn gid_hex_short(gid: &[u8]) -> String {
    let take = gid.len().min(8);
    let mut out = String::with_capacity(take * 2);
    for byte in &gid[..take] {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Stable sort: namespace ascending, then node name ascending. Keeps row
/// order deterministic across polls so the UI doesn't shimmy.
pub fn sort_endpoints(endpoints: &mut [EndpointInfo]) {
    endpoints.sort_by(|a, b| {
        a.node_namespace
            .cmp(&b.node_namespace)
            .then_with(|| a.node_name.cmp(&b.node_name))
    });
}

#[cfg(test)]
mod tests;
