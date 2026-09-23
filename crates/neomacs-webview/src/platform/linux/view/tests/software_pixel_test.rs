use super::{SoftwarePixelLayout, SoftwarePixelLayoutError};

#[test]
fn pixel_layout_accepts_padded_rows() {
    let layout = SoftwarePixelLayout::new(784, 2, 6_656).unwrap();

    assert_eq!(layout.stride(), 3_328);
    assert_eq!(layout.packed_len(), 6_272);
}

#[test]
fn pixel_copy_discards_row_padding_and_forces_opaque_alpha() {
    let layout = SoftwarePixelLayout::new(1, 2, 16).unwrap();
    let imported = [1, 2, 3, 4, 90, 91, 92, 93, 5, 6, 7, 8, 94, 95, 96, 97];

    assert_eq!(
        layout.copy_bgra_opaque(&imported),
        [1, 2, 3, 255, 5, 6, 7, 255]
    );
}

#[test]
fn pixel_layout_rejects_an_import_larger_than_any_supported_frame() {
    assert_eq!(
        SoftwarePixelLayout::new(1_280, 1_122, 1_342_003_188_534_479_329),
        Err(SoftwarePixelLayoutError::ExceedsFrameLimit {
            actual: 1_342_003_188_534_479_329,
            maximum: 268_435_456,
        })
    );
}

#[test]
fn pixel_layout_rejects_partial_rows() {
    assert_eq!(
        SoftwarePixelLayout::new(2, 2, 17),
        Err(SoftwarePixelLayoutError::PartialRow {
            byte_len: 17,
            height: 2,
        })
    );
}

#[test]
fn pixel_layout_rejects_short_rows() {
    assert_eq!(
        SoftwarePixelLayout::new(2, 2, 8),
        Err(SoftwarePixelLayoutError::StrideTooShort {
            actual: 4,
            minimum: 8,
        })
    );
}
