use super::*;

const TEST_COMPRESSED_MODIFIER: u64 = 0x0100_0000_0000_0004;

#[test]
fn linear_validation_rejects_zero_dimensions_and_zero_bpp() {
    assert_eq!(
        validate_linear_region(0, 4, 16, 0, 4, Some(64)),
        Err(DmaBufValidationError::ZeroDimensions {
            width: 0,
            height: 4
        })
    );
    assert_eq!(
        validate_linear_region(4, 0, 16, 0, 4, Some(64)),
        Err(DmaBufValidationError::ZeroDimensions {
            width: 4,
            height: 0
        })
    );
    assert_eq!(
        validate_linear_region(4, 4, 16, 0, 0, Some(64)),
        Err(DmaBufValidationError::ZeroBytesPerPixel)
    );
}

#[test]
fn linear_validation_rejects_short_stride_overflow_and_short_backing() {
    assert_eq!(
        validate_linear_region(16, 2, 32, 0, 4, Some(256)),
        Err(DmaBufValidationError::StrideTooSmall {
            plane: 0,
            stride: 32,
            min_stride: 64,
        })
    );
    assert_eq!(
        validate_linear_region(u32::MAX, 1, u32::MAX, 0, 4, Some(u64::MAX)),
        Err(DmaBufValidationError::LayoutOverflow { plane: 0 })
    );
    assert_eq!(
        validate_linear_region(4096, 4096, 4096 * 4, 0, 4, Some(16)),
        Err(DmaBufValidationError::BackingTooSmall {
            plane: 0,
            required_end: 4096 * 4 * 4096,
            backing_len: 16,
        })
    );
    assert_eq!(
        validate_linear_region(4, 2, 16, 0, 4, None),
        Err(DmaBufValidationError::UnknownBackingSize { plane: 0 })
    );
}

#[test]
fn linear_validation_accepts_padded_stride_with_nonzero_offset() {
    assert_eq!(
        validate_linear_region(4, 2, 24, 16, 4, Some(64)),
        Ok(ValidatedLinearRegion {
            offset: 16,
            byte_len: 48,
        })
    );
}

#[test]
fn hal_validation_rejects_plane_count_mismatch_both_directions() {
    // Fewer planes than the driver modifier requires used to silently alias
    // missing planes onto plane 0 with offset 0 and row_pitch 0.
    assert_eq!(
        validate_hal_import_layout(
            64,
            64,
            4,
            TEST_COMPRESSED_MODIFIER,
            1,
            2,
            1,
            &[256],
            &[0],
            &[Some(64 * 256)],
        ),
        Err(DmaBufValidationError::PlaneCountMismatch {
            declared: 1,
            required: 2,
        })
    );

    assert_eq!(
        validate_hal_import_layout(
            64,
            64,
            4,
            DRM_FORMAT_MOD_LINEAR,
            2,
            1,
            2,
            &[256, 256],
            &[0, 16384],
            &[Some(32768), Some(32768)],
        ),
        Err(DmaBufValidationError::PlaneCountMismatch {
            declared: 2,
            required: 1,
        })
    );
}

#[test]
fn hal_validation_rejects_missing_metadata_and_unsupported_plane_counts() {
    assert_eq!(
        validate_hal_import_layout(16, 16, 4, DRM_FORMAT_MOD_LINEAR, 0, 1, 0, &[], &[], &[],),
        Err(DmaBufValidationError::UnsupportedPlaneCount {
            declared: 0,
            max: MAX_DMA_BUF_PLANES,
        })
    );
    assert_eq!(
        validate_hal_import_layout(
            16,
            16,
            4,
            TEST_COMPRESSED_MODIFIER,
            2,
            2,
            2,
            &[64],
            &[0, 1024],
            &[Some(2048), Some(2048)],
        ),
        Err(DmaBufValidationError::MissingPlaneMetadata {
            fds: 2,
            strides: 1,
            offsets: 2,
            backing_lens: 2,
            required: 2,
        })
    );
}

#[test]
fn hal_validation_checks_plane_zero_stride_and_backing_bounds() {
    assert_eq!(
        validate_hal_import_layout(
            64,
            32,
            4,
            DRM_FORMAT_MOD_LINEAR,
            1,
            1,
            1,
            &[128],
            &[0],
            &[Some(65536)],
        ),
        Err(DmaBufValidationError::StrideTooSmall {
            plane: 0,
            stride: 128,
            min_stride: 256,
        })
    );

    assert_eq!(
        validate_hal_import_layout(
            64,
            32,
            4,
            DRM_FORMAT_MOD_LINEAR,
            1,
            1,
            1,
            &[256],
            &[1024],
            &[Some(256 * 32)],
        ),
        Err(DmaBufValidationError::BackingTooSmall {
            plane: 0,
            required_end: 1024 + 256 * 32,
            backing_len: 256 * 32,
        })
    );
}

#[test]
fn hal_validation_checks_compressed_aux_planes_without_false_rejections() {
    // 1920x1080 BGRA8 main plane + Intel CCS-like aux plane sharing one fd.
    let main_stride = 1920 * 4;
    let main_size = u64::from(main_stride) * 1088;
    let aux_offset = main_size as u32;
    let aux_stride = 256;
    let total_size = main_size + u64::from(aux_stride) * 34;

    assert_eq!(
        validate_hal_import_layout(
            1920,
            1080,
            4,
            TEST_COMPRESSED_MODIFIER,
            2,
            2,
            2,
            &[main_stride, aux_stride],
            &[0, aux_offset],
            &[Some(total_size), Some(total_size)],
        ),
        Ok(vec![
            ValidatedPlaneLayout {
                offset: 0,
                row_pitch: u64::from(main_stride),
            },
            ValidatedPlaneLayout {
                offset: u64::from(aux_offset),
                row_pitch: u64::from(aux_stride),
            },
        ])
    );

    // Zero aux stride is rejected.
    assert_eq!(
        validate_hal_import_layout(
            1920,
            1080,
            4,
            TEST_COMPRESSED_MODIFIER,
            2,
            2,
            2,
            &[main_stride, 0],
            &[0, aux_offset],
            &[Some(total_size), Some(total_size)],
        ),
        Err(DmaBufValidationError::ZeroPlaneStride { plane: 1 })
    );

    // Aux plane offset + stride past backing fd length is rejected.
    assert_eq!(
        validate_hal_import_layout(
            1920,
            1080,
            4,
            TEST_COMPRESSED_MODIFIER,
            2,
            2,
            2,
            &[main_stride, aux_stride],
            &[0, aux_offset],
            &[Some(main_size), Some(main_size)],
        ),
        Err(DmaBufValidationError::BackingTooSmall {
            plane: 1,
            required_end: main_size + u64::from(aux_stride),
            backing_len: main_size,
        })
    );
}
