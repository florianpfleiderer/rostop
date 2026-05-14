use super::*;

fn ns(s: f64) -> u64 {
    (s * 1_000_000_000.0) as u64
}

#[test]
fn empty_registry_has_no_topics() {
    let reg = TopicRegistry::new();
    assert_eq!(reg.len(), 0);
    assert!(reg.get("/scan").is_none());
}

#[test]
fn upsert_adds_a_new_topic_and_is_idempotent() {
    let mut reg = TopicRegistry::new();
    reg.upsert("/scan", "sensor_msgs/msg/LaserScan");
    reg.upsert("/scan", "sensor_msgs/msg/LaserScan");
    assert_eq!(reg.len(), 1);
    let e = reg.get("/scan").unwrap();
    assert_eq!(e.type_name, "sensor_msgs/msg/LaserScan");
}

#[test]
fn record_feeds_stats_through_the_registry() {
    let mut reg = TopicRegistry::new();
    reg.upsert("/scan", "sensor_msgs/msg/LaserScan");
    for i in 0..10 {
        reg.record("/scan", ns(i as f64 * 0.1), 120);
    }
    let e = reg.get("/scan").unwrap();
    let hz = e.stats.hz(ns(0.9));
    assert!((hz - 10.0).abs() < 1e-6, "hz = {hz}");
}

#[test]
fn record_for_unknown_topic_is_a_noop() {
    let mut reg = TopicRegistry::new();
    reg.record("/ghost", ns(0.0), 100); // must not panic
    assert_eq!(reg.len(), 0);
}

#[test]
fn mark_seen_sets_first_seen_lazily() {
    let mut reg = TopicRegistry::new();
    reg.upsert("/scan", "sensor_msgs/msg/LaserScan");
    assert!(reg.get("/scan").unwrap().first_seen_ns.is_none());

    reg.mark_seen("/scan", ns(1.0));
    assert_eq!(reg.get("/scan").unwrap().first_seen_ns, Some(ns(1.0)));

    // A later mark_seen must not overwrite the first one.
    reg.mark_seen("/scan", ns(5.0));
    assert_eq!(reg.get("/scan").unwrap().first_seen_ns, Some(ns(1.0)));
}

#[test]
fn mark_seen_for_unknown_topic_is_a_noop() {
    let mut reg = TopicRegistry::new();
    reg.mark_seen("/ghost", ns(1.0)); // must not panic
    assert_eq!(reg.len(), 0);
}

#[test]
fn set_endpoints_tracks_pub_sub_counts() {
    let mut reg = TopicRegistry::new();
    reg.upsert("/cmd_vel", "geometry_msgs/msg/Twist");
    reg.set_endpoints("/cmd_vel", 1, 3);
    let e = reg.get("/cmd_vel").unwrap();
    assert_eq!(e.publishers, 1);
    assert_eq!(e.subscribers, 3);
}

#[test]
fn filter_by_substring_matches_name_or_type() {
    let mut reg = TopicRegistry::new();
    reg.upsert("/scan", "sensor_msgs/msg/LaserScan");
    reg.upsert("/camera/image", "sensor_msgs/msg/Image");
    reg.upsert("/tf", "tf2_msgs/msg/TFMessage");

    let names: Vec<String> = reg
        .filtered("Image")
        .into_iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, vec!["/camera/image".to_string()]);

    let names: Vec<String> = reg
        .filtered("/sc")
        .into_iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, vec!["/scan".to_string()]);
}

#[test]
fn sort_by_hz_descending() {
    let mut reg = TopicRegistry::new();
    reg.upsert("/slow", "x");
    reg.upsert("/fast", "x");
    reg.upsert("/medium", "x");
    // 5, 50, 20 Hz
    for i in 0..5 {
        reg.record("/slow", ns(i as f64 * 0.2), 10);
    }
    for i in 0..50 {
        reg.record("/fast", ns(i as f64 * 0.02), 10);
    }
    for i in 0..20 {
        reg.record("/medium", ns(i as f64 * 0.05), 10);
    }
    let order: Vec<String> = reg
        .sorted_by(SortKey::Hz, SortOrder::Descending, ns(1.0))
        .into_iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(order, vec!["/fast", "/medium", "/slow"]);
}

#[test]
fn remove_drops_a_topic() {
    let mut reg = TopicRegistry::new();
    reg.upsert("/a", "x");
    reg.upsert("/b", "x");
    reg.remove("/a");
    assert_eq!(reg.len(), 1);
    assert!(reg.get("/a").is_none());
    assert!(reg.get("/b").is_some());
}
