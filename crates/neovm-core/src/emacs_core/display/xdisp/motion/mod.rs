//! Measured display-row motion shared by Lisp motion and window scrolling.
//!
//! GNU's interactive `vertical-motion` uses the same display iterator as
//! redisplay. Retained rows are a fast path, not a substitute for measurement.
//! GNU's intentionally different batch engine stays in `editing/indent`.

mod measurement;
pub(crate) mod paging;
pub(crate) mod pixels;
mod policy;
pub(crate) use policy::ScrollGoal;

use crate::buffer::{AccessibleCharRange, BufferId, LispCharPos1};
use crate::emacs_core::Context;
use crate::emacs_core::Value;
use crate::emacs_core::error::{Flow, LispCondition, signal};
use crate::emacs_core::xdisp::LineWrap;
use crate::window::{DisplayRowSnapshot, FrameId, WindowDisplaySnapshot, WindowId};

/// Which of GNU's TWO screen-line engines answers a motion question.
///
/// `Fvertical_motion` is not one algorithm with a display switch inside it:
/// its body is an `if` over `noninteractive` choosing between two
/// implementations that share no code (`src/indent.c:2280-2287`):
///
/// ```c
///   if (noninteractive)
///     {
///       struct position pos;
///       pos = *vmotion (PT, PT_BYTE, XFIXNUM (lines), w);
///       SET_PT_BOTH (pos.bufpos, pos.bytepos);
///       it.vpos = pos.vpos;
///     }
///   else
///     { ... start_display / move_it_by_lines / move_it_in_display_line ... }
/// ```
///
/// The batch arm is `vmotion` -> `compute_motion` (`src/indent.c:1963-1964`,
/// `:1253-1254`).  Two things follow from that being a different program rather
/// than a different setting:
///
/// * `compute_motion` has **no word-wrap concept at all**.  Its only line-end
///   decision is truncate-or-continue at `width` (`src/indent.c:1474-1527`);
///   the identifier `word_wrap` does not occur anywhere in `src/indent.c`.
///   So `LineWrap::WordWrap` is not reachable from the batch engine.
/// * The `(COLS . LINES)` goal column is never applied: the `lcols` walk lives
///   inside the `else` (`src/indent.c:2528-2558`), so a batch
///   `vertical-motion` answers a cons argument using only its cdr.
///
/// Measured under GNU Emacs 31.0.90 over one 201-character line carrying a
/// single space at column 100, in an 80-column terminal:
///
/// ```text
///   emacs --batch        word-wrap nil -> rows 1 80 159      count-screen-lines 3
///                        word-wrap t   -> rows 1 80 159      count-screen-lines 3
///   emacs -nw in a pty   word-wrap nil -> rows 1 80 159      count-screen-lines 3
///                        word-wrap t   -> rows 1 80 102 181  count-screen-lines 4
/// ```
///
/// and, for the goal column, `(vertical-motion '(40 . 0))` from the start of a
/// long line answers point 1 under `--batch` and point 41 in a terminal.
///
/// This is a type rather than a condition spelled at each site because the two
/// engines are not interchangeable and their difference is invisible in a
/// value: ledger 191 gated the goal column on `noninteractive` correctly and
/// missed that the very same branch also decides whether `word-wrap` exists at
/// all, which made every batch `count-screen-lines` over a wrapped word answer
/// one screen line too many.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MotionEngine {
    /// GNU `vmotion` -> `compute_motion` (`src/indent.c:1963-1964`): the engine
    /// `Fvertical_motion` uses under `noninteractive`.
    ComputeMotion,
    /// GNU's display iterator (`start_display` / `move_it_by_lines` /
    /// `move_it_in_display_line`): the engine `Fvertical_motion` uses when a
    /// terminal or window system is live.
    DisplayIterator,
}

impl MotionEngine {
    /// GNU's own branch, `if (noninteractive)` (`src/indent.c:2280`).
    pub(crate) fn for_context(eval: &Context) -> Self {
        if eval.noninteractive() {
            Self::ComputeMotion
        } else {
            Self::DisplayIterator
        }
    }

    /// The wrap method a NON-truncating display line uses under this engine.
    ///
    /// This is the ONLY producer of [`LineWrap::WordWrap`] in the port.
    /// `init_iterator` reaches `WORD_WRAP` from the buffer's `word-wrap`
    /// (`src/xdisp.c:3425-3426`), and `init_iterator` runs only in the
    /// interactive arm; `compute_motion` continues at `width` whatever the
    /// buffer asks for.
    pub(crate) fn continuation_wrap(self, word_wrap: bool) -> LineWrap {
        match self {
            Self::ComputeMotion => LineWrap::WindowWrap,
            Self::DisplayIterator if word_wrap => LineWrap::WordWrap,
            Self::DisplayIterator => LineWrap::WindowWrap,
        }
    }

    /// Whether `(COLS . LINES)`'s COLS is applied at all
    /// (`src/indent.c:2528-2558`, inside the interactive arm).
    pub(crate) fn honors_goal_column(self) -> bool {
        matches!(self, Self::DisplayIterator)
    }

    /// Whether realized display rows may answer the question.  GNU's batch
    /// engine walks buffer text and never consults a glyph matrix, so a
    /// retained redisplay snapshot is not an input to it.
    pub(crate) fn uses_display_rows(self) -> bool {
        matches!(self, Self::DisplayIterator)
    }
}

/// All coordinates are in the source buffer, except the optional GNU
/// window-relative goal column. The resolver owns conversion to measured rows.
#[derive(Clone, Copy)]
pub(crate) struct MotionRequest {
    pub(crate) buffer: BufferId,
    pub(crate) window: Option<WindowId>,
    pub(crate) origin: LispCharPos1,
    pub(crate) rows: i64,
    pub(crate) goal_column: Option<i64>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MeasuredMotion {
    pub(crate) target: LispCharPos1,
    pub(crate) moved: i64,
}

/// Exhausting measured coverage is not proof of reaching a buffer boundary.
/// Keep that distinction inside the resolver; only settled motion may be
/// returned to a caller that will commit point or window-start.
enum RowMotion {
    Complete(MeasuredMotion),
    AccessibleBoundary(MeasuredMotion),
    NeedsMoreRows,
}

impl RowMotion {
    fn settled(self) -> Option<MeasuredMotion> {
        match self {
            Self::Complete(motion) | Self::AccessibleBoundary(motion) => Some(motion),
            Self::NeedsMoreRows => None,
        }
    }
}

fn motion_window(
    eval: &Context,
    window: Option<WindowId>,
    current_buffer: BufferId,
) -> Option<(FrameId, WindowId)> {
    let (frame_id, window_id) = if let Some(window_id) = window {
        let frame_id = eval.frames.find_window_frame_id(window_id)?;
        (frame_id, window_id)
    } else {
        let frame = eval.frames.selected_frame()?;
        (frame.id, frame.selected_window)
    };
    let leaf = eval.frames.get(frame_id)?.find_window(window_id)?;
    (leaf.buffer_id() == Some(current_buffer)).then_some((frame_id, window_id))
}

fn snapshot_text_rows(snapshot: &WindowDisplaySnapshot) -> Vec<&DisplayRowSnapshot> {
    let mut rows: Vec<_> = snapshot
        .rows
        .iter()
        .filter(|row| row.start_buffer_pos.is_some() && row.end_buffer_pos.is_some())
        .collect();
    rows.sort_by_key(|row| row.row);
    rows
}

fn snapshot_row_index_for_pos(rows: &[&DisplayRowSnapshot], pos: LispCharPos1) -> Option<usize> {
    rows.iter().position(|row| {
        row.start_buffer_pos.is_some_and(|start| start <= pos)
            && row.end_buffer_pos.is_some_and(|end| pos <= end)
    })
}

/// The row a display-motion origin belongs to.
///
/// GNU never asks which row contains a position.  `Fvertical_motion` reseats at
/// the start of the origin's line, walks forward to the origin, and -- when that
/// walk overshoots a line that is truncated on the right (`it.line_wrap ==
/// TRUNCATE && it.current_x >= it.last_visible_x`) -- backtracks one line,
/// landing back on the truncated row itself (src/indent.c:2393-2400, "We might
/// overshoot if lines are truncated and point lies beyond the right margin").
///
/// A row answers the same question with its span, and a right-truncated row's
/// span stops where the drawing stopped (`find_row_edges` gives it
/// `it->current.pos` for `maxpos`, src/xdisp.c:25269), so a position beyond the
/// right margin falls in the gap before the next row starts instead of in any
/// row at all.  Rows partition the source, so such a gap is always text a row
/// stopped drawing without ending its line, and that row owns the origin --
/// which is exactly the row GNU's overshoot correction lands on.
fn snapshot_row_index_for_pos_or_truncated_line(
    rows: &[&DisplayRowSnapshot],
    pos: LispCharPos1,
    end_lisp: LispCharPos1,
) -> Option<usize> {
    if let Some(index) = snapshot_row_index_for_pos(rows, pos) {
        return Some(index);
    }
    rows.iter().enumerate().rev().find_map(|(index, row)| {
        // A next row bounds an omitted (e.g. truncated) source tail. Without
        // one, only an explicitly measured truncation boundary grants ownership.
        let inside_line = match rows.get(index + 1).and_then(|next| next.start_buffer_pos) {
            Some(next_start) => pos < next_start,
            None => row
                .truncated_end_buffer_pos
                .is_some_and(|end| pos <= end.min(end_lisp)),
        };
        (row.start_buffer_pos.is_some_and(|start| start <= pos) && inside_line).then_some(index)
    })
}

/// Leave pushed replacement text without discarding the physical rows crossed.
/// GNU's move_it_by_lines advances out of a replacing string before returning
/// a buffer point. Its vpos still counts those rows.
fn stop_after_source_anchor(
    snapshot: &WindowDisplaySnapshot,
    rows: &[&DisplayRowSnapshot],
    from: usize,
    anchor: LispCharPos1,
) -> Option<(usize, LispCharPos1)> {
    for (index, row) in rows.iter().enumerate().skip(from) {
        let start = row.start_buffer_pos?;
        let end = row.end_buffer_pos?;
        if start > anchor {
            return Some((index, start));
        }
        if end > anchor {
            // Only the final mixed row needs a glyph scan. String-only rows
            // are skipped in O(1), keeping the entire walk O(rows + points).
            let target = snapshot
                .iter_row_points(row.row)
                .filter(|point| point.buffer_pos > anchor)
                .map(|point| point.buffer_pos)
                .chain(std::iter::once(end))
                .min()?;
            return Some((index, target));
        }
    }
    None
}

/// One place on a screen row where GNU's goal-column walk can come to rest.
///
/// GNU reaches a `vertical-motion` goal column with
/// `move_it_in_display_line (&it, ZV, first_x + to_x, MOVE_TO_X)`
/// (src/indent.c:2540), and `move_it_in_display_line_to` has TWO ways to stop:
/// at a glyph that reaches the goal x, or -- when the goal is past everything
/// the row draws -- where the DISPLAY LINE itself ends. Naming both as stops
/// keeps that second exit from being an afterthought: a row's end is a
/// position in its own right, and on a newline-terminated row it is the
/// newline, which sits one column past the last glyph because it draws none.
#[derive(Clone, Copy)]
struct RowGoalStop {
    col: i64,
    x: i64,
    pos: LispCharPos1,
}

impl RowGoalStop {
    /// Ordering key for "the LAST stop that does not pass the goal column".
    ///
    /// GNU's `MOVE_TO_X` walk places a glyph only while it still fits before
    /// the goal, and backs up to `x_before_this_char` as soon as one would
    /// pass it (src/xdisp.c:10385-10400), so the answer is the greatest stop
    /// column that is `<= goal` -- never the nearer stop beyond it.  Measured
    /// under GNU Emacs 31.0.90 on a 24-column window whose row starts with a
    /// TAB: goal columns 1 through 7 all answer the TAB's own position at
    /// column 0, and only goal 8 reaches the glyph after it.
    fn reach_key(self, target_col: i64) -> (i64, i64, i64, i64) {
        let reached = self.col <= target_col;
        // Among reachable stops take the greatest column; among unreachable
        // ones (a goal before the row's first stop) take the smallest.
        let order = if reached { self.col } else { -self.col };
        (i64::from(reached), order, self.x, self.pos.as_i64())
    }
}

/// Every stop the goal-column walk may land on for one row: the drawn glyphs,
/// then the row's own end boundary.
fn row_goal_stops(
    snapshot: &WindowDisplaySnapshot,
    row: &DisplayRowSnapshot,
    wrap: LineWrap,
) -> impl Iterator<Item = RowGoalStop> {
    let admit_edge = wrap.goal_stops_at_row_edge();
    let glyphs = snapshot
        .iter_row_points(row.row)
        // A marker column IS a goal stop, except under WORD_WRAP.  See
        // [`LineWrap::goal_stops_at_row_edge`] for GNU's mechanism and the
        // measurement; ledger 212 section 5 declined this without the gate and
        // recorded the 45 probes it cost, and ledger 212 residual 1 named the
        // reading it could not make come out -- it was reading
        // `move_it_in_display_line_to`, and the deciding code is its CALLER.
        .filter(move |point| admit_edge || point.role.is_position())
        .map(|point| RowGoalStop {
            col: point.col,
            x: point.x,
            pos: point.buffer_pos,
        });
    let row_end = row.end_buffer_pos.map(|pos| RowGoalStop {
        col: row.end_col,
        x: row.end_x,
        pos,
    });
    glyphs.chain(row_end)
}

/// The position a `(COLS . LINES)` goal lands on within one published row.
///
/// `cols` stays exactly as Lisp wrote it: the snapshot's columns are
/// WINDOW-relative (`DisplayPointSnapshot::col` is measured from the text
/// area's left edge), which is the space GNU's goal is already expressed in.
/// GNU has to add `it->first_visible_x` (`src/indent.c:2540`) only because its
/// walk counts from the LINE start; the scanner in `editing/indent` does the same for
/// the same reason (`ScreenLineExtent::goal_col_in_line_space`), and doing it
/// here as well would apply the hscroll twice.
fn snapshot_target_pos_on_row(
    snapshot: &WindowDisplaySnapshot,
    row: &DisplayRowSnapshot,
    cols: Option<i64>,
    wrap: LineWrap,
) -> Option<LispCharPos1> {
    let Some(target_col) = cols.map(|col| col.max(0)) else {
        return row.start_buffer_pos;
    };
    row_goal_stops(snapshot, row, wrap)
        .max_by_key(|stop| stop.reach_key(target_col))
        .map(|stop| stop.pos)
        .or(row.start_buffer_pos)
}

/// Resolve motion from fresh retained rows or the canonical row producer.
/// No point, viewport marker, or presentation is committed here.
pub(crate) fn resolve(
    eval: &mut Context,
    request: MotionRequest,
) -> Result<Option<MeasuredMotion>, Flow> {
    if !MotionEngine::for_context(eval).uses_display_rows() {
        return Ok(None);
    }
    let Some((frame_id, window_id)) = motion_window(eval, request.window, request.buffer) else {
        return Ok(None);
    };
    let request = MotionRequest {
        window: Some(window_id),
        ..request
    };
    let Some((accessible, wrap)) = motion_parameters(eval, request) else {
        return Ok(None);
    };
    if let Some(snapshot) = eval.fresh_window_display_snapshot(frame_id, window_id, request.buffer)
        && let Some(motion) = vertical_motion_on_rows(snapshot, request, accessible, wrap).settled()
    {
        return Ok(Some(motion));
    }

    // GNU reruns the display iterator when redisplay's rows are unavailable.
    // The column scanner cannot substitute for that walk: display strings,
    // proportional faces and replacements can change the row boundaries.
    measurement::resolve(eval, frame_id, window_id, request)
}

fn motion_parameters(
    eval: &mut Context,
    request: MotionRequest,
) -> Option<(AccessibleCharRange, LineWrap)> {
    // Read wrapping after a query's Lisp callbacks, not before them. A plain
    // row request has no goal-column walk and needs no wrapping lookup.
    let wrap = match request.goal_column {
        Some(_) => crate::emacs_core::window_cmds::window_line_wrap(
            eval,
            request.window.map(|id| Value::make_window(id.0)),
            request.buffer,
            MotionEngine::DisplayIterator,
        ),
        None => LineWrap::Truncate,
    };
    Some((
        eval.buffers.get(request.buffer)?.accessible_char_region(),
        wrap,
    ))
}

fn vertical_motion_on_rows(
    snapshot: &WindowDisplaySnapshot,
    request: MotionRequest,
    accessible: AccessibleCharRange,
    wrap: LineWrap,
) -> RowMotion {
    use crate::window::DisplayRowEndSource;

    let rows = snapshot_text_rows(snapshot);
    let Some(mut current_idx) =
        snapshot_row_index_for_pos_or_truncated_line(&rows, request.origin, accessible.end_lisp())
    else {
        return RowMotion::NeedsMoreRows;
    };
    // A before-string inserts rows before the source cursor; positive motion
    // begins where its buffer character resumes. Backwards motion still
    // counts every inserted physical row.
    if request.rows >= 0 {
        while current_idx + 1 < rows.len()
            && rows[current_idx].end_source == DisplayRowEndSource::OverlayBeforeString
            && rows[current_idx].end_buffer_pos == Some(request.origin)
        {
            current_idx += 1;
        }
    }
    let mut target_idx = (current_idx as i64).saturating_add(request.rows);
    let mut source_floor = None;

    // GNU's first positive step leaves the entire replacement at point; any
    // remaining steps advance from that valid source position. Keep the
    // original physical index so traversed replacement rows remain counted.
    let current = rows[current_idx];
    if request.rows > 0 && current.end_source.is_overlay_string() {
        let mut first_step = current_idx + 1;
        while first_step < rows.len() && rows[first_step].end_source.is_overlay_string() {
            first_step += 1;
        }
        if first_step == rows.len() {
            return RowMotion::NeedsMoreRows;
        }
        target_idx = (first_step as i64).saturating_add(request.rows - 1);
    }
    // Zero motion reseats before the current logical line. Reaching an
    // anchor from the preceding buffer newline is a valid source cursor;
    // starting at BEGV already inside pushed text is not. Query the missing
    // predecessor rather than treating the retained viewport edge as BEGV.
    let zero_at_source_boundary = request.rows == 0
        && current_idx > 0
        && rows[current_idx - 1].end_source == DisplayRowEndSource::Buffer;
    if request.rows == 0
        && current_idx == 0
        && request.origin > accessible.start_lisp()
        && current.end_source.is_display_string()
    {
        return RowMotion::NeedsMoreRows;
    }
    if request.rows >= 0
        && !zero_at_source_boundary
        && current.end_source.is_display_string()
        && current.end_buffer_pos == Some(request.origin)
    {
        let Some((after_idx, after_pos)) =
            stop_after_source_anchor(snapshot, &rows, current_idx + 1, request.origin)
        else {
            return RowMotion::NeedsMoreRows;
        };
        let steps = if request.rows == 0 {
            // GNU move_it_vertically_backward(dy=0) probes until the source
            // cursor is valid, or until an explicit newline in a pushed
            // string, then replays that many physical rows. A visual wrap is
            // not the same cursor state as a string newline.
            rows[current_idx..after_idx]
                .iter()
                .position(|row| row.end_source == DisplayRowEndSource::DisplayStringNewline)
                .map_or(after_idx - current_idx, |index| index + 1) as i64
        } else {
            request.rows
        };
        target_idx = (after_idx as i64).saturating_add(steps.saturating_sub(1).max(0));
        source_floor = Some((after_idx, after_pos));
    }

    if target_idx < 0 {
        if rows[0].start_buffer_pos != Some(accessible.start_lisp()) {
            return RowMotion::NeedsMoreRows;
        }
        let Some(target) = snapshot_target_pos_on_row(snapshot, rows[0], request.goal_column, wrap)
        else {
            return RowMotion::NeedsMoreRows;
        };
        return RowMotion::AccessibleBoundary(MeasuredMotion {
            target,
            moved: -(current_idx as i64),
        });
    }
    if target_idx >= rows.len() as i64 {
        // A viewport edge is not evidence of an accessible-buffer boundary.
        let last_idx = rows.len() - 1;
        if rows[last_idx].end_buffer_pos != Some(accessible.end_lisp()) {
            return RowMotion::NeedsMoreRows;
        }
        return RowMotion::AccessibleBoundary(MeasuredMotion {
            target: accessible.end_lisp(),
            moved: last_idx as i64 - current_idx as i64,
        });
    }
    let mut target_idx = target_idx as usize;
    if request.rows > 0 {
        while target_idx + 1 < rows.len() && rows[target_idx].end_source.is_overlay_string() {
            target_idx += 1;
        }
        if rows[target_idx].end_source.is_overlay_string() {
            return RowMotion::NeedsMoreRows;
        }
    }
    if request.rows >= 0 && target_idx > 0 {
        let previous = rows[target_idx - 1];
        if previous.end_source.is_display_string()
            && previous.end_buffer_pos == rows[target_idx].start_buffer_pos
        {
            let anchor = previous.end_buffer_pos.expect("source row");
            let Some((after_idx, after_pos)) =
                stop_after_source_anchor(snapshot, &rows, target_idx, anchor)
            else {
                return RowMotion::NeedsMoreRows;
            };
            target_idx = after_idx;
            source_floor = Some((after_idx, after_pos));
        }
    }
    let Some(mut target) =
        snapshot_target_pos_on_row(snapshot, rows[target_idx], request.goal_column, wrap)
    else {
        return RowMotion::NeedsMoreRows;
    };
    if let Some((row_idx, source_pos)) = source_floor
        && row_idx == target_idx
    {
        // A goal column inside pushed text cannot move point back into the
        // replacement after the vertical walk has left it.
        target = target.max(source_pos);
    }
    RowMotion::Complete(MeasuredMotion {
        target,
        moved: if request.rows == 0 {
            0
        } else {
            target_idx as i64 - current_idx as i64
        },
    })
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
