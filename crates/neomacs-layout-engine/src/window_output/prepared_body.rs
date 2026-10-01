//! Position future buffer rows with the same recorder used by synchronous
//! output. This stage grants neither freshness nor window publication rights.

use super::row_geometry::WindowRowGeometry;
use super::snapshot_rows::PreparedWindowRows;
use super::{DisplayRowTerminator, DisplayRowTerminatorCell};
use crate::display_item::DisplaySourcePosition;
use crate::row_layout::program::{ComputedRow, ComputedRowEnd, RowProgramError};
use neomacs_display_protocol::glyph_matrix::GlyphRow;
use neovm_core::buffer::LispCharPos1;

pub(crate) struct PreparedBody {
    pub glyph_rows: Vec<GlyphRow>,
    pub geometry: PreparedWindowRows,
}

/// All origins are in frame pixels, except returned row Y, which is relative
/// to the owning window. Natural buffer text, including visual continuations;
/// display strings require
/// their canonical source-slot projection before they can use this recorder.
#[allow(clippy::too_many_arguments)]
pub(crate) fn position_buffer_rows(
    computed: Vec<ComputedRow>,
    text_row_base: usize,
    text_x: f32,
    body_y: f32,
    window_top: f32,
    window_id: u64,
    window_bounds: neomacs_display_protocol::types::Rect,
    matrix_ncols: usize,
) -> Result<PreparedBody, RowProgramError> {
    let mut geometry = WindowRowGeometry::new(text_row_base, text_x, window_top);
    let mut glyph_rows = Vec::with_capacity(computed.len());
    let mut y = body_y;
    for (index, mut computed) in computed.into_iter().enumerate() {
        let (
            DisplaySourcePosition::Buffer {
                buffer_id,
                char_pos: start,
                ..
            },
            DisplaySourcePosition::Buffer {
                buffer_id: end_buffer,
                char_pos: end,
                ..
            },
        ) = (&computed.source.start, &computed.source.end)
        else {
            return Err(RowProgramError::Unsupported);
        };
        if buffer_id != end_buffer || end <= start {
            return Err(RowProgramError::Unsupported);
        }
        let row_index = (text_row_base + index) as i64;
        let relative_y = (y - window_top).round() as i64;
        geometry.begin_current_row_progress(Some(index), row_index, 0, relative_y, 0);
        geometry.note_row_walk_start(LispCharPos1::from_one_based_usize(start.get() + 1));
        if computed.slots.len() != computed.slot_heights.len() {
            return Err(RowProgramError::Unsupported);
        }
        for (slot, slot_height) in computed.slots.into_iter().zip(computed.slot_heights) {
            // Overlay insertions carry glyph provenance, but do not consume
            // buffer positions or add ordinary buffer point cells.
            if matches!(slot.source(), DisplaySourcePosition::LispString { .. }) {
                continue;
            }
            if matches!(slot.source(), DisplaySourcePosition::Synthetic { source_id, .. }
                if source_id.get() == crate::display_row::source_append::SyntheticTextMarker::InvisibleEllipsis.source_id())
            {
                continue;
            }
            let DisplaySourcePosition::Buffer {
                buffer_id: slot_buffer,
                char_pos,
                ..
            } = slot.source()
            else {
                return Err(RowProgramError::Unsupported);
            };
            if slot_buffer != *buffer_id {
                return Err(RowProgramError::Unsupported);
            }
            geometry.push_text_display_point(
                LispCharPos1::from_one_based_usize(char_pos.get() + 1),
                text_x + slot.x_px(),
                y,
                slot.width_px(),
                slot_height,
                index,
                slot.col(),
            );
        }
        geometry.update_current_row_progress(
            row_index,
            computed.end.col() as i64,
            relative_y,
            computed.end.x_px().round() as i64,
        );
        // A complete physical line's last consumed character is its newline.
        if computed.end_kind == ComputedRowEnd::Newline {
            geometry.note_row_terminator(DisplayRowTerminator::new(
                LispCharPos1::from_one_based_usize(end.get()),
                DisplayRowTerminatorCell::new(
                    computed.terminator_width,
                    computed.terminator_height,
                ),
            ));
        }
        geometry.push_text_row(y, computed.row.height_px, computed.row.ascent_px);
        computed.row.pixel_y = y - window_top;
        computed.row.start_charpos = start.get();
        computed.row.end_charpos =
            end.get() - usize::from(computed.end_kind == ComputedRowEnd::Newline);
        crate::display_row::finalizer::GlyphRowFinalizationContext::new(
            window_id,
            text_row_base + index,
            window_bounds,
        )
        .finalize_row(&mut computed.row, matrix_ncols, None);
        // Decoration depends on the finalized paragraph direction. Resolve
        // it after bidi, just as the visible window's fringe pass does.
        if let Some(fringe) = &computed.fringe {
            fringe.decorate_row(
                &mut computed.row,
                computed.end_kind == ComputedRowEnd::Continuation,
                computed.continuation,
            );
        }
        computed.row.hash = computed.row.compute_hash();
        y += computed.row.height_px;
        glyph_rows.push(computed.row);
    }
    Ok(PreparedBody {
        glyph_rows,
        geometry: geometry.finish((body_y - window_top).round() as i64),
    })
}
