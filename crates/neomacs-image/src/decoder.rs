use crate::image_sequence::{ImageSequenceCache, ImageSequenceResolution};
use neomacs_display_protocol::{
    DecodedImage, ImageColorContext, ImageEmbeddedMetadata, ImageFrameIndex, ImageHeuristicMask,
    ImageIntrinsicExtent, ImageLoadToken, ImageMaskKind, ImageMaskPolicy, ImageMetadata,
    ImageNativeExtent, ImageRasterExtent, ImageRealization, ImageRotation, ImageSequenceId,
    ImageSizeSpec, ResolvedImageGeometry,
};
#[cfg(test)]
use neomacs_display_protocol::{ImageId, ImageLoadAttempt};
use std::fs::File;
use std::io::BufReader;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
#[cfg(test)]
use std::sync::Arc;

/// Maximum texture dimension (width or height)
const MAX_TEXTURE_SIZE: u32 = 4096;

/// Clamp to the renderer's texture limit, preserving aspect ratio.
///
/// This is a GPU constraint only — GNU's `:max-width`/`:max-height` are applied
/// by `ImageSizeSpec::desired`, which knows the native size and so can keep the
/// aspect ratio against the right numbers.
pub(crate) fn constrain_dimensions(width: u32, height: u32) -> (u32, u32) {
    let mut width = width;
    let mut height = height;
    let width_limit = MAX_TEXTURE_SIZE;
    let height_limit = MAX_TEXTURE_SIZE;

    if width > width_limit {
        height = (f64::from(height) * f64::from(width_limit) / f64::from(width)) as u32;
        width = width_limit;
    }
    if height > height_limit {
        width = (f64::from(width) * f64::from(height_limit) / f64::from(height)) as u32;
        height = height_limit;
    }

    (width.max(1), height.max(1))
}

pub(crate) fn constrain_raster_extent(extent: ImageRasterExtent) -> ImageRasterExtent {
    let (width, height) = constrain_dimensions(extent.width(), extent.height());
    ImageRasterExtent::new(width, height)
}

/// Pixels directly emitted by a decoder before an image spec is realized.
struct NativePixels {
    extent: ImageNativeExtent,
    rgba: Vec<u8>,
    embedded: ImageEmbeddedMetadata,
}

/// Pixels whose layout, GNU-reported, and GPU extents were resolved together.
struct DecodedPixels {
    geometry: ResolvedImageGeometry,
    rgba: Vec<u8>,
    mask: ImageMaskKind,
    embedded: ImageEmbeddedMetadata,
}

impl NativePixels {
    fn raster(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            extent: ImageNativeExtent::new(width, height),
            rgba,
            embedded: ImageEmbeddedMetadata::default(),
        }
    }

    fn from_raster_tuple((width, height, rgba): (u32, u32, Vec<u8>)) -> Self {
        Self::raster(width, height, rgba)
    }

    /// Resolve the spec's requested size against the decoded native size, then
    /// realize to texture pixels.
    ///
    /// This is where GNU's `compute_image_size` lands: the size cannot be known
    /// before decoding, so `:width`/`:height`/`:max-*` are applied here rather
    /// than as a bounding box handed to the decoder. With `AxisSize::Native` on
    /// both axes this reduces to `layout_dimension`, the previous behavior.
    fn realize_bitmap(
        mut self,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        mask_policy: ImageMaskPolicy,
    ) -> Option<DecodedPixels> {
        let mask = apply_mask_policy(&mut self.rgba, self.extent.dimensions(), mask_policy);
        let geometry = realization.resolve_geometry(size, self.extent, ImageRotation::None);
        let raster = constrain_raster_extent(geometry.raster());
        let geometry = geometry.with_raster(raster);
        let (raster_width, raster_height) = raster.dimensions();
        let (native_width, native_height) = self.extent.dimensions();
        let rgba = if raster == ImageRasterExtent::new(native_width, native_height) {
            self.rgba
        } else {
            let source = image::RgbaImage::from_raw(native_width, native_height, self.rgba)?;
            image::imageops::resize(
                &source,
                raster_width,
                raster_height,
                image::imageops::FilterType::Lanczos3,
            )
            .into_raw()
        };
        // GNU rotates AFTER sizing, so `:width` sizes the upright image and the
        // turn then exchanges the axes (src/image.c:3169-3201). Quarter turns
        // are lossless, which is exactly why GNU only offers multiples of 90.
        let rgba = match rotation {
            ImageRotation::None => rgba,
            turn => {
                let source = image::RgbaImage::from_raw(raster_width, raster_height, rgba)?;
                let turned = match turn {
                    ImageRotation::Quarter => image::imageops::rotate90(&source),
                    ImageRotation::Half => image::imageops::rotate180(&source),
                    ImageRotation::ThreeQuarter => image::imageops::rotate270(&source),
                    ImageRotation::None => unreachable!("handled above"),
                };
                turned.into_raw()
            }
        };
        Some(DecodedPixels {
            geometry: geometry.oriented(rotation),
            rgba,
            mask,
            embedded: self.embedded,
        })
    }
}

fn classify_alpha(rgba: &[u8]) -> ImageMaskKind {
    let mut has_transparent = false;
    for alpha in rgba.iter().skip(3).step_by(4).copied() {
        match alpha {
            255 => {}
            0 => has_transparent = true,
            _ => return ImageMaskKind::AlphaChannel,
        }
    }
    if has_transparent {
        ImageMaskKind::Clipping
    } else {
        ImageMaskKind::None
    }
}

fn most_frequent_corner_rgb(rgba: &[u8], (width, height): (u32, u32)) -> [u8; 3] {
    let pixel = |x: u32, y: u32| {
        let offset = ((y * width + x) * 4) as usize;
        [rgba[offset], rgba[offset + 1], rgba[offset + 2]]
    };
    let corners = [
        pixel(0, 0),
        pixel(width - 1, 0),
        pixel(width - 1, height - 1),
        pixel(0, height - 1),
    ];
    let mut best = corners[0];
    let mut best_count = 0;
    for candidate in corners {
        let count = corners
            .iter()
            .filter(|corner| **corner == candidate)
            .count();
        if count > best_count {
            best = candidate;
            best_count = count;
        }
    }
    best
}

fn rgb16_to_rgb8(rgb: [u16; 3]) -> [u8; 3] {
    rgb.map(|component| ((u32::from(component) + 128) / 257) as u8)
}

/// Apply GNU's image-mask policy at the decoder boundary.
///
/// This runs before bitmap scaling so mask identity describes the decoded
/// source, rather than alpha values introduced by interpolation. The renderer
/// may store each variant in RGBA, but storage must not collapse clipping masks
/// and continuous alpha into one semantic state.
fn apply_mask_policy(
    rgba: &mut [u8],
    dimensions: (u32, u32),
    policy: ImageMaskPolicy,
) -> ImageMaskKind {
    match policy {
        ImageMaskPolicy::Preserve => classify_alpha(rgba),
        ImageMaskPolicy::Suppress => {
            let mask = classify_alpha(rgba);
            if mask.has_clipping_mask() {
                for alpha in rgba.iter_mut().skip(3).step_by(4) {
                    *alpha = 255;
                }
                ImageMaskKind::None
            } else {
                // GNU's `:mask nil` clears only `img->mask`; it does not
                // discard continuous alpha represented by the image itself.
                mask
            }
        }
        ImageMaskPolicy::Heuristic(heuristic) => {
            let background = match heuristic {
                ImageHeuristicMask::FourCorners => most_frequent_corner_rgb(rgba, dimensions),
                ImageHeuristicMask::Rgb16(rgb) => rgb16_to_rgb8(rgb),
            };
            for pixel in rgba.chunks_exact_mut(4) {
                pixel[3] = if pixel[..3] == background { 0 } else { 255 };
            }
            ImageMaskKind::Clipping
        }
    }
}

pub enum WorkerDecodeOutcome {
    Ready(DecodedImage),
    Failed(ImageLoadToken),
}

impl WorkerDecodeOutcome {
    pub fn load(&self) -> ImageLoadToken {
        match self {
            Self::Ready(decoded) => decoded.load,
            Self::Failed(load) => *load,
        }
    }
}

/// Request to decode an image
pub struct DecodeRequest {
    pub load: ImageLoadToken,
    pub source: ImageSource,
    pub size: ImageSizeSpec,
    pub rotation: ImageRotation,
    /// Semantic and device geometry resolved by evaluator/layout.
    pub realization: ImageRealization,
    /// Resolved face colors used by face-sensitive formats and cache identity.
    pub colors: ImageColorContext,
    pub mask: ImageMaskPolicy,
    pub frame: ImageFrameIndex,
}

/// Image source
pub enum ImageSource {
    File {
        path: String,
        sequence: ImageSequenceId,
    },
    Data {
        data: Vec<u8>,
        resources: crate::svg::SvgResourceContext,
        sequence: ImageSequenceId,
    },
    #[cfg(test)]
    Panic,
    /// Raw ARGB32 pixel data (A,R,G,B byte order, 4 bytes per pixel)
    RawArgb32 {
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
    },
    /// Raw RGB24 pixel data (R,G,B byte order, 3 bytes per pixel)
    RawRgb24 {
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
    },
}

/// Shared CPU image decoding and intrinsic measurement.
pub struct ImageDecoder;

impl ImageDecoder {
    pub fn decode_request(
        request: DecodeRequest,
        sequence_cache: &ImageSequenceCache,
    ) -> WorkerDecodeOutcome {
        let DecodeRequest {
            load,
            source,
            size,
            rotation,
            realization,
            colors,
            mask,
            frame,
        } = request;
        let result = catch_unwind(AssertUnwindSafe(|| match source {
            #[cfg(test)]
            ImageSource::Panic => panic!("injected decoder panic"),
            ImageSource::File { path, sequence } => Self::decode_file(
                &path,
                size,
                rotation,
                colors,
                realization,
                mask,
                frame,
                sequence_cache,
                sequence,
            ),
            ImageSource::Data {
                data,
                resources,
                sequence,
            } => Self::decode_data(
                &data,
                size,
                rotation,
                colors,
                realization,
                mask,
                frame,
                resources,
                sequence_cache,
                sequence,
            ),
            ImageSource::RawArgb32 {
                data,
                width,
                height,
                stride,
            } => Self::convert_argb32_to_rgba(&data, width, height, stride)
                .map(NativePixels::from_raster_tuple)
                .and_then(|pixels| pixels.realize_bitmap(size, rotation, realization, mask)),
            ImageSource::RawRgb24 {
                data,
                width,
                height,
                stride,
            } => Self::convert_rgb24_to_rgba(&data, width, height, stride)
                .map(NativePixels::from_raster_tuple)
                .and_then(|pixels| pixels.realize_bitmap(size, rotation, realization, mask)),
        }));

        match result {
            Ok(Some(pixels)) => WorkerDecodeOutcome::Ready(Self::decoded_image(load, pixels)),
            Ok(None) => WorkerDecodeOutcome::Failed(load),
            Err(_) => {
                tracing::warn!(
                    image = %load.image(),
                    "image decoder recovered from a panic"
                );
                WorkerDecodeOutcome::Failed(load)
            }
        }
    }

    /// Decode image file with size constraints
    fn decode_file(
        path: &str,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        colors: ImageColorContext,
        realization: ImageRealization,
        mask: ImageMaskPolicy,
        frame: ImageFrameIndex,
        sequence_cache: &ImageSequenceCache,
        sequence: ImageSequenceId,
    ) -> Option<DecodedPixels> {
        let encoded = std::fs::read(path).ok();
        if let Some(pixels) = encoded
            .as_deref()
            .and_then(|data| Self::decode_raster_data(data, frame, sequence_cache, sequence))
        {
            return pixels.realize_bitmap(size, rotation, realization, mask);
        }
        if !frame.is_first() {
            return None;
        }
        // Fallback: try XPM
        if let Some(result) = crate::xpm::decode_xpm_file(Path::new(path)) {
            return NativePixels::from_raster_tuple(result).realize_bitmap(
                size,
                rotation,
                realization,
                mask,
            );
        }
        // Fallback: try XBM
        let fg = colors.foreground().rgba8();
        let bg = colors.background_rgba8();
        if let Some(result) = crate::xbm::decode_xbm_file(Path::new(path), fg, bg) {
            return NativePixels::from_raster_tuple(result).realize_bitmap(
                size,
                rotation,
                realization,
                mask,
            );
        }
        // Fallback: try SVG via the shared vector backend.
        let data = encoded?;
        Self::decode_svg_data(
            &data,
            size,
            rotation,
            realization,
            colors,
            mask,
            crate::svg::SvgResourceContext::BaseUri(path.to_owned()),
        )
    }

    /// Decode image data with size constraints
    fn decode_data(
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        colors: ImageColorContext,
        realization: ImageRealization,
        mask: ImageMaskPolicy,
        frame: ImageFrameIndex,
        resources: crate::svg::SvgResourceContext,
        sequence_cache: &ImageSequenceCache,
        sequence: ImageSequenceId,
    ) -> Option<DecodedPixels> {
        if let Some(pixels) = Self::decode_raster_data(data, frame, sequence_cache, sequence) {
            return pixels.realize_bitmap(size, rotation, realization, mask);
        }
        if !frame.is_first() {
            return None;
        }
        // Fallback: try XPM
        if let Some(result) = crate::xpm::decode_xpm_data(data) {
            return NativePixels::from_raster_tuple(result).realize_bitmap(
                size,
                rotation,
                realization,
                mask,
            );
        }
        // Fallback: try XBM
        let fg = colors.foreground().rgba8();
        let bg = colors.background_rgba8();
        if let Some(result) = crate::xbm::decode_xbm_data(data, fg, bg) {
            return NativePixels::from_raster_tuple(result).realize_bitmap(
                size,
                rotation,
                realization,
                mask,
            );
        }
        // Fallback: try SVG via the shared vector backend.
        Self::decode_svg_data(data, size, rotation, realization, colors, mask, resources)
    }

    /// Decode a raster source while preserving multi-frame semantics.
    ///
    /// `DynamicImage` intentionally represents one still image and therefore
    /// drops both frame selection and animation metadata. Route animated
    /// formats through `AnimationDecoder` first, then use the still-image path
    /// only for frame zero.
    fn decode_raster_data(
        data: &[u8],
        frame: ImageFrameIndex,
        sequence_cache: &ImageSequenceCache,
        sequence: ImageSequenceId,
    ) -> Option<NativePixels> {
        match sequence_cache.resolve(sequence, data, frame) {
            ImageSequenceResolution::Frame(frame) => {
                let (width, height) = frame.dimensions();
                let (rgba, embedded) = frame.into_parts();
                Some(NativePixels {
                    extent: ImageNativeExtent::new(width, height),
                    rgba,
                    embedded,
                })
            }
            ImageSequenceResolution::MissingFrame => None,
            ImageSequenceResolution::NotAnimated => {
                Self::process_image(image::load_from_memory(data).ok()?)
            }
        }
    }

    #[cfg(test)]
    fn decode_data_with_metadata(
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        fg_bg: (u32, u32),
    ) -> Option<DecodedImage> {
        Self::decode_data_with_metadata_at_scale(data, size, rotation, fg_bg, 1.0)
    }

    #[cfg(test)]
    fn decode_data_with_metadata_for_frame(
        data: &[u8],
        frame: ImageFrameIndex,
    ) -> Option<DecodedImage> {
        let pixels = Self::decode_data(
            data,
            ImageSizeSpec::default(),
            ImageRotation::None,
            ImageColorContext::default(),
            ImageRealization::default(),
            ImageMaskPolicy::Preserve,
            frame,
            crate::svg::SvgResourceContext::Isolated,
            &ImageSequenceCache::new(),
            ImageSequenceId::new(1).expect("non-zero test sequence"),
        )?;
        Some(Self::decoded_image(
            ImageLoadToken::new(
                ImageId::new(0),
                ImageLoadAttempt::new(1).expect("test load attempt"),
            ),
            pixels,
        ))
    }

    #[cfg(test)]
    fn decode_data_with_metadata_at_scale(
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        fg_bg: (u32, u32),
        raster_scale: f32,
    ) -> Option<DecodedImage> {
        Self::decode_data_with_metadata_at_realization(
            data,
            size,
            rotation,
            fg_bg,
            1.0,
            raster_scale,
        )
    }

    #[cfg(test)]
    fn decode_data_with_metadata_at_realization(
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        fg_bg: (u32, u32),
        layout_scale: f32,
        device_scale: f32,
    ) -> Option<DecodedImage> {
        // Convenience path: layout already equals image-pixel space.
        Self::decode_data_with_metadata_at_full_realization(
            data,
            size,
            rotation,
            fg_bg,
            ImageRealization::with_device_scale(layout_scale, device_scale),
        )
    }

    #[cfg(test)]
    fn decode_data_with_metadata_at_full_realization(
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        fg_bg: (u32, u32),
        realization: ImageRealization,
    ) -> Option<DecodedImage> {
        let pixels = Self::decode_data(
            data,
            size,
            rotation,
            ImageColorContext::from_pixels(fg_bg.0, fg_bg.1),
            realization,
            ImageMaskPolicy::Preserve,
            ImageFrameIndex::default(),
            crate::svg::SvgResourceContext::Isolated,
            &ImageSequenceCache::new(),
            ImageSequenceId::new(1).expect("non-zero test sequence"),
        )?;
        Some(Self::decoded_image(
            ImageLoadToken::new(
                ImageId::new(0),
                ImageLoadAttempt::new(1).expect("test load attempt"),
            ),
            pixels,
        ))
    }

    fn decoded_image(load: ImageLoadToken, pixels: DecodedPixels) -> DecodedImage {
        let metadata =
            Self::metadata_from_rgba(pixels.geometry, &pixels.rgba, pixels.mask, pixels.embedded);
        DecodedImage {
            load,
            geometry: pixels.geometry,
            data: pixels.rgba,
            metadata,
        }
    }

    fn metadata_from_rgba(
        geometry: ResolvedImageGeometry,
        rgba: &[u8],
        mask_kind: ImageMaskKind,
        embedded: ImageEmbeddedMetadata,
    ) -> ImageMetadata {
        let (raster_width, raster_height) = geometry.raster().dimensions();
        let pixel = |x: u32, y: u32| {
            let offset = ((y * raster_width + x) * 4) as usize;
            [
                rgba[offset],
                rgba[offset + 1],
                rgba[offset + 2],
                rgba[offset + 3],
            ]
        };
        let corners = [
            pixel(0, 0),
            pixel(raster_width - 1, 0),
            pixel(raster_width - 1, raster_height - 1),
            pixel(0, raster_height - 1),
        ];
        let most_frequent = |values: [[u8; 4]; 4], key: fn([u8; 4]) -> u32| {
            let mut best = values[0];
            let mut best_count = 0;
            for candidate in values {
                let count = values
                    .iter()
                    .filter(|value| key(**value) == key(candidate))
                    .count();
                if count > best_count {
                    best = candidate;
                    best_count = count;
                }
            }
            best
        };
        let background = most_frequent(corners, |pixel| {
            (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 8) | u32::from(pixel[2])
        });
        let mask = most_frequent(corners, |pixel| u32::from(pixel[3] == 0));
        ImageMetadata {
            layout: geometry.layout(),
            reported: geometry.reported(),
            background: (u32::from(background[0]) << 16)
                | (u32::from(background[1]) << 8)
                | u32::from(background[2]),
            background_transparent: mask[3] == 0,
            mask: mask_kind,
            embedded,
        }
    }

    /// Decode SVG data through the platform SVG backend, returning RGBA pixels.
    fn decode_svg_data(
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        colors: ImageColorContext,
        mask_policy: ImageMaskPolicy,
        resources: crate::svg::SvgResourceContext,
    ) -> Option<DecodedPixels> {
        let mut decoded = crate::svg::decode(data, size, rotation, realization, colors, resources)?;
        let mask = apply_mask_policy(
            &mut decoded.rgba,
            decoded.geometry.raster().dimensions(),
            mask_policy,
        );
        Some(DecodedPixels {
            geometry: decoded.geometry,
            rgba: decoded.rgba,
            mask,
            embedded: ImageEmbeddedMetadata::default(),
        })
    }

    /// Process decoded image: resize if needed, convert to RGBA
    /// Decode to NATIVE pixels. Sizing happens in `realize_bitmap`, which is
    /// the only place that knows both the native size and the requested one.
    fn process_image(img: image::DynamicImage) -> Option<NativePixels> {
        let rgba = img.to_rgba8();
        Some(NativePixels::raster(
            rgba.width(),
            rgba.height(),
            rgba.into_raw(),
        ))
    }
    fn convert_argb32_to_rgba(
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let bytes_per_pixel = 4u32;
        let expected_min_size = (height.saturating_sub(1)) * stride + width * bytes_per_pixel;
        if data.len() < expected_min_size as usize {
            tracing::warn!(
                "ARGB32 data too small: got {} bytes, expected at least {} for {}x{} with stride {}",
                data.len(),
                expected_min_size,
                width,
                height,
                stride
            );
            return None;
        }

        // Convert ARGB32 to RGBA
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            let row_start = (y * stride) as usize;
            for x in 0..width {
                let pixel_start = row_start + (x * bytes_per_pixel) as usize;
                let a = data[pixel_start];
                let r = data[pixel_start + 1];
                let g = data[pixel_start + 2];
                let b = data[pixel_start + 3];
                let idx = ((y * width + x) * 4) as usize;
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = a;
            }
        }

        // Apply size constraints if needed
        let (cw, ch) = constrain_dimensions(width, height);
        if cw != width || ch != height {
            // Need to resize - use image crate
            let img = image::RgbaImage::from_raw(width, height, rgba)?;
            let resized =
                image::imageops::resize(&img, cw, ch, image::imageops::FilterType::Lanczos3);
            Some((cw, ch, resized.into_raw()))
        } else {
            Some((width, height, rgba))
        }
    }

    /// Convert RGB24 raw pixel data to RGBA
    /// Input format: R,G,B byte order (3 bytes per pixel)
    /// Output format: R,G,B,A byte order (4 bytes per pixel, alpha=255)
    fn convert_rgb24_to_rgba(
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let bytes_per_pixel = 3u32;
        let expected_min_size = (height.saturating_sub(1)) * stride + width * bytes_per_pixel;
        if data.len() < expected_min_size as usize {
            tracing::warn!(
                "RGB24 data too small: got {} bytes, expected at least {} for {}x{} with stride {}",
                data.len(),
                expected_min_size,
                width,
                height,
                stride
            );
            return None;
        }

        // Convert RGB24 to RGBA (add alpha=255)
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            let row_start = (y * stride) as usize;
            for x in 0..width {
                let pixel_start = row_start + (x * bytes_per_pixel) as usize;
                let r = data[pixel_start];
                let g = data[pixel_start + 1];
                let b = data[pixel_start + 2];
                let idx = ((y * width + x) * 4) as usize;
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = 255;
            }
        }

        // Apply size constraints if needed
        let (cw, ch) = constrain_dimensions(width, height);
        if cw != width || ch != height {
            // Need to resize - use image crate
            let img = image::RgbaImage::from_raw(width, height, rgba)?;
            let resized =
                image::imageops::resize(&img, cw, ch, image::imageops::FilterType::Lanczos3);
            Some((cw, ch, resized.into_raw()))
        } else {
            Some((width, height, rgba))
        }
    }

    /// Query image file dimensions.
    ///
    /// Raster formats read only their header; SVG requires document parsing.
    pub fn query_file_dimensions(path: &str) -> Option<ImageNativeExtent> {
        Self::query_file_intrinsic_extent(path).map(Self::unscaled_dimensions)
    }

    pub fn query_file_intrinsic_extent(path: &str) -> Option<ImageIntrinsicExtent> {
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        // Use image crate's dimension reader (reads header only)
        if let Ok(dims) = image::ImageReader::new(reader)
            .with_guessed_format()
            .ok()?
            .into_dimensions()
        {
            return Some(ImageNativeExtent::new(dims.0, dims.1).into());
        }

        // Fallback: try SVG.
        let data = std::fs::read(path).ok()?;
        crate::svg::query_intrinsic_extent(&data)
    }

    /// Query image data dimensions.
    ///
    /// Raster formats read only their header; SVG requires document parsing.
    pub fn query_data_dimensions(data: &[u8]) -> Option<ImageNativeExtent> {
        Self::query_data_intrinsic_extent(data).map(Self::unscaled_dimensions)
    }

    pub fn query_data_intrinsic_extent(data: &[u8]) -> Option<ImageIntrinsicExtent> {
        let cursor = std::io::Cursor::new(data);
        if let Ok(dims) = image::ImageReader::new(BufReader::new(cursor))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
        {
            return Some(ImageNativeExtent::new(dims.0, dims.1).into());
        }

        // Fallback: try XPM header
        if let Some((w, h)) = crate::xpm::query_xpm_dimensions(data) {
            return Some(ImageNativeExtent::new(w, h).into());
        }

        // Fallback: try XBM header
        if let Some((w, h)) = crate::xbm::query_xbm_dimensions(data) {
            return Some(ImageNativeExtent::new(w, h).into());
        }

        // Fallback: try SVG.
        crate::svg::query_intrinsic_extent(data)
    }

    /// Preserve the pixel-query contract: return a bounding integer extent.
    /// Pending-image sizing must instead retain the fractional source extent.
    fn unscaled_dimensions(intrinsic: ImageIntrinsicExtent) -> ImageNativeExtent {
        let (width, height) = intrinsic.dimensions();
        ImageNativeExtent::new(width.ceil() as u32, height.ceil() as u32)
    }
}

#[cfg(test)]
#[path = "decoder_test.rs"]
mod tests;
