//! Bounded process-isolated ROS domain scanner.

use std::collections::VecDeque;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context as _;

use crate::domain::{DomainId, DomainProbeResult};

#[derive(Debug, Clone, Copy)]
pub struct ScanConfig {
    pub discovery_window: Duration,
    pub process_timeout: Duration,
    pub concurrency: usize,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            discovery_window: Duration::from_millis(800),
            process_timeout: Duration::from_secs(2),
            concurrency: 3,
        }
    }
}

#[derive(Debug)]
pub enum ScanUpdate {
    Started(DomainId),
    Probed(DomainProbeResult),
    Failed { domain: DomainId, message: String },
    Finished,
}

pub struct DomainScan {
    pub updates: Receiver<ScanUpdate>,
    cancel: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<()>>,
}

impl DomainScan {
    pub fn start(
        domains: impl IntoIterator<Item = DomainId>,
        config: ScanConfig,
    ) -> anyhow::Result<Self> {
        let executable = std::env::current_exe().context("cannot locate rostop executable")?;
        let queue = Arc::new(Mutex::new(domains.into_iter().collect::<VecDeque<_>>()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (updates_tx, updates) = mpsc::channel();
        let worker_count = config.concurrency.max(1).min(queue.lock().unwrap().len().max(1));
        let remaining = Arc::new(Mutex::new(worker_count));
        let mut workers = Vec::with_capacity(worker_count);

        for _ in 0..worker_count {
            let executable = executable.clone();
            let queue = Arc::clone(&queue);
            let cancel = Arc::clone(&cancel);
            let updates_tx = updates_tx.clone();
            let remaining = Arc::clone(&remaining);
            workers.push(thread::spawn(move || {
                while !cancel.load(Ordering::Relaxed) {
                    let domain = queue.lock().unwrap().pop_front();
                    let Some(domain) = domain else { break };
                    let _ = updates_tx.send(ScanUpdate::Started(domain));
                    match run_probe_process(&executable, domain, config, &cancel) {
                        Ok(result) => {
                            let _ = updates_tx.send(ScanUpdate::Probed(result));
                        }
                        Err(error) => {
                            let _ = updates_tx.send(ScanUpdate::Failed {
                                domain,
                                message: error.to_string(),
                            });
                        }
                    }
                }
                let mut remaining = remaining.lock().unwrap();
                *remaining -= 1;
                if *remaining == 0 {
                    let _ = updates_tx.send(ScanUpdate::Finished);
                }
            }));
        }

        Ok(Self {
            updates,
            cancel,
            workers,
        })
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for DomainScan {
    fn drop(&mut self) {
        self.cancel();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn run_probe_process(
    executable: &std::path::Path,
    domain: DomainId,
    config: ScanConfig,
    cancel: &AtomicBool,
) -> anyhow::Result<DomainProbeResult> {
    let mut child = Command::new(executable)
        .arg("--probe-domain")
        .arg(domain.to_string())
        .arg("--probe-ms")
        .arg(config.discovery_window.as_millis().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start domain {domain} probe"))?;
    let deadline = Instant::now() + config.process_timeout;

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("scan cancelled");
        }
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("domain {domain} probe timed out");
        }
        thread::sleep(Duration::from_millis(20));
    }

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)?;
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)?;
    }
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!(
            "domain {domain} probe exited with {status}: {}",
            stderr.trim()
        );
    }
    let result: DomainProbeResult =
        serde_json::from_str(stdout.trim()).context("invalid domain probe response")?;
    if result.protocol_version != DomainProbeResult::PROTOCOL_VERSION
        || result.domain_id != domain.get()
    {
        anyhow::bail!("domain {domain} probe returned mismatched protocol data");
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_defaults_are_bounded() {
        let config = ScanConfig::default();
        assert!(config.concurrency <= 4);
        assert!(config.process_timeout > config.discovery_window);
    }
}
