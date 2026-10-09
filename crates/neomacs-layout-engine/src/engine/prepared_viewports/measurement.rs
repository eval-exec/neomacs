//! Read certified worker geometry as a bounded, renderer-inert row query.
//! History alone grants no authority: source identity and captured Lisp reads
//! must still match. Partial, ambiguous or unsupported coverage falls back.
use super::*;
use neovm_core::{
    buffer::LispCharPos1,
    window::{
        FrameId, PresentedBodyRowSnapshot, WindowId, WindowLayoutQuery, WindowLayoutQueryScope,
    },
};

impl PreparedViewports {
    pub(in crate::engine) fn measure_rows(
        &self,
        evaluator: &neovm_core::emacs_core::Context,
        frame: FrameId,
        window: WindowId,
        scope: WindowLayoutQueryScope,
    ) -> Option<WindowLayoutQuery> {
        let start = match scope {
            WindowLayoutQueryScope::Rows { start, .. }
            | WindowLayoutQueryScope::Pixels { start, .. } => start,
            WindowLayoutQueryScope::Viewport
            | WindowLayoutQueryScope::Position { .. }
            | WindowLayoutQueryScope::TextExtent { .. } => {
                return None;
            }
        };
        if !scroll_coverage::inactive_overlay_arrows(evaluator) {
            return None;
        }
        let buffer_id = evaluator
            .frame_manager()
            .get(frame)?
            .find_window(window)?
            .buffer_id()?;
        let buffer = evaluator.buffer_manager().get(buffer_id)?;
        // A synchronous query must still run unfontified-source Lisp. Worker
        // paint alone does not certify that those callbacks have happened.
        let fontification = buffer
            .buffer_local_value("fontification-functions")
            .or_else(|| {
                evaluator
                    .obarray()
                    .symbol_value_copied("fontification-functions")
            });
        if fontification.is_some_and(|value| !value.is_nil()) {
            return None;
        }
        let current = evaluator.window_display_snapshot_freshness(frame, window, buffer_id)?;
        self.entries.iter().rev().find_map(|entry| {
            if !entry.computed
                || entry.frame != frame
                || entry.window.get() != window.0 as i64
                || !entry.dependencies_valid()
                || !entry
                    .query_freshness
                    .as_ref()?
                    .same_query_row_content(&current)
                || buffer.point_lisp_char_pos().as_i64() != entry.retained.key.point + 1
            {
                return None;
            }
            let retained = &entry.retained;
            let original = &retained.display_snapshot;
            let first = original
                .rows
                .iter()
                .position(|row| row.start_buffer_pos == Some(start))?;
            let available = original.rows.get(first..)?;
            let pixel_bottom = match scope {
                WindowLayoutQueryScope::Pixels { height, .. } => Some(
                    available
                        .first()?
                        .y
                        .checked_add(i64::try_from(height.get()).ok()?)?,
                ),
                _ => None,
            };
            let count = match scope {
                WindowLayoutQueryScope::Rows { count, .. } => count.get(),
                WindowLayoutQueryScope::Pixels { .. } => available
                    .iter()
                    .take_while(|row| row.y < pixel_bottom.expect("pixel extent"))
                    .count(),
                WindowLayoutQueryScope::Viewport
                | WindowLayoutQueryScope::Position { .. }
                | WindowLayoutQueryScope::TextExtent { .. } => {
                    return None;
                }
            };
            let selected = available.get(..count)?;
            let first_row = selected.first()?;
            let last_row = selected.last()?;
            // Starting in a continuation can change prefix/tab/bidi context.
            // Leave that case to the canonical source walker.
            if start != buffer.accessible_char_region().start_lisp()
                && buffer.char_before_emacs_byte_pos(buffer.lisp_pos_to_emacs_byte_pos(start))
                    != Some('\n')
            {
                return None;
            }
            if selected
                .windows(2)
                .any(|pair| pair[0].y + pair[0].height != pair[1].y)
                || selected.iter().any(|row| {
                    row.height <= 0
                        || row.start_buffer_pos.is_none()
                        || row.end_buffer_pos.is_none()
                })
            {
                return None;
            }
            let last_glyph = retained
                .matrix
                .rows
                .get(usize::try_from(last_row.row).ok()?)?;
            let end = LispCharPos1::from_one_based_usize(
                last_glyph.next_buffer_row_start()?.checked_add(1)?,
            )
            .min(buffer.accessible_char_region().end_lisp());
            // Worker admission can hold only a prefix of the requested
            // extent. Never confuse that storage boundary with buffer EOB.
            if let Some(bottom) = pixel_bottom
                && last_row.y.checked_add(last_row.height)? < bottom
                && end != buffer.accessible_char_region().end_lisp()
            {
                return None;
            }
            let top = (original.regions.text_body.y - original.regions.outer.y).round() as i64;
            let dy = top.checked_sub(first_row.y)?;
            let base = original.rows.first()?.row;
            let dr = base.checked_sub(first_row.row)?;
            // An owned copy: the query takes the snapshot by value.
            let mut snapshot = neovm_core::window::WindowDisplaySnapshot::clone(original);
            snapshot.rows = selected.to_vec();
            for row in &mut snapshot.rows {
                row.row += dr;
                row.y += dy;
            }
            snapshot.materialize_points_mut();
            snapshot
                .points
                .retain(|point| point.row >= first_row.row && point.row <= last_row.row);
            for point in &mut snapshot.points {
                point.row += dr;
                point.y += dy;
            }
            snapshot.body_rows = snapshot
                .rows
                .iter()
                .map(|row| PresentedBodyRowSnapshot {
                    output_row: row.row,
                    body_row: row.row - base,
                    body_y: row.y - top,
                })
                .collect();
            snapshot.logical_cursor = None;
            snapshot.phys_cursor = None;
            snapshot.chrome_strings = Default::default();
            snapshot.regions_materialized = true;
            snapshot.layout_freshness = Some(current.clone());
            snapshot.window_end_record = None;
            snapshot.buffer_modiff = Some(buffer.modified_tick() as i64);
            Some(WindowLayoutQuery::new(end, Some(snapshot)))
        })
    }
}
