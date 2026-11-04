//! Image-catalog vocabulary owned by the evaluator layer (stage-4 W2 E5).
//!
//! Requests, decoded-image metadata and lookup states that cross the
//! evaluator/display-host seam. They name only display-protocol vocabulary,
//! value heap types and the value-owned ImageSpecIdentity; the catalog
//! implementation and the scale-environment behaviour stay in the display
//! module, which re-exports these items.

use neomacs_display_protocol::ImageRealization as ResolvedImageRealization;
use neomacs_display_protocol::image::EncodedBytes;
use neomacs_display_protocol::image_diagnostic::{ImageDiagnostic, ImageLoadIdentity};
use neomacs_display_protocol::{
    ImageAnimationPolicy, ImageColorContext, ImageEmbeddedMetadata, ImageFrameIndex, ImageId,
    ImageLayoutExtent, ImageLoadToken, ImageMaskKind, ImageMaskPolicy, ImageReportedExtent,
    ImageRotation, ImageSizeLimit, ImageSizeSpec, ImageStateEvent,
};
use neovm_value::heap_types::{LispString, LispStringStorageKind};
use neovm_value::image_identity::ImageSpecIdentity;

/// Property-free image source text, preserving the Lisp cache-key spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ImageSourceSpelling {
    bytes: Vec<u8>,
    storage_kind: LispStringStorageKind,
}

static_assertions::assert_impl_all!(ImageSourceSpelling: Send, Sync, Clone, std::fmt::Debug, Eq, std::hash::Hash);

impl From<&LispString> for ImageSourceSpelling {
    fn from(text: &LispString) -> Self {
        Self {
            bytes: text.as_bytes().to_vec(),
            storage_kind: text.storage_kind(),
        }
    }
}

impl std::hash::Hash for ImageSourceSpelling {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.bytes, state);
        std::hash::Hash::hash(
            &match self.storage_kind {
                LispStringStorageKind::Unibyte => false,
                LispStringStorageKind::Multibyte => true,
            },
            state,
        );
    }
}

/// A GNU image `:base-uri` spelling without Lisp properties or heap pointers.
///
/// Retain its original bytes and string mode for request identity. Decoders
/// interpret those bytes as UTF-8 exactly as they did at the Lisp-string edge.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageBaseUri(ImageSourceSpelling);

static_assertions::assert_impl_all!(ImageBaseUri: Send, Sync, Clone, std::fmt::Debug, Eq, std::hash::Hash);

impl ImageBaseUri {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0.bytes
    }

    #[must_use]
    pub fn as_utf8_str(&self) -> Option<&str> {
        std::str::from_utf8(self.as_bytes()).ok()
    }
}

impl From<&LispString> for ImageBaseUri {
    fn from(uri: &LispString) -> Self {
        Self(ImageSourceSpelling::from(uri))
    }
}

/// A Lisp image `:file` spelling, before GNU image-path resolution.
///
/// It may contain non-UTF-8 Emacs bytes; filename classification retains its
/// existing UTF-8 admission rule rather than normalizing the spelling here.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageFileName(ImageSourceSpelling);

static_assertions::assert_impl_all!(ImageFileName: Send, Sync, Clone, std::fmt::Debug, Eq, std::hash::Hash);

impl ImageFileName {
    #[must_use]
    pub fn from_utf8(path: &str) -> Self {
        Self(ImageSourceSpelling {
            bytes: path.as_bytes().to_vec(),
            storage_kind: LispStringStorageKind::Multibyte,
        })
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0.bytes
    }

    #[must_use]
    pub fn as_utf8_str(&self) -> Option<&str> {
        std::str::from_utf8(self.as_bytes()).ok()
    }
}

impl From<&LispString> for ImageFileName {
    fn from(path: &LispString) -> Self {
        Self(ImageSourceSpelling::from(path))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageDataSource {
    /// Encoded bytes with no authority to resolve external resources.
    Isolated(EncodedBytes),
    /// Encoded bytes whose relative resources may be resolved against the
    /// explicitly supplied GNU image `:base-uri`.
    WithBaseUri {
        data: EncodedBytes,
        base_uri: ImageBaseUri,
    },
}

static_assertions::assert_impl_all!(ImageDataSource: Send, Sync, Clone, std::fmt::Debug, Eq, std::hash::Hash);

impl ImageDataSource {
    /// The encoded bytes, whichever authority they carry.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Isolated(data) => data,
            Self::WithBaseUri { data, .. } => data,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageResolveSource {
    File(ImageFileName),
    Data(ImageDataSource),
}

static_assertions::assert_impl_all!(ImageResolveSource: Send, Sync, Clone, std::fmt::Debug, Eq, std::hash::Hash);

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
    /// Neomacs `:animation` policy: whether computed animation (SVG SMIL)
    /// may materialize. Distinct specs already differ in `spec` identity,
    /// so this field is a materialization recipe, not a second key.
    pub animation: ImageAnimationPolicy,
    pub realization: ResolvedImageRealization,
    /// The type and subject GNU's loaders word their diagnostics with.
    ///
    /// Derived from the same specification the rest of this request is, so it
    /// cannot disagree with it; it travels here rather than being recomputed
    /// at the failure site because only this side can print a `:data` image's
    /// specification the way GNU does.
    pub identity: ImageLoadIdentity,
}

static_assertions::assert_impl_all!(ImageResolveRequest: Send, Sync, Clone, std::fmt::Debug, Eq, std::hash::Hash);

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
            reported: realization.report_extent(layout),
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

    /// The slot's logical layout extent as a pair.
    #[must_use]
    pub const fn dimensions(self) -> (u32, u32) {
        (self.width(), self.height())
    }
}

/// Geometry for an image that is still being decoded.
///
/// The layout is the best the implementation can know without pixels: an
/// encoded header when one can be read off-thread, otherwise the request's
/// pinned placeholder. It must agree with the layout the completed decode
/// reports — a pending slot that moves when the pixels land is worse than a
/// slot that stayed at the placeholder — so implementations resolve it through
/// the same sizing inputs the decoder will use, and must not block to get it.
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
    /// GNU's diagnostic for the failure.  It is a value with its own text
    /// rather than a message string, so a consumer cannot hold the failure
    /// without also holding what to say about it.
    pub error: ImageDiagnostic,
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
    pub fn failed(self, error: ImageDiagnostic) -> FailedImage {
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
    /// renderer queue capacity, metadata locks, file I/O, decode, or upload —
    /// including while resolving the pending layout, which is why header
    /// geometry arrives from a producer thread rather than from this call.
    ///
    /// `limit` is the frame's resolved `max-image-size`
    /// ([`ImageScaleEnvironment::size_limit`]). It is a parameter rather than a
    /// property of the request because it is *not* part of an image's identity:
    /// GNU leaves an already-loaded image alone when the frame is resized, so
    /// folding the bound into the key would re-decode every image on every
    /// resize step. It is a parameter rather than a defaulted property of the
    /// implementation so that a lookup cannot be written without stating the
    /// bound it loads under.
    fn lookup(&self, request: ImageResolveRequest, limit: ImageSizeLimit) -> ImageLookup;

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

    /// Take the failures this catalog has observed since the last call, as
    /// GNU's `image_error` would have worded them.
    ///
    /// A failure is recorded by [`Self::lookup`] itself, so no consumer of a
    /// failed lookup can consume its geometry and forget the reason. What
    /// remains is to *say* it: `Context::log_pending_image_diagnostics` is the
    /// one place that does, and it is called from the same pass that performed
    /// the lookups. Hosts without a catalog return nothing.
    ///
    /// The strings are GNU's C-level text; the caller applies
    /// `text-quoting-style` exactly as GNU's `vadd_to_log` does.
    fn take_pending_diagnostics(&self) -> Vec<String> {
        Vec::new()
    }

    /// Reconcile catalog lifecycle with renderer-published state before a
    /// media-generation rebuild: promote completed `Pending` entries and mark
    /// formerly ready images whose renderer residency disappeared as evicted.
    ///
    /// Redisplay `lookup` stays non-blocking (`try_lock`); this path may wait
    /// briefly for the shared renderer-state map.
    fn reconcile_renderer_state(&self, _event: ImageStateEvent) {}
}
