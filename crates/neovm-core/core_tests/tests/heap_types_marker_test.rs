use super::*;

#[test]
fn marker_data_new_fields_default() {
    let data = LispMarker::new(
        crate::heap_types::MarkerBuffer::DETACHED,
        crate::buffer::CharPos0::new(0),
        crate::buffer::EmacsBytePos::new(0),
        false,
        false,
    );
    assert_eq!(data.bytepos.get(), 0);
    assert_eq!(data.charpos.get(), 0);
    assert!(data.next_marker_ptr().is_null());
    assert!(!data.chained());
    assert!(!data.insertion_type());
    assert!(!data.last_position_valid());
}
