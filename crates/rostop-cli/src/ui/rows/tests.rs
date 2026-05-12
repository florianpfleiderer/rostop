use super::*;
use rostop_core::registry::{SortKey, SortOrder, TopicRegistry};

fn ns(s: f64) -> u64 {
    (s * 1_000_000_000.0) as u64
}

fn populated_registry() -> TopicRegistry {
    let mut r = TopicRegistry::new();
    r.upsert("/scan", "sensor_msgs/msg/LaserScan");
    r.upsert("/camera/image", "sensor_msgs/msg/Image");
    r.upsert("/cmd_vel", "geometry_msgs/msg/Twist");
    for i in 0..40 {
        r.record("/scan", ns(i as f64 * 0.025), 120);
    }
    for i in 0..30 {
        r.record("/camera/image", ns(i as f64 * 0.033), 9_400_000);
    }
    for i in 0..100 {
        r.record("/cmd_vel", ns(i as f64 * 0.01), 48);
    }
    r.set_endpoints("/scan", 1, 2);
    r.set_endpoints("/camera/image", 1, 1);
    r.set_endpoints("/cmd_vel", 1, 1);
    r
}

#[test]
fn build_rows_returns_all_topics_with_no_filter() {
    let r = populated_registry();
    let rows = build_rows(&r, SortKey::Name, SortOrder::Ascending, "", ns(1.0));
    assert_eq!(rows.len(), 3);
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["/camera/image", "/cmd_vel", "/scan"]);
}

#[test]
fn build_rows_applies_filter() {
    let r = populated_registry();
    let rows = build_rows(&r, SortKey::Name, SortOrder::Ascending, "image", ns(1.0));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "/camera/image");
}

#[test]
fn build_rows_sorts_by_hz_descending() {
    let r = populated_registry();
    let rows = build_rows(&r, SortKey::Hz, SortOrder::Descending, "", ns(1.0));
    // cmd_vel (100 Hz) > scan (40 Hz) > camera/image (30 Hz)
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["/cmd_vel", "/scan", "/camera/image"]);
}

#[test]
fn build_rows_populates_endpoint_counts() {
    let r = populated_registry();
    let rows = build_rows(&r, SortKey::Name, SortOrder::Ascending, "", ns(1.0));
    let scan = rows.iter().find(|r| r.name == "/scan").unwrap();
    assert_eq!(scan.publishers, 1);
    assert_eq!(scan.subscribers, 2);
}

#[test]
fn fmt_bps_picks_a_sensible_unit() {
    assert_eq!(fmt_bps(0.0), "0 B/s");
    assert_eq!(fmt_bps(500.0), "500 B/s");
    assert_eq!(fmt_bps(2048.0), "2.0 KB/s");
    assert_eq!(fmt_bps(5.0 * 1024.0 * 1024.0), "5.0 MB/s");
}
