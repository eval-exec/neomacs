//! Numeric word-boundary checkpoints shared by visible and worker row walks.
use crate::coords::layout_i64_char_pos_to_lisp_char_pos;
use crate::display_item::DisplaySourcePosition;
use crate::display_row::builder::{
    DisplayRowAppendProgress, DisplayRowGlyphCheckpoint, DisplayRowGlyphSlot,
};
use crate::display_row::face_state::DisplayRowExtendFace;
use crate::display_row::walk_state::WordWrapRenderState;
use crate::display_source::{DisplaySourceTextOrigin, DisplaySourceTextPosition};
use neovm_core::buffer::LispCharPos1;

fn buffer_slot_window_source_position(
    slot: &DisplayRowGlyphSlot,
    text_origin: DisplaySourceTextOrigin,
) -> Option<DisplaySourceTextPosition> {
    let DisplaySourcePosition::Buffer {
        char_pos, byte_pos, ..
    } = slot.source()
    else {
        return None;
    };
    text_origin.position_from_buffer(byte_pos, char_pos)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn record_text_progress(
    text: &str,
    text_origin: DisplaySourceTextOrigin,
    word_wrap: &mut WordWrapRenderState,
    output_display_point_start: usize,
    output_row_positions_start: (Option<LispCharPos1>, Option<LispCharPos1>),
    row_glyph_checkpoint_start: DisplayRowGlyphCheckpoint,
    row_glyph_checkpoint_after_append: DisplayRowGlyphCheckpoint,
    append_progress: &DisplayRowAppendProgress,
    predecessor_row_extend: Option<DisplayRowExtendFace>,
    active_row_extend: Option<DisplayRowExtendFace>,
) {
    if !word_wrap.is_enabled() {
        return;
    }
    let mut first_run_charpos = output_row_positions_start.0;
    let mut previous_charpos = output_row_positions_start.1;
    for (char_offset, (ch, slot)) in text.chars().zip(append_progress.slots()).enumerate() {
        if let Some(source_position) = buffer_slot_window_source_position(slot, text_origin) {
            let charpos = source_position.charpos();
            let row_first =
                first_run_charpos.or_else(|| Some(layout_i64_char_pos_to_lisp_char_pos(charpos)));
            if word_wrap.can_record_candidate(ch) {
                word_wrap.record_candidate_at(
                    ch,
                    source_position,
                    output_display_point_start + char_offset,
                    (row_first, previous_charpos),
                    // Wide-character padding and composition make source and
                    // glyph offsets differ. Use the boundary recorded by the
                    // canonical writer before it appended this source slot.
                    row_glyph_checkpoint_start.with_added_text_glyphs(
                        slot.text_glyph_offset().unwrap_or(char_offset),
                        row_glyph_checkpoint_after_append,
                    ),
                    slot.start_position(),
                    if char_offset == 0 {
                        predecessor_row_extend
                    } else {
                        active_row_extend
                    },
                );
            }
            first_run_charpos = row_first;
            previous_charpos = Some(layout_i64_char_pos_to_lisp_char_pos(charpos));
        }
        word_wrap.allow_after_current_char(ch);
    }
}
