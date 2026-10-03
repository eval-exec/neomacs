//! Graphical scrolling: measure first, choose a viewport, then commit its
//! start, point and pixel offset together. GNU: window_scroll_pixel_based.

use super::policy::{PreservePoint, ScrollPolicy};
use super::*;
use crate::window::{Window, WindowScrollUpdate};
use std::num::NonZeroUsize;

/// The goal is committed with the viewport, never during speculative layout.
pub(crate) struct ScrollPlan {
    viewport: WindowScrollUpdate,
    goal: Option<ScrollGoal>,
}

impl From<WindowScrollUpdate> for ScrollPlan {
    fn from(viewport: WindowScrollUpdate) -> Self {
        Self {
            viewport,
            goal: None,
        }
    }
}

#[derive(Clone, Copy)]
enum ScrollAmount {
    Rows(i64),
    Page(i64),
}

impl ScrollAmount {
    fn from_argument(arg: Option<Value>, direction: i64) -> Self {
        match arg {
            None => Self::Page(direction),
            Some(value) if value.is_nil() => Self::Page(direction),
            Some(value) if value == Value::symbol("-") => Self::Page(-direction),
            Some(value) => Self::Rows(value.as_fixnum().unwrap_or(1).saturating_mul(direction)),
        }
    }

    fn pixels(self, body_height: i64, line_height: i64, context: i64) -> i64 {
        match self {
            Self::Rows(rows) => rows.saturating_mul(line_height),
            Self::Page(direction) => direction.saturating_mul(
                (body_height / line_height - context)
                    .max(1)
                    .saturating_mul(line_height),
            ),
        }
    }
}

fn failure(message: &str) -> Flow {
    signal(LispCondition::Error, vec![Value::string(message)])
}

/// Row production is shared with redisplay, but these owned rows do not become
/// the visible frame. Expand coverage, not the live viewport.
struct RowMeasurer {
    frame: FrameId,
    window: WindowId,
    buffer: BufferId,
}

impl RowMeasurer {
    fn accessible(&self, eval: &Context) -> Result<AccessibleCharRange, Flow> {
        eval.buffers
            .get(self.buffer)
            .map(|buffer| buffer.accessible_char_region())
            .ok_or_else(|| failure("Scroll buffer was deleted"))
    }

    fn backtrack(
        &self,
        eval: &Context,
        start: LispCharPos1,
        lines: usize,
    ) -> Result<LispCharPos1, Flow> {
        let buffer = eval
            .buffers
            .get(self.buffer)
            .ok_or_else(|| failure("Scroll buffer was deleted"))?;
        Ok(measurement::backtrack(buffer, start, lines))
    }

    fn pixels(
        &self,
        eval: &mut Context,
        origin: LispCharPos1,
        delta: i64,
        line_height: i64,
    ) -> Result<LispCharPos1, Flow> {
        // The goal is in pixels. Measure its extent directly instead of
        // repeatedly guessing a visual-row count for mixed-height text.
        let mut height = i64::try_from(delta.unsigned_abs())
            .ok()
            .and_then(|pixels| pixels.checked_add(line_height.max(1)))
            .ok_or_else(|| failure("Scroll measurement exceeds the pixel address space"))?;
        let origin_line = self.backtrack(eval, origin, 0)?;
        let mut backtracked_lines = usize::from(delta < 0);
        let mut start = self.backtrack(eval, origin, backtracked_lines)?;
        loop {
            let snapshot = self
                .pixel_rows(eval, start, height)?
                .ok_or_else(|| failure("Scroll row producer disappeared"))?;
            let rows = snapshot_text_rows(&snapshot);
            let accessible = self.accessible(eval)?;
            // A point past the right margin of a truncated line is on that
            // line's row, the same rule screen-line motion resolves by
            // (src/indent.c:2393-2400); without it the origin looks like a
            // position the rows never covered and the scroll gives up.
            if let Some(index) =
                snapshot_row_index_for_pos_or_truncated_line(&rows, origin, accessible.end_lisp())
            {
                let goal = rows[index].y.saturating_add(delta);
                if delta < 0 && goal < rows[0].y && start > accessible.start_lisp() {
                    // Physical lines can contain many visual rows. Extend
                    // by the pixel deficit and the measured source density.
                    let missing = rows[0].y.saturating_sub(goal).max(1) as u64;
                    // Estimate from complete preceding physical lines,
                    // excluding the origin line's partial visual prefix.
                    // Counting that prefix as one of the backtracked lines
                    // underestimates how many more source lines we need.
                    let preceding_end = snapshot_row_index_for_pos_or_truncated_line(
                        &rows,
                        origin_line,
                        accessible.end_lisp(),
                    )
                    .map_or(rows[index].y, |line| rows[line].y);
                    let covered = (preceding_end - rows[0].y).max(line_height).max(1) as u64;
                    let lines = missing
                        .saturating_mul(backtracked_lines as u64)
                        .div_ceil(covered)
                        .clamp(1, 64) as usize;
                    start = self.backtrack(eval, start, lines)?;
                    backtracked_lines = backtracked_lines.saturating_add(lines);
                    // The newly included source lines can be taller than
                    // the observed average. Grow geometrically, as when the
                    // origin lies beyond coverage, to avoid a nearly-complete
                    // intermediate walk followed by another full restart.
                    height = height.checked_mul(2).ok_or_else(|| {
                        failure("Scroll measurement exceeds the pixel address space")
                    })?;
                    continue;
                } else if goal >= rows.last().expect("origin row").y
                    && rows.last().and_then(|row| row.end_buffer_pos) != Some(accessible.end_lisp())
                {
                    // The requested pixel falls beyond measured coverage.
                } else {
                    let mut target = rows.iter().rposition(|row| row.y <= goal).unwrap_or(0);
                    // GNU rounds a backwards page to the nearer boundary if
                    // it overshot by more than half a default-height row.
                    if delta < 0
                        && target + 1 < rows.len()
                        && goal - rows[target].y > line_height / 2
                        && rows[target + 1].y - goal < goal - rows[target].y
                    {
                        target += 1;
                    }
                    if delta > 0 {
                        while target + 1 < rows.len()
                            && rows[target].start_buffer_pos <= Some(origin)
                        {
                            target += 1;
                        }
                    } else if delta < 0 {
                        while target > 0 && rows[target].start_buffer_pos >= Some(origin) {
                            target -= 1;
                        }
                    }
                    return rows[target]
                        .start_buffer_pos
                        .ok_or_else(|| failure("Scroll row has no source position"));
                }
            } else if rows.last().and_then(|row| row.end_buffer_pos) == Some(accessible.end_lisp())
            {
                return Err(failure("Scroll origin is outside measured source coverage"));
            }
            height = height
                .checked_mul(2)
                .ok_or_else(|| failure("Scroll measurement exceeds the pixel address space"))?;
        }
    }

    fn pixel_rows(
        &self,
        eval: &mut Context,
        start: LispCharPos1,
        height: i64,
    ) -> Result<Option<WindowDisplaySnapshot>, Flow> {
        let height = usize::try_from(height.max(1))
            .ok()
            .and_then(NonZeroUsize::new)
            .ok_or_else(|| failure("Scroll pixel extent exceeds the address space"))?;
        measurement::query_scope(
            eval,
            self.frame,
            self.window,
            self.buffer,
            crate::window::WindowLayoutQueryScope::Pixels { start, height },
        )
    }
}

pub(crate) fn try_scroll(
    eval: &mut Context,
    argument: Option<Value>,
    direction: i64,
) -> Result<bool, Flow> {
    commit_scroll_plan(eval, |eval| plan_scroll(eval, argument, direction))
}

/// Both graphical and terminal planners cross Lisp-bearing measurement.
/// Keep one transaction boundary around the entire plan, not each query.
pub(crate) fn commit_scroll_plan<Plan: Into<ScrollPlan>>(
    eval: &mut Context,
    mut measure: impl FnMut(&mut Context) -> Result<Option<Plan>, Flow>,
) -> Result<bool, Flow> {
    // Fontification is Lisp and may edit the source, resize a window, or
    // change its restrictions. Per-query freshness is insufficient: all
    // measurements in a plan must belong to one coherent input state.
    for _ in 0..12 {
        eval.maybe_quit()?;
        eval.sync_pending_resize_events();
        let Some(frame) = eval.frames.selected_frame() else {
            return Ok(false);
        };
        let frame_id = frame.id;
        let window_id = frame.selected_window;
        let Some(buffer_id) = frame.find_window(window_id).and_then(Window::buffer_id) else {
            return Ok(false);
        };
        crate::window::window_markers::sync_all_frames_for_buffer(&mut eval.frames, buffer_id);
        crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
            &mut eval.frames,
            &mut eval.buffers,
            frame_id,
        );
        let before = eval.window_layout_attempt_freshness(frame_id, window_id, buffer_id);
        let policy = ScrollPolicy::capture(eval);
        let plan = measure(eval);
        let after = eval.window_layout_attempt_freshness(frame_id, window_id, buffer_id);
        if before != after || policy != ScrollPolicy::capture(eval) {
            continue;
        }
        return match plan? {
            Some(plan) => {
                let plan = plan.into();
                plan.viewport.commit(eval)?;
                eval.scroll_goal = plan.goal;
                Ok(true)
            }
            None => Ok(false),
        };
    }
    Err(failure("Scroll measurement did not converge"))
}

fn plan_scroll(
    eval: &mut Context,
    argument: Option<Value>,
    direction: i64,
) -> Result<Option<ScrollPlan>, Flow> {
    if !MotionEngine::for_context(eval).uses_display_rows() {
        return Ok(None);
    }
    let Some(frame) = eval.frames.selected_frame() else {
        return Ok(None);
    };
    if frame.window_system.is_none() {
        return Ok(None);
    }
    let frame_id = frame.id;
    let window_id = frame.selected_window;
    let line_height = (frame.char_height as i64).max(1);
    let Some(Window::Leaf {
        buffer_id,
        window_start,
        vscroll,
        ..
    }) = frame.find_window(window_id)
    else {
        return Ok(None);
    };
    let buffer_id = *buffer_id;
    let mut start = *window_start;
    let hidden_top = -(*vscroll as i64);
    let point = eval
        .buffers
        .get(buffer_id)
        .map(|buffer| buffer.emacs_byte_pos_to_lisp_char_pos(buffer.point_emacs_byte_pos()))
        .ok_or_else(|| failure("Scroll buffer was deleted"))?;
    let height = crate::emacs_core::window_cmds::builtin_window_body_height(
        eval,
        vec![Value::make_window(window_id.0), Value::T],
    )?
    .as_fixnum()
    .unwrap_or(1)
    .max(1);
    let policy = ScrollPolicy::capture(eval);
    let amount = ScrollAmount::from_argument(argument, direction);
    let delta = amount.pixels(height, line_height, policy.context_lines);
    let measurer = RowMeasurer {
        frame: frame_id,
        window: window_id,
        buffer: buffer_id,
    };
    let accessible = measurer.accessible(eval)?;
    start = start.clamp(accessible.start_lisp(), accessible.end_lisp());
    let Some(snapshot) = measurer.pixel_rows(eval, start, height + hidden_top)? else {
        return Ok(None);
    };
    let rows = snapshot_text_rows(&snapshot);
    let point_index =
        snapshot_row_index_for_pos_or_truncated_line(&rows, point, accessible.end_lisp());
    let point_geometry = point_index.map(|index| {
        let row = rows[index];
        let top = row.y - rows[0].y - hidden_top;
        (index, top, top + row.height)
    });
    let auto_vscroll = policy.auto_vscroll;
    let next_offset = point_geometry
        .filter(|(_, top, bottom)| *top < height && *bottom > 0)
        .and_then(|(point_index, top, bottom)| {
            if auto_vscroll && delta < 0 && hidden_top > 0 && top < 0 {
                Some((hidden_top - (-top).min(-delta)).max(0))
            } else if auto_vscroll
                && delta > 0
                && bottom > height
                && (hidden_top > 0 || point_index == 0)
            {
                Some(hidden_top + (bottom - height).min(delta))
            } else {
                None
            }
        });
    if let Some(offset) = next_offset {
        return Ok(Some(ScrollPlan {
            goal: eval.scroll_goal,
            viewport: WindowScrollUpdate {
                frame: frame_id,
                window: window_id,
                buffer: buffer_id,
                start,
                point,
                hidden_top_pixels: offset.min(i32::MAX as i64) as i32,
            },
        }));
    }
    if auto_vscroll
        && delta > 0
        && let Some((index, top, bottom)) = point_geometry
        && top < height
        && bottom > height
        && index > 0
    {
        // GNU first promotes a bottom-clipped point row past ordinary rows
        // above it. Do not lose point by treating that row as invisible; the
        // following command can then scroll within its pixel extent.
        return Ok(Some(ScrollPlan {
            goal: eval.scroll_goal,
            viewport: WindowScrollUpdate {
                frame: frame_id,
                window: window_id,
                buffer: buffer_id,
                start: rows[index].start_buffer_pos.expect("source row"),
                point,
                hidden_top_pixels: 0,
            },
        }));
    }
    if point_geometry.is_none_or(|(_, top, bottom)| top >= height || bottom <= 0) {
        start = measurer.pixels(eval, point, -(height / 2), line_height)?;
    }
    let new_start = match amount {
        ScrollAmount::Page(_) => {
            // A forward page normally fits inside the current measured body.
            // Use that exact geometry, including a wrapped first row's actual
            // context, instead of starting another walk at its physical line.
            let covered = (delta > 0)
                .then(|| rows.first())
                .flatten()
                .filter(|first| first.start_buffer_pos == Some(start))
                .and_then(|first| {
                    let goal = first.y.saturating_add(delta);
                    rows.iter()
                        .rev()
                        .find(|row| row.y <= goal)
                        .filter(|row| goal < row.y.saturating_add(row.height))
                        .and_then(|row| row.start_buffer_pos)
                        .filter(|target| *target > start)
                });
            match covered {
                Some(target) => target,
                None => measurer.pixels(eval, start, delta, line_height)?,
            }
        }
        ScrollAmount::Rows(rows) => {
            super::resolve(
                eval,
                MotionRequest {
                    buffer: buffer_id,
                    window: Some(window_id),
                    origin: start,
                    rows,
                    goal_column: None,
                },
            )?
            .ok_or_else(|| failure("Scroll row producer disappeared"))?
            .target
        }
    };
    if delta > 0 && new_start >= accessible.end_lisp() {
        return Err(signal(LispCondition::EndOfBuffer, vec![]));
    }
    if delta < 0 && new_start == start && hidden_top == 0 {
        return Err(signal(LispCondition::BeginningOfBuffer, vec![]));
    }
    let candidate = measurer
        .pixel_rows(eval, new_start, height)?
        .ok_or_else(|| failure("Scroll row producer disappeared"))?;
    let candidate_rows = snapshot_text_rows(&candidate);
    let first_y = candidate_rows[0].y;
    let margin = policy.margin_pixels(height, line_height);
    let goal = (policy.preserve != PreservePoint::KeepVisible).then(|| {
        eval.scroll_goal
            .filter(|goal| {
                policy.continues_scroll
                    && goal.frame == frame_id
                    && goal.window == window_id
                    && goal.buffer == buffer_id
            })
            .unwrap_or_else(|| ScrollGoal {
                frame: frame_id,
                window: window_id,
                buffer: buffer_id,
                y: point_geometry.map_or(0, |(_, top, _)| top),
                x: snapshot
                    .iter_points()
                    .find(|stop| stop.buffer_pos == point && stop.role.is_position())
                    .map_or(0, |stop| stop.x),
            })
    });
    // A backwards scroll must not leave point on a partially visible bottom
    // row. A row taller than the entire body is the unavoidable exception.
    let mut visible = candidate_rows.iter().filter(|row| {
        let top = row.y - first_y;
        top >= margin && top + row.height <= height - margin
    });
    let first = *visible.next().unwrap_or(&candidate_rows[0]);
    let last = visible.next_back().copied().unwrap_or(first);
    // The same rule again: a point past the right margin sits on the truncated
    // row, which is visible, so the new viewport can keep it.
    let point_stays_visible =
        snapshot_row_index_for_pos_or_truncated_line(&candidate_rows, point, accessible.end_lisp())
            .is_some_and(|index| {
                candidate_rows[index].row >= first.row && candidate_rows[index].row <= last.row
            });
    let next_point = if point_stays_visible && policy.preserve != PreservePoint::Always {
        point
    } else if let Some(goal) = goal {
        let goal_y = goal.y.clamp(margin, (height - margin - 1).max(margin));
        let goal_x = goal.x;
        let row = candidate_rows
            .iter()
            .copied()
            .rfind(|row| row.row >= first.row && row.row <= last.row && row.y - first_y <= goal_y)
            .unwrap_or(first);
        row_goal_stops(&candidate, row, LineWrap::Truncate)
            .max_by_key(|stop| {
                (
                    stop.x <= goal_x,
                    if stop.x <= goal_x { stop.x } else { -stop.x },
                    stop.pos,
                )
            })
            .map_or_else(
                || row.start_buffer_pos.expect("source row"),
                |stop| stop.pos,
            )
    } else if point < new_start || delta >= 0 {
        first.start_buffer_pos.expect("source row")
    } else {
        last.start_buffer_pos.expect("source row")
    };
    Ok(Some(ScrollPlan {
        goal,
        viewport: WindowScrollUpdate {
            frame: frame_id,
            window: window_id,
            buffer: buffer_id,
            start: new_start,
            point: next_point,
            hidden_top_pixels: 0,
        },
    }))
}
