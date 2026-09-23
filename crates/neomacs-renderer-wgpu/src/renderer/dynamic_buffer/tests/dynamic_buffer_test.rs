use super::*;

#[test]
fn vertex_upload_byte_range_starts_at_zero_for_first_upload() {
    assert_eq!(upload_byte_range(0, 48), 0..48);
}

#[test]
fn vertex_upload_byte_range_tracks_arena_offset() {
    assert_eq!(upload_byte_range(48, 48), 48..96);
}

#[test]
fn align_up_basic() {
    assert_eq!(align_up(0, 4), 0);
    assert_eq!(align_up(1, 4), 4);
    assert_eq!(align_up(4, 4), 4);
    assert_eq!(align_up(5, 4), 8);
    assert_eq!(align_up(48, 4), 48);
    assert_eq!(align_up(49, 4), 52);
}

#[test]
fn grown_capacity_first_allocation_is_at_least_4096() {
    assert_eq!(grown_capacity(0, 1), Some(4096));
    assert_eq!(grown_capacity(0, 4096), Some(4096));
    assert_eq!(grown_capacity(0, 5000), Some(5000));
}

#[test]
fn grown_capacity_doubles_until_fit() {
    assert_eq!(grown_capacity(4096, 4097), Some(8192));
    assert_eq!(grown_capacity(4096, 20000), Some(32768));
}

#[test]
fn grown_capacity_steady_state_allocates_nothing() {
    assert_eq!(grown_capacity(4096, 4096), None);
    assert_eq!(grown_capacity(8192, 100), None);
}
