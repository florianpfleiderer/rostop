//! Topic endpoint metadata for the fullscreen single-topic panel.
//!
//! `EndpointInfo` and `QosSnapshot` are pure-data carriers — no ROS / DDS
//! dependency. The live backend converts r2r / rcl FFI types into these and
//! forwards them on `BackendEvent::Endpoints`; the UI consumes them as-is.

use std::time::Duration;

/// Fixed RMW GID storage size. RCL hard-codes this as 24 across every RMW
/// shipping today (FastDDS, CycloneDDS, ConnextDDS). The live backend
/// debug-asserts that the bound matches `RMW_GID_STORAGE_SIZE` when copying
/// out of the FFI struct.
pub const GID_SIZE: usize = 24;

/// One ROS 2 graph endpoint (publisher or subscriber) attached to a topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointInfo {
    pub node_name: String,
    pub node_namespace: String,
    pub topic_type: String,
    pub endpoint_gid: [u8; GID_SIZE],
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

/// Hex-render the first 8 bytes of an endpoint GID — enough to disambiguate
/// in practice without making rows unreadable.
pub fn gid_hex_short(gid: &[u8; GID_SIZE]) -> String {
    let mut out = String::with_capacity(16);
    for byte in &gid[..8] {
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
