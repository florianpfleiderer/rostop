//! Per-topic rolling statistics: rate, bandwidth, jitter, drop detection.
//!
//! `TopicStats` accepts a stream of message samples — each carrying an arrival
//! instant (as a logical timestamp in nanoseconds from a monotonic clock),
//! the serialized message size in bytes, and an optional `header.stamp`
//! sequence-like value used for drop/latency analysis — and computes rolling
//! statistics over a configurable sliding time window.
//!
//! The crate exposes these as pure data-in/data-out functions so they can be
//! unit-tested without any ROS2 runtime.

use std::collections::VecDeque;

/// Rolling per-topic statistics over a sliding nanosecond window.
#[derive(Debug)]
pub struct TopicStats {
    window_ns: u64,
    samples: VecDeque<Sample>,
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    t_ns: u64,
    bytes: u32,
}

impl TopicStats {
    /// Create a new stats accumulator with the given rolling window in nanoseconds.
    pub fn new(window_ns: u64) -> Self {
        Self {
            window_ns,
            samples: VecDeque::new(),
        }
    }

    /// Record a single message arrival at `t_ns` with `bytes` payload size.
    /// Older samples falling outside the rolling window are evicted so memory
    /// remains bounded regardless of stream length.
    pub fn record(&mut self, t_ns: u64, bytes: u32) {
        self.samples.push_back(Sample { t_ns, bytes });
        let cutoff = t_ns.saturating_sub(self.window_ns);
        while let Some(front) = self.samples.front() {
            if front.t_ns < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Number of samples retained in memory. Bounded by the rolling window.
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Average frequency in Hz over the window ending at `now_ns`.
    pub fn hz(&self, now_ns: u64) -> f64 {
        let (count, _bytes) = self.window_summary(now_ns);
        if count == 0 {
            return 0.0;
        }
        count as f64 / self.window_seconds()
    }

    /// Average bandwidth in bytes/second over the window ending at `now_ns`.
    pub fn bps(&self, now_ns: u64) -> f64 {
        let (count, bytes) = self.window_summary(now_ns);
        if count == 0 {
            return 0.0;
        }
        bytes as f64 / self.window_seconds()
    }

    fn window_summary(&self, now_ns: u64) -> (usize, u64) {
        let window_start = now_ns.saturating_sub(self.window_ns);
        let mut count = 0usize;
        let mut bytes = 0u64;
        for s in &self.samples {
            if s.t_ns >= window_start && s.t_ns <= now_ns {
                count += 1;
                bytes += s.bytes as u64;
            }
        }
        (count, bytes)
    }

    fn window_seconds(&self) -> f64 {
        self.window_ns as f64 / 1_000_000_000.0
    }

    /// Inter-arrival jitter as the standard deviation of consecutive
    /// inter-arrival times within the window, in milliseconds. Returns 0 if
    /// fewer than 2 samples fall in the window.
    pub fn jitter_ms(&self, now_ns: u64) -> f64 {
        let window_start = now_ns.saturating_sub(self.window_ns);
        let in_window: Vec<u64> = self
            .samples
            .iter()
            .filter(|s| s.t_ns >= window_start && s.t_ns <= now_ns)
            .map(|s| s.t_ns)
            .collect();
        if in_window.len() < 2 {
            return 0.0;
        }
        let deltas_ms: Vec<f64> = in_window
            .windows(2)
            .map(|w| (w[1] - w[0]) as f64 / 1_000_000.0)
            .collect();
        let mean: f64 = deltas_ms.iter().sum::<f64>() / deltas_ms.len() as f64;
        let var: f64 =
            deltas_ms.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / deltas_ms.len() as f64;
        var.sqrt()
    }
}

#[cfg(test)]
mod tests;
