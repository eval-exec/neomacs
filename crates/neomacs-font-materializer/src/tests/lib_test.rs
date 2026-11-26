use super::*;

#[test]
fn gnu_default_line_height_discards_driver_internal_leading() {
    assert_eq!(
        bitmap_line_height(9.0, 3.0, 16.0, BitmapLineHeightPolicy::GnuDefault),
        12.0
    );
    assert_eq!(
        bitmap_line_height(9.0, 3.0, 16.0, BitmapLineHeightPolicy::NativeMetrics),
        16.0
    );
}

#[test]
fn gnu_fixed_mono_metrics_use_the_maximum_advance() {
    assert_eq!(
        fixed_font_horizontal_metrics(FixedFontSpacing::MonospaceOrCharacterCell, 8.0, 4.0, 7.5,),
        (8.0, 8.0)
    );
    assert_eq!(
        fixed_font_horizontal_metrics(FixedFontSpacing::ProportionalOrDual, 8.0, 4.0, 7.5),
        (4.0, 7.5)
    );
}

#[test]
fn bitmap_row_rejects_out_of_bounds_and_non_positive_rows() {
    let mut backing = [
        0x10u8, 0x11, 0x12, 0x13, 0x20, 0x21, 0x22, 0x23, 0x30, 0x31, 0x32, 0x33, 0x40, 0x41, 0x42,
        0x43,
    ];
    for (rows, y) in [(1, 1), (0, 0), (-1, 0)] {
        let raw = freetype::ffi::FT_Bitmap {
            rows,
            width: 4,
            pitch: 4,
            buffer: backing.as_mut_ptr(),
            num_grays: 256,
            pixel_mode: freetype::ffi::FT_PIXEL_MODE_GRAY as std::ffi::c_char,
            palette_mode: 0,
            palette: std::ptr::null_mut(),
        };
        let bitmap = unsafe { freetype::Bitmap::from_raw(&raw) };
        let result = bitmap_row(&bitmap, y, 4);
        assert!(
            matches!(
                result,
                Err(FontMaterializationError::MalformedBitmapBuffer { .. })
            ),
            "expected MalformedBitmapBuffer for rows={rows}, y={y}, got {result:?}"
        );
    }
}

#[test]
fn bitmap_row_reads_negative_pitch_from_the_bottom_row_up() {
    // Keep a 4-byte guard prefix so even the unfixed negative-pitch offset
    // stays inside `backing` during red-first testing.
    let mut backing = [
        0xAAu8, 0xAB, 0xAC, 0xAD, // guard before buffer
        0x01, 0x02, 0x03, 0x04, // lowest address = bottom logical row (y = 1)
        0x11, 0x12, 0x13, 0x14, // highest address = top logical row (y = 0)
    ];
    let raw = freetype::ffi::FT_Bitmap {
        rows: 2,
        width: 4,
        pitch: -4,
        buffer: unsafe { backing.as_mut_ptr().add(4) },
        num_grays: 256,
        pixel_mode: freetype::ffi::FT_PIXEL_MODE_GRAY as std::ffi::c_char,
        palette_mode: 0,
        palette: std::ptr::null_mut(),
    };
    let bitmap = unsafe { freetype::Bitmap::from_raw(&raw) };
    assert_eq!(bitmap_row(&bitmap, 0, 4), Ok(&[0x11, 0x12, 0x13, 0x14][..]));
    assert_eq!(bitmap_row(&bitmap, 1, 4), Ok(&[0x01, 0x02, 0x03, 0x04][..]));
    assert!(matches!(
        bitmap_row(&bitmap, 2, 4),
        Err(FontMaterializationError::MalformedBitmapBuffer { .. })
    ));
}
