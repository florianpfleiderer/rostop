//! Demo backend — fabricates a plausible ROS 2 system for offline / talk-track
//! / portfolio use. Designed to look like a real mobile-robot stack so that
//! screenshots and recordings are persuasive.

use std::time::{Duration, Instant};

use rostop_core::message::DynamicValue;

use super::{BackendEvent, RosBackend};

struct DemoTopic {
    name: &'static str,
    type_name: &'static str,
    publishers: u32,
    subscribers: u32,
    rate_hz: f64,
    bytes_per_msg: u32,
    next_emit: Duration, // time relative to backend start
    seq: u64,
    /// Optional jitter as a fraction of the period. 0.0 = perfectly regular.
    jitter: f64,
}

pub struct DemoBackend {
    start: Instant,
    topics: Vec<DemoTopic>,
    announced: bool,
}

impl DemoBackend {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            topics: default_topics(),
            announced: false,
        }
    }
}

impl Default for DemoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RosBackend for DemoBackend {
    fn label(&self) -> &'static str {
        "demo"
    }

    fn poll(&mut self, budget: Duration) -> Vec<BackendEvent> {
        let mut out = Vec::new();
        if !self.announced {
            for t in &self.topics {
                out.push(BackendEvent::Topic {
                    name: t.name.into(),
                    type_name: t.type_name.into(),
                    publishers: t.publishers,
                    subscribers: t.subscribers,
                });
            }
            self.announced = true;
        }
        let deadline = Instant::now() + budget;
        loop {
            let now = self.start.elapsed();
            // Emit any topics whose next_emit time has arrived.
            let mut emitted_any = false;
            for t in self.topics.iter_mut() {
                while t.next_emit <= now {
                    let value = build_value(t);
                    out.push(BackendEvent::Sample {
                        name: t.name.into(),
                        bytes: t.bytes_per_msg,
                        value,
                        at: Instant::now(),
                    });
                    t.seq = t.seq.wrapping_add(1);
                    let period_s = 1.0 / t.rate_hz;
                    let jitter_s = if t.jitter > 0.0 {
                        let phase = (t.seq as f64 * 1.6180339887) % 1.0;
                        (phase - 0.5) * 2.0 * period_s * t.jitter
                    } else {
                        0.0
                    };
                    t.next_emit += Duration::from_secs_f64((period_s + jitter_s).max(1e-4));
                    emitted_any = true;
                }
            }
            // If we've passed the budget, exit. Otherwise sleep until next event.
            let now2 = Instant::now();
            if now2 >= deadline {
                break;
            }
            if !emitted_any {
                // Sleep until next emit or deadline, whichever is sooner.
                let next = self
                    .topics
                    .iter()
                    .map(|t| t.next_emit)
                    .min()
                    .unwrap_or(Duration::from_millis(50));
                let until_next = next.saturating_sub(self.start.elapsed());
                let sleep_for = until_next.min(deadline - now2);
                if !sleep_for.is_zero() {
                    std::thread::sleep(sleep_for);
                }
            }
        }
        out
    }
}

fn build_value(t: &DemoTopic) -> DynamicValue {
    let phase = (t.seq as f64 * 0.1).sin();
    let now_s = t.next_emit.as_secs_f64();
    match t.type_name {
        "sensor_msgs/msg/LaserScan" => DynamicValue::Struct(vec![
            (
                "header".into(),
                DynamicValue::Struct(vec![
                    ("frame_id".into(), DynamicValue::Str("laser".into())),
                    ("stamp_sec".into(), DynamicValue::F64(now_s)),
                ]),
            ),
            ("angle_min".into(), DynamicValue::F64(-std::f64::consts::PI)),
            ("angle_max".into(), DynamicValue::F64(std::f64::consts::PI)),
            ("ranges".into(), DynamicValue::Bytes(720 * 4)),
        ]),
        "sensor_msgs/msg/Image" => DynamicValue::Struct(vec![
            (
                "header".into(),
                DynamicValue::Struct(vec![
                    ("frame_id".into(), DynamicValue::Str("camera".into())),
                    ("stamp_sec".into(), DynamicValue::F64(now_s)),
                ]),
            ),
            ("height".into(), DynamicValue::U64(720)),
            ("width".into(), DynamicValue::U64(1280)),
            ("encoding".into(), DynamicValue::Str("rgb8".into())),
            ("data".into(), DynamicValue::Bytes(720 * 1280 * 3)),
        ]),
        "geometry_msgs/msg/Twist" => DynamicValue::Struct(vec![
            (
                "linear".into(),
                DynamicValue::Struct(vec![
                    ("x".into(), DynamicValue::F64(0.45 + 0.05 * phase)),
                    ("y".into(), DynamicValue::F64(0.0)),
                    ("z".into(), DynamicValue::F64(0.0)),
                ]),
            ),
            (
                "angular".into(),
                DynamicValue::Struct(vec![
                    ("x".into(), DynamicValue::F64(0.0)),
                    ("y".into(), DynamicValue::F64(0.0)),
                    ("z".into(), DynamicValue::F64(0.3 * phase)),
                ]),
            ),
        ]),
        "tf2_msgs/msg/TFMessage" => DynamicValue::Struct(vec![(
            "transforms".into(),
            DynamicValue::Array(vec![DynamicValue::Struct(vec![
                (
                    "child_frame_id".into(),
                    DynamicValue::Str("base_link".into()),
                ),
                ("parent_frame_id".into(), DynamicValue::Str("odom".into())),
                ("translation_x".into(), DynamicValue::F64(2.5 + phase)),
            ])]),
        )]),
        "nav_msgs/msg/Odometry" => DynamicValue::Struct(vec![
            (
                "header".into(),
                DynamicValue::Struct(vec![
                    ("frame_id".into(), DynamicValue::Str("odom".into())),
                    ("stamp_sec".into(), DynamicValue::F64(now_s)),
                ]),
            ),
            (
                "child_frame_id".into(),
                DynamicValue::Str("base_link".into()),
            ),
            (
                "pose_position".into(),
                DynamicValue::Struct(vec![
                    ("x".into(), DynamicValue::F64(2.5 + phase)),
                    ("y".into(), DynamicValue::F64(1.0 + 0.2 * phase)),
                    ("z".into(), DynamicValue::F64(0.0)),
                ]),
            ),
            (
                "twist_linear".into(),
                DynamicValue::Struct(vec![
                    ("x".into(), DynamicValue::F64(0.45)),
                    ("y".into(), DynamicValue::F64(0.0)),
                    ("z".into(), DynamicValue::F64(0.0)),
                ]),
            ),
        ]),
        "diagnostic_msgs/msg/DiagnosticArray" => DynamicValue::Struct(vec![
            (
                "header".into(),
                DynamicValue::Struct(vec![("stamp_sec".into(), DynamicValue::F64(now_s))]),
            ),
            (
                "status".into(),
                DynamicValue::Array(vec![
                    DynamicValue::Struct(vec![
                        ("name".into(), DynamicValue::Str("battery".into())),
                        ("level".into(), DynamicValue::U64(0)),
                        ("message".into(), DynamicValue::Str("OK 78%".into())),
                    ]),
                    DynamicValue::Struct(vec![
                        ("name".into(), DynamicValue::Str("motors".into())),
                        ("level".into(), DynamicValue::U64(0)),
                        ("message".into(), DynamicValue::Str("OK".into())),
                    ]),
                ]),
            ),
        ]),
        _ => DynamicValue::Struct(vec![("seq".into(), DynamicValue::U64(t.seq))]),
    }
}

fn default_topics() -> Vec<DemoTopic> {
    let zero = Duration::from_secs(0);
    vec![
        DemoTopic {
            name: "/scan",
            type_name: "sensor_msgs/msg/LaserScan",
            publishers: 1,
            subscribers: 2,
            rate_hz: 40.0,
            bytes_per_msg: 2_900,
            next_emit: zero,
            seq: 0,
            jitter: 0.02,
        },
        DemoTopic {
            name: "/camera/image_raw",
            type_name: "sensor_msgs/msg/Image",
            publishers: 1,
            subscribers: 1,
            rate_hz: 30.0,
            bytes_per_msg: 720 * 1280 * 3 + 60,
            next_emit: zero,
            seq: 0,
            jitter: 0.05,
        },
        DemoTopic {
            name: "/cmd_vel",
            type_name: "geometry_msgs/msg/Twist",
            publishers: 1,
            subscribers: 1,
            rate_hz: 100.0,
            bytes_per_msg: 48,
            next_emit: zero,
            seq: 0,
            jitter: 0.01,
        },
        DemoTopic {
            name: "/tf",
            type_name: "tf2_msgs/msg/TFMessage",
            publishers: 3,
            subscribers: 4,
            rate_hz: 50.0,
            bytes_per_msg: 220,
            next_emit: zero,
            seq: 0,
            jitter: 0.03,
        },
        DemoTopic {
            name: "/odom",
            type_name: "nav_msgs/msg/Odometry",
            publishers: 1,
            subscribers: 2,
            rate_hz: 50.0,
            bytes_per_msg: 720,
            next_emit: zero,
            seq: 0,
            jitter: 0.01,
        },
        DemoTopic {
            name: "/diagnostics",
            type_name: "diagnostic_msgs/msg/DiagnosticArray",
            publishers: 5,
            subscribers: 1,
            rate_hz: 1.0,
            bytes_per_msg: 1_400,
            next_emit: zero,
            seq: 0,
            jitter: 0.10,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announces_topics_on_first_poll() {
        let mut b = DemoBackend::new();
        let events = b.poll(Duration::from_millis(0));
        let topic_events: Vec<&BackendEvent> = events
            .iter()
            .filter(|e| matches!(e, BackendEvent::Topic { .. }))
            .collect();
        assert!(
            topic_events.len() >= 5,
            "expected several topics, got {topic_events:?}"
        );
        let mut b2 = b;
        let events2 = b2.poll(Duration::from_millis(0));
        assert!(
            events2
                .iter()
                .all(|e| !matches!(e, BackendEvent::Topic { .. })),
            "topics should not be re-announced on subsequent polls"
        );
    }

    #[test]
    fn emits_samples_over_time() {
        let mut b = DemoBackend::new();
        let _ = b.poll(Duration::from_millis(0));
        let events = b.poll(Duration::from_millis(200));
        let samples: Vec<&BackendEvent> = events
            .iter()
            .filter(|e| matches!(e, BackendEvent::Sample { .. }))
            .collect();
        assert!(!samples.is_empty(), "expected samples in 200 ms, got none");
    }

    #[test]
    fn cmd_vel_emits_about_100_hz() {
        let mut b = DemoBackend::new();
        let _ = b.poll(Duration::from_millis(0));
        let events = b.poll(Duration::from_millis(500));
        let cmd_vel_samples = events
            .iter()
            .filter(|e| matches!(e, BackendEvent::Sample { name, .. } if name == "/cmd_vel"))
            .count();
        // 100 Hz × 0.5 s = ~50 ± slop
        assert!(
            (30..=70).contains(&cmd_vel_samples),
            "expected ~50 cmd_vel samples in 500 ms, got {cmd_vel_samples}"
        );
    }
}
