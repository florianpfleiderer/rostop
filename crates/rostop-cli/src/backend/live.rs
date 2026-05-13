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
//! Sample bytes come from `subscribe_raw` (no JSON decode — accurate Hz/BW,
//! `DynamicValue::Bytes(len)` payload). Field-level inspection of live topics
//! is a v0.2 item.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use futures::executor::LocalPool;
use futures::task::LocalSpawnExt;
use futures::StreamExt;

use rostop_core::message::DynamicValue;

use crate::backend::{BackendEvent, RosBackend};

const GRAPH_POLL_INTERVAL: Duration = Duration::from_millis(500);
const SPIN_TICK: Duration = Duration::from_millis(50);

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
    let mut node = match r2r::Node::create(ctx, "rostop", "").context("r2r::Node::create failed") {
        Ok(n) => n,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };
    let _ = init_tx.send(Ok(()));

    let mut pool = LocalPool::new();
    let spawner = pool.spawner();
    let mut known: HashMap<String, String> = HashMap::new();
    let mut last_poll = Instant::now()
        .checked_sub(GRAPH_POLL_INTERVAL)
        .unwrap_or_else(Instant::now);

    loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }

        if last_poll.elapsed() >= GRAPH_POLL_INTERVAL {
            last_poll = Instant::now();
            if let Ok(nt) = node.get_topic_names_and_types() {
                let current: HashSet<String> = nt.keys().cloned().collect();

                // Additions
                for (name, types) in &nt {
                    if known.contains_key(name) {
                        continue;
                    }
                    let Some(ty) = types.first() else { continue };
                    let publishers = node
                        .get_publishers_info_by_topic(name, false)
                        .map(|v| v.len() as u32)
                        .unwrap_or(0);
                    let _ = event_tx.send(BackendEvent::Topic {
                        name: name.clone(),
                        type_name: ty.clone(),
                        publishers,
                        subscribers: 0,
                    });
                    known.insert(name.clone(), ty.clone());

                    // Spawn a per-topic sample forwarder. Uses subscribe_raw
                    // for accurate wire-byte counts; field decoding is a v0.2
                    // item, so the payload is DynamicValue::Bytes(len).
                    if let Ok(stream) = node.subscribe_raw(name, ty, r2r::QosProfile::default()) {
                        let name_owned = name.clone();
                        let tx = event_tx.clone();
                        let _ = spawner.spawn_local(async move {
                            let mut stream = stream;
                            while let Some(bytes) = stream.next().await {
                                if tx
                                    .send(BackendEvent::Sample {
                                        name: name_owned.clone(),
                                        bytes: bytes.len() as u32,
                                        value: DynamicValue::Bytes(bytes.len()),
                                        at: Instant::now(),
                                    })
                                    .is_err()
                                {
                                    break; // receiver dropped → bail out
                                }
                            }
                        });
                    }
                }

                // Removals
                let gone: Vec<String> = known
                    .keys()
                    .filter(|k| !current.contains(*k))
                    .cloned()
                    .collect();
                for name in gone {
                    known.remove(&name);
                    let _ = event_tx.send(BackendEvent::TopicRemoved(name));
                    // Sample task for this topic will exit on next Sample failure or remain dormant.
                }
            }
        }

        node.spin_once(SPIN_TICK);
        pool.run_until_stalled();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};

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
