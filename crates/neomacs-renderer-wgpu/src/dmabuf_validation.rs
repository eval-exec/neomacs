//! Pure, ungated validation for Linux DMA-BUF import metadata.
//!
//! Keeping layout, plane-count, stride, and backing-size checks in an ungated
//! module lets both the Vulkan HAL importer and the linear mmap fallback share
//! one typed validation contract and lets default CI execute the full test
//! suite without requiring `webview`/`video-dmabuf` features or a GPU adapter.

/// Maximum number of DMA-BUF planes supported by the importer.
pub const MAX_DMA_BUF_PLANES: usize = 4;

/// DRM linear (untiled) format modifier.
pub const DRM_FORMAT_MOD_LINEAR: u64 = 0;

/// DRM invalid/unspecified format modifier.
pub const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

/// Structured rejection reason for invalid DMA-BUF import parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DmaBufValidationError {
    #[error("DMA-BUF has zero width or height ({width}x{height})")]
    ZeroDimensions { width: u32, height: u32 },
    #[error("DMA-BUF format has zero bytes per pixel")]
    ZeroBytesPerPixel,
    #[error("DMA-BUF plane count {declared} is outside supported range 1..={max}")]
    UnsupportedPlaneCount { declared: u32, max: usize },
    #[error("DMA-BUF declared {declared} planes, but driver modifier requires {required}")]
    PlaneCountMismatch { declared: u32, required: u32 },
    #[error(
        "DMA-BUF metadata slices (fds={fds}, strides={strides}, offsets={offsets}, backing_lens={backing_lens}) do not cover {required} planes"
    )]
    MissingPlaneMetadata {
        fds: usize,
        strides: usize,
        offsets: usize,
        backing_lens: usize,
        required: usize,
    },
    #[error("DMA-BUF plane {plane} has zero stride")]
    ZeroPlaneStride { plane: usize },
    #[error("DMA-BUF plane {plane} stride {stride} is smaller than minimum row bytes {min_stride}")]
    StrideTooSmall {
        plane: usize,
        stride: u32,
        min_stride: u32,
    },
    #[error("DMA-BUF plane {plane} layout arithmetic overflowed")]
    LayoutOverflow { plane: usize },
    #[error("DMA-BUF plane {plane} backing fd size could not be determined")]
    UnknownBackingSize { plane: usize },
    #[error(
        "DMA-BUF plane {plane} requires {required_end} bytes, exceeding backing fd size {backing_len}"
    )]
    BackingTooSmall {
        plane: usize,
        required_end: u64,
        backing_len: u64,
    },
}

/// Validated byte range for a linear single-plane CPU mmap copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedLinearRegion {
    pub offset: u64,
    pub byte_len: usize,
}

/// Validated per-plane layout for `VkImageDrmFormatModifierExplicitCreateInfoEXT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedPlaneLayout {
    pub offset: u64,
    pub row_pitch: u64,
}

/// Validate a linear single-plane DMA-BUF region before `mmap`.
pub fn validate_linear_region(
    width: u32,
    height: u32,
    stride: u32,
    offset: u32,
    bytes_per_pixel: u32,
    backing_len: Option<u64>,
) -> Result<ValidatedLinearRegion, DmaBufValidationError> {
    if width == 0 || height == 0 {
        return Err(DmaBufValidationError::ZeroDimensions { width, height });
    }
    if bytes_per_pixel == 0 {
        return Err(DmaBufValidationError::ZeroBytesPerPixel);
    }
    let min_stride = width
        .checked_mul(bytes_per_pixel)
        .ok_or(DmaBufValidationError::LayoutOverflow { plane: 0 })?;
    if stride < min_stride {
        return Err(DmaBufValidationError::StrideTooSmall {
            plane: 0,
            stride,
            min_stride,
        });
    }
    let byte_len_u64 = u64::from(stride)
        .checked_mul(u64::from(height))
        .ok_or(DmaBufValidationError::LayoutOverflow { plane: 0 })?;
    let byte_len = usize::try_from(byte_len_u64)
        .map_err(|_| DmaBufValidationError::LayoutOverflow { plane: 0 })?;
    let required_end = u64::from(offset)
        .checked_add(byte_len_u64)
        .ok_or(DmaBufValidationError::LayoutOverflow { plane: 0 })?;
    let backing_len = backing_len.ok_or(DmaBufValidationError::UnknownBackingSize { plane: 0 })?;
    if required_end > backing_len {
        return Err(DmaBufValidationError::BackingTooSmall {
            plane: 0,
            required_end,
            backing_len,
        });
    }
    Ok(ValidatedLinearRegion {
        offset: u64::from(offset),
        byte_len,
    })
}

/// Validate DMA-BUF geometry, plane count, strides, offsets, and backing fd
/// sizes before Vulkan HAL image creation (`vkCreateImage`).
///
/// Enforces an exact match between `num_planes` and `driver_plane_count` so
/// missing planes can never be filled with zero offset/pitch and alias plane 0.
/// Plane 0 (and any linear plane) is checked against `width * bytes_per_pixel`
/// and `offset + stride * height <= backing_len`. Auxiliary metadata planes on
/// non-linear compressed modifiers (e.g. Intel CCS, AMD DCC) carry smaller
/// hardware-defined pitches and subsampled heights, so they are validated for
/// non-zero stride and `offset + stride <= backing_len` without falsely
/// rejecting valid compressed hardware frames.
#[allow(clippy::too_many_arguments)]
pub fn validate_hal_import_layout(
    width: u32,
    height: u32,
    bytes_per_pixel: u32,
    modifier: u64,
    num_planes: u32,
    driver_plane_count: u32,
    fd_count: usize,
    strides: &[u32],
    offsets: &[u32],
    backing_lens: &[Option<u64>],
) -> Result<Vec<ValidatedPlaneLayout>, DmaBufValidationError> {
    if width == 0 || height == 0 {
        return Err(DmaBufValidationError::ZeroDimensions { width, height });
    }
    if bytes_per_pixel == 0 {
        return Err(DmaBufValidationError::ZeroBytesPerPixel);
    }
    if driver_plane_count == 0 || driver_plane_count as usize > MAX_DMA_BUF_PLANES {
        return Err(DmaBufValidationError::UnsupportedPlaneCount {
            declared: driver_plane_count,
            max: MAX_DMA_BUF_PLANES,
        });
    }
    if num_planes == 0 || num_planes as usize > MAX_DMA_BUF_PLANES {
        return Err(DmaBufValidationError::UnsupportedPlaneCount {
            declared: num_planes,
            max: MAX_DMA_BUF_PLANES,
        });
    }
    if num_planes != driver_plane_count {
        return Err(DmaBufValidationError::PlaneCountMismatch {
            declared: num_planes,
            required: driver_plane_count,
        });
    }

    let plane_count = driver_plane_count as usize;
    if fd_count < plane_count
        || strides.len() < plane_count
        || offsets.len() < plane_count
        || backing_lens.len() < plane_count
    {
        return Err(DmaBufValidationError::MissingPlaneMetadata {
            fds: fd_count,
            strides: strides.len(),
            offsets: offsets.len(),
            backing_lens: backing_lens.len(),
            required: plane_count,
        });
    }

    let is_linear = modifier == DRM_FORMAT_MOD_LINEAR || modifier == DRM_FORMAT_MOD_INVALID;
    let min_primary_stride = width
        .checked_mul(bytes_per_pixel)
        .ok_or(DmaBufValidationError::LayoutOverflow { plane: 0 })?;

    let mut layouts = Vec::with_capacity(plane_count);
    for plane in 0..plane_count {
        let stride = strides[plane];
        let offset = offsets[plane];
        let backing_len =
            backing_lens[plane].ok_or(DmaBufValidationError::UnknownBackingSize { plane })?;

        if plane == 0 || is_linear {
            if stride < min_primary_stride {
                return Err(DmaBufValidationError::StrideTooSmall {
                    plane,
                    stride,
                    min_stride: min_primary_stride,
                });
            }
            let plane_bytes = u64::from(stride)
                .checked_mul(u64::from(height))
                .ok_or(DmaBufValidationError::LayoutOverflow { plane })?;
            let required_end = u64::from(offset)
                .checked_add(plane_bytes)
                .ok_or(DmaBufValidationError::LayoutOverflow { plane })?;
            if required_end > backing_len {
                return Err(DmaBufValidationError::BackingTooSmall {
                    plane,
                    required_end,
                    backing_len,
                });
            }
        } else {
            if stride == 0 {
                return Err(DmaBufValidationError::ZeroPlaneStride { plane });
            }
            let required_end = u64::from(offset)
                .checked_add(u64::from(stride))
                .ok_or(DmaBufValidationError::LayoutOverflow { plane })?;
            if required_end > backing_len {
                return Err(DmaBufValidationError::BackingTooSmall {
                    plane,
                    required_end,
                    backing_len,
                });
            }
        }

        layouts.push(ValidatedPlaneLayout {
            offset: u64::from(offset),
            row_pitch: u64::from(stride),
        });
    }

    Ok(layouts)
}

#[cfg(test)]
#[path = "dmabuf_validation/tests/dmabuf_validation_test.rs"]
mod tests;
