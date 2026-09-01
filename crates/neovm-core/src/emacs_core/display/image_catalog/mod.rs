//! Nonblocking image lookup used by redisplay.
//!
//! Image decoding and renderer upload happen outside the evaluator thread.
//! Callers receive a complete state immediately and never infer readiness
//! from optional metadata.

use crate::emacs_core::Value;
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::{HashKey, HashTableTest, list_to_vec};
use crate::heap_types::LispString;
use crate::window::Frame;
pub use neomacs_display_protocol::ImageRealization as ResolvedImageRealization;
pub use neomacs_display_protocol::{
    AxisSize, ImageColorContext, ImageEmbeddedMetadata, ImageFrameDelay, ImageFrameIndex,
    ImageHeuristicMask, ImageId, ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken,
    ImageMaskKind, ImageMaskPolicy, ImageReportedExtent, ImageRotation, ImageSizeSpec,
    ImageStateEvent,
};

/// A finite, non-negative image scale stored by bits so image requests remain
/// exact cache keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageScaleFactor(u32);

impl ImageScaleFactor {
    #[must_use]
    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

impl TryFrom<f32> for ImageScaleFactor {
    type Error = &'static str;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if value.is_finite() && value >= 0.0 {
            Ok(Self(if value == 0.0 {
                0.0_f32.to_bits()
            } else {
                value.to_bits()
            }))
        } else {
            Err("image scale must be finite and non-negative")
        }
    }
}

/// Meaning of an image spec's `:scale` property before frame realization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageScalePolicy {
    /// No `:scale` key in the spec at all. GNU leaves the scale at 1 here and
    /// never consults `image-scaling-factor` (`double scale = 1` with no
    /// matching branch, src/image.c:2697-2736) — only an explicit
    /// `:scale default` opts into the variable.
    Unspecified,
    /// `:scale default` — resolve through GNU's `image-scaling-factor`.
    Default,
    /// A numeric scale written directly in the image spec.
    Explicit(ImageScaleFactor),
}

/// Parsed value of GNU's global `image-scaling-factor` variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageDefaultScale {
    Auto,
    Explicit(ImageScaleFactor),
}

/// Frame facts needed to resolve semantic GNU image scaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageScaleEnvironment {
    frame_column_width: ImageScaleFactor,
    device_scale: ImageScaleFactor,
    default_scale: ImageDefaultScale,
}

impl ImageScaleEnvironment {
    #[must_use]
    pub fn new(
        frame_column_width: f32,
        device_scale: f32,
        default_scale: ImageDefaultScale,
    ) -> Self {
        let frame_column_width = if frame_column_width.is_finite() && frame_column_width > 0.0 {
            frame_column_width
        } else {
            10.0
        };
        let device_scale = if device_scale.is_finite() && device_scale > 0.0 {
            device_scale
        } else {
            1.0
        };
        Self {
            frame_column_width: ImageScaleFactor::try_from(frame_column_width)
                .expect("sanitized frame column width is valid"),
            device_scale: ImageScaleFactor::try_from(device_scale)
                .expect("sanitized device scale is valid"),
            default_scale,
        }
    }

    /// The validated logical-to-device scale carried by this frame snapshot.
    ///
    /// Return the display protocol's domain type so layout geometry cannot
    /// accidentally consume the raw image-scaling scalar as logical pixels.
    #[must_use]
    pub fn device_scale(self) -> neomacs_display_protocol::DeviceScale {
        neomacs_display_protocol::DeviceScale::new(self.device_scale.get())
            .expect("ImageScaleEnvironment stores a validated positive device scale")
    }

    #[must_use]
    pub fn resolve(self, policy: ImageScalePolicy) -> ResolvedImageRealization {
        let device_scale = self.device_scale.get();
        // `report_scale` maps logical layout pixels → GNU Fimage_size pixels.
        // Only `:scale default` divides layout by device_scale; report multiplies
        // it back. Other policies leave layout already in image-pixel space.
        let (layout_scale, report_scale) = match policy {
            ImageScalePolicy::Unspecified => (1.0, 1.0),
            ImageScalePolicy::Explicit(scale) => (scale.get(), 1.0),
            ImageScalePolicy::Default => {
                let device_factor = match self.default_scale {
                    ImageDefaultScale::Auto => {
                        // GNU's FRAME_COLUMN_WIDTH is an integer device-pixel
                        // font metric.  Neomacs stores frame geometry in
                        // logical pixels, so recover the enclosing device
                        // column instead of rounding to the nearest pixel:
                        // the latter turns a 7px cell at 1.75 scale into 12px
                        // and loses the 13th pixel occupied by the font.
                        let device_column_width =
                            (self.frame_column_width.get() * device_scale).ceil();
                        if device_column_width > 10.0 {
                            device_column_width / 10.0
                        } else {
                            1.0
                        }
                    }
                    ImageDefaultScale::Explicit(scale) => scale.get(),
                };
                (device_factor / device_scale, device_scale)
            }
        };
        ResolvedImageRealization::new(layout_scale, device_scale, report_scale)
    }
}

impl Default for ImageScaleEnvironment {
    fn default() -> Self {
        Self::new(10.0, 1.0, ImageDefaultScale::Auto)
    }
}

/// Resolve GNU's dynamically bound `image-scaling-factor` together with the
/// selected frame facts.  Redisplay and synchronous image builtins share this
/// entry point so the same image spec cannot acquire two geometries.
#[must_use]
pub fn image_scale_environment(frame: &Frame, obarray: &Obarray) -> ImageScaleEnvironment {
    let default_scale = match obarray.symbol_value("image-scaling-factor").copied() {
        Some(value) if value.is_symbol_named("auto") => ImageDefaultScale::Auto,
        Some(value) => numeric_image_scale(value)
            .map(ImageDefaultScale::Explicit)
            .unwrap_or(ImageDefaultScale::Auto),
        None => ImageDefaultScale::Auto,
    };
    ImageScaleEnvironment::new(
        frame.char_width,
        frame.device_scale_factor as f32,
        default_scale,
    )
}

#[must_use]
pub fn numeric_image_scale(value: Value) -> Option<ImageScaleFactor> {
    let scale = value
        .as_float()
        .or_else(|| value.as_int().map(|value| value as f64))?;
    (scale.is_finite() && scale >= 0.0)
        .then(|| ImageScaleFactor::try_from(scale as f32).ok())
        .flatten()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageDataSource {
    /// Encoded bytes with no authority to resolve external resources.
    Isolated(Vec<u8>),
    /// Encoded bytes whose relative resources may be resolved against the
    /// explicitly supplied GNU image `:base-uri`.
    WithBaseUri { data: Vec<u8>, base_uri: LispString },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageResolveSource {
    File(LispString),
    Data(ImageDataSource),
}

/// Owned, structural identity of a complete GNU image specification.
///
/// The catalog must retain more than the parsed load recipe: type-specific
/// keys such as `:index`, `:mask`, and `:css` participate in GNU's `equal`
/// comparison even when a particular decoder does not understand them yet.
/// Converting the Lisp tree to an equal-hash key snapshots that identity
/// without retaining unrooted Lisp heap pointers in the host catalog.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageSpecIdentity(HashKey);

impl ImageSpecIdentity {
    /// Snapshot a proper `(image ...)` specification using Lisp `equal`
    /// semantics, matching GNU `search_image_cache` and `uncache_image`.
    #[must_use]
    pub fn from_lisp_spec(spec: &Value) -> Option<Self> {
        let items = list_to_vec(spec)?;
        (items.first()?.as_symbol_name() == Some("image"))
            .then(|| Self(spec.to_hash_key(&HashTableTest::Equal)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageResolveRequest {
    /// Full Lisp-spec identity. Parsed fields below are the materialization
    /// recipe; they are intentionally not used as a substitute for the spec.
    pub spec: ImageSpecIdentity,
    pub source: ImageResolveSource,
    /// GNU's `compute_image_size` inputs. Resolved after decoding, once the
    /// native size is known.
    pub size: ImageSizeSpec,
    /// GNU `:rotation`, reduced to a quarter turn. Applied AFTER sizing.
    pub rotation: ImageRotation,
    /// Face-sensitive materialization colors. These are part of the cache key,
    /// as in GNU `search_image_cache`, even for intrinsically colored images.
    pub colors: ImageColorContext,
    /// Typed GNU `:mask` / `:heuristic-mask` postprocessing intent.
    pub mask: ImageMaskPolicy,
    /// Zero-based GNU `:index` selected from a multi-frame source.
    pub frame: ImageFrameIndex,
    pub realization: ResolvedImageRealization,
}

/// Cache operation requested by the Lisp image compatibility layer.
///
/// GNU gives `image-flush` and `clear-image-cache FILTER` different matching
/// rules: the former removes every face realization of one exact spec, while
/// the latter removes every image depending on a source. Encoding that choice
/// in the type prevents another source-only approximation at call sites.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageInvalidation {
    Spec { spec: ImageSpecIdentity },
    Dependency(ImageResolveSource),
    All,
}

/// Independent invalidation domain for decoder/compositor sequence state.
/// GNU keeps this cache separate from per-frame image textures.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageAnimationInvalidation {
    Source(ImageResolveSource),
    All,
}

/// Whether a catalog invalidation retired any logical image identities.
///
/// Redisplay invalidation belongs to the evaluator and must happen
/// synchronously when this says `Changed`; physical GPU retirement is a later
/// presentation-lifetime concern owned by the renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageInvalidationResult {
    #[default]
    Unchanged,
    Changed,
}

impl ImageInvalidationResult {
    #[must_use]
    pub const fn changed(self) -> bool {
        matches!(self, Self::Changed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedImageMetadata {
    /// Logical layout size (redisplay / `frame-char-width` space).
    pub layout: ImageLayoutExtent,
    /// GNU `Fimage_size` with PIXELS non-nil (`img->width` / `img->height` space).
    pub reported: ImageReportedExtent,
    /// GNU's decoded four-corner background guess (0x00RRGGBB).
    pub background: u32,
    /// GNU's decoded four-corner mask classification.
    pub background_transparent: bool,
    /// Distinguishes a GNU-compatible clipping mask from continuous alpha.
    pub mask: ImageMaskKind,
    /// Format metadata owned by the decoder, never renderer geometry.
    pub embedded: ImageEmbeddedMetadata,
}

impl ResolvedImageMetadata {
    /// Build metadata when layout already equals GNU image-pixel size.
    #[must_use]
    pub const fn layout_is_image_pixels(
        width: u32,
        height: u32,
        background: u32,
        background_transparent: bool,
        mask: ImageMaskKind,
    ) -> Self {
        Self {
            layout: ImageLayoutExtent::new(width, height),
            reported: ImageReportedExtent::new(width, height),
            background,
            background_transparent,
            mask,
            embedded: ImageEmbeddedMetadata::EMPTY,
        }
    }

    /// Build metadata from logical layout + realization report scale.
    #[must_use]
    pub fn from_layout(
        layout: ImageLayoutExtent,
        realization: ResolvedImageRealization,
        background: u32,
        background_transparent: bool,
        mask: ImageMaskKind,
    ) -> Self {
        Self {
            layout,
            reported: ImageReportedExtent::new(
                realization.image_pixel_dimension(layout.width()),
                realization.image_pixel_dimension(layout.height()),
            ),
            background,
            background_transparent,
            mask,
            embedded: ImageEmbeddedMetadata::default(),
        }
    }

    /// Attach decoder-owned metadata without making plain-image constructors
    /// fabricate animation state.
    #[must_use]
    pub fn with_embedded(mut self, embedded: ImageEmbeddedMetadata) -> Self {
        self.embedded = embedded;
        self
    }
}

/// Decoded image whose intrinsic metadata is available for layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadyImage {
    pub load: ImageLoadToken,
    pub metadata: ResolvedImageMetadata,
}

impl ReadyImage {
    #[must_use]
    pub const fn image_id(&self) -> ImageId {
        self.load.image()
    }
}

/// Stable renderer identity and layout slot for an image lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImagePlacement {
    image_id: ImageId,
    layout: ImageLayoutExtent,
}

impl ImagePlacement {
    #[must_use]
    pub const fn new(image_id: ImageId, layout: ImageLayoutExtent) -> Self {
        Self { image_id, layout }
    }

    #[must_use]
    pub const fn image_id(self) -> ImageId {
        self.image_id
    }

    #[must_use]
    pub const fn layout(self) -> ImageLayoutExtent {
        self.layout
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.layout.width()
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.layout.height()
    }
}

/// Stable placeholder geometry while an image is decoded asynchronously.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingImage {
    load: ImageLoadToken,
    placement: ImagePlacement,
}

/// Stable failed state retaining the slot that was allocated while pending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailedImage {
    load: ImageLoadToken,
    placement: ImagePlacement,
    pub error: String,
}

impl PendingImage {
    #[must_use]
    pub const fn new(load: ImageLoadToken, layout: ImageLayoutExtent) -> Self {
        Self {
            load,
            placement: ImagePlacement::new(load.image(), layout),
        }
    }

    #[must_use]
    pub const fn load(&self) -> ImageLoadToken {
        self.load
    }

    #[must_use]
    pub const fn placement(&self) -> ImagePlacement {
        self.placement
    }

    #[must_use]
    pub fn failed(self, error: String) -> FailedImage {
        FailedImage {
            load: self.load,
            placement: self.placement,
            error,
        }
    }
}

impl FailedImage {
    #[must_use]
    pub const fn load(&self) -> ImageLoadToken {
        self.load
    }

    #[must_use]
    pub const fn placement(&self) -> ImagePlacement {
        self.placement
    }
}

/// Result of a nonblocking image catalog lookup.
///
/// Every non-ready state retains stable placement geometry, so asynchronous
/// completion or failure never changes a published frame in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageLookup {
    Ready(ReadyImage),
    Pending(PendingImage),
    Failed(FailedImage),
}

impl ImageLookup {
    /// Return the stable renderer identity and dimensions represented by this
    /// state. Ready images use decoded dimensions; pending and failed images
    /// retain their placeholder slot.
    #[must_use]
    pub const fn placement(&self) -> ImagePlacement {
        match self {
            Self::Ready(image) => ImagePlacement::new(image.image_id(), image.metadata.layout),
            Self::Pending(image) => image.placement(),
            Self::Failed(image) => image.placement(),
        }
    }

    #[must_use]
    pub const fn ready_metadata(&self) -> Option<&ResolvedImageMetadata> {
        match self {
            Self::Ready(image) => Some(&image.metadata),
            Self::Pending(_) | Self::Failed(_) => None,
        }
    }
}

/// Catalog seam used by redisplay to schedule or inspect image work.
pub trait ImageCatalog {
    /// Return the current state immediately. A cache miss schedules decoding
    /// and returns [`ImageLookup::Pending`]. Implementations must not wait for
    /// renderer queue capacity, metadata locks, file I/O, decode, or upload.
    fn lookup(&self, request: ImageResolveRequest) -> ImageLookup;

    /// Apply one explicitly typed cache operation. The next matching lookup
    /// must allocate a fresh renderer identity and decode again. Hosts without
    /// an image cache may keep the default no-op.
    fn invalidate(&self, _target: ImageInvalidation) -> ImageInvalidationResult {
        ImageInvalidationResult::Unchanged
    }

    /// Retire decoder/compositor state without invalidating frame textures.
    fn invalidate_animation(&self, _target: ImageAnimationInvalidation) -> ImageInvalidationResult {
        ImageInvalidationResult::Unchanged
    }

    /// Renderer-published resident texture and decoded-sequence bytes for
    /// `image-cache-size`. Default 0 when the host does not track accounting.
    fn cached_size_bytes(&self) -> i64 {
        0
    }

    /// Reconcile catalog lifecycle with renderer-published state before a
    /// media-generation rebuild: promote completed `Pending` entries and mark
    /// formerly ready images whose renderer residency disappeared as evicted.
    ///
    /// Redisplay `lookup` stays non-blocking (`try_lock`); this path may wait
    /// briefly for the shared renderer-state map.
    fn reconcile_renderer_state(&self, _event: ImageStateEvent) {}
}

#[cfg(test)]
mod tests {
    use super::{ImageDefaultScale, ImageScaleEnvironment, ImageScalePolicy};

    #[test]
    fn auto_scale_realizes_gnu_x11_sized_pixels_on_fractional_wayland() {
        let environment = ImageScaleEnvironment::new(7.2, 1.75, ImageDefaultScale::Auto);

        let realization = environment.resolve(ImageScalePolicy::Default);

        // GNU's auto policy sees a 13-device-pixel frame column and therefore
        // realizes 24px at 1.3x.  Neomacs lays that out in logical pixels and
        // rasterizes it in device pixels.
        assert_eq!(realization.layout_dimension(24), 18);
        assert_eq!(realization.raster_dimension(18), 32);
    }

    #[test]
    fn auto_scale_reconstructs_gnu_device_column_from_integer_logical_geometry() {
        // Neomacs exposes an integer logical `frame-char-width`, while GNU's
        // FRAME_COLUMN_WIDTH is the corresponding integer device-pixel font
        // metric.  At 1.75 scale a 7px logical cell therefore occupies the
        // 13px device column used by GNU's automatic image scale, not 12px.
        let environment = ImageScaleEnvironment::new(7.0, 1.75, ImageDefaultScale::Auto);

        let realization = environment.resolve(ImageScalePolicy::Default);

        assert_eq!(realization.layout_dimension(24), 18);
        assert_eq!(realization.raster_dimension(18), 32);
    }

    #[test]
    fn auto_scale_is_identity_at_one_x_when_the_column_is_under_ten_pixels() {
        let environment = ImageScaleEnvironment::new(7.2, 1.0, ImageDefaultScale::Auto);

        let realization = environment.resolve(ImageScalePolicy::Default);

        assert_eq!(realization.layout_dimension(24), 24);
        assert_eq!(realization.raster_dimension(24), 24);
    }

    #[test]
    fn explicit_image_scale_does_not_consult_the_default_policy() {
        let environment = ImageScaleEnvironment::new(
            7.2,
            1.75,
            ImageDefaultScale::Explicit(2.0.try_into().expect("valid scale")),
        );

        let realization = environment.resolve(ImageScalePolicy::Explicit(
            0.5.try_into().expect("valid scale"),
        ));

        assert_eq!(realization.layout_dimension(24), 12);
        assert_eq!(realization.raster_dimension(12), 21);
    }
}
