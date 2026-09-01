use crate::heap_types::LispMarker;

use super::buffer::{BufferId, InsertionType};
use super::position::{CharPos0, EmacsBytePos, TextPositionAnchor};
use super::text::TextExtentDelta;

pub(super) fn marker_data_anchor(data: &LispMarker) -> TextPositionAnchor {
    TextPositionAnchor::new(CharPos0::new(data.charpos), EmacsBytePos::new(data.bytepos))
}

pub(super) fn marker_data_byte_pos(data: &LispMarker) -> EmacsBytePos {
    EmacsBytePos::new(data.bytepos)
}

pub(super) fn set_marker_data_anchor(data: &mut LispMarker, anchor: TextPositionAnchor) {
    data.bytepos = anchor.emacs_byte_pos_usize();
    data.charpos = anchor.char_pos_usize();
}

pub(super) fn apply_marker_data_delta(data: &mut LispMarker, delta: TextExtentDelta) {
    set_marker_data_anchor(data, delta.apply_to_anchor(marker_data_anchor(data)));
}

pub(super) fn positioned_marker_data(
    buffer: BufferId,
    marker_id: u64,
    anchor: TextPositionAnchor,
    insertion_type: InsertionType,
) -> LispMarker {
    let mut data = LispMarker {
        buffer: Some(buffer),
        insertion_type: insertion_type == InsertionType::After,
        marker_id: Some(marker_id),
        bytepos: 0,
        charpos: 0,
        last_position_valid: true,
        next_marker: std::ptr::null_mut(),
    };
    set_marker_data_anchor(&mut data, anchor);
    data
}
