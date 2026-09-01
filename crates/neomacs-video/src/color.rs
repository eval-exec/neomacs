use crate::{
    BiPlanarVideoFormat, VideoChromaLocation, VideoColorPrimaries, VideoColorRange,
    VideoColorimetry, VideoGeometry, VideoMatrixCoefficients, VideoTransferCharacteristic,
};

/// Shader-ready conversion from native bi-planar sample values to linear
/// display RGB. Rows are vec4-aligned so the Rust and WGSL layouts are stable.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct VideoColorTransform {
    pub(crate) yuv_rows: [[f32; 4]; 3],
    pub(crate) gamut_rows: [[f32; 4]; 3],
    /// Chroma-coordinate offset followed by reserved values.
    pub(crate) chroma: [f32; 4],
    /// Transfer characteristic followed by reserved values.
    pub(crate) params: [u32; 4],
}

impl VideoColorTransform {
    pub(crate) fn new(
        format: BiPlanarVideoFormat,
        color: VideoColorimetry,
        geometry: VideoGeometry,
    ) -> Self {
        let (word_scale, max_code, y_min, y_max, chroma_mid, chroma_span) = match format {
            BiPlanarVideoFormat::Nv12 => (1.0, 255.0, 16.0, 235.0, 128.0, 224.0),
            BiPlanarVideoFormat::P010 => (
                u16::MAX as f32 / (1023_u16 << 6) as f32,
                1023.0,
                64.0,
                940.0,
                512.0,
                896.0,
            ),
        };
        let (y_offset, y_scale, chroma_offset, chroma_scale) = match color.range {
            VideoColorRange::Limited => (
                y_min / max_code,
                max_code / (y_max - y_min),
                chroma_mid / max_code,
                max_code / chroma_span,
            ),
            VideoColorRange::Full => (0.0, 1.0, chroma_mid / max_code, 1.0),
        };
        let (red_cr, green_cb, green_cr, blue_cb) = match color.matrix {
            VideoMatrixCoefficients::Identity => (0.0, 0.0, 0.0, 0.0),
            VideoMatrixCoefficients::Bt601 => (1.402, -0.344_136, -0.714_136, 1.772),
            VideoMatrixCoefficients::Bt709 => (1.5748, -0.187_324, -0.468_124, 1.8556),
            VideoMatrixCoefficients::Bt2020NonConstantLuminance => {
                (1.4746, -0.164_553, -0.571_353, 1.8814)
            }
        };
        let y = y_scale * word_scale;
        let cb = chroma_scale * word_scale;
        let cr = chroma_scale * word_scale;
        let y_bias = -y_scale * y_offset;
        let cb_bias = -chroma_scale * chroma_offset;
        let cr_bias = -chroma_scale * chroma_offset;
        let yuv_rows = if color.matrix == VideoMatrixCoefficients::Identity {
            [
                [word_scale, 0.0, 0.0, 0.0],
                [0.0, word_scale, 0.0, 0.0],
                [0.0, 0.0, word_scale, 0.0],
            ]
        } else {
            [
                [y, 0.0, red_cr * cr, y_bias + red_cr * cr_bias],
                [
                    y,
                    green_cb * cb,
                    green_cr * cr,
                    y_bias + green_cb * cb_bias + green_cr * cr_bias,
                ],
                [y, blue_cb * cb, 0.0, y_bias + blue_cb * cb_bias],
            ]
        };
        let gamut_rows = gamut_rows(color.primaries);
        let (chroma_x, chroma_y) = match color.chroma_location {
            VideoChromaLocation::Center => (0.0, 0.0),
            VideoChromaLocation::Left => (0.5 / geometry.coded_width.max(1) as f32, 0.0),
            VideoChromaLocation::TopLeft => (
                0.5 / geometry.coded_width.max(1) as f32,
                0.5 / geometry.coded_height.max(1) as f32,
            ),
        };
        Self {
            yuv_rows,
            gamut_rows,
            chroma: [chroma_x, chroma_y, 0.0, 0.0],
            params: [transfer_code(color.transfer), 0, 0, 0],
        }
    }

    #[cfg(test)]
    pub(crate) fn encoded_rgb(self, yuv: [f32; 3]) -> [f32; 3] {
        self.yuv_rows
            .map(|row| row[0] * yuv[0] + row[1] * yuv[1] + row[2] * yuv[2] + row[3])
    }
}

const fn transfer_code(transfer: VideoTransferCharacteristic) -> u32 {
    match transfer {
        VideoTransferCharacteristic::Srgb => 0,
        VideoTransferCharacteristic::Bt709 => 1,
        VideoTransferCharacteristic::Pq => 2,
        VideoTransferCharacteristic::Hlg => 3,
    }
}

const fn gamut_rows(primaries: VideoColorPrimaries) -> [[f32; 4]; 3] {
    match primaries {
        VideoColorPrimaries::Bt709 => [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
        VideoColorPrimaries::Bt2020 => [
            [1.6605, -0.5876, -0.0728, 0.0],
            [-0.1246, 1.1329, -0.0083, 0.0],
            [-0.0182, -0.1006, 1.1187, 0.0],
        ],
        VideoColorPrimaries::Bt601_525 => [
            [0.9395, 0.0502, 0.0103, 0.0],
            [0.0178, 0.9658, 0.0164, 0.0],
            [-0.0016, -0.0044, 1.006, 0.0],
        ],
        VideoColorPrimaries::Bt601_625 => [
            [1.044, -0.044, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, -0.0118, 1.0118, 0.0],
        ],
    }
}
