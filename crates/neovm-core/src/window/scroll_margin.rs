//! GNU's effective scroll margin.
//!
//! `scroll-margin` asks for N lines of context at the window edges, but GNU
//! caps the request per window (`window_scroll_margin`, src/window.c:6207):
//! `maximum-scroll-margin` — a float, 0.25 by default — limits it to that
//! fraction of the window's lines, and the margin can never consume more than
//! half the lines (`(/ (1- window-lines) 2)`), so the top and bottom margins
//! cannot meet (bug#5718).
//!
//! The same cap governs the layout engine's scroll decisions and the
//! pixel-level motion policy, so both resolve their margin through this type
//! instead of re-deriving the formula (and the historical hardcoded
//! quarter-window cap) at each call site.

/// A `scroll-margin` request together with the `maximum-scroll-margin` cap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollMargin {
    lines: i64,
    maximum_fraction: f64,
}

// `maximum_fraction` is normalized to a finite value in `[0.0, 0.5]` by every
// constructor, so `PartialEq` is reflexive for every reachable value and the
// type may participate in the `Eq`-deriving display snapshots that compare
// layout inputs. (NaN and infinities never reach the field.)
impl Eq for ScrollMargin {}

impl Default for ScrollMargin {
    fn default() -> Self {
        Self {
            lines: 0,
            maximum_fraction: Self::DEFAULT_MAXIMUM_FRACTION,
        }
    }
}

impl ScrollMargin {
    /// GNU's default when `maximum-scroll-margin` is not a float
    /// (src/xdisp.c:38576).
    pub const DEFAULT_MAXIMUM_FRACTION: f64 = 0.25;

    /// Build from the raw Lisp values.
    ///
    /// `scroll_margin` is the fixnum request; negative values behave as zero
    /// (GNU's `scroll_margin > 0` guard). `maximum_scroll_margin` is the
    /// variable's value only when it is a float — GNU ignores every other
    /// type and keeps the 0.25 default (`FLOATP (Vmaximum_scroll_margin)`).
    pub fn from_lisp_values(scroll_margin: i64, maximum_scroll_margin: Option<f64>) -> Self {
        let maximum_fraction = maximum_scroll_margin
            .filter(|fraction| fraction.is_finite())
            .map_or(Self::DEFAULT_MAXIMUM_FRACTION, |fraction| {
                // GNU `max (0.0, ratio)` then `min (ratio, 0.5)`.
                fraction.clamp(0.0, 0.5)
            });
        Self {
            lines: scroll_margin.max(0),
            maximum_fraction,
        }
    }

    /// Whether redisplay must consider the margin at all (GNU's
    /// `scroll_margin > 0`). The effective margin may still be zero in a
    /// window too short for it; this reports the requested state.
    pub fn is_zero(self) -> bool {
        self.lines <= 0
    }

    /// Effective margin in lines for a window `window_lines` display lines
    /// tall — GNU `window_scroll_margin` with `MARGIN_IN_LINES`.
    pub fn lines(self, window_lines: i64) -> i64 {
        if self.lines <= 0 {
            return 0;
        }
        // GNU: min ((window_lines - 1) / 2, (int) (window_lines * ratio)).
        // C integer division truncates toward zero, as Rust's does.
        let half_window = (window_lines - 1) / 2;
        let fractional = (window_lines as f64 * self.maximum_fraction) as i64;
        self.lines.min(half_window).min(fractional).max(0)
    }

    /// Effective margin in pixels — GNU `window_scroll_margin` with
    /// `MARGIN_IN_PIXELS`. The line count is derived from the window's pixel
    /// height and line height first, then multiplied back, exactly as GNU
    /// does, so partial bottom lines do not inflate the margin.
    pub fn pixels(self, window_height: i64, line_height: i64) -> i64 {
        let line_height = line_height.max(1);
        self.lines(window_height / line_height) * line_height
    }
}

#[cfg(test)]
#[path = "scroll_margin/tests/scroll_margin_test.rs"]
mod tests;
