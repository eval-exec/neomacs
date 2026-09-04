//! Display-row evidence used to resolve a semantic window viewport.
//!
//! GNU's display iterator may need to walk beyond the current viewport before
//! `try_scrolling` can decide how far to move `window-start`.  That forward
//! walk is measurement, not a viewport change.  This module keeps the two
//! states distinct so an uncertain display span cannot be published as an
//! arbitrary page-sized scroll.

use crate::buffer_source::window_source::ResolvedWindowStart;
use crate::display_row::walk_state::row_next_window_start_charpos;
use crate::scroll_policy::{ForwardScroll, ScrollPolicy, last_usable_row};
use crate::types::LayoutCharPos0;
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::DisplayRowSnapshot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ViewportDecision {
    Keep,
    NeedMoreMeasurement(ForwardViewportMeasurement),
    Commit {
        window_start: ResolvedWindowStart,
    },
    PlaceRelativeToPoint {
        lines_above_point: i64,
        fallback_window_start: ResolvedWindowStart,
    },
}

/// A forward display-row probe rooted at the last semantic `window-start`.
///
/// `probe_window_start` is deliberately not a candidate for publication.  The
/// accumulated row starts are evidence from which [`Self::observe`] eventually
/// derives one policy-approved commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ForwardViewportMeasurement {
    origin_window_start: ResolvedWindowStart,
    probe_window_start: LayoutCharPos0,
    viewport_rows: usize,
    rows_before_probe: i64,
    /// One possible semantic start per measured display row. `None` means the
    /// row has no distinct buffer boundary that a live window marker can
    /// represent (for example an overlay-string-only row).
    scroll_starts: Vec<Option<LayoutCharPos0>>,
    scroll_policy: ScrollPolicy,
    scroll_margin: i64,
}

impl ForwardViewportMeasurement {
    pub(crate) fn begin(
        rows: &[DisplayRowSnapshot],
        origin_window_start: ResolvedWindowStart,
        scroll_policy: ScrollPolicy,
        scroll_margin: i64,
    ) -> ViewportDecision {
        let mut scroll_starts = scroll_starts_after(rows, origin_window_start.get());
        let viewport_rows = rows.len();
        if scroll_policy.search_limit_lines() == 0 {
            return relative_point_placement(
                scroll_policy,
                viewport_rows,
                scroll_margin,
                origin_window_start,
            );
        }
        let Some((probe_row, probe_window_start)) = scroll_starts
            .iter()
            .enumerate()
            .rev()
            .find_map(|(row, start)| start.map(|start| (row, start)))
        else {
            return relative_point_placement(
                scroll_policy,
                viewport_rows,
                scroll_margin,
                origin_window_start,
            );
        };
        scroll_starts.truncate(probe_row + 1);

        ViewportDecision::NeedMoreMeasurement(Self {
            origin_window_start,
            probe_window_start,
            viewport_rows,
            rows_before_probe: probe_row as i64 + 1,
            scroll_starts,
            scroll_policy,
            scroll_margin,
        })
    }

    pub(crate) const fn probe_window_start(&self) -> LayoutCharPos0 {
        self.probe_window_start
    }

    pub(crate) const fn origin_window_start(&self) -> ResolvedWindowStart {
        self.origin_window_start
    }

    pub(crate) fn observe(
        mut self,
        rows: &[DisplayRowSnapshot],
        point: LispCharPos1,
        point_beyond_visible_span: bool,
    ) -> ViewportDecision {
        let point_row_in_probe = rows.iter().position(|row| {
            row.start_buffer_pos
                .zip(row.end_buffer_pos)
                .is_some_and(|(start, end)| start <= point && point <= end)
        });
        let point_row = point_row_in_probe.map(|row| self.rows_before_probe + row as i64);
        let previous_probe = self.probe_window_start;
        self.extend_evidence(rows);

        if let Some(point_row) = point_row {
            return self.resolve_measured_point(point_row);
        }
        let minimum_unseen_point_row = self.rows_before_probe;
        let bottom_row = last_usable_row(self.viewport_rows, self.scroll_margin);
        if !point_beyond_visible_span
            || self.probe_window_start.get() <= previous_probe.get()
            || minimum_unseen_point_row - bottom_row > self.scroll_policy.search_limit_lines()
        {
            return self.fallback_placement();
        }

        ViewportDecision::NeedMoreMeasurement(self)
    }

    fn extend_evidence(&mut self, rows: &[DisplayRowSnapshot]) {
        let mut starts = scroll_starts_after(rows, self.probe_window_start.get());
        let Some((probe_row, probe_window_start)) = starts
            .iter()
            .enumerate()
            .rev()
            .find_map(|(row, start)| start.map(|start| (row, start)))
        else {
            return;
        };
        starts.truncate(probe_row + 1);
        self.rows_before_probe += probe_row as i64 + 1;
        self.scroll_starts.extend(starts);
        self.probe_window_start = probe_window_start;
    }

    fn resolve_measured_point(self, point_row: i64) -> ViewportDecision {
        let bottom_row = last_usable_row(self.viewport_rows, self.scroll_margin);
        let dy = point_row - bottom_row;
        if dy <= 0 {
            return ViewportDecision::Commit {
                window_start: self.origin_window_start,
            };
        }
        let bounded = dy <= self.scroll_policy.search_limit_lines();
        match self
            .scroll_policy
            .forward_scroll(dy, bounded, self.viewport_rows, self.scroll_margin)
        {
            ForwardScroll::Advance { lines } => {
                let fallback = self.fallback_placement();
                self.start_after_rows(lines)
                    .map_or(fallback, |window_start| ViewportDecision::Commit {
                        window_start,
                    })
            }
            ForwardScroll::Recenter { lines_above_point } => {
                let rows_from_origin = (point_row - lines_above_point).max(0);
                if rows_from_origin == 0 {
                    ViewportDecision::Commit {
                        window_start: self.origin_window_start,
                    }
                } else {
                    self.start_after_rows(rows_from_origin).map_or(
                        ViewportDecision::PlaceRelativeToPoint {
                            lines_above_point,
                            fallback_window_start: self.origin_window_start,
                        },
                        |window_start| ViewportDecision::Commit { window_start },
                    )
                }
            }
        }
    }

    pub(crate) fn fallback_placement(&self) -> ViewportDecision {
        let ForwardScroll::Recenter { lines_above_point } =
            self.scroll_policy
                .forward_scroll(1, false, self.viewport_rows, self.scroll_margin)
        else {
            unreachable!("an unbounded forward distance must use policy fallback placement")
        };
        ViewportDecision::PlaceRelativeToPoint {
            lines_above_point,
            fallback_window_start: self.origin_window_start,
        }
    }

    fn start_after_rows(&self, rows: i64) -> Option<ResolvedWindowStart> {
        if rows <= 0 {
            return Some(self.origin_window_start);
        }
        self.scroll_starts
            .get(rows as usize - 1)
            .copied()
            .flatten()
            .map(|start| ResolvedWindowStart::from_layout_charpos(start.get()))
    }
}

fn relative_point_placement(
    policy: ScrollPolicy,
    viewport_rows: usize,
    scroll_margin: i64,
    fallback_window_start: ResolvedWindowStart,
) -> ViewportDecision {
    let ForwardScroll::Recenter { lines_above_point } =
        policy.forward_scroll(1, false, viewport_rows, scroll_margin)
    else {
        unreachable!("an unbounded forward distance must use policy fallback placement")
    };
    ViewportDecision::PlaceRelativeToPoint {
        lines_above_point,
        fallback_window_start,
    }
}

fn scroll_starts_after(
    rows: &[DisplayRowSnapshot],
    current_start: i64,
) -> Vec<Option<LayoutCharPos0>> {
    let mut previous = current_start;
    rows.iter()
        .map(|row| {
            let start = row_next_window_start_charpos(row)?;
            if start <= previous {
                return None;
            }
            previous = start;
            Some(LayoutCharPos0::new(start))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(index: i64, start: i64, end: i64) -> DisplayRowSnapshot {
        DisplayRowSnapshot {
            row: index,
            start_buffer_pos: Some(LispCharPos1::new(start)),
            end_buffer_pos: Some(LispCharPos1::new(end)),
            ..DisplayRowSnapshot::default()
        }
    }

    #[test]
    fn measurement_probe_is_not_itself_a_viewport_commit() {
        let initial = vec![row(0, 1, 10), row(1, 11, 20), row(2, 21, 30)];
        let ViewportDecision::NeedMoreMeasurement(measurement) = ForwardViewportMeasurement::begin(
            &initial,
            ResolvedWindowStart::from_layout_charpos(1),
            ScrollPolicy::Unlimited,
            0,
        ) else {
            panic!("display uncertainty should request measurement")
        };
        assert_eq!(measurement.probe_window_start(), LayoutCharPos0::new(30));

        let probe = vec![row(0, 30, 39), row(1, 40, 49), row(2, 50, 59)];
        assert_eq!(
            measurement.observe(&probe, LispCharPos1::new(45), false),
            ViewportDecision::Commit {
                window_start: ResolvedWindowStart::from_layout_charpos(20)
            },
            "point is two rows below the old bottom, so policy advances two old rows"
        );
    }

    #[test]
    fn default_policy_places_point_relative_without_a_forward_probe() {
        let initial = vec![row(0, 1, 10), row(1, 11, 20), row(2, 21, 30)];
        assert_eq!(
            ForwardViewportMeasurement::begin(
                &initial,
                ResolvedWindowStart::from_layout_charpos(1),
                ScrollPolicy::Recenter,
                0,
            ),
            ViewportDecision::PlaceRelativeToPoint {
                lines_above_point: 1,
                fallback_window_start: ResolvedWindowStart::from_layout_charpos(1),
            },
            "GNU's default policy skips try_scrolling and measures backward from point"
        );
    }

    #[test]
    fn trailing_non_buffer_row_does_not_hide_the_last_probe_boundary() {
        let mut initial = vec![row(0, 1, 10), row(1, 11, 20), row(2, 21, 30)];
        initial.push(DisplayRowSnapshot::default());

        let ViewportDecision::NeedMoreMeasurement(measurement) = ForwardViewportMeasurement::begin(
            &initial,
            ResolvedWindowStart::from_layout_charpos(1),
            ScrollPolicy::Unlimited,
            0,
        ) else {
            panic!("an EOB/filler row must not discard the preceding measured boundary")
        };

        assert_eq!(measurement.probe_window_start(), LayoutCharPos0::new(30));
        assert_eq!(measurement.rows_before_probe, 3);
        assert_eq!(measurement.viewport_rows, 4);
    }

    #[test]
    fn bounded_forward_probe_switches_to_point_relative_placement() {
        let initial = vec![row(0, 1, 10), row(1, 11, 20), row(2, 21, 30)];
        let ViewportDecision::NeedMoreMeasurement(measurement) = ForwardViewportMeasurement::begin(
            &initial,
            ResolvedWindowStart::from_layout_charpos(1),
            ScrollPolicy::Conservative { max_lines: 2 },
            0,
        ) else {
            panic!("bounded conservative scrolling should measure nearby rows")
        };
        let probe = vec![row(0, 30, 39), row(1, 40, 49), row(2, 50, 59)];
        assert_eq!(
            measurement.observe(&probe, LispCharPos1::new(90), true),
            ViewportDecision::PlaceRelativeToPoint {
                lines_above_point: 1,
                fallback_window_start: ResolvedWindowStart::from_layout_charpos(1),
            },
            "once the nearest possible point row exceeds scroll_max, GNU recenters"
        );
    }

    #[test]
    fn measured_point_without_enough_progress_never_accepts_the_probe() {
        let initial = vec![row(0, 1, 10)];
        let ViewportDecision::NeedMoreMeasurement(measurement) = ForwardViewportMeasurement::begin(
            &initial,
            ResolvedWindowStart::from_layout_charpos(1),
            ScrollPolicy::Unlimited,
            0,
        ) else {
            panic!("display uncertainty should request measurement")
        };
        let probe = vec![
            row(0, 10, 10),
            row(1, 10, 10),
            row(2, 10, 10),
            row(3, 10, 10),
            row(4, 10, 99),
        ];

        assert_eq!(
            measurement.observe(&probe, LispCharPos1::new(99), false),
            ViewportDecision::PlaceRelativeToPoint {
                lines_above_point: 0,
                fallback_window_start: ResolvedWindowStart::from_layout_charpos(1),
            },
            "a transient probe must not become the presentation when its rows cannot supply a policy-approved start"
        );
    }
}
