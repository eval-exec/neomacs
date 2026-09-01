//! Pure sub-line vertical pixel-scroll resolution (smooth scrolling, Phase 1).
//!
//! Converts a vertical pixel delta into a new `(window-start row, vscroll)` pair by
//! walking a contiguous run of laid-out row metrics by their real (variable) heights.
//! Pure + side-effect free so it is unit-testable without the layout engine or GPU.
//! See `docs/superpowers/specs/2026-06-29-smooth-scroll-design.md`.

/// One laid-out row's scroll-relevant metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollRowMetric {
    /// Buffer char position where this row starts (the candidate `window-start`).
    pub start_charpos: i64,
    /// Row height in whole pixels.
    pub height_px: i32,
}

/// Resolve a vertical pixel scroll.
///
/// `rows` is a contiguous run of laid-out rows in ascending buffer order. `top_idx`
/// is the index of the current `window-start` row within `rows`. `current_vscroll`
/// (>= 0) is the pixels of `rows[top_idx]` currently hidden above the top edge.
/// `delta` is pixels to scroll: positive scrolls **down** (content moves up),
/// negative scrolls **up**. Returns the new `(top_idx, vscroll)`, clamped to the
/// provided run (`vscroll` never goes below 0; the walk stops at the first/last row).
pub fn resolve_pixel_scroll(
    rows: &[ScrollRowMetric],
    top_idx: usize,
    current_vscroll: i32,
    delta: i32,
) -> (usize, i32) {
    if rows.is_empty() {
        return (top_idx, current_vscroll);
    }
    let last = rows.len() - 1;
    let mut idx = top_idx.min(last);
    let mut v = current_vscroll + delta;
    // Scroll down: consume whole rows off the top until the residual fits the top row.
    while v >= rows[idx].height_px && idx < last {
        v -= rows[idx].height_px;
        idx += 1;
    }
    // Scroll up: retreat to earlier rows, adding their heights back.
    while v < 0 && idx > 0 {
        idx -= 1;
        v += rows[idx].height_px;
    }
    // Clamp at the top edge of the provided run.
    if v < 0 {
        v = 0;
    }
    (idx, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<ScrollRowMetric> {
        [1, 40, 80, 120, 160]
            .iter()
            .map(|&c| ScrollRowMetric {
                start_charpos: c,
                height_px: 20,
            })
            .collect()
    }

    #[test]
    fn within_row_down() {
        assert_eq!(resolve_pixel_scroll(&rows(), 0, 0, 8), (0, 8));
    }
    #[test]
    fn cross_one_row_down() {
        assert_eq!(resolve_pixel_scroll(&rows(), 0, 15, 8), (1, 3));
    }
    #[test]
    fn cross_multi_rows_down() {
        assert_eq!(resolve_pixel_scroll(&rows(), 0, 0, 45), (2, 5));
    }
    #[test]
    fn within_row_up() {
        assert_eq!(resolve_pixel_scroll(&rows(), 2, 10, -6), (2, 4));
    }
    #[test]
    fn cross_one_row_up() {
        assert_eq!(resolve_pixel_scroll(&rows(), 2, 5, -8), (1, 17));
    }
    #[test]
    fn clamp_top() {
        assert_eq!(resolve_pixel_scroll(&rows(), 0, 0, -100), (0, 0));
    }
    #[test]
    fn clamp_bottom_stops_at_last_row() {
        let (idx, _) = resolve_pixel_scroll(&rows(), 4, 0, 1000);
        assert_eq!(idx, 4);
    }
}
