//! Sparkline renderer used by the topic inspector pane.
//!
//! Maintains a fixed-width ring of recent values and renders them to a
//! Unicode block-character string (`▁▂▃▄▅▆▇█`) for in-line display.
//! Auto-scales to the maximum value currently in the buffer.

use std::collections::VecDeque;

/// Block characters from "no value" through "full". A leading space gives the
/// zero level so a sparkline of zeros looks empty rather than a flat bottom line.
const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Fixed-width auto-scaling sparkline.
#[derive(Debug)]
pub struct Sparkline {
    width: usize,
    values: VecDeque<f64>,
}

impl Sparkline {
    /// Construct a sparkline of `width` cells.
    pub fn new(width: usize) -> Self {
        Self {
            width,
            values: VecDeque::with_capacity(width),
        }
    }

    /// Append a new value. Oldest value is evicted once the buffer is full.
    pub fn push(&mut self, v: f64) {
        if self.values.len() == self.width {
            self.values.pop_front();
        }
        self.values.push_back(v);
    }

    /// Render the current values as a `width`-character string. Cells that have
    /// not yet been filled are rendered as spaces, so the latest sample stays
    /// right-aligned and the line always has a stable width.
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(self.width * 3);
        let pad = self.width.saturating_sub(self.values.len());
        for _ in 0..pad {
            out.push(' ');
        }
        let max = self
            .values
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        // No data, or all zero / negative — render as blanks for a clean look.
        if !max.is_finite() || max <= 0.0 {
            for _ in 0..self.values.len() {
                out.push(' ');
            }
            return out;
        }
        let last_idx = BLOCKS.len() - 1; // 8
        for &v in &self.values {
            let ratio = (v / max).clamp(0.0, 1.0);
            let idx = (ratio * last_idx as f64).round() as usize;
            out.push(BLOCKS[idx]);
        }
        out
    }
}

#[cfg(test)]
mod tests;
