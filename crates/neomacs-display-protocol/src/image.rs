//! Immutable image geometry shared by layout, async decoding, and rendering.

use crate::types::{ImageId, ImageLoadToken};
use std::collections::HashSet;
use std::marker::PhantomData;

/// One renderer-side image lifecycle fact.
///
/// A decode completion belongs to one particular load attempt, while eviction
/// belongs to the stable image identity. Encoding that distinction
/// in the variants makes stale completion handling mandatory and prevents an
/// impossible `DecodeCompleted` event without an attempt token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageStateEvent {
    DecodeCompleted(ImageLoadToken),
    Evicted(ImageId),
}

impl ImageStateEvent {
    #[must_use]
    pub const fn image(self) -> ImageId {
        match self {
            Self::DecodeCompleted(load) => load.image(),
            Self::Evicted(image) => image,
        }
    }
}

/// Image identities retained by accepted or queued render presentations.
///
/// The set is a lifetime fence, not a cache-lookup result: a logically
/// invalidated image remains here until every frame that can draw it has been
/// replaced or discarded. Renderer eviction and physical release must exclude
/// every member.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetainedImageSet(HashSet<ImageId>);

impl RetainedImageSet {
    #[must_use]
    pub fn contains(&self, image: ImageId) -> bool {
        self.0.contains(&image)
    }

    pub fn insert(&mut self, image: ImageId) {
        self.0.insert(image);
    }

    pub fn extend(&mut self, images: impl IntoIterator<Item = ImageId>) {
        self.0.extend(images);
    }

    pub fn iter(&self) -> impl Iterator<Item = ImageId> + '_ {
        self.0.iter().copied()
    }
}

impl FromIterator<ImageId> for RetainedImageSet {
    fn from_iter<T: IntoIterator<Item = ImageId>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// A normalized, non-empty rectangle sampled from an image texture.
///
/// Layout resolves GNU's pixel/fraction `(slice …)` operands once it knows the
/// realized image size. Rendering receives only this closed normalized form,
/// so it cannot reinterpret Lisp values or confuse crop geometry with window
/// clipping geometry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageSourceRect {
    x_start: u16,
    y_start: u16,
    x_end: u16,
    y_end: u16,
}

impl ImageSourceRect {
    const NORMALIZED_MAX: f32 = u16::MAX as f32;

    pub const FULL: Self = Self {
        x_start: 0,
        y_start: 0,
        x_end: u16::MAX,
        y_end: u16::MAX,
    };

    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Option<Self> {
        let values = [x, y, width, height];
        if values.iter().any(|value| !value.is_finite())
            || x < 0.0
            || y < 0.0
            || width <= 0.0
            || height <= 0.0
            || x + width > 1.0 + f32::EPSILON
            || y + height > 1.0 + f32::EPSILON
        {
            return None;
        }
        let encode = |value: f32| (value.clamp(0.0, 1.0) * Self::NORMALIZED_MAX).round() as u16;
        let x_start = encode(x);
        let y_start = encode(y);
        let x_end = encode((x + width).min(1.0));
        let y_end = encode((y + height).min(1.0));
        (x_end > x_start && y_end > y_start).then_some(Self {
            x_start,
            y_start,
            x_end,
            y_end,
        })
    }

    #[must_use]
    pub fn x(self) -> f32 {
        f32::from(self.x_start) / Self::NORMALIZED_MAX
    }

    #[must_use]
    pub fn y(self) -> f32 {
        f32::from(self.y_start) / Self::NORMALIZED_MAX
    }

    #[must_use]
    pub fn width(self) -> f32 {
        f32::from(self.x_end - self.x_start) / Self::NORMALIZED_MAX
    }

    #[must_use]
    pub fn height(self) -> f32 {
        f32::from(self.y_end - self.y_start) / Self::NORMALIZED_MAX
    }

    /// Map UV coordinates produced by frame/window clipping into this image
    /// sample. Keeping this composition beside the crop type prevents the GPU
    /// path from accidentally treating a clipped slice as a full texture.
    #[must_use]
    pub fn map_uv(self, u: f32, v: f32) -> (f32, f32) {
        (self.x() + u * self.width(), self.y() + v * self.height())
    }
}

/// GNU image margins are non-negative integer pixels. Store them in that
/// domain instead of two `f32`s so adding crop state does not enlarge every
/// glyph in every text row.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub struct ImageMargins {
    left: u16,
    right: u16,
    top: u16,
    bottom: u16,
}

impl ImageMargins {
    #[must_use]
    pub fn new(horizontal: f32, vertical: f32) -> Self {
        Self::asymmetric(horizontal, horizontal, vertical, vertical)
    }

    #[must_use]
    pub fn asymmetric(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        let encode = |value: f32| {
            if value.is_finite() {
                value.round().clamp(0.0, f32::from(u16::MAX)) as u16
            } else {
                0
            }
        };
        Self {
            left: encode(left),
            right: encode(right),
            top: encode(top),
            bottom: encode(bottom),
        }
    }

    #[must_use]
    pub const fn left(self) -> f32 {
        self.left as f32
    }

    #[must_use]
    pub const fn right(self) -> f32 {
        self.right as f32
    }

    #[must_use]
    pub const fn top(self) -> f32 {
        self.top as f32
    }

    #[must_use]
    pub const fn bottom(self) -> f32 {
        self.bottom as f32
    }

    #[must_use]
    pub const fn packed(self) -> u64 {
        (self.left as u64)
            | ((self.right as u64) << 16)
            | ((self.top as u64) << 32)
            | ((self.bottom as u64) << 48)
    }
}

/// Optional opaque image background packed into one word. Decoded image
/// backgrounds are RGB24, leaving `0x01000000` as an impossible sentinel.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageOpaqueBackground(u32);

impl ImageOpaqueBackground {
    const NONE: u32 = 0x0100_0000;

    #[must_use]
    pub const fn new(color: Option<u32>) -> Self {
        match color {
            Some(color) => Self(color & 0x00ff_ffff),
            None => Self(Self::NONE),
        }
    }

    #[must_use]
    pub const fn get(self) -> Option<u32> {
        if self.0 == Self::NONE {
            None
        } else {
            Some(self.0)
        }
    }

    #[must_use]
    pub const fn packed(self) -> u32 {
        self.0
    }
}

impl Default for ImageOpaqueBackground {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Default for ImageSourceRect {
    fn default() -> Self {
        Self::FULL
    }
}

/// One opaque sRGB color used while materializing a face-sensitive image.
///
/// GNU image colors are frame pixel values, but only their RGB24 payload is
/// meaningful to the cross-platform decoders.  Keeping that invariant in a
/// type makes black (`0x000000`) a real color instead of an accidental
/// "unspecified" sentinel.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub struct ImageRgb(u32);

impl ImageRgb {
    #[must_use]
    pub const fn from_pixel(pixel: u32) -> Self {
        Self(pixel & 0x00ff_ffff)
    }

    #[must_use]
    pub const fn rgb24(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn rgba8(self) -> [u8; 4] {
        [
            ((self.0 >> 16) & 0xff) as u8,
            ((self.0 >> 8) & 0xff) as u8,
            (self.0 & 0xff) as u8,
            0xff,
        ]
    }
}

/// Resolved face colors that participate in image identity and decoding.
///
/// This is one value throughout layout, the evaluator-owned image catalog,
/// the render command, and the decoder.  Consequently a decoder cannot accept
/// an unlabelled `(u32, u32)` pair or mistake valid black for a missing color.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageColorContext {
    foreground: ImageRgb,
    background: ImageRgb,
}

impl ImageColorContext {
    #[must_use]
    pub const fn from_pixels(foreground: u32, background: u32) -> Self {
        Self {
            foreground: ImageRgb::from_pixel(foreground),
            background: ImageRgb::from_pixel(background),
        }
    }

    #[must_use]
    pub const fn foreground(self) -> ImageRgb {
        self.foreground
    }

    #[must_use]
    pub const fn background(self) -> ImageRgb {
        self.background
    }
}

impl Default for ImageColorContext {
    /// Preserve the renderer's historical visible fallback for callers that
    /// have no Emacs face, such as raw pixels and native toolbar resources.
    /// Redisplay image requests carry their resolved face colors explicitly.
    fn default() -> Self {
        Self::from_pixels(0x00ff_ffff, 0x0000_0000)
    }
}

/// How GNU image postprocessing should derive or remove a clipping mask.
///
/// This is part of image realization identity and travels with the load. The
/// decoder therefore cannot accidentally render `:mask nil` or a heuristic
/// mask as if the property had been absent.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub enum ImageMaskPolicy {
    /// Preserve transparency supplied by the image format.
    #[default]
    Preserve,
    /// GNU `:mask nil`: remove format-supplied clipping transparency.
    Suppress,
    /// GNU `:mask heuristic` / `:heuristic-mask`: chroma-key a background.
    Heuristic(ImageHeuristicMask),
}

/// Background selection for GNU's heuristic clipping-mask construction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ImageHeuristicMask {
    /// Select the most frequent RGB value among the four corners.
    FourCorners,
    /// Explicit GNU 16-bit RGB components.
    Rgb16([u16; 3]),
}

/// Transparency representation produced by image decoding and postprocessing.
///
/// A clipping mask and continuous alpha can render through the same RGBA GPU
/// texture, but they are observably different to GNU's `image-mask-p` API.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub enum ImageMaskKind {
    #[default]
    None,
    Clipping,
    AlphaChannel,
}

impl ImageMaskKind {
    #[must_use]
    pub const fn has_clipping_mask(self) -> bool {
        matches!(self, Self::Clipping)
    }
}

/// Zero-based frame selected from a multi-image source.
///
/// Keeping this distinct from dimensions and renderer IDs makes it impossible
/// to accidentally route GNU's `:index` into an unrelated integer field.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ImageFrameIndex(u64);

impl ImageFrameIndex {
    #[must_use]
    pub const fn new(index: u64) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn is_first(self) -> bool {
        self.0 == 0
    }
}

/// Stable identity of one encoded multi-frame source.
///
/// This is intentionally independent of [`ImageId`]: each animation frame has
/// its own texture identity, while all frames share one decoder/compositor
/// cache entry.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct ImageSequenceId(std::num::NonZeroU64);

impl ImageSequenceId {
    #[must_use]
    pub const fn new(id: u64) -> Option<Self> {
        match std::num::NonZeroU64::new(id) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Generation-qualified invalidation for the decoder-side sequence cache.
///
/// `AllocatedThrough` is a high-water fence: decoder jobs already in flight
/// for any older identity may finish for their caller, but cannot repopulate
/// the cache after a global clear.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ImageSequenceRetirement {
    One(ImageSequenceId),
    AllocatedThrough(ImageSequenceId),
}

/// Exact renderer-owned storage currently retained by the image subsystem.
///
/// Texture and decoded-sequence bytes have different lifetimes, so they stay
/// separate until the Lisp compatibility boundary asks for GNU's one-number
/// `image-cache-size` result.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImageCacheUsage {
    texture_bytes: u64,
    decoded_sequence_bytes: u64,
}

impl ImageCacheUsage {
    #[must_use]
    pub const fn new(texture_bytes: u64, decoded_sequence_bytes: u64) -> Self {
        Self {
            texture_bytes,
            decoded_sequence_bytes,
        }
    }

    #[must_use]
    pub const fn texture_bytes(self) -> u64 {
        self.texture_bytes
    }

    #[must_use]
    pub const fn decoded_sequence_bytes(self) -> u64 {
        self.decoded_sequence_bytes
    }

    #[must_use]
    pub const fn total_bytes(self) -> u64 {
        self.texture_bytes
            .saturating_add(self.decoded_sequence_bytes)
    }
}

/// GNU-compatible delay for the currently decoded animation frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ImageFrameDelay {
    /// The format supplied no positive duration. GNU publishes `t`, which
    /// `image-multi-frame-p` resolves through `image-default-frame-delay`.
    UseDefault,
    /// An exact rational number of milliseconds.
    Milliseconds {
        numerator: u32,
        denominator: std::num::NonZeroU32,
    },
}

impl ImageFrameDelay {
    #[must_use]
    pub const fn milliseconds(numerator: u32, denominator: u32) -> Option<Self> {
        let Some(denominator) = std::num::NonZeroU32::new(denominator) else {
            return None;
        };
        Some(if numerator == 0 {
            Self::UseDefault
        } else {
            Self::Milliseconds {
                numerator,
                denominator,
            }
        })
    }

    #[must_use]
    pub fn seconds(self) -> Option<f64> {
        match self {
            Self::UseDefault => None,
            Self::Milliseconds {
                numerator,
                denominator,
            } => Some(f64::from(numerator) / f64::from(denominator.get()) / 1_000.0),
        }
    }
}

/// Decoder-owned metadata exposed by GNU's `image-metadata`.
///
/// Geometry and renderer bookkeeping deliberately do not belong here. The
/// evaluator converts this closed, thread-safe representation to Lisp only at
/// the API boundary.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageEmbeddedMetadata {
    frame_count: Option<std::num::NonZeroU32>,
    frame_delay: Option<ImageFrameDelay>,
}

impl ImageEmbeddedMetadata {
    pub const EMPTY: Self = Self {
        frame_count: None,
        frame_delay: None,
    };

    #[must_use]
    pub fn animation(frame_count: u32, frame_delay: ImageFrameDelay) -> Self {
        Self {
            frame_count: std::num::NonZeroU32::new(frame_count).filter(|count| count.get() > 1),
            frame_delay: Some(frame_delay),
        }
    }

    #[must_use]
    pub const fn frame_count(&self) -> Option<u32> {
        match self.frame_count {
            Some(count) => Some(count.get()),
            None => None,
        }
    }

    #[must_use]
    pub const fn frame_delay(&self) -> Option<ImageFrameDelay> {
        self.frame_delay
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.frame_count.is_none() && self.frame_delay.is_none()
    }
}

#[cfg(test)]
mod image_source_rect_tests {
    use super::{ImageMargins, ImageOpaqueBackground, ImageSourceRect};

    const QUANTIZATION_TOLERANCE: f32 = 2.0 / u16::MAX as f32;

    fn assert_approximately_eq(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= QUANTIZATION_TOLERANCE,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn window_clip_coordinates_are_mapped_inside_the_image_slice() {
        let slice = ImageSourceRect::new(0.25, 0.5, 0.5, 0.25).expect("valid source rect");
        for ((actual_u, actual_v), (expected_u, expected_v)) in [
            (slice.map_uv(0.0, 0.0), (0.25, 0.5)),
            (slice.map_uv(0.5, 0.5), (0.5, 0.625)),
            (slice.map_uv(1.0, 1.0), (0.75, 0.75)),
        ] {
            assert_approximately_eq(actual_u, expected_u);
            assert_approximately_eq(actual_v, expected_v);
        }
    }

    #[test]
    fn image_geometry_and_optional_background_remain_compact_and_unambiguous() {
        assert_eq!(std::mem::size_of::<ImageSourceRect>(), 8);
        assert_eq!(std::mem::size_of::<ImageMargins>(), 8);
        assert_eq!(std::mem::size_of::<ImageOpaqueBackground>(), 4);
        assert_eq!(ImageOpaqueBackground::new(None).get(), None);
        assert_eq!(ImageOpaqueBackground::new(Some(0)).get(), Some(0));
        assert_eq!(
            ImageOpaqueBackground::new(Some(0x12_34_56)).get(),
            Some(0x12_34_56)
        );
    }
}

/// Coordinate space carried by an [`ImageExtent`].
///
/// The marker never exists at runtime. It makes native decoder pixels,
/// logical layout pixels, GPU raster pixels, and GNU-reported image pixels
/// incompatible at compile time even though all four use the same integer
/// representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageNativeSpace;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageLayoutSpace;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageRasterSpace;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageReportedSpace;

/// A two-dimensional image extent whose coordinate space is part of its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageExtent<Space> {
    width: u32,
    height: u32,
    space: PhantomData<fn() -> Space>,
}

impl<Space> ImageExtent<Space> {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            space: PhantomData,
        }
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

pub type ImageNativeExtent = ImageExtent<ImageNativeSpace>;
pub type ImageLayoutExtent = ImageExtent<ImageLayoutSpace>;
pub type ImageRasterExtent = ImageExtent<ImageRasterSpace>;
pub type ImageReportedExtent = ImageExtent<ImageReportedSpace>;

/// All extents derived for one decoded image realization.
///
/// Keeping the spaces in one value gives bitmap and SVG decoders one sizing
/// operation and one rotation operation. Callers can no longer independently
/// recompute one of the three output extents with subtly different rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedImageGeometry {
    layout: ImageLayoutExtent,
    reported: ImageReportedExtent,
    raster: ImageRasterExtent,
}

impl ResolvedImageGeometry {
    #[must_use]
    pub const fn new(
        layout: ImageLayoutExtent,
        reported: ImageReportedExtent,
        raster: ImageRasterExtent,
    ) -> Self {
        Self {
            layout,
            reported,
            raster,
        }
    }

    #[must_use]
    pub const fn layout(self) -> ImageLayoutExtent {
        self.layout
    }

    #[must_use]
    pub const fn reported(self) -> ImageReportedExtent {
        self.reported
    }

    #[must_use]
    pub const fn raster(self) -> ImageRasterExtent {
        self.raster
    }

    #[must_use]
    pub const fn with_raster(self, raster: ImageRasterExtent) -> Self {
        Self { raster, ..self }
    }

    #[must_use]
    pub fn oriented(self, rotation: ImageRotation) -> Self {
        Self {
            layout: rotation.orient_extent(self.layout),
            reported: rotation.orient_extent(self.reported),
            raster: rotation.orient_extent(self.raster),
        }
    }
}

/// One resolved image realization for one frame presentation.
///
/// - `layout_scale` maps native/GNU image pixels into **logical** layout pixels
///   (same space as `frame-char-width`).
/// - `device_scale` maps those logical pixels to physical texture pixels.
/// - `report_scale` maps layout pixels back to **GNU `Fimage_size` / image-pixel**
///   space (`PIXELS` non-nil). For `:scale default` this is the frame device
///   scale (layout was divided by it); otherwise it is 1.
///
/// The three factors travel together so pending layout, decoded metadata, and
/// GPU upload cannot consult different scale-factor snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageRealization {
    layout_scale_bits: u32,
    device_scale_bits: u32,
    report_scale_bits: u32,
}

impl ImageRealization {
    #[must_use]
    pub fn new(layout_scale: f32, device_scale: f32, report_scale: f32) -> Self {
        let layout_scale = if layout_scale.is_finite() && layout_scale >= 0.0 {
            layout_scale
        } else {
            1.0
        };
        let device_scale = if device_scale.is_finite() && device_scale > 0.0 {
            device_scale
        } else {
            1.0
        };
        let report_scale = if report_scale.is_finite() && report_scale > 0.0 {
            report_scale
        } else {
            1.0
        };
        Self {
            // Normalize signed zero so equal numeric realizations have one
            // cache identity.
            layout_scale_bits: if layout_scale == 0.0 {
                0.0_f32.to_bits()
            } else {
                layout_scale.to_bits()
            },
            device_scale_bits: device_scale.to_bits(),
            report_scale_bits: report_scale.to_bits(),
        }
    }

    /// Convenience when layout already lives in GNU image-pixel space
    /// (`report_scale = 1`).
    #[must_use]
    pub fn with_device_scale(layout_scale: f32, device_scale: f32) -> Self {
        Self::new(layout_scale, device_scale, 1.0)
    }

    #[must_use]
    pub fn layout_scale(self) -> f32 {
        f32::from_bits(self.layout_scale_bits)
    }

    #[must_use]
    pub fn device_scale(self) -> f32 {
        f32::from_bits(self.device_scale_bits)
    }

    #[must_use]
    pub fn report_scale(self) -> f32 {
        f32::from_bits(self.report_scale_bits)
    }

    /// Convert a GNU image dimension to integer logical layout pixels.
    #[must_use]
    pub fn layout_dimension(self, dimension: u32) -> u32 {
        ((f64::from(dimension) * f64::from(self.layout_scale()))
            .round()
            .max(1.0)) as u32
    }

    /// Convert an integer logical extent to physical texture pixels.
    #[must_use]
    pub fn raster_dimension(self, layout_dimension: u32) -> u32 {
        ((f64::from(layout_dimension) * f64::from(self.device_scale()))
            .ceil()
            .max(1.0)) as u32
    }

    /// Convert a logical layout extent to GNU `Fimage_size` pixel extent.
    ///
    /// Prefer re-running `ImageSizeSpec::desired` at [`Self::image_pixel_scale`]
    /// when the native size is still known — `ceil` is not invertible.
    #[must_use]
    pub fn image_pixel_dimension(self, layout_dimension: u32) -> u32 {
        ((f64::from(layout_dimension) * f64::from(self.report_scale()))
            .ceil()
            .max(1.0)) as u32
    }

    /// Scale factor for GNU `compute_image_size` / Fimage_size pixel space.
    ///
    /// Equal to `layout_scale × report_scale`, with a near-1.0 snap so that
    /// `1/1.25 × 1.25` does not become `1.0000001` and ceil native extents
    /// by an extra pixel.
    #[must_use]
    pub fn image_pixel_scale(self) -> f64 {
        if (self.report_scale() - 1.0).abs() <= f32::EPSILON {
            return f64::from(self.layout_scale());
        }
        let product = f64::from(self.layout_scale()) * f64::from(self.report_scale());
        if (product - 1.0).abs() < 1e-4 {
            1.0
        } else {
            product
        }
    }

    /// Resolve all image geometry once, then apply GNU's post-sizing rotation
    /// to every coordinate space together.
    #[must_use]
    pub fn resolve_geometry(
        self,
        size: ImageSizeSpec,
        native: ImageNativeExtent,
        rotation: ImageRotation,
    ) -> ResolvedImageGeometry {
        let (native_width, native_height) = native.dimensions();
        let (layout_width, layout_height) =
            size.desired(native_width, native_height, f64::from(self.layout_scale()));
        let (reported_width, reported_height) =
            size.desired(native_width, native_height, self.image_pixel_scale());
        let raster_width = self.raster_dimension(layout_width);
        let raster_height = self.raster_dimension(layout_height);

        ResolvedImageGeometry::new(
            ImageLayoutExtent::new(layout_width, layout_height),
            ImageReportedExtent::new(reported_width, reported_height),
            ImageRasterExtent::new(raster_width, raster_height),
        )
        .oriented(rotation)
    }
}

impl Default for ImageRealization {
    fn default() -> Self {
        Self::new(1.0, 1.0, 1.0)
    }
}

/// What a spec asks for along one axis.
///
/// GNU resolves `:width` vs `:max-width` by precedence — ":width overrides
/// :max-width" (src/image.c:2767) — which means the two can never both apply.
/// Making that a sum type retires the bug this replaces: the old code kept one
/// `max_width` field that BOTH keys wrote into, so a target silently became a
/// clamp and the aspect ratio was computed against the wrong number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AxisSize {
    /// Nothing requested: the native extent, scaled.
    #[default]
    Native,
    /// `:width` / `:height` — an exact target, itself multiplied by the scale.
    Exact(u32),
    /// `:max-width` / `:max-height` — an upper bound; the other axis follows to
    /// preserve the aspect ratio.
    AtMost(u32),
}

impl AxisSize {
    /// Apply GNU's precedence once, at construction: a target wins over a clamp.
    #[must_use]
    pub fn resolve(target: Option<u32>, at_most: Option<u32>) -> Self {
        match (target, at_most) {
            (Some(target), _) => Self::Exact(target),
            (None, Some(at_most)) => Self::AtMost(at_most),
            (None, None) => Self::Native,
        }
    }

    fn target(self, scale: f64) -> Option<u32> {
        match self {
            // GNU scales the target too (src/image.c:2766).
            Self::Exact(size) => Some(scale_size(size, 1, scale)),
            _ => None,
        }
    }

    /// The extent this axis pins, if any. `Native` pins nothing — the answer
    /// is not knowable until the image is decoded.
    #[must_use]
    pub fn pinned(self) -> Option<u32> {
        match self {
            Self::Exact(size) | Self::AtMost(size) => Some(size),
            Self::Native => None,
        }
    }

    fn at_most(self) -> Option<u32> {
        match self {
            Self::AtMost(size) => Some(size),
            _ => None,
        }
    }
}

/// A quarter-turn rotation, the only kind native transforms perform.
///
/// GNU accepts any number for `:rotation` but only multiplies of 90 actually
/// turn the image; everything else is a no-op (src/image.c:2928-2958).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageRotation {
    #[default]
    None,
    Quarter,
    Half,
    ThreeQuarter,
}

impl ImageRotation {
    /// GNU's `image_compute_rotation` followed by its 90-degree dispatch.
    #[must_use]
    pub fn from_degrees(degrees: f64) -> Self {
        if !degrees.is_finite() {
            return Self::None;
        }
        // Emacs `mod` keeps the divisor's sign, so -90 → 270, not 90.
        let mut reduced = degrees % 360.0;
        if reduced < 0.0 {
            reduced += 360.0;
        }
        // Snap near-integers; non-multiples of 90 stay upright.
        let nearest = reduced.round();
        if (reduced - nearest).abs() > 1e-6 {
            return Self::None;
        }
        match nearest as i32 {
            0 | 360 => Self::None,
            90 => Self::Quarter,
            180 => Self::Half,
            270 => Self::ThreeQuarter,
            _ => Self::None,
        }
    }

    /// Swap width/height for 90° and 270° turns (GNU after sizing).
    #[must_use]
    pub fn orient(self, width: u32, height: u32) -> (u32, u32) {
        match self {
            Self::None | Self::Half => (width, height),
            Self::Quarter | Self::ThreeQuarter => (height, width),
        }
    }

    /// Apply the turn without erasing the extent's coordinate-space marker.
    #[must_use]
    pub fn orient_extent<Space>(self, extent: ImageExtent<Space>) -> ImageExtent<Space> {
        let (width, height) = self.orient(extent.width(), extent.height());
        ImageExtent::new(width, height)
    }
}

/// This is GNU's `compute_image_size` input set (src/image.c:2750). The size
/// cannot be resolved until the native size is known, i.e. after decoding, so
/// this travels to the decoder rather than being applied up front.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ImageSizeSpec {
    width: AxisSize,
    height: AxisSize,
}

impl ImageSizeSpec {
    #[must_use]
    pub fn new(width: AxisSize, height: AxisSize) -> Self {
        Self { width, height }
    }

    /// Size to reserve for an image that has not been decoded yet.
    ///
    /// `None` when neither axis is pinned: the native size is the answer then,
    /// and it is not known until decoding finishes.
    #[must_use]
    pub fn placeholder_extent(self) -> Option<(u32, u32)> {
        match (self.width.pinned(), self.height.pinned()) {
            (Some(width), Some(height)) => Some((width, height)),
            // One axis pinned: the other follows the native aspect ratio, which
            // is still unknown, so fall back to a square of the known extent.
            (Some(width), None) => Some((width, width)),
            (None, Some(height)) => Some((height, height)),
            (None, None) => None,
        }
    }

    /// The size a `native_width` x `native_height` image should be drawn at.
    ///
    /// Mirrors GNU `compute_image_size` (src/image.c:2750) step for step.
    #[must_use]
    pub fn desired(self, native_width: u32, native_height: u32, scale: f64) -> (u32, u32) {
        let native_width = native_width.max(1);
        let native_height = native_height.max(1);

        let (mut width, mut height) = match (self.width.target(scale), self.height.target(scale)) {
            // Both given: GNU skips the aspect-preserving work entirely.
            (Some(width), Some(height)) => return (width.max(1), height.max(1)),
            (Some(width), None) => (width, ratio(width, native_width, native_height)),
            (None, Some(height)) => (ratio(height, native_height, native_width), height),
            (None, None) => (
                scale_size(native_width, 1, scale),
                scale_size(native_height, 1, scale),
            ),
        };

        // Clamps, each preserving the aspect ratio (src/image.c:2798-2810).
        if let Some(max) = self.width.at_most().filter(|max| *max < width) {
            width = max;
            height = ratio(width, native_width, native_height);
        }
        if let Some(max) = self.height.at_most().filter(|max| *max < height) {
            height = max;
            width = ratio(height, native_height, native_width);
        }

        (width.max(1), height.max(1))
    }
}

/// GNU `scale_image_size` (src/image.c:2700): `size * multiplier / divisor`.
///
/// Uses `ceil` like GNU so fractional SVG/device pixels are never discarded.
fn scale_size(size: u32, divisor: u32, multiplier: f64) -> u32 {
    let scaled = f64::from(size) * multiplier / f64::from(divisor.max(1));
    if scaled.is_finite() && scaled >= 1.0 {
        scaled.ceil() as u32
    } else {
        1
    }
}

/// Keep the aspect ratio: `size * to / from`.
fn ratio(size: u32, from: u32, to: u32) -> u32 {
    scale_size(size, from, f64::from(to))
}

#[cfg(test)]
mod tests {
    use super::{AxisSize, ImageRealization, ImageRotation, ImageSizeSpec};

    /// Every expectation below was measured from GNU Emacs 31 on a 40x20 PNG
    /// with `image-scaling-factor` pinned to 1, so the numbers are observed
    /// rather than derived from reading `compute_image_size`.
    const NATIVE: (u32, u32) = (40, 20);

    fn desired(spec: ImageSizeSpec, scale: f64) -> (u32, u32) {
        spec.desired(NATIVE.0, NATIVE.1, scale)
    }

    /// Measured from GNU Emacs 31 on the same 40x20 PNG.
    #[test]
    fn rotation_reduces_modulo_360_and_only_turns_on_multiples_of_90() {
        use ImageRotation as R;
        for (degrees, expected) in [
            (0.0, R::None),
            (90.0, R::Quarter),
            (180.0, R::Half),
            (270.0, R::ThreeQuarter),
            (360.0, R::None),
            (450.0, R::Quarter),
            // Emacs `mod` takes the divisor's sign: -90 reduces to 270.
            (-90.0, R::ThreeQuarter),
            // Not a multiple of 90: GNU leaves the image upright.
            (45.0, R::None),
            (f64::NAN, R::None),
        ] {
            assert_eq!(R::from_degrees(degrees), expected, "rotation {degrees}");
        }
    }

    #[test]
    fn quarter_turns_exchange_the_axes() {
        // GNU: 40x20 with `:rotation 90` reports (20 . 40).
        assert_eq!(ImageRotation::Quarter.orient(40, 20), (20, 40));
        assert_eq!(ImageRotation::ThreeQuarter.orient(40, 20), (20, 40));
        assert_eq!(ImageRotation::Half.orient(40, 20), (40, 20));
        assert_eq!(ImageRotation::None.orient(40, 20), (40, 20));
    }

    #[test]
    fn sizing_happens_before_rotation() {
        // GNU: `:rotation 90 :width 80` on 40x20 reports (40 . 80) — `:width`
        // sizes the upright image (80x40), then the turn swaps the axes.
        let sized = ImageSizeSpec::new(AxisSize::Exact(80), AxisSize::Native).desired(40, 20, 1.0);
        assert_eq!(sized, (80, 40));
        assert_eq!(ImageRotation::Quarter.orient(sized.0, sized.1), (40, 80));
    }

    #[test]
    fn native_size_survives_when_nothing_is_requested() {
        assert_eq!(desired(ImageSizeSpec::default(), 1.0), (40, 20));
    }

    #[test]
    fn scale_multiplies_both_axes() {
        assert_eq!(desired(ImageSizeSpec::default(), 2.0), (80, 40));
    }

    #[test]
    fn width_is_a_target_and_keeps_the_aspect_ratio() {
        // GNU: `:width 80` => (80 . 40) — the height follows from the ratio.
        assert_eq!(
            desired(
                ImageSizeSpec::new(AxisSize::Exact(80), AxisSize::Native),
                1.0
            ),
            (80, 40)
        );
    }

    #[test]
    fn height_is_a_target_and_keeps_the_aspect_ratio() {
        // GNU: `:height 40` => (80 . 40).
        assert_eq!(
            desired(
                ImageSizeSpec::new(AxisSize::Native, AxisSize::Exact(40)),
                1.0
            ),
            (80, 40)
        );
    }

    #[test]
    fn max_width_clamps_and_keeps_the_aspect_ratio() {
        // GNU: `:max-width 20` => (20 . 10), NOT (20 . <unbounded>).
        assert_eq!(
            desired(
                ImageSizeSpec::new(AxisSize::AtMost(20), AxisSize::Native),
                1.0
            ),
            (20, 10)
        );
    }

    #[test]
    fn width_overrides_max_width() {
        // GNU: `:width 80 :max-width 20` => (80 . 40). ":width overrides
        // :max-width" (src/image.c:2767) — conflating the two keys is what
        // made neomacs answer (20 . 4096).
        assert_eq!(
            desired(
                // GNU precedence resolved at construction: the target wins.
                ImageSizeSpec::new(AxisSize::resolve(Some(80), Some(20)), AxisSize::Native),
                1.0
            ),
            (80, 40)
        );
    }

    #[test]
    fn explicit_width_and_height_skip_the_aspect_computation() {
        assert_eq!(
            desired(
                ImageSizeSpec::new(AxisSize::Exact(11), AxisSize::Exact(99)),
                1.0
            ),
            (11, 99)
        );
    }

    #[test]
    fn targets_are_themselves_scaled() {
        // GNU multiplies :width/:height by the scale (src/image.c:2766).
        assert_eq!(
            desired(
                ImageSizeSpec::new(AxisSize::Exact(40), AxisSize::Native),
                2.0
            ),
            (80, 40)
        );
    }

    #[test]
    fn fractional_realization_has_one_layout_and_raster_rounding_policy() {
        let realization = ImageRealization::new(1.3 / 1.75, 1.75, 1.75);

        assert_eq!(realization.layout_dimension(24), 18);
        assert_eq!(realization.raster_dimension(18), 32);
        // report recovers GNU image pixels from logical layout.
        assert_eq!(realization.image_pixel_dimension(18), 32);
    }

    #[test]
    fn report_scale_one_leaves_layout_as_image_pixels() {
        let realization = ImageRealization::with_device_scale(1.0, 1.25);
        assert_eq!(realization.image_pixel_dimension(333), 333);
        assert_eq!(realization.raster_dimension(333), 417); // ceil(333*1.25)
    }

    #[test]
    fn image_pixel_scale_snaps_default_hidpi_product_to_one() {
        // 1/1.25 × 1.25 is not bit-exact in f32; snap so native 333 stays 333.
        let realization = ImageRealization::new(1.0 / 1.25, 1.25, 1.25);
        assert_eq!(realization.image_pixel_scale(), 1.0);
        assert!((realization.layout_scale() - 0.8).abs() < 1e-6);
    }
}
