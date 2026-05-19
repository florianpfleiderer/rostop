use super::*;
use std::time::Duration;

#[test]
fn duration_sentinels_normalise_to_none() {
    // DDS reports "infinite" / "default" as either Duration::MAX or
    // Duration::ZERO depending on the field. The normaliser collapses both
    // to None so the renderer doesn't have to know DDS magic numbers.
    assert_eq!(normalise_duration(Duration::MAX), None);
    assert_eq!(normalise_duration(Duration::ZERO), None);
    assert_eq!(normalise_duration(Duration::from_millis(100)), Some(Duration::from_millis(100)));
}

#[test]
fn gid_hex_short_renders_first_eight_bytes() {
    let mut gid = [0u8; GID_SIZE];
    gid[..8].copy_from_slice(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
    assert_eq!(gid_hex_short(&gid), "0123456789abcdef");
}

#[test]
fn gid_hex_short_pads_short_prefixes() {
    let gid = [0u8; GID_SIZE];
    assert_eq!(gid_hex_short(&gid), "0000000000000000");
}

#[test]
fn qos_history_keepall_ignores_depth_in_display() {
    let q = QosSnapshot {
        reliability: ReliabilityKind::Reliable,
        durability: DurabilityKind::Volatile,
        history: HistoryKind::KeepAll,
        depth: 0,
        deadline: None,
        lifespan: None,
        liveliness: LivelinessKind::Automatic,
        liveliness_lease: None,
    };
    assert_eq!(q.history_display(), "KeepAll");
}

#[test]
fn qos_history_keeplast_includes_depth() {
    let q = QosSnapshot {
        reliability: ReliabilityKind::Reliable,
        durability: DurabilityKind::Volatile,
        history: HistoryKind::KeepLast,
        depth: 10,
        deadline: None,
        lifespan: None,
        liveliness: LivelinessKind::Automatic,
        liveliness_lease: None,
    };
    assert_eq!(q.history_display(), "KeepLast(10)");
}

#[test]
fn endpoint_sort_key_is_namespace_then_node_name() {
    let a = sample_endpoint("/n2", "/");
    let b = sample_endpoint("/n1", "/");
    let c = sample_endpoint("/n0", "/ns");
    let mut v = vec![a.clone(), b.clone(), c.clone()];
    sort_endpoints(&mut v);
    // root namespace "/" sorts before "/ns"; within "/", n1 before n2.
    assert_eq!(v[0].node_name, "/n1");
    assert_eq!(v[1].node_name, "/n2");
    assert_eq!(v[2].node_name, "/n0");
}

#[test]
fn unknown_qos_renders_em_dash() {
    assert_eq!(ReliabilityKind::Unknown.as_str(), "—");
    assert_eq!(DurabilityKind::Unknown.as_str(), "—");
    assert_eq!(LivelinessKind::Unknown.as_str(), "—");
}

fn sample_endpoint(node: &str, ns: &str) -> EndpointInfo {
    EndpointInfo {
        node_name: node.into(),
        node_namespace: ns.into(),
        topic_type: "std_msgs/msg/String".into(),
        endpoint_gid: [0u8; GID_SIZE],
        qos: QosSnapshot {
            reliability: ReliabilityKind::Reliable,
            durability: DurabilityKind::Volatile,
            history: HistoryKind::KeepLast,
            depth: 10,
            deadline: None,
            lifespan: None,
            liveliness: LivelinessKind::Automatic,
            liveliness_lease: None,
        },
    }
}
