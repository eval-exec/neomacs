//! Recenter placement retains its units until the canonical row producer can
//! resolve it. GNU window.c:7309-7389 measures bottom placement in pixels.

use super::*;
use crate::buffer::EmacsBytePos;
use std::num::{NonZeroU64, NonZeroUsize};

#[derive(Clone, Copy, Debug)]
pub(crate) enum RecenterPlacement {
    Center,
    FromTop(i64),
    FromBottom(NonZeroU64),
}

impl RecenterPlacement {
    pub(crate) fn from_argument(argument: Option<&Value>) -> Self {
        match argument.copied().and_then(Value::as_fixnum) {
            Some(lines) if lines >= 0 => Self::FromTop(lines),
            Some(lines) => {
                Self::FromBottom(NonZeroU64::new(lines.unsigned_abs()).expect("negative row count"))
            }
            None => Self::Center,
        }
    }

    /// GNU's initial-frame/batch path uses canonical screen-line counts.
    pub(crate) fn canonical_rows(self, height: i64) -> i64 {
        match self {
            Self::Center => height / 2,
            Self::FromTop(lines) => lines,
            Self::FromBottom(lines) => height.saturating_sub_unsigned(lines.get()).max(0),
        }
    }
}

pub(crate) enum RecenterTarget {
    Keep,
    Start(EmacsBytePos),
}

fn failure(message: &str) -> Flow {
    signal(LispCondition::Error, vec![Value::string(message)])
}

/// None retains the canonical engine when no graphical row producer exists.
pub(crate) fn resolve(
    eval: &mut Context,
    frame: FrameId,
    window: WindowId,
    buffer: BufferId,
    placement: RecenterPlacement,
) -> Result<Option<RecenterTarget>, Flow> {
    if matches!(placement, RecenterPlacement::FromTop(_))
        || eval.noninteractive()
        || eval
            .frames
            .get(frame)
            .is_none_or(|frame| frame.initial || frame.effective_window_system().is_none())
    {
        return Ok(None);
    }
    paging::measure_stable_plan(
        eval,
        paging::MeasurementTarget::Fixed {
            frame,
            window,
            buffer,
        },
        |eval| measure(eval, frame, window, buffer, placement),
    )
}

fn measure(
    eval: &mut Context,
    frame: FrameId,
    window: WindowId,
    buffer: BufferId,
    placement: RecenterPlacement,
) -> Result<Option<RecenterTarget>, Flow> {
    let Some(source) = eval.buffers.get(buffer) else {
        return Err(failure("Recenter buffer was deleted"));
    };
    // GNU's selected window uses the buffer's live point. Once a callback
    // deselects this captured target, its independent window point takes over.
    let selected = eval
        .frames
        .selected_frame()
        .is_some_and(|selected| selected.id == frame && selected.selected_window == window);
    let origin = if selected {
        source.emacs_byte_pos_to_lisp_char_pos(source.point_emacs_byte_pos())
    } else {
        match eval
            .frames
            .get(frame)
            .and_then(|frame| frame.find_window(window))
        {
            Some(crate::window::Window::Leaf { point, .. }) => *point,
            _ => return Err(failure("Recenter window was deleted")),
        }
    };
    let accessible = source.accessible_char_region();
    let origin = origin.clamp(accessible.start_lisp(), accessible.end_lisp());
    let mut start = measurement::backtrack(source, origin, 1);
    let body_height = crate::emacs_core::window_cmds::builtin_window_body_height(
        eval,
        &[Value::make_window(window.0), Value::T],
    )?
    .as_fixnum()
    .unwrap_or(0);
    let line_height = eval
        .frames
        .get(frame)
        .map_or(1, |frame| frame.char_height.round().max(1.0) as i64);
    let canonical_height = (body_height / line_height).max(1);
    let margin = policy::ScrollPolicy::capture_for_buffer(eval, buffer)
        .margin_pixels(body_height, line_height)
        / line_height;
    let rows_below = match placement {
        RecenterPlacement::Center => None,
        RecenterPlacement::FromBottom(rows) => Some(rows.get().clamp(
            (margin + 1) as u64,
            (canonical_height - margin).max(margin + 1) as u64,
        ) as usize),
        RecenterPlacement::FromTop(_) => return Ok(None),
    };
    let mut count = rows_below.unwrap_or(0).saturating_add(4);
    loop {
        let Some(snapshot) = measurement::query(
            eval,
            frame,
            window,
            buffer,
            start,
            NonZeroUsize::new(count).expect("nonzero row budget"),
        )?
        else {
            return Ok(None);
        };
        let source = eval
            .buffers
            .get(buffer)
            .ok_or_else(|| failure("Recenter buffer was deleted"))?;
        let accessible = source.accessible_char_region();
        let rows = snapshot_text_rows(&snapshot);
        if rows.is_empty() {
            return Err(failure("Recenter measurement has no source rows"));
        }
        if let Some(anchor) = rows
            .iter()
            .position(|row| row.start_buffer_pos == Some(origin))
            .or_else(|| {
                snapshot_row_index_for_pos_or_truncated_line(&rows, origin, accessible.end_lisp())
            })
        {
            let budget = if let Some(below) = rows_below {
                let available = rows.len() - anchor;
                let at_end =
                    rows.last().and_then(|row| row.end_buffer_pos) == Some(accessible.end_lisp());
                if below > available && !at_end {
                    count = expand(count)?;
                    continue;
                }
                let last = rows[anchor + below.min(available) - 1];
                let missing = below.saturating_sub(available) as i64;
                // GNU first checks whether the complete advances leave any
                // room above point. Only then is the final row's extra
                // spacing ignored by the backward overshoot correction.
                let reserved = last
                    .y
                    .saturating_sub(rows[anchor].y)
                    .saturating_add(last.height)
                    .saturating_add(
                        missing.saturating_mul(line_height.saturating_add(last.extra_line_spacing)),
                    );
                let remaining = body_height.saturating_sub(reserved);
                if remaining <= 0 {
                    return Ok(Some(RecenterTarget::Keep));
                }
                remaining.saturating_add(last.extra_line_spacing)
            } else {
                body_height / 2
            };
            let goal = rows[anchor].y.saturating_sub(budget);
            if goal >= rows[0].y || start == accessible.start_lisp() {
                let target = if rows_below.is_some() {
                    // A row before the goal would leave point partially
                    // visible. GNU corrects that overshoot by moving forward.
                    rows[..=anchor].iter().position(|row| row.y >= goal)
                } else {
                    rows[..=anchor].iter().rposition(|row| row.y <= goal)
                }
                .unwrap_or(0);
                let position = source
                    .lisp_pos_to_emacs_byte_pos(rows[target].start_buffer_pos.expect("source row"));
                return Ok(Some(RecenterTarget::Start(position)));
            }
            start = measurement::backtrack(source, start, count);
        } else if rows.last().and_then(|row| row.end_buffer_pos) == Some(accessible.end_lisp()) {
            return Err(failure("Recenter point is outside measured coverage"));
        }
        count = expand(count)?;
    }
}

fn expand(count: usize) -> Result<usize, Flow> {
    count
        .checked_mul(2)
        .ok_or_else(|| failure("Recenter measurement exceeds the row address space"))
}
