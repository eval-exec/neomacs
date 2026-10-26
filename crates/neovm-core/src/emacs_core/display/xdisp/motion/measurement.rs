//! Offscreen motion through the canonical row producer. Source newlines only
//! choose a safe backtracking region; they never answer a display-row count.

use super::*;
use crate::buffer::{Buffer, EmacsByteLen, EmacsBytePos};
use crate::window::{WindowLayoutQueryOutcome, WindowLayoutQueryScope};
use std::num::NonZeroUsize;

fn source_line_start(buffer: &Buffer, pos: EmacsBytePos) -> EmacsBytePos {
    let region = buffer.accessible_emacs_byte_region();
    let raw_line_start = |position| {
        buffer
            .prev_newline_emacs_byte(position, region.start())
            .map_or(region.start(), |newline| {
                newline.add_len(EmacsByteLen::new(1))
            })
    };
    let mut start = raw_line_start(region.clamp(pos));
    // A source newline is not necessarily a display boundary. A replacing
    // or invisible property may consume it. Retreat to the owning span and
    // measure forward; never start a fresh interpretation in its middle.
    // Conservatively include all overlay carriers: over-measuring is safe,
    // choosing a boundary based on a lower-priority carrier is not.
    // Fixed GNU text-property DEFSYMs; this measurement runs for every
    // queried motion stop and previously re-interned both spellings.
    static DISPLAY_PROPERTY: crate::emacs_core::intern::EpochCachedSymbol =
        crate::emacs_core::intern::EpochCachedSymbol::new("display");
    static INVISIBLE_PROPERTY: crate::emacs_core::intern::EpochCachedSymbol =
        crate::emacs_core::intern::EpochCachedSymbol::new("invisible");
    let properties = [
        Value::symbol(&DISPLAY_PROPERTY),
        Value::symbol(&INVISIBLE_PROPERTY),
    ];
    while start > region.start() {
        let newline = start.saturating_sub_len(EmacsByteLen::new(1));
        let mut owner_start = start;
        for property in properties {
            if buffer
                .text_props_get_property_at_emacs_byte_pos(newline, property)
                .is_some_and(|value| value.is_truthy())
            {
                owner_start = owner_start.min(
                    buffer
                        .text_props_previous_single_change_before_emacs_byte_pos(start, property)
                        .unwrap_or(region.start()),
                );
            }
            for overlay in buffer.overlays().iter_overlays_at_emacs_byte_pos(newline) {
                if buffer
                    .overlays()
                    .overlay_get_named(overlay, property)
                    .is_some_and(|value| value.is_truthy())
                {
                    owner_start = owner_start.min(
                        buffer
                            .overlays()
                            .overlay_start_emacs_byte_pos(overlay)
                            .unwrap_or(start),
                    );
                }
            }
        }
        if owner_start >= start {
            break;
        }
        start = raw_line_start(region.clamp(owner_start));
    }
    start
}

pub(super) fn backtrack(buffer: &Buffer, start: LispCharPos1, count: usize) -> LispCharPos1 {
    let begv = buffer.accessible_emacs_byte_region().start();
    let mut position = source_line_start(buffer, buffer.lisp_pos_to_emacs_byte_pos(start));
    for _ in 0..count {
        if position <= begv {
            break;
        }
        position = source_line_start(buffer, position.saturating_sub_len(EmacsByteLen::new(1)));
    }
    buffer.emacs_byte_pos_to_lisp_char_pos(position)
}

fn failed(message: &str) -> Flow {
    signal(LispCondition::Error, vec![Value::string(message)])
}

/// A row query is speculative: it never installs a presentation or moves a
/// window marker. All motion consumers share its identity/freshness checks.
pub(super) fn query(
    eval: &mut Context,
    frame: FrameId,
    window: WindowId,
    buffer: BufferId,
    start: LispCharPos1,
    count: NonZeroUsize,
) -> Result<Option<WindowDisplaySnapshot>, Flow> {
    query_scope(
        eval,
        frame,
        window,
        buffer,
        WindowLayoutQueryScope::Rows { start, count },
    )
}

pub(super) fn query_scope(
    eval: &mut Context,
    frame: FrameId,
    window: WindowId,
    buffer: BufferId,
    scope: WindowLayoutQueryScope,
) -> Result<Option<WindowDisplaySnapshot>, Flow> {
    eval.maybe_quit()?;
    let snapshot = match eval.query_window_layout_scope(frame, window, scope) {
        WindowLayoutQueryOutcome::Ready(query) => query
            .into_geometry()
            .ok_or_else(|| failed("Display motion query produced no rows"))?,
        WindowLayoutQueryOutcome::Unavailable => return Ok(None),
        WindowLayoutQueryOutcome::LayoutBusy => {
            return Err(failed(
                "Window layout query reentered an active layout callback",
            ));
        }
        WindowLayoutQueryOutcome::Failed(error) => return Err(failed(error.message())),
    };
    if motion_window(eval, Some(window), buffer) != Some((frame, window)) {
        return Err(failed("Window changed during display motion"));
    }
    if snapshot.layout_freshness.as_ref().is_some_and(|freshness| {
        Some(freshness)
            != eval
                .window_display_snapshot_freshness(frame, window, buffer)
                .as_ref()
    }) {
        return Err(failed("Display motion query returned stale rows"));
    }
    Ok(Some(snapshot))
}

pub(super) fn resolve(
    eval: &mut Context,
    frame: FrameId,
    window: WindowId,
    request: MotionRequest,
) -> Result<Option<MeasuredMotion>, Flow> {
    let Some(buffer) = eval.buffers.get(request.buffer) else {
        return Ok(None);
    };
    let mut backtrack_lines = if request.rows < 0 {
        request.rows.unsigned_abs().min(64) as usize
    } else if request.rows == 0 {
        1
    } else {
        0
    };
    let mut start = backtrack(buffer, request.origin, backtrack_lines);
    // Start with the requested displacement plus boundary context. Wrapped
    // source lines can require more rows; the measured result below, rather
    // than a fixed 64-row minimum, decides when to expand.
    let initial_rows = request.rows.unsigned_abs().saturating_add(2).clamp(4, 64) as usize;
    let mut count = NonZeroUsize::new(initial_rows).expect("nonzero initial row budget");
    loop {
        let Some(snapshot) = query(eval, frame, window, request.buffer, start, count)? else {
            return Ok(None);
        };
        let (accessible, wrap) = motion_parameters(eval, request)
            .ok_or_else(|| failed("Buffer changed during display motion"))?;
        if let Some(motion) =
            vertical_motion_on_rows(&snapshot, request, accessible, wrap).settled()
        {
            return Ok(Some(motion));
        }
        // GNU backs up through source lines and measures forward. Expand
        // geometrically only when the measured rows prove the region too short.
        let rows = snapshot_text_rows(&snapshot);
        if rows.is_empty() {
            return Err(failed("Display motion measurement has no source rows"));
        }
        let incomplete_eob_tail = request.rows > 0
            && request.goal_column.is_none()
            && matches!(
                snapshot.eob_overlay_motion,
                crate::window::EobOverlayMotionCoverage::Incomplete { anchor }
                    if anchor == accessible.end_lisp()
            )
            && snapshot_row_index_for_pos_or_truncated_line(
                &rows,
                request.origin,
                accessible.end_lisp(),
            )
            .is_some();
        if request.rows < 0
            && snapshot_row_index_for_pos(&rows, request.origin).is_some()
            && start > accessible.start_lisp()
        {
            backtrack_lines = backtrack_lines.saturating_mul(2).max(1);
            let buffer = eval
                .buffers
                .get(request.buffer)
                .expect("validated source buffer");
            start = backtrack(buffer, start, backtrack_lines);
        } else if rows.last().and_then(|row| row.end_buffer_pos) == Some(accessible.end_lisp())
            && !incomplete_eob_tail
        {
            // More rows cannot expose an origin missing from complete source
            // coverage. An observed, unfinished EOB overlay is different: its
            // source anchor is covered, but its physical traversal still needs
            // rows. Do not terminate that query just because it reached ZV.
            return Err(failed(
                "Display motion origin is outside measured source coverage",
            ));
        }
        let Some(next_count) = count.get().checked_mul(2).and_then(NonZeroUsize::new) else {
            return Err(failed(
                "Display motion measurement exceeds the row address space",
            ));
        };
        count = next_count;
    }
}
