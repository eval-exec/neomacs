use super::*;

#[test]
fn native_bitmap_rows_follow_signed_pitch_without_copying_padding() {
    let bytes = [1, 2, 99, 3, 4, 99];
    // SAFETY: both two-byte rows and their positive/negative strides stay
    // within bytes; the second pointer names the first logical reversed row.
    unsafe {
        assert_eq!(copy_rows(bytes.as_ptr(), 3, 2, 2), Some(vec![1, 2, 3, 4]));
        assert_eq!(
            copy_rows(bytes.as_ptr().add(3), -3, 2, 2),
            Some(vec![3, 4, 1, 2])
        );
        assert_eq!(copy_rows(bytes.as_ptr(), 1, 2, 2), None);
    }
}

#[test]
fn native_mono_and_lcd_masks_keep_their_coverage_channels() {
    assert_eq!(
        unpack_mono(&[0b10100000, 0b01000000], 3, 2),
        Some(vec![255, 0, 255, 0, 255, 0])
    );
    assert_eq!(
        normalize_gray(vec![0, 1, 2, 3], 4),
        Some(vec![0, 85, 170, 255])
    );
    assert_eq!(
        pack_lcd(&[10, 80, 30, 255, 0, 7]),
        Some(vec![10, 80, 30, 80, 255, 0, 7, 255])
    );
    assert_eq!(pack_lcd(&[10, 80]), None);
}
