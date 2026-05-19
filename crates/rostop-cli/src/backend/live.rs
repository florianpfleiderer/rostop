//! Live backend backed by [`r2r`] — the real ROS 2 implementation.
//!
//! Gated behind the `live` cargo feature so hosts without a ROS 2 install can
//! still build and run `cargo test --workspace` against `rostop-core` and the
//! demo backend.
//!
//! Architecture: a single OS thread owns the `r2r::Node` and a
//! `futures::executor::LocalPool`. It loops over `node.spin_once` +
//! `pool.run_until_stalled`, polling the ROS graph on a 500 ms cadence and
//! forwarding events to the UI thread over an `std::sync::mpsc` channel.
//! Wire bytes come from `subscribe_raw` (accurate Hz/BW). Each sample is
//! decoded for field inspection via `r2r::WrappedNativeMsgUntyped` —
//! deserialising the CDR payload into a `serde_json::Value` that is then
//! mapped to [`DynamicValue`]. On decode failure (e.g. type-support not
//! available) we fall back to `DynamicValue::Bytes(len)`.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use futures::executor::LocalPool;
use futures::task::LocalSpawnExt;
use futures::StreamExt;

use rostop_core::endpoint::{
    normalise_duration, DurabilityKind, EndpointInfo, HistoryKind, LivelinessKind, QosSnapshot,
    ReliabilityKind,
};
use rostop_core::message::DynamicValue;

use crate::backend::{BackendEvent, RosBackend};

const GRAPH_POLL_INTERVAL: Duration = Duration::from_millis(500);
const SPIN_TICK: Duration = Duration::from_millis(50);

/// rostop's own ROS node name.
const SELF_NODE_NAME: &str = "rostop";

pub struct LiveBackend {
    rx: Receiver<BackendEvent>,
    shutdown_tx: Sender<()>,
    spin_thread: Option<thread::JoinHandle<()>>,
}

impl LiveBackend {
    pub fn new() -> anyhow::Result<Self> {
        let (event_tx, event_rx) = mpsc::channel::<BackendEvent>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        let (init_tx, init_rx) = mpsc::sync_channel::<anyhow::Result<()>>(1);

        let spin_thread = thread::Builder::new()
            .name("rostop-live-spin".into())
            .spawn(move || {
                spin_loop(event_tx, shutdown_rx, init_tx);
            })
            .context("failed to spawn rostop-live-spin thread")?;

        match init_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                rx: event_rx,
                shutdown_tx,
                spin_thread: Some(spin_thread),
            }),
            Ok(Err(e)) => {
                let _ = spin_thread.join();
                Err(e)
            }
            Err(_) => Err(anyhow::anyhow!(
                "spin thread exited before reporting init status"
            )),
        }
    }
}

impl Drop for LiveBackend {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.spin_thread.take() {
            let _ = handle.join();
        }
    }
}

impl RosBackend for LiveBackend {
    fn poll(&mut self, budget: Duration) -> Vec<BackendEvent> {
        let mut out = Vec::new();
        // Drain everything currently buffered.
        while let Ok(ev) = self.rx.try_recv() {
            out.push(ev);
        }
        // If nothing was buffered, wait up to `budget` for the first event,
        // then drain the rest non-blockingly.
        if out.is_empty() {
            if let Ok(ev) = self.rx.recv_timeout(budget) {
                out.push(ev);
                while let Ok(ev) = self.rx.try_recv() {
                    out.push(ev);
                }
            }
        }
        out
    }

    fn label(&self) -> &'static str {
        "live"
    }
}

fn spin_loop(
    event_tx: Sender<BackendEvent>,
    shutdown_rx: Receiver<()>,
    init_tx: mpsc::SyncSender<anyhow::Result<()>>,
) {
    let ctx = match r2r::Context::create().context("r2r::Context::create failed") {
        Ok(c) => c,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };
    let mut node =
        match r2r::Node::create(ctx, SELF_NODE_NAME, "").context("r2r::Node::create failed") {
            Ok(n) => n,
            Err(e) => {
                let _ = init_tx.send(Err(e));
                return;
            }
        };

    let mut pool = LocalPool::new();
    let spawner = pool.spawner();
    let mut known: HashMap<String, String> = HashMap::new();
    let mut last_poll = Instant::now()
        .checked_sub(GRAPH_POLL_INTERVAL)
        .unwrap_or_else(Instant::now);

    // Always start. Previous releases ran a 2 s probe and refused to open
    // when foreign publishers were seen with zero decoded samples; that
    // misfired on idle systems (e.g. only `/rosout` from the daemon) and
    // was unhelpful even when correct — the user wanted a topic viewer,
    // not a diagnostics gate.
    let _ = init_tx.send(Ok(()));

    loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }

        if last_poll.elapsed() >= GRAPH_POLL_INTERVAL {
            last_poll = Instant::now();
            if let Ok(nt) = node.get_topic_names_and_types() {
                let current: std::collections::HashSet<String> = nt.keys().cloned().collect();

                for (name, types) in &nt {
                    let Some(ty) = types.first() else { continue };
                    let pubs_info = node
                        .get_publishers_info_by_topic(name, false)
                        .unwrap_or_default();
                    let publishers = pubs_info.len() as u32;
                    // Map inline: r2r 0.9.5 does not re-export TopicEndpointInfo
                    // (it lives in a private `nodes` module) so we cannot name
                    // it as a function parameter. The public fields are still
                    // reachable through type inference here.
                    //
                    // GID is a fixed-size array whose bound r2r's bindgen
                    // copies from the active rcl headers — 24 bytes on Humble,
                    // 16 on Jazzy. We store the raw bytes verbatim so the size
                    // doesn't matter to the renderer.
                    let publisher_infos: Vec<EndpointInfo> = pubs_info
                        .into_iter()
                        .map(|info| EndpointInfo {
                            node_name: info.node_name,
                            node_namespace: info.node_namespace,
                            topic_type: info.topic_type,
                            endpoint_gid: info.endpoint_gid.to_vec(),
                            qos: qos_from_r2r(info.qos_profile),
                        })
                        .collect();

                    // Subscriber endpoint info is not reachable: r2r::Node
                    // exposes node_handle as pub(crate) only, and there is no
                    // public accessor for the *const rcl_node_t we'd need to
                    // call rcl_get_subscriptions_info_by_topic ourselves. We
                    // emit None so the UI shows "(not available)" rather than
                    // a misleading empty list. See follow-up issue.
                    let _ = event_tx.send(BackendEvent::Endpoints {
                        topic: name.clone(),
                        publishers: Some(publisher_infos),
                        subscribers: None,
                    });

                    if known.contains_key(name) {
                        continue;
                    }
                    let _ = event_tx.send(BackendEvent::Topic {
                        name: name.clone(),
                        type_name: ty.clone(),
                        publishers,
                        subscribers: 0,
                    });
                    known.insert(name.clone(), ty.clone());

                    // Per-topic sample forwarder. subscribe_raw for accurate
                    // wire-byte counts; decode via WrappedNativeMsgUntyped.
                    if let Ok(stream) = node.subscribe_raw(name, ty, r2r::QosProfile::default()) {
                        let name_owned = name.clone();
                        let ty_owned = ty.clone();
                        let tx = event_tx.clone();
                        let _ = spawner.spawn_local(async move {
                            let mut stream = stream;
                            let mut decoder =
                                r2r::WrappedNativeMsgUntyped::new_from(&ty_owned).ok();
                            let mut emitted_decode_failure = false;
                            while let Some(bytes) = stream.next().await {
                                let (value, decode_failed) =
                                    decode_sample(decoder.as_mut(), &bytes);
                                if decode_failed && !emitted_decode_failure {
                                    emitted_decode_failure = true;
                                    let _ = tx.send(BackendEvent::DecodeFailure {
                                        topic: name_owned.clone(),
                                        type_name: ty_owned.clone(),
                                    });
                                }
                                if tx
                                    .send(BackendEvent::Sample {
                                        name: name_owned.clone(),
                                        bytes: bytes.len() as u32,
                                        value,
                                        at: Instant::now(),
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        });
                    }
                }

                let gone: Vec<String> = known
                    .keys()
                    .filter(|k| !current.contains(*k))
                    .cloned()
                    .collect();
                for name in gone {
                    known.remove(&name);
                    let _ = event_tx.send(BackendEvent::TopicRemoved(name));
                }
            }
        }

        node.spin_once(SPIN_TICK);
        pool.run_until_stalled();
    }
}

/// Decode one raw CDR sample into a `DynamicValue`.
///
/// Returns `(DynamicValue::Bytes(len), false)` if no decoder is compiled in
/// for this type (missing type-support — a different error class, not a
/// mismatch). Returns `(DynamicValue::Bytes(len), true)` if a decoder exists
/// but `from_serialized_bytes` / `to_json` fails — the signature of a ROS 2
/// distro or RMW mismatch on the wire. Callers use the boolean to decide
/// whether to emit a `BackendEvent::DecodeFailure`.
fn decode_sample(
    decoder: Option<&mut r2r::WrappedNativeMsgUntyped>,
    bytes: &[u8],
) -> (DynamicValue, bool) {
    match decoder {
        None => (DynamicValue::Bytes(bytes.len()), false),
        Some(d) => {
            if d.from_serialized_bytes(bytes).is_ok() {
                if let Ok(json) = d.to_json() {
                    return (json_to_dynamic(json), false);
                }
            }
            (DynamicValue::Bytes(bytes.len()), true)
        }
    }
}

/// Cutoff above which an all-primitive array is replaced by `ArrayElided`
/// rather than materialised element-by-element.
///
/// A `sensor_msgs/msg/Image` published at 30 Hz has ~2.76 M uint8 elements per
/// frame — fully decoding it produces ~150 MB of `DynamicValue::U64`s every
/// frame and pegs both CPU and RSS. Real diagnostic arrays (LaserScan ranges
/// at ~720–1080, JointState position at <100, transforms list at <20) all sit
/// comfortably below the cutoff, so the user can still drill into them.
const MAX_INLINE_ARRAY_LEN: usize = 4096;

/// Map a `serde_json::Value` produced by `WrappedNativeMsgUntyped::to_json`
/// onto the structural `DynamicValue` shape the inspector renders.
///
/// Large arrays of primitive leaves (e.g. an Image's `data: uint8[]`) are
/// summarised as [`DynamicValue::ArrayElided`] rather than materialised — see
/// [`MAX_INLINE_ARRAY_LEN`].
fn json_to_dynamic(v: serde_json::Value) -> DynamicValue {
    use serde_json::Value;
    match v {
        Value::Null => DynamicValue::Str("null".into()),
        Value::Bool(b) => DynamicValue::Bool(b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                DynamicValue::I64(i)
            } else if let Some(u) = n.as_u64() {
                DynamicValue::U64(u)
            } else if let Some(f) = n.as_f64() {
                DynamicValue::F64(f)
            } else {
                DynamicValue::Str(n.to_string())
            }
        }
        Value::String(s) => DynamicValue::Str(s),
        Value::Array(items) => {
            if items.len() > MAX_INLINE_ARRAY_LEN && items.iter().all(is_json_leaf_primitive) {
                DynamicValue::ArrayElided(items.len())
            } else {
                DynamicValue::Array(items.into_iter().map(json_to_dynamic).collect())
            }
        }
        Value::Object(map) => DynamicValue::Struct(
            map.into_iter()
                .map(|(k, v)| (k, json_to_dynamic(v)))
                .collect(),
        ),
    }
}

/// True for JSON values that decode to a single scalar `DynamicValue` (i.e.
/// not a struct or array). Used to detect the "huge bulk-data" pattern where
/// every element is a primitive number — characteristic of message fields
/// like `Image::data`, `PointCloud2::data`, or `LaserScan::ranges`.
fn is_json_leaf_primitive(v: &serde_json::Value) -> bool {
    matches!(
        v,
        serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_)
    )
}

fn qos_from_r2r(q: r2r::QosProfile) -> QosSnapshot {
    use r2r::qos::{DurabilityPolicy, HistoryPolicy, LivelinessPolicy, ReliabilityPolicy};

    let reliability = match q.reliability {
        ReliabilityPolicy::Reliable => ReliabilityKind::Reliable,
        ReliabilityPolicy::BestEffort => ReliabilityKind::BestEffort,
        _ => ReliabilityKind::Unknown,
    };
    let durability = match q.durability {
        DurabilityPolicy::TransientLocal => DurabilityKind::TransientLocal,
        DurabilityPolicy::Volatile => DurabilityKind::Volatile,
        _ => DurabilityKind::Unknown,
    };
    let history = match q.history {
        HistoryPolicy::KeepLast => HistoryKind::KeepLast,
        HistoryPolicy::KeepAll => HistoryKind::KeepAll,
        _ => HistoryKind::Unknown,
    };
    let liveliness = match q.liveliness {
        LivelinessPolicy::Automatic => LivelinessKind::Automatic,
        LivelinessPolicy::ManualByTopic => LivelinessKind::ManualByTopic,
        _ => LivelinessKind::Unknown,
    };

    QosSnapshot {
        reliability,
        durability,
        history,
        depth: q.depth,
        deadline: normalise_duration(q.deadline),
        lifespan: normalise_duration(q.lifespan),
        liveliness,
        liveliness_lease: normalise_duration(q.liveliness_lease_duration),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};

    #[test]
    fn json_to_dynamic_maps_a_joint_state_shape() {
        // Mirrors the `sensor_msgs/msg/JointState` payload reported in the
        // wild: header struct, parallel arrays of names / positions /
        // velocities / efforts. The whole tree must come through as a
        // Struct with nested Arrays — not collapsed into DynamicValue::Bytes.
        let raw = serde_json::json!({
            "header": { "stamp": { "sec": 1, "nanosec": 2 }, "frame_id": "base_link" },
            "name": ["a", "b"],
            "position": [0.1, 0.2],
            "velocity": [-0.001, 0.0],
            "effort": [0.058, -0.012],
        });
        let v = json_to_dynamic(raw);
        let DynamicValue::Struct(fields) = &v else {
            panic!("expected Struct, got {v:?}");
        };
        let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"header"));
        assert!(keys.contains(&"name"));
        assert!(keys.contains(&"position"));
        let position = fields.iter().find(|(k, _)| k == "position").unwrap();
        let DynamicValue::Array(items) = &position.1 else {
            panic!("position should be an array");
        };
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], DynamicValue::F64(_)));
    }

    #[test]
    fn decode_sample_falls_back_to_bytes_when_no_decoder() {
        let payload = [1u8, 2, 3, 4];
        let (v, failed) = decode_sample(None, &payload);
        assert_eq!(v, DynamicValue::Bytes(4));
        // Missing type-support is a different error class from a distro/RMW
        // mismatch — don't treat it as a decode failure.
        assert!(!failed);
    }

    #[test]
    fn json_to_dynamic_elides_image_sized_byte_array() {
        // sensor_msgs/msg/Image: header + height/width/encoding/step + data[uint8].
        // The `data` field is the dangerous one — for a 720×1280×3 frame that's
        // ~2.76M elements, which would be ~150 MB of DynamicValue::U64 per frame.
        // After elision it must be a single `ArrayElided(len)` summary scalar.
        let big_len = MAX_INLINE_ARRAY_LEN + 10;
        let raw = serde_json::json!({
            "header": { "stamp": { "sec": 1, "nanosec": 2 }, "frame_id": "camera" },
            "height": 720u64,
            "width": 1280u64,
            "encoding": "rgb8",
            "step": 3840u64,
            "data": vec![0u8; big_len],
        });
        let v = json_to_dynamic(raw);
        let DynamicValue::Struct(fields) = &v else {
            panic!("expected Struct, got {v:?}");
        };
        let data = fields
            .iter()
            .find(|(k, _)| k == "data")
            .map(|(_, v)| v)
            .expect("Image has a data field");
        assert_eq!(
            data,
            &DynamicValue::ArrayElided(big_len),
            "data was not elided"
        );
        // The other (small) fields stay fully materialised.
        let header = fields
            .iter()
            .find(|(k, _)| k == "header")
            .map(|(_, v)| v)
            .unwrap();
        assert!(matches!(header, DynamicValue::Struct(_)));
    }

    #[test]
    fn json_to_dynamic_keeps_pointcloud_sized_pointfield_array() {
        // PointCloud2.fields is a struct array of length 4-ish (xyz + intensity).
        // Even at the elision threshold, an array of structs should never be
        // collapsed — the user needs to drill in to inspect individual fields.
        let items: Vec<serde_json::Value> = (0..16)
            .map(|i| serde_json::json!({"name": format!("f{i}"), "offset": i * 4}))
            .collect();
        let v = json_to_dynamic(serde_json::Value::Array(items));
        let DynamicValue::Array(arr) = &v else {
            panic!("expected an Array of structs, got {v:?}");
        };
        assert_eq!(arr.len(), 16);
        assert!(matches!(arr[0], DynamicValue::Struct(_)));
    }

    #[test]
    fn json_to_dynamic_keeps_laserscan_sized_ranges_array() {
        // A 1080-element float32 ranges array is below the cutoff and must
        // stay drillable so the user can confirm a specific beam.
        let ranges: Vec<serde_json::Value> = (0..1080)
            .map(|i| serde_json::json!(i as f64 * 0.01))
            .collect();
        let v = json_to_dynamic(serde_json::Value::Array(ranges));
        let DynamicValue::Array(arr) = &v else {
            panic!("expected an Array, got {v:?}");
        };
        assert_eq!(arr.len(), 1080);
    }

    #[test]
    fn json_to_dynamic_elides_large_float_array() {
        // A wonkily-large LaserScan or pointfield could blow past the cutoff
        // entirely as a flat float array — still primitives, still elide.
        let big_len = MAX_INLINE_ARRAY_LEN * 2;
        let arr: Vec<serde_json::Value> = (0..big_len).map(|_| serde_json::json!(0.0)).collect();
        let v = json_to_dynamic(serde_json::Value::Array(arr));
        assert_eq!(v, DynamicValue::ArrayElided(big_len));
    }

    #[test]
    fn json_to_dynamic_keeps_tf_message_with_many_transforms() {
        // tf2_msgs/msg/TFMessage occasionally publishes hundreds of transforms.
        // They're structs, so they must remain individually drillable.
        let count = MAX_INLINE_ARRAY_LEN / 4;
        let items: Vec<serde_json::Value> = (0..count)
            .map(|i| {
                serde_json::json!({
                    "child_frame_id": format!("link_{i}"),
                    "parent_frame_id": "base_link",
                    "transform": { "translation": { "x": 0.0, "y": 0.0, "z": 0.0 } }
                })
            })
            .collect();
        let v = json_to_dynamic(serde_json::Value::Array(items));
        let DynamicValue::Array(arr) = &v else {
            panic!("transforms should stay an Array, got {v:?}");
        };
        assert_eq!(arr.len(), count);
        assert!(matches!(arr[0], DynamicValue::Struct(_)));
    }

    #[test]
    fn new_succeeds_and_label_is_live() {
        let backend = LiveBackend::new().expect("LiveBackend::new should succeed");
        assert_eq!(backend.label(), "live");
    }

    /// Kills a child process on drop. Keeps test cleanup robust even on panic.
    struct ProcessGuard(Child);
    impl Drop for ProcessGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn spawn_publisher(topic: &str, ty: &str, payload: &str) -> ProcessGuard {
        let child = Command::new("ros2")
            .args(["topic", "pub", "-r", "10", topic, ty, payload])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("ros2 topic pub failed to spawn — is ROS 2 sourced?");
        ProcessGuard(child)
    }

    fn poll_until(
        backend: &mut LiveBackend,
        deadline: Instant,
        mut pred: impl FnMut(&BackendEvent) -> bool,
    ) -> bool {
        while Instant::now() < deadline {
            for ev in backend.poll(Duration::from_millis(200)) {
                if pred(&ev) {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn subscribe_emits_sample_events_with_nonzero_bytes() {
        let _pub = spawn_publisher(
            "/rostop_test_samples",
            "std_msgs/msg/String",
            "{data: 'payload-bytes'}",
        );
        let mut backend = LiveBackend::new().expect("LiveBackend new");
        let deadline = Instant::now() + Duration::from_secs(10);
        let saw_sample = poll_until(&mut backend, deadline, |ev| {
            matches!(
                ev,
                BackendEvent::Sample { name, bytes, .. }
                    if name == "/rostop_test_samples" && *bytes > 0
            )
        });
        assert!(
            saw_sample,
            "did not see a Sample event with bytes > 0 for /rostop_test_samples within 10s"
        );
    }

    #[test]
    fn graph_poll_emits_topic_event_for_a_live_publisher() {
        let _pub = spawn_publisher("/rostop_test_graph", "std_msgs/msg/String", "{data: 'hi'}");
        let mut backend = LiveBackend::new().expect("LiveBackend new");
        let deadline = Instant::now() + Duration::from_secs(10);
        let saw_topic = poll_until(&mut backend, deadline, |ev| {
            matches!(
                ev,
                BackendEvent::Topic { name, type_name, .. }
                    if name == "/rostop_test_graph"
                        && type_name == "std_msgs/msg/String"
            )
        });
        assert!(
            saw_topic,
            "did not see Topic event for /rostop_test_graph within 10s"
        );
    }
}
