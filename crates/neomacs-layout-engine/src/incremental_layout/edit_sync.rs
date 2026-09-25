//! P3.5 G2 (`NEOMACS_LAYOUT_EDIT_SYNC`): an edit replay that reuses the rows
//! below the edit the way GNU's `try_window_id` does -- by walking from the
//! first dirty row until the walk reaches the first unchanged row's start,
//! with no prediction of how many rows the changed lines now take.
//!
//! The `prove` path (the default) reuses the rows below an edit only when it
//! can PROVE AHEAD that every changed line still occupies exactly one row:
//! printable-ASCII text, no structure properties, monospace glyphs, a width
//! that still fits. A tab, a CJK character, a `display` string or an
//! inserted newline anywhere in the line defeats the proof, and every row
//! below the edit is laid out again -- 20 rows per keystroke on the P3.5
//! `typemid` fixture, the common case when editing tab-indented Lisp.
//!
//! The `sync` path ports GNU's synchronization instead (xdisp.c:22561-23300):
//!
//! 1. `first_unchanged_at_end` is the first old row that starts at or after
//!    `Z - END_UNCHANGED + 1` in old coordinates (the `+ 1` keeps the
//!    newline before it unchanged too, `find_first_unchanged_at_end_row`,
//!    xdisp.c:22325-22380); continuation rows are skipped forward
//!    (xdisp.c:22944-22950).
//! 2. `stop_pos` is that row's start shifted by the size delta.
//! 3. The walk runs from the first dirty row while the next row does not
//!    begin at `stop_pos` (xdisp.c:22998-23005). The walker itself decides:
//!    see [`EditSyncStop::reached_at`].
//! 4. The old rows from `first_unchanged_at_end` on are then installed at
//!    `index + dvpos` and `y + dy`, where dvpos/dy are what the walk
//!    produced minus what the replaced rows occupied; rows pushed past the
//!    window bottom are dropped (xdisp.c:23026-23045, 23180-23225).
//!
//! What GNU also handles and this does not yet: rows moving UP (dy < 0, a
//! joined line), which needs a second walk at the window bottom
//! (xdisp.c:23237-23284). Such a walk simply does not stop at `stop_pos` and
//! continues to the bottom -- exactly today's above-only replay.

use super::{EditDamage, RetainedWindowMatrix};
use crate::types::LayoutCharPos0;
use neomacs_display_protocol::glyph_matrix::{
    GlyphPointerOccurrenceIdentity, GlyphPointerSourceKind, GlyphRow, MatrixRow,
};
use neovm_core::buffer::position::LispCharPos1;
use neovm_core::window::{DisplayPointSnapshot, DisplayRowSnapshot};

/// `NEOMACS_LAYOUT_EDIT_SYNC`: how an edit replay may reuse the rows below
/// the edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditSyncMode {
    /// Prove ahead that each changed line stays one row (default).
    Prove,
    /// Walk until the first unchanged row's start (GNU `try_window_id`).
    Sync,
}

#[cfg(test)]
thread_local! {
    static MODE_OVERRIDE: std::cell::Cell<Option<EditSyncMode>> =
        const { std::cell::Cell::new(None) };
}

/// Force the mode on this thread (tests only); `None` restores the knob.
#[cfg(test)]
pub(crate) fn set_edit_sync_mode_for_test(mode: Option<EditSyncMode>) {
    MODE_OVERRIDE.with(|cell| cell.set(mode));
}

/// The mode in effect. Read once per process; default `prove`.
pub(crate) fn edit_sync_mode() -> EditSyncMode {
    #[cfg(test)]
    if let Some(mode) = MODE_OVERRIDE.with(std::cell::Cell::get) {
        return mode;
    }
    static MODE: std::sync::OnceLock<EditSyncMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        match std::env::var("NEOMACS_LAYOUT_EDIT_SYNC")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("sync" | "on" | "1") => EditSyncMode::Sync,
            _ => EditSyncMode::Prove,
        }
    })
}

#[cfg(test)]
thread_local! {
    static SCROLL_BACK_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force `NEOMACS_LAYOUT_SCROLL_BACK` on this thread (tests only).
#[cfg(test)]
pub(crate) fn set_scroll_back_for_test(enabled: Option<bool>) {
    SCROLL_BACK_OVERRIDE.with(|cell| cell.set(enabled));
}

/// `NEOMACS_LAYOUT_SCROLL_BACK=on` (P3.5 G3): a window whose start moved
/// BACK reuses its old rows below the newly exposed ones (GNU
/// `try_window_reusing_current_matrix`, xdisp.c:21766). Read once; default
/// off.
pub(crate) fn scroll_back_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = SCROLL_BACK_OVERRIDE.with(std::cell::Cell::get) {
        return enabled;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("NEOMACS_LAYOUT_SCROLL_BACK")
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            Some("on" | "1" | "true" | "yes")
        )
    })
}

/// The synchronization plan of a BACKWARD scroll: the walk runs from the
/// new window start until the next row would begin at the old first row's
/// start, and every old row is reused below what it produced, shifted down
/// by its height; rows pushed past the bottom are dropped (GNU
/// `try_window_reusing_current_matrix`, the "start moved backward" branch,
/// xdisp.c:21932-22060). `body` is the retained body rows in matrix order.
pub(crate) fn backward_scroll_plan(
    prev: &RetainedWindowMatrix,
    body: &[(usize, &MatrixRow)],
) -> Option<EditSyncPlan> {
    let &(first_index, first_row) = body.first()?;
    let mut indices = rustc_hash::FxHashSet::default();
    let rows = body
        .iter()
        .map(|&(index, row)| {
            indices.insert(index as i64);
            (index, shift_row_positions(row, 0, 0))
        })
        .collect();
    let (row_snapshots, points) = shifted_snapshots(prev, &indices, 0);
    Some(EditSyncPlan {
        stop_charpos: first_row.start_charpos,
        first_unchanged_index: first_index,
        first_unchanged_y: first_row.pixel_y,
        rows,
        row_snapshots,
        points,
    })
}

/// How an edit replay may reuse the rows below the edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BelowReuse {
    /// Above-only: every row from the edit down is walked.
    Off,
    /// The prove-ahead gates (the caller vetted the span).
    Prove,
    /// GNU's synchronization; `prove_fallback` says whether the prove-ahead
    /// path may still be tried when no synchronization row exists.
    Sync { prove_fallback: bool },
}

impl BelowReuse {
    pub(crate) fn prove_allowed(self) -> bool {
        matches!(
            self,
            Self::Prove
                | Self::Sync {
                    prove_fallback: true
                }
        )
    }
}

/// The rows below an edit that the walk may synchronize with, prepared at
/// plan time in OLD placement with NEW (delta-shifted) buffer positions.
#[derive(Clone, Debug)]
pub struct EditSyncPlan {
    /// Where the walk stops: the first unchanged row's start plus the size
    /// delta (GNU `stop_pos`), 0-based.
    pub(crate) stop_charpos: usize,
    /// Old matrix index of the first unchanged row.
    pub(crate) first_unchanged_index: usize,
    /// Old window-relative y of the first unchanged row.
    pub(crate) first_unchanged_y: f32,
    /// `(old matrix index, row)` for every candidate, cursor stripped.
    pub(crate) rows: Vec<(usize, MatrixRow)>,
    /// Their display-snapshot rows (old `row`/`y`, shifted positions).
    pub(crate) row_snapshots: Vec<DisplayRowSnapshot>,
    /// Their display points (old `row`/`y`, shifted positions).
    pub(crate) points: Vec<DisplayPointSnapshot>,
}

/// What the row walker checks at every row boundary. Installed on the output
/// builder for the duration of one replay walk. Placements are
/// window-relative (the retained rows' `pixel_y`); the walker's absolute y is
/// converted with the window origin the builder already knows.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EditSyncStop {
    stop_charpos: usize,
    first_unchanged_y: f32,
}

/// Where the walk synchronized: the matrix index and window-relative y at
/// which the first unchanged row now begins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EditSyncReached {
    pub(crate) display_row_index: usize,
    pub(crate) y: f32,
}

impl EditSyncPlan {
    /// The walker's stop for this plan.
    pub(crate) fn stop(&self) -> EditSyncStop {
        EditSyncStop {
            stop_charpos: self.stop_charpos,
            first_unchanged_y: self.first_unchanged_y,
        }
    }
}

impl EditSyncStop {
    /// Whether a row about to begin at `start` (window-relative `y`, matrix
    /// index `display_row_index`) is where the walk synchronizes.
    ///
    /// GNU stops when `IT_CHARPOS >= stop_pos`. Here the stop must be exact:
    /// a walk that overshoots `stop_pos` (text that became invisible or a
    /// display string spanning it) has no row to synchronize with, and just
    /// continues to the window bottom. So does a walk whose rows now take
    /// LESS room than before: the rows below would move up and leave space at
    /// the bottom that only a second walk could fill. A row the shift leaves
    /// partly below the window bottom is kept as it is, as GNU keeps it: a
    /// full layout lays out the partly visible last row in full too.
    #[cold]
    #[inline(never)]
    pub(crate) fn reached_at(
        &self,
        start: LayoutCharPos0,
        y: f32,
        display_row_index: usize,
    ) -> Option<EditSyncReached> {
        if usize::try_from(start.get()).ok() != Some(self.stop_charpos) {
            return None;
        }
        if y - self.first_unchanged_y < -0.5 {
            return None;
        }
        Some(EditSyncReached {
            display_row_index,
            y,
        })
    }
}

/// The synchronized rows, placed: `(new matrix index, row)` plus their
/// snapshots and points, and the uniform shift they took.
pub(crate) struct EditSyncInstall {
    pub(crate) rows: Vec<(usize, MatrixRow)>,
    pub(crate) row_snapshots: Vec<DisplayRowSnapshot>,
    pub(crate) points: Vec<DisplayPointSnapshot>,
    /// Vertical shift in pixels (GNU `dy`); 0 when the rows kept their place.
    pub(crate) dy: f32,
}

impl EditSyncPlan {
    /// Place the candidates after a walk that synchronized at `reached`:
    /// every row moves by the same `dvpos`/`dy`, and rows pushed to or past
    /// the window-relative `bottom_y`, or to a matrix index at or past
    /// `index_limit` (the first bottom chrome row), are dropped.
    pub(crate) fn install(
        self,
        reached: EditSyncReached,
        bottom_y: f32,
        index_limit: usize,
    ) -> EditSyncInstall {
        let dvpos = reached.display_row_index as i64 - self.first_unchanged_index as i64;
        let dy = reached.y - self.first_unchanged_y;
        let shift_y = dy.abs() >= 0.5;
        let dy_px = dy.round() as i64;
        let mut kept = rustc_hash::FxHashMap::<i64, i64>::default();
        let mut rows = Vec::with_capacity(self.rows.len());
        for (old_index, row) in self.rows {
            let new_y = row.pixel_y + if shift_y { dy } else { 0.0 };
            let Ok(new_index) = usize::try_from(old_index as i64 + dvpos) else {
                continue;
            };
            if new_y >= bottom_y - 0.5 || new_index >= index_limit {
                continue;
            }
            kept.insert(old_index as i64, new_index as i64);
            let row = if shift_y {
                // A shift changes the row's placement, so the reuse is a copy
                // (the scroll replay pays the same; `RowPlacement` removes it).
                let mut shifted = GlyphRow::clone(&row);
                shifted.pixel_y = new_y;
                MatrixRow::new(shifted)
            } else {
                row
            };
            rows.push((new_index, row));
        }
        let row_snapshots = self
            .row_snapshots
            .into_iter()
            .filter_map(|mut snapshot| {
                let new_row = *kept.get(&snapshot.row)?;
                snapshot.row = new_row;
                if shift_y {
                    snapshot.y += dy_px;
                }
                Some(snapshot)
            })
            .collect();
        let points = self
            .points
            .into_iter()
            .filter_map(|mut point| {
                let new_row = *kept.get(&point.row)?;
                point.row = new_row;
                if shift_y {
                    point.y += dy_px;
                }
                Some(point)
            })
            .collect();
        EditSyncInstall {
            rows,
            row_snapshots,
            points,
            dy: if shift_y { dy } else { 0.0 },
        }
    }
}

/// Whether a row's pointer identities (mouse-face ranges, display
/// replacement anchors) move with a shift of the text at or after
/// `dirty_start`: they must lie entirely at or after it.
pub(crate) fn pointers_shiftable(row: &GlyphRow, dirty_start: i64) -> bool {
    row.pointer_appearances().iter().all(|appearance| {
        let identity = appearance.source;
        let range_ok = identity.kind != GlyphPointerSourceKind::Buffer
            || identity.range_start as i64 >= dirty_start;
        let anchor_ok = match identity.occurrence {
            GlyphPointerOccurrenceIdentity::BufferDisplayReplacement { anchor, .. } => {
                anchor as i64 >= dirty_start
            }
            _ => true,
        };
        range_ok && anchor_ok
    })
}

/// A retained row below an edit, re-keyed to post-edit buffer positions:
/// every position at or after `dirty_start` moves by `delta`, cursor
/// decoration is stripped. `delta == 0` is a refcount bump.
pub(crate) fn shift_row_positions(row: &MatrixRow, dirty_start: i64, delta: i64) -> MatrixRow {
    if delta == 0 {
        return if row.cursor_col.is_some() || row.cursor_type.is_some() {
            let mut stripped = GlyphRow::clone(row);
            stripped.cursor_col = None;
            stripped.cursor_type = None;
            MatrixRow::new(stripped)
        } else {
            MatrixRow::clone(row)
        };
    }
    let mut row = GlyphRow::clone(row);
    row.cursor_col = None;
    row.cursor_type = None;
    // Every enabled body row below the edit sits at a real buffer position
    // past the edit, so all of them move by the delta -- including empty
    // lines (which carry their line's charpos) and the trailing EOB
    // placeholder (which carries ZV, and ZV moves by `delta`). Glyphs that
    // map to no buffer position keep their sentinel.
    row.start_charpos = (row.start_charpos as i64 + delta) as usize;
    row.end_charpos = (row.end_charpos as i64 + delta) as usize;
    let from = dirty_start.max(0) as usize;
    for area in row.glyphs.iter_mut() {
        for glyph in area.iter_mut() {
            glyph.provenance = glyph.provenance.shifted_buffer_positions(from, delta);
        }
    }
    // String indices are row-local and do not move; replacement coverage is
    // occurrence-wide, so shift each row side-table entry exactly once.
    row.shift_string_source_buffer_positions(from, delta);
    // Pointer identities carry buffer positions too. The shifting API is
    // add-only; callers admit a negative delta only for pointer-free rows.
    if delta > 0 {
        row.shift_pointer_appearance_buffer_positions(dirty_start.max(0) as u64, delta as u64);
    }
    MatrixRow::new(row)
}

/// The snapshot rows and points of the retained rows in `indices`, re-keyed
/// to post-edit buffer positions (their `row`/`y` are left as they were).
pub(crate) fn shifted_snapshots(
    prev: &RetainedWindowMatrix,
    indices: &rustc_hash::FxHashSet<i64>,
    delta: i64,
) -> (Vec<DisplayRowSnapshot>, Vec<DisplayPointSnapshot>) {
    let shift = |p: LispCharPos1| {
        LispCharPos1::from_one_based_usize((p.to_one_based_usize() as i64 + delta) as usize)
    };
    let rows = prev
        .display_snapshot
        .rows
        .iter()
        .filter(|row| indices.contains(&row.row))
        .map(|row| {
            let mut row = row.clone();
            row.start_buffer_pos = row.start_buffer_pos.map(shift);
            row.end_buffer_pos = row.end_buffer_pos.map(shift);
            row
        })
        .collect();
    let points = prev
        .display_snapshot
        .points
        .iter()
        .filter(|point| indices.contains(&point.row))
        .map(|point| {
            let mut point = point.clone();
            point.buffer_pos = shift(point.buffer_pos);
            point
        })
        .collect();
    (rows, points)
}

/// Whether the body row `row` carries a box face anywhere: only then can its
/// final glyph's box-run ownership depend on the first character after it
/// (GNU `end_of_box_run_p`), which is what the edit replay's one-character
/// lookbehind protects.
pub(crate) fn row_has_boxed_glyph(row: &GlyphRow) -> bool {
    row.glyphs.iter().any(|area| {
        area.iter().any(|glyph| {
            glyph.box_vertical_edges != neomacs_display_protocol::face::BoxVerticalEdges::Unboxed
        })
    })
}

/// GNU's `try_window_id` give-ups that concern the rows BELOW an edit
/// (xdisp.c:22630-22700): `word-wrap` (21), bidi reordering with an
/// automatic paragraph direction (22) -- an edit can flip the direction of
/// the rest of its paragraph --, `line-spacing` (23), `show-trailing-whitespace`
/// (11) and a horizontal scroll (7). A scroll margin is refused too: a row
/// pushed into it would make the full layout scroll, which a replay never
/// does.
pub(crate) fn sync_allowed(
    curr: &super::RetainedWindowKey,
    scroll_margin: i64,
    buffer: &neovm_core::buffer::Buffer,
) -> bool {
    !curr.word_wrap
        && curr.extra_line_spacing == 0.0
        && !curr.show_trailing_whitespace
        && curr.hscroll == 0
        && scroll_margin <= 0
        && !super::mode_line_gate::bidi_auto_paragraph(buffer)
}

/// Build the synchronization plan for an edit, or `None` when no old row
/// below the edit can be synchronized with.
///
/// `body` is the retained body rows in matrix order; rows `[..first_dirty]`
/// are reused verbatim by the caller.
pub(crate) fn plan(
    prev: &RetainedWindowMatrix,
    body: &[(usize, &MatrixRow)],
    first_dirty: usize,
    damage: EditDamage,
) -> Option<EditSyncPlan> {
    let delta = damage.delta();
    let dirty_start = damage.start();
    // `last_unchanged_pos_old` in 0-based terms: the first old position whose
    // character AND predecessor are both outside the change.
    let min_start = damage.end_old() + 1;
    // GNU searches back from the last text row for the smallest row starting
    // at or after that position (xdisp.c:22364-22376). Row starts increase
    // down the window, so the first such row going forward is the same row.
    // Every enabled body row carries its real start -- an empty line its
    // line's position, the end-of-buffer rows ZV -- so none is skipped.
    let mut first_unchanged = (first_dirty + 1..body.len())
        .find(|&index| body[index].1.start_charpos as i64 >= min_start)?;
    // A continuation row depends on the row before it: move forward to the
    // next line start (xdisp.c:22944-22950).
    while first_unchanged < body.len() && body[first_unchanged - 1].1.continued {
        first_unchanged += 1;
    }
    if first_unchanged >= body.len() {
        return None;
    }
    let candidates = &body[first_unchanged..];
    // Pointer identities shift with the text only when they lie wholly after
    // the edit, and the shift is add-only: a delete needs pointer-free rows.
    if !candidates
        .iter()
        .all(|(_, row)| pointers_shiftable(row, dirty_start))
    {
        return None;
    }
    if delta < 0
        && !candidates
            .iter()
            .all(|(_, row)| row.pointer_appearances().is_empty())
    {
        return None;
    }
    let (first_index, first_row) = candidates[0];
    let stop_charpos = first_row.start_charpos as i64 + delta;
    let stop_charpos = usize::try_from(stop_charpos).ok()?;
    let mut indices = rustc_hash::FxHashSet::default();
    let rows = candidates
        .iter()
        .map(|&(index, row)| {
            indices.insert(index as i64);
            (index, shift_row_positions(row, dirty_start, delta))
        })
        .collect();
    let (row_snapshots, points) = shifted_snapshots(prev, &indices, delta);
    Some(EditSyncPlan {
        stop_charpos,
        first_unchanged_index: first_index,
        first_unchanged_y: first_row.pixel_y,
        rows,
        row_snapshots,
        points,
    })
}

#[cfg(test)]
#[path = "tests/edit_sync_test.rs"]
mod tests;
