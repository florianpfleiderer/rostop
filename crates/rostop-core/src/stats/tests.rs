use super::*;

/// Helper: ns-since-some-epoch convenience.
fn ns(seconds: f64) -> u64 {
    (seconds * 1_000_000_000.0) as u64
}

#[test]
fn empty_stats_report_zero_hz() {
    let stats = TopicStats::new(/* window_ns = */ ns(1.0));
    assert_eq!(stats.hz(ns(5.0)), 0.0);
}

#[test]
fn bps_sums_bytes_in_window() {
    let mut stats = TopicStats::new(ns(1.0));
    for i in 0..10 {
        stats.record(ns(i as f64 * 0.1), 100);
    }
    // 10 samples × 100 B over a 1 s window = 1000 B/s.
    let bps = stats.bps(ns(0.9));
    assert!((bps - 1000.0).abs() < 1e-6, "expected ~1000 B/s, got {bps}");
}

#[test]
fn jitter_is_zero_for_perfectly_regular_arrivals() {
    let mut stats = TopicStats::new(ns(1.0));
    for i in 0..10 {
        stats.record(ns(i as f64 * 0.1), 100);
    }
    let jitter = stats.jitter_ms(ns(0.9));
    assert!(jitter < 1e-6, "expected 0 jitter, got {jitter}");
}

#[test]
fn jitter_is_nonzero_for_irregular_arrivals() {
    let mut stats = TopicStats::new(ns(2.0));
    // 100 ms, 100 ms, 100 ms, 500 ms, 100 ms — clearly irregular
    let times = [0.0, 0.1, 0.2, 0.3, 0.8, 0.9];
    for t in times {
        stats.record(ns(t), 100);
    }
    let jitter = stats.jitter_ms(ns(0.9));
    // Inter-arrival deltas (ms): 100, 100, 100, 500, 100 → stddev ≈ 160 ms
    assert!(
        jitter > 100.0 && jitter < 250.0,
        "expected jitter ~160 ms, got {jitter}"
    );
}

#[test]
fn old_samples_are_evicted_to_bound_memory() {
    let mut stats = TopicStats::new(ns(1.0));
    // 10 000 samples spread over 100 s; eviction must keep memory bounded.
    for i in 0..10_000 {
        stats.record(ns(i as f64 * 0.01), 100);
    }
    // Allow some slack but well under the 10k inserted.
    assert!(
        stats.sample_count() < 1_000,
        "sample_count grew unbounded: {}",
        stats.sample_count()
    );
    // Stats over the last 1 s window still correct (~100 Hz, inclusive of endpoints).
    let hz = stats.hz(ns(99.99));
    assert!((hz - 100.0).abs() <= 1.5, "hz = {hz}");
}

#[test]
fn samples_outside_window_are_ignored() {
    let mut stats = TopicStats::new(ns(1.0));
    stats.record(ns(0.0), 100); // far in the past
    stats.record(ns(10.0), 200); // in window
    stats.record(ns(10.5), 200); // in window
    let hz = stats.hz(ns(10.5));
    assert!((hz - 2.0).abs() < 1e-6, "expected 2 Hz, got {hz}");
}

#[test]
fn hz_reflects_samples_in_window() {
    let mut stats = TopicStats::new(ns(1.0));
    // 10 samples at t = 0.0, 0.1, ..., 0.9 (each 100 bytes)
    for i in 0..10 {
        stats.record(ns(i as f64 * 0.1), 100);
    }
    // At t = 0.9 the 1-second window covers all 10 → 10 Hz.
    let hz = stats.hz(ns(0.9));
    assert!((hz - 10.0).abs() < 1e-6, "expected ~10 Hz, got {hz}");
}
