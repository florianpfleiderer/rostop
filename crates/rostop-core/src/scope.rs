//! Numeric-field discovery and bounded time-series data for scope views.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::message::DynamicValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    Field(String),
    Index(usize),
}

pub type NumericPath = Vec<PathSegment>;

pub fn numeric_paths(value: &DynamicValue) -> Vec<NumericPath> {
    fn walk(value: &DynamicValue, path: &mut NumericPath, out: &mut Vec<NumericPath>) {
        match value {
            DynamicValue::I64(_) | DynamicValue::U64(_) | DynamicValue::F64(_) => {
                out.push(path.clone());
            }
            DynamicValue::Struct(fields) => {
                for (name, child) in fields {
                    path.push(PathSegment::Field(name.clone()));
                    walk(child, path, out);
                    path.pop();
                }
            }
            DynamicValue::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    path.push(PathSegment::Index(index));
                    walk(child, path, out);
                    path.pop();
                }
            }
            _ => {}
        }
    }

    let mut out = Vec::new();
    walk(value, &mut Vec::new(), &mut out);
    out
}

pub fn numeric_value(value: &DynamicValue, path: &[PathSegment]) -> Option<f64> {
    let mut current = value;
    for segment in path {
        current = match (current, segment) {
            (DynamicValue::Struct(fields), PathSegment::Field(name)) => {
                &fields.iter().find(|(key, _)| key == name)?.1
            }
            (DynamicValue::Array(items), PathSegment::Index(index)) => items.get(*index)?,
            _ => return None,
        };
    }
    match current {
        DynamicValue::I64(value) => Some(*value as f64),
        DynamicValue::U64(value) => Some(*value as f64),
        DynamicValue::F64(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

pub fn display_path(path: &[PathSegment]) -> String {
    let mut text = String::new();
    for segment in path {
        match segment {
            PathSegment::Field(name) => {
                if !text.is_empty() {
                    text.push('.');
                }
                text.push_str(name);
            }
            PathSegment::Index(index) => text.push_str(&format!("[{index}]")),
        }
    }
    text
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeriesStats {
    pub current: f64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

#[derive(Debug)]
pub struct TimeSeries {
    samples: VecDeque<(Instant, f64)>,
    retention: Duration,
    max_samples: usize,
}

impl TimeSeries {
    pub fn new(retention: Duration, max_samples: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            retention,
            max_samples: max_samples.max(1),
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn push(&mut self, at: Instant, value: f64) {
        if !value.is_finite() {
            return;
        }
        self.samples.push_back((at, value));
        let cutoff = at.checked_sub(self.retention);
        while self.samples.len() > self.max_samples
            || cutoff.is_some_and(|cutoff| {
                self.samples
                    .front()
                    .is_some_and(|(sample_at, _)| *sample_at < cutoff)
            })
        {
            self.samples.pop_front();
        }
    }

    pub fn stats(&self, now: Instant, window: Duration) -> Option<SeriesStats> {
        let visible = self.visible(now, window);
        let (_, current) = *visible.last()?;
        let mut min = current;
        let mut max = current;
        let mut sum = 0.0;
        for (_, value) in &visible {
            min = min.min(*value);
            max = max.max(*value);
            sum += value;
        }
        Some(SeriesStats {
            current,
            min,
            max,
            mean: sum / visible.len() as f64,
        })
    }

    pub fn plot_points(&self, now: Instant, window: Duration, width: usize) -> Vec<(f64, f64)> {
        let visible = self.visible(now, window);
        if width == 0 || visible.is_empty() {
            return Vec::new();
        }
        if visible.len() <= width {
            return visible
                .into_iter()
                .map(|(at, value)| (signed_age_seconds(at, now), value))
                .collect();
        }

        let bucket_count = width.max(2) / 2;
        let bucket_size = visible.len().div_ceil(bucket_count.max(1));
        let mut points = Vec::with_capacity(bucket_count * 2);
        for bucket in visible.chunks(bucket_size) {
            let min = bucket.iter().min_by(|a, b| a.1.total_cmp(&b.1)).copied();
            let max = bucket.iter().max_by(|a, b| a.1.total_cmp(&b.1)).copied();
            if let (Some(min), Some(max)) = (min, max) {
                let ordered = if min.0 <= max.0 {
                    [min, max]
                } else {
                    [max, min]
                };
                for (at, value) in ordered {
                    points.push((signed_age_seconds(at, now), value));
                }
            }
        }
        points
    }

    fn visible(&self, now: Instant, window: Duration) -> Vec<(Instant, f64)> {
        let cutoff = now.checked_sub(window);
        self.samples
            .iter()
            .filter(|(at, _)| *at <= now && cutoff.is_none_or(|cutoff| *at >= cutoff))
            .copied()
            .collect()
    }
}

fn signed_age_seconds(at: Instant, now: Instant) -> f64 {
    if at <= now {
        -now.duration_since(at).as_secs_f64()
    } else {
        at.duration_since(now).as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_and_resolves_nested_numeric_paths() {
        let value = DynamicValue::Struct(vec![(
            "linear".into(),
            DynamicValue::Struct(vec![(
                "values".into(),
                DynamicValue::Array(vec![DynamicValue::F64(1.5), DynamicValue::I64(-2)]),
            )]),
        )]);
        let paths = numeric_paths(&value);
        assert_eq!(display_path(&paths[0]), "linear.values[0]");
        assert_eq!(numeric_value(&value, &paths[1]), Some(-2.0));
    }

    #[test]
    fn retention_and_capacity_bound_memory() {
        let start = Instant::now();
        let mut series = TimeSeries::new(Duration::from_secs(2), 3);
        for index in 0..5 {
            series.push(start + Duration::from_secs(index), index as f64);
        }
        let stats = series.stats(start + Duration::from_secs(4), Duration::from_secs(10));
        assert_eq!(stats.map(|stats| stats.min), Some(2.0));
    }

    #[test]
    fn decimation_preserves_a_narrow_spike() {
        let start = Instant::now();
        let mut series = TimeSeries::new(Duration::from_secs(10), 100);
        for index in 0..40 {
            let value = if index == 17 { 100.0 } else { 0.0 };
            series.push(start + Duration::from_millis(index * 10), value);
        }
        let points = series.plot_points(start + Duration::from_secs(1), Duration::from_secs(2), 8);
        assert!(points.iter().any(|(_, value)| *value == 100.0));
        assert!(points.len() <= 8);
    }

    #[test]
    fn non_finite_samples_are_rejected() {
        let now = Instant::now();
        let mut series = TimeSeries::new(Duration::from_secs(1), 10);
        series.push(now, f64::NAN);
        assert!(series.stats(now, Duration::from_secs(1)).is_none());
    }
}
