use super::*;

#[test]
fn empty_sparkline_renders_to_blanks() {
    let s = Sparkline::new(8);
    assert_eq!(s.render(), "        ");
}

#[test]
fn single_value_renders_as_full_block_right_aligned() {
    let mut s = Sparkline::new(4);
    s.push(42.0);
    // Auto-scaled: the single value is the max, so it renders as the highest block.
    // We pad on the left so the latest sample stays right-aligned (newest on the right).
    assert_eq!(s.render(), "   █");
}

#[test]
fn increasing_values_render_with_increasing_heights() {
    let mut s = Sparkline::new(8);
    for v in [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
        s.push(v);
    }
    let r = s.render();
    let chars: Vec<char> = r.chars().collect();
    assert_eq!(chars.len(), 8);
    // Strictly non-decreasing block heights.
    let blocks = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let idx = |c: char| blocks.iter().position(|&b| b == c).unwrap();
    for w in chars.windows(2) {
        assert!(
            idx(w[1]) >= idx(w[0]),
            "non-monotonic sparkline: {r:?}"
        );
    }
}

#[test]
fn buffer_evicts_oldest_when_over_capacity() {
    let mut s = Sparkline::new(3);
    s.push(10.0);
    s.push(20.0);
    s.push(30.0);
    s.push(40.0); // should evict the 10.0
    // Max is now 40, so the third (newest) cell should be the full block.
    let chars: Vec<char> = s.render().chars().collect();
    assert_eq!(chars.len(), 3);
    assert_eq!(chars[2], '█');
}

#[test]
fn all_zero_values_render_as_blanks_not_a_flat_line() {
    let mut s = Sparkline::new(4);
    for _ in 0..4 {
        s.push(0.0);
    }
    assert_eq!(s.render(), "    ");
}
