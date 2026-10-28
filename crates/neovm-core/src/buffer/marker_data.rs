use crate::heap_types::{LispMarker, MarkerBuffer};

use super::buffer::{BufferId, InsertionType};
use super::position::{CharPos0, EmacsBytePos, TextPositionAnchor};
use super::text::TextExtentDelta;

pub(super) fn marker_data_anchor(data: &LispMarker) -> TextPositionAnchor {
    TextPositionAnchor::new(data.charpos, data.bytepos)
}

pub(super) fn marker_data_byte_pos(data: &LispMarker) -> EmacsBytePos {
    data.bytepos
}

pub(super) fn set_marker_data_anchor(data: &mut LispMarker, anchor: TextPositionAnchor) {
    data.bytepos = anchor.emacs_byte_pos();
    data.charpos = anchor.char_pos();
}

pub(super) fn apply_marker_data_delta(data: &mut LispMarker, delta: TextExtentDelta) {
    set_marker_data_anchor(data, delta.apply_to_anchor(marker_data_anchor(data)));
}

pub(super) fn positioned_marker_data(
    buffer: BufferId,
    anchor: TextPositionAnchor,
    insertion_type: InsertionType,
) -> LispMarker {
    let mut data = LispMarker::new(
        MarkerBuffer::attached(buffer),
        CharPos0::default(),
        EmacsBytePos::default(),
        insertion_type == InsertionType::After,
        true,
    );
    set_marker_data_anchor(&mut data, anchor);
    data
}
