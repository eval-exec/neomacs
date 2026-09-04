//! Buffer window source read bounds and text extraction.

use crate::neovm_bridge::{ForwardScrollMeasurement, LayoutBufferView, RustBufferAccess};
use crate::scroll_policy::{
    ForwardScroll, ScrollPolicy, count_lines_bounded, last_usable_row, line_start_above,
    line_start_below, top_margin,
};
use crate::types::{PartialBodyWalkStart, WindowKind, WindowParams};
use neovm_core::buffer::{CharPos0, EmacsBytePos, TextPositionAnchor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferWindowSource {
    window_start: i64,
    text_start_byte: usize,
    bytes_read: usize,
    point_charpos: i64,
    accessible_start: i64,
    accessible_end: TextPositionAnchor,
}

impl BufferWindowSource {
    pub(crate) const fn window_start(self) -> i64 {
        self.window_start
    }

    pub(crate) const fn text_start_byte(self) -> usize {
        self.text_start_byte
    }

    pub(crate) const fn bytes_read(self) -> usize {
        self.bytes_read
    }

    pub(crate) const fn point_charpos(self) -> i64 {
        self.point_charpos
    }

    pub(crate) const fn accessible_start(self) -> i64 {
        self.accessible_start
    }

    pub(crate) const fn accessible_end(self) -> i64 {
        self.accessible_end.char_pos().get() as i64
    }

    pub(crate) const fn accessible_end_position(self) -> TextPositionAnchor {
        self.accessible_end
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferWindowSourceRequest {
    requested_window_start: i64,
    previous_viewport: Option<PreviousViewportEvidence>,
    point_charpos: i64,
    accessible_start: i64,
    accessible_end: i64,
    max_rows: usize,
    kind: WindowKind,
    scroll_policy: ScrollPolicy,
    scroll_margin: i64,
}

/// A viewport start after GNU's scrolling/recentering policy has selected it.
///
/// Keeping this distinct from `WindowParams::window_start` prevents a row
/// walk from silently choosing a start that was never committed to the live
/// window (and therefore never reached `window-scroll-functions`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ResolvedWindowStart(i64);

impl ResolvedWindowStart {
    pub(crate) const fn from_layout_charpos(charpos: i64) -> Self {
        Self(charpos)
    }

    pub(crate) const fn get(self) -> i64 {
        self.0
    }
}

/// Display evidence retained from the previous accepted layout.
///
/// This is deliberately evidence, not a viewport decision: an exclusive
/// visible end can prove positions before it visible and positions after it
/// hidden, but equality at accessible EOB is ambiguous.  The EOB cursor lives
/// at the insertion boundary after the last character, so only the current
/// rendered cursor row can decide whether it is still visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreviousViewportEvidence {
    visible_end_exclusive: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviousViewportPointRelation {
    /// No accepted previous layout describes this viewport.
    Unknown,
    /// Point is strictly inside the previous visible buffer span.
    Visible,
    /// Point equals both the previous exclusive end and accessible EOB.
    /// Defer to the current layout's exact cursor-row measurement.
    NeedsMeasuredLayout,
    /// Point is at or below the first position outside the previous span.
    Below { visible_end_exclusive: i64 },
}

impl BufferWindowSourceRequest {
    pub(crate) fn from_window_params(params: &WindowParams, max_rows: usize) -> Self {
        Self::new(
            params.window_start_charpos().get(),
            params.previous_visible_end_charpos().map(|pos| pos.get()),
            params.point_charpos().get(),
            params.accessible_start_charpos().get(),
            params.accessible_end_charpos().get(),
            max_rows,
            params.kind,
            ScrollPolicy::from_window_params(params),
            params.scroll_margin,
        )
    }

    /// Build a source request for an incremental partial walk without changing
    /// semantic window state.  [`Self::read_exact_into`] consumes the typed
    /// walk start directly, while point and every other display dependency
    /// remain sourced from the authoritative [`WindowParams`] snapshot.
    pub(crate) fn for_partial_walk(
        params: &WindowParams,
        walk_start: PartialBodyWalkStart,
        max_rows: usize,
    ) -> Self {
        let mut request = Self::from_window_params(params, max_rows);
        request.requested_window_start = walk_start.get();
        request
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        requested_window_start: i64,
        previous_visible_end: Option<i64>,
        point_charpos: i64,
        accessible_start: i64,
        accessible_end: i64,
        max_rows: usize,
        kind: WindowKind,
        scroll_policy: ScrollPolicy,
        scroll_margin: i64,
    ) -> Self {
        Self {
            requested_window_start,
            previous_viewport: previous_visible_end.map(|visible_end_exclusive| {
                PreviousViewportEvidence {
                    visible_end_exclusive,
                }
            }),
            point_charpos,
            accessible_start,
            accessible_end,
            max_rows,
            kind,
            scroll_policy,
            scroll_margin,
        }
    }

    fn previous_viewport_point_relation(self) -> PreviousViewportPointRelation {
        let Some(previous) = self.previous_viewport else {
            return PreviousViewportPointRelation::Unknown;
        };
        let end = previous.visible_end_exclusive;
        if self.point_charpos < end {
            PreviousViewportPointRelation::Visible
        } else if self.point_charpos == end && self.point_charpos == self.accessible_end {
            PreviousViewportPointRelation::NeedsMeasuredLayout
        } else {
            PreviousViewportPointRelation::Below {
                visible_end_exclusive: end,
            }
        }
    }

    /// Select the viewport start without producing rows or copying text.
    ///
    /// Redisplay uses this preflight before entering Lisp so the selected
    /// value can be committed at GNU's leaf-local scroll-hook boundary.
    pub(crate) fn resolve<B: LayoutBufferView>(
        self,
        access: &RustBufferAccess<'_, B>,
    ) -> ResolvedWindowStart {
        let requested = self
            .requested_window_start
            .clamp(self.accessible_start, self.accessible_end);
        let measurement = self.forward_scroll_measurement(requested, access);
        ResolvedWindowStart(self.resolve_window_start_with_measurement(
            |charpos| access.byte_at(access.charpos_to_bytepos(charpos)),
            measurement,
        ))
    }

    fn forward_scroll_measurement<B: LayoutBufferView>(
        self,
        window_start: i64,
        access: &RustBufferAccess<'_, B>,
    ) -> ForwardScrollMeasurement {
        let scan_start = match self.previous_viewport_point_relation() {
            PreviousViewportPointRelation::Visible
            | PreviousViewportPointRelation::NeedsMeasuredLayout => {
                return ForwardScrollMeasurement::SourceLineEstimate;
            }
            PreviousViewportPointRelation::Below {
                visible_end_exclusive,
            } => visible_end_exclusive,
            PreviousViewportPointRelation::Unknown => window_start,
        };
        if self.point_charpos <= scan_start {
            return ForwardScrollMeasurement::SourceLineEstimate;
        }

        access.forward_scroll_measurement(scan_start, self.point_charpos)
    }

    /// Treat the requested start as authoritative, clamped only to the live
    /// accessible range. Used by synchronous queries, incremental partial
    /// walks, and GNU's explicit `force_start` branch.
    pub(crate) fn resolve_exact(self) -> ResolvedWindowStart {
        ResolvedWindowStart(
            self.requested_window_start
                .clamp(self.accessible_start, self.accessible_end),
        )
    }

    /// Read from an already-resolved partial-layout boundary.
    ///
    /// Incremental replay computes the exact first character that must be
    /// relaid. Applying normal viewport scrolling/recentering to that boundary
    /// would change the requested source range and duplicate retained rows.
    pub(crate) fn read_exact_into<B: LayoutBufferView>(
        self,
        access: &RustBufferAccess<'_, B>,
        out: &mut Vec<u8>,
    ) -> BufferWindowSource {
        self.read_resolved_into(self.resolve_exact(), access, out)
    }

    /// Read from a viewport decision already committed by the coordinator.
    pub(crate) fn read_resolved_into<B: LayoutBufferView>(
        self,
        window_start: ResolvedWindowStart,
        access: &RustBufferAccess<'_, B>,
        out: &mut Vec<u8>,
    ) -> BufferWindowSource {
        self.read_from_resolved_start(window_start.get(), access, out)
    }

    fn read_from_resolved_start<B: LayoutBufferView>(
        self,
        window_start: i64,
        access: &RustBufferAccess<'_, B>,
        out: &mut Vec<u8>,
    ) -> BufferWindowSource {
        let text_start_byte = access.charpos_to_bytepos(window_start) as usize;
        let read_chars = self.accessible_end - window_start + 1;
        let bytes_read = if read_chars <= 0 {
            out.clear();
            0
        } else {
            let text_end = (window_start + read_chars).min(self.accessible_end);
            let byte_to = access.charpos_to_bytepos(text_end);
            // Bound the copy to the lines the walk can actually display
            // instead of the whole accessible tail (O(visible), not
            // O(buffer)). `max_rows + 2` COMPLETE lines always over-cover the
            // walk: wrapping only makes a line span MORE rows, and truncation
            // consumes at most the full line, which the bound includes. The
            // bound is only sound when nothing can make the walk consume text
            // non-linearly — overlays (display/invisible/before/after
            // strings), display or invisible text properties, and
            // selective-display all fall back to the full-tail read.
            let bounded_to = access
                .find_nth_newline_after(text_start_byte as i64, self.max_rows + 2)
                .filter(|bound| {
                    *bound < byte_to
                        && crate::neovm_bridge::buffer_selective_display(access.view()) == 0
                        && !access.has_walk_consumption_hazard(text_start_byte as i64, *bound)
                })
                .unwrap_or(byte_to);
            access.copy_text(text_start_byte as i64, bounded_to, out);
            out.len()
        };

        BufferWindowSource {
            window_start,
            text_start_byte,
            bytes_read,
            point_charpos: self.point_charpos,
            accessible_start: self.accessible_start,
            accessible_end: TextPositionAnchor::new(
                CharPos0::new(self.accessible_end.max(0) as usize),
                EmacsBytePos::new(access.zv().max(0) as usize),
            ),
        }
    }

    #[cfg(test)]
    fn resolve_window_start(self, byte_at_charpos: impl Fn(i64) -> Option<u8>) -> i64 {
        self.resolve_window_start_with_measurement(
            byte_at_charpos,
            ForwardScrollMeasurement::SourceLineEstimate,
        )
    }

    fn resolve_window_start_with_measurement(
        self,
        byte_at_charpos: impl Fn(i64) -> Option<u8>,
        forward_measurement: ForwardScrollMeasurement,
    ) -> i64 {
        let mut window_start = self.requested_window_start.max(self.accessible_start);

        if window_start > self.accessible_start {
            let remaining_chars = self.accessible_end - window_start;
            if remaining_chars < self.max_rows as i64 && self.accessible_end > self.max_rows as i64
            {
                window_start = self
                    .line_start_above_point((self.max_rows as i64 / 2).max(1), &byte_at_charpos);
            }
        }

        if self.point_charpos >= self.accessible_start
            && self.point_is_above_top_margin(window_start, &byte_at_charpos)
        {
            let adjusted = self.backward_scroll_window_start(window_start, &byte_at_charpos);
            tracing::debug!(
                "layout_window_rust: adjusted window_start {} -> {} (point={})",
                self.requested_window_start,
                adjusted,
                self.point_charpos
            );
            return adjusted;
        }

        if let Some(adjusted) =
            self.forward_scroll_window_start(window_start, forward_measurement, &byte_at_charpos)
        {
            tracing::debug!(
                "layout_window_rust: forward-adjusted window_start {} -> {} (point={})",
                self.requested_window_start,
                adjusted,
                self.point_charpos,
            );
            return adjusted;
        }

        window_start
    }

    /// Whether point sits above the top scroll-margin row, which is where GNU
    /// starts scrolling backward — not at the window start itself (its
    /// `PT < CHARPOS (scroll_margin_pos)` test, src/xdisp.c:19580).
    fn point_is_above_top_margin(
        self,
        window_start: i64,
        byte_at_charpos: &impl Fn(i64) -> Option<u8>,
    ) -> bool {
        if self.point_charpos < window_start {
            return true;
        }
        let margin = top_margin(self.max_rows, self.scroll_margin);
        margin > 0
            && count_lines_bounded(window_start, self.point_charpos, margin, byte_at_charpos).0
                < margin
    }

    /// New window start when point moved UP out of the window, per GNU
    /// `try_scrolling`'s backward branch (src/xdisp.c:19558-19613).
    ///
    /// Minimal scrolling puts point on the top scroll-margin row; a jump too
    /// far for the policy falls back to `recenter:`.
    fn backward_scroll_window_start(
        self,
        window_start: i64,
        byte_at_charpos: &impl Fn(i64) -> Option<u8>,
    ) -> i64 {
        let (lines_back, bounded) = count_lines_bounded(
            self.point_charpos,
            window_start,
            self.scroll_policy.search_limit_lines(),
            byte_at_charpos,
        );
        let lines_above_point = self.scroll_policy.backward_scroll(
            lines_back,
            bounded,
            self.max_rows,
            self.scroll_margin,
        );
        self.line_start_above_point(lines_above_point, byte_at_charpos)
    }

    /// New window start when point sits below the window, per GNU
    /// `try_scrolling` (src/xdisp.c:19359). `None` when point is already on
    /// screen or this pass must not scroll.
    ///
    /// The distance below the window is measured from `previous_visible_end`,
    /// the only display-accurate fact available before laying out. Without it
    /// this falls back to counting BUFFER lines from the window start, which is
    /// exact only when nothing wraps and nothing is hidden.
    ///
    /// Under-scrolling is safe: the visibility retry measures real display rows
    /// and finishes the job. Over-scrolling is not — point ends up visible, so
    /// nothing corrects it — which is why the trigger uses the measured end
    /// rather than a line count that grows with every hidden line.
    fn forward_scroll_window_start(
        self,
        window_start: i64,
        measurement: ForwardScrollMeasurement,
        byte_at_charpos: &impl Fn(i64) -> Option<u8>,
    ) -> Option<i64> {
        if self.kind.is_minibuffer() {
            return None;
        }
        // A non-minibuffer window laid out at a degenerate (<= 1 row) height is a
        // transient/probe state — e.g. an intermediate pass while a child-frame
        // (posframe) or frame resize is in flight. Its viewport is too small to
        // estimate a real scroll from: every point past the first row looks "far
        // below", so this would scroll window_start to point. That scrolled start
        // then PERSISTS and corrupts the real (tall) window (the Doom dashboard
        // banner scrolls off when `SPC SPC` opens find-file). GNU never scrolls
        // an editing window from such a state.
        if self.max_rows <= 1 {
            return None;
        }

        let bottom_row = last_usable_row(self.max_rows, self.scroll_margin);
        let (point_row, bounded) = match self.previous_viewport_point_relation() {
            // Point was on screen last time. Any scroll it still needs is one
            // the bottom scroll-margin asks for, and only real display rows can
            // measure that — leave it to the visibility retry.
            PreviousViewportPointRelation::Visible
            | PreviousViewportPointRelation::NeedsMeasuredLayout => return None,
            PreviousViewportPointRelation::Below {
                visible_end_exclusive: end,
            } => {
                if measurement == ForwardScrollMeasurement::DisplayRowsRequired {
                    return None;
                }
                let (extra_lines, bounded) = count_lines_bounded(
                    end,
                    self.point_charpos,
                    self.scroll_policy.search_limit_lines(),
                    byte_at_charpos,
                );
                (self.max_rows as i64 + extra_lines, bounded)
            }
            PreviousViewportPointRelation::Unknown => {
                if measurement == ForwardScrollMeasurement::DisplayRowsRequired {
                    return None;
                }
                count_lines_bounded(
                    window_start,
                    self.point_charpos,
                    bottom_row + self.scroll_policy.search_limit_lines(),
                    byte_at_charpos,
                )
            }
        };
        // GNU's `dy`: how far point falls past the last row the bottom
        // scroll-margin leaves usable (xdisp.c:19443). `<= 0` means point is
        // already visible, which is GNU's `if (dy > 0) scroll_down_p = true`.
        let dy = point_row - bottom_row;
        if dy <= 0 {
            return None;
        }

        Some(
            match self
                .scroll_policy
                .forward_scroll(dy, bounded, self.max_rows, self.scroll_margin)
            {
                ForwardScroll::Advance { lines } => {
                    line_start_below(window_start, lines, self.accessible_end, byte_at_charpos)
                }
                ForwardScroll::Recenter { lines_above_point } => {
                    self.line_start_above_point(lines_above_point, byte_at_charpos)
                }
            },
        )
    }

    fn line_start_above_point(
        self,
        lines_above: i64,
        byte_at_charpos: &impl Fn(i64) -> Option<u8>,
    ) -> i64 {
        line_start_above(
            self.point_charpos,
            lines_above,
            self.accessible_start,
            byte_at_charpos,
        )
    }
}

#[cfg(test)]
#[path = "window_source_test.rs"]
mod tests;
