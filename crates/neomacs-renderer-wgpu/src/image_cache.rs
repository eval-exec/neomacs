//! Target-aware image loading and caching for the wgpu renderer.
//!
//! Provides:
//! - Dimension queries for pending-image placeholders
//! - A bounded background decoder pool on native targets
//! - Inline decoding on browser Wasm, which has no ambient native threads
//! - GPU texture upload when ready
//! - LRU cache with memory limits

use neomacs_display_protocol::image::EncodedBytes;
use neomacs_display_protocol::image_diagnostic::{ImageDiagnostic, ImageLoadIdentity};
use neomacs_display_protocol::{
    ImageCacheUsage, ImageColorContext, ImageEmbeddedMetadata, ImageFrameIndex, ImageHeuristicMask,
    ImageId, ImageIntrinsicExtent, ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken,
    ImageMaskKind, ImageMaskPolicy, ImageNativeExtent, ImageRasterExtent, ImageRealization,
    ImageReportedExtent, ImageRotation, ImageSequenceId, ImageSequenceRetirement, ImageSizeSpec,
    ResolvedImageGeometry, RetainedImageSet,
};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
#[cfg(not(target_family = "wasm"))]
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
#[cfg(not(target_family = "wasm"))]
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, mpsc};
#[cfg(not(target_family = "wasm"))]
use std::thread;

use crate::image_bands::{
    BandFilling, BandSource, BandStep, DecodedBand, RasterBand, RowRange, TextureRows,
    classify_alpha,
};
use crate::image_sequence::{ImageSequenceCache, ImageSequenceResolution};

#[cfg(target_os = "linux")]
use crate::external_buffer::DmaBufBuffer;

/// Maximum texture dimension (width or height)
const MAX_TEXTURE_SIZE: u32 = 4096;

/// Test-only: abandon a banded attempt after this many bands have been
/// published. Zero disables it.
///
/// The property this serves — a banded decode that stops part-way leaves the
/// caller with a whole image rather than a truncated one — cannot be expressed
/// with a fixture: both paths read the same bytes, so a source the row-wise
/// reader rejects is one the whole-image decoder rejects too. Injecting the cut
/// leaves the consequence under test exactly as it would be, and `nextest`
/// gives each test its own process, so the hook cannot leak into another.
#[cfg(test)]
static ABANDON_AFTER_BANDS: AtomicU32 = AtomicU32::new(0);

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

/// The geometry a decode realizes to, raster clamped by the texture limit.
///
/// One function rather than a line in each caller, because two callers must
/// agree about it: `NativePixels::realize_bitmap` scales the whole image onto
/// this raster, and a banded decode builds the raster it writes into from it
/// (`image_bands::BandPlan::target`). A decode writing under a different raster
/// than the upload that finishes it would leave the texture holding rows from
/// two scales, which nothing downstream could detect.
pub(crate) fn realized_geometry(
    extent: ImageNativeExtent,
    size: ImageSizeSpec,
    realization: ImageRealization,
) -> ResolvedImageGeometry {
    let geometry = realization.resolve_geometry(size, extent, ImageRotation::None);
    geometry.with_raster(constrain_raster_extent(geometry.raster()))
}

/// Maximum total cache memory in bytes (64MB)
const MAX_CACHE_MEMORY: usize = 64 * 1024 * 1024;

#[cfg(not(target_family = "wasm"))]
const MAX_IMAGE_DECODER_THREADS: usize = 4;

/// A deliberately small, non-empty pool of persistent image decoders.
///
/// Image requests are normally sparse and already queue through one shared
/// receiver.  Scaling this pool to every host CPU made each GUI reserve dozens
/// of idle thread stacks before it had seen an image.  GNU image decoding is
/// synchronous (`image.c` even declines WebP's multithreaded option), so four
/// asynchronous workers retain useful parallelism without making GUI startup
/// resources proportional to machine size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(not(target_family = "wasm"))]
struct ImageDecoderPoolSize(NonZeroUsize);

#[cfg(not(target_family = "wasm"))]
impl ImageDecoderPoolSize {
    fn detected() -> Self {
        Self::from_available_parallelism(std::thread::available_parallelism().ok())
    }

    fn from_available_parallelism(available: Option<NonZeroUsize>) -> Self {
        let available = available.map_or(MAX_IMAGE_DECODER_THREADS, NonZeroUsize::get);
        Self(
            NonZeroUsize::new(available.min(MAX_IMAGE_DECODER_THREADS))
                .expect("the image decoder pool cap is nonzero"),
        )
    }

    const fn get(self) -> usize {
        self.0.get()
    }
}

/// Image loading state
#[derive(Debug, Clone)]
pub enum ImageState {
    /// Queued for loading
    Pending,
    /// Currently being decoded
    Decoding,
    /// Ready with texture
    Ready,
    /// Logically invalidated but retained by at least one presentation.
    Retiring,
    /// Failed to load
    Failed(String),
}

/// How much of a texture holds pixels, as a prefix of its rows.
///
/// A banded decode fills from the top: its bands are disjoint, in order, and
/// the first starts at row 0, so the rows that exist are always a *prefix* of
/// the texture — never a set, never a hole. That is what lets the draw side ask
/// one number how far down it may draw, and it is why `filled <= total` is an
/// invariant of this type rather than a check at each use: a value claiming more
/// rows than the texture has cannot be built, so "draw a row that was never
/// uploaded" cannot be said.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilledRows {
    filled: u32,
    total: NonZeroU32,
}

impl FilledRows {
    /// Every row of a texture that was written whole.
    #[must_use]
    pub fn complete(raster: ImageRasterExtent) -> Self {
        Self::partial(raster.height(), raster.height())
    }

    /// A texture whose rows are all still to be written.
    #[must_use]
    pub fn empty(raster: ImageRasterExtent) -> Self {
        Self::partial(0, raster.height())
    }

    fn partial(filled: u32, total: u32) -> Self {
        Self {
            filled,
            total: NonZeroU32::new(total).unwrap_or(NonZeroU32::MIN),
        }
    }

    /// The part of a `total`-row texture that holds pixels.
    #[must_use]
    pub const fn filled(&self) -> u32 {
        self.filled
    }

    /// How many rows the texture has.
    #[must_use]
    pub const fn total(&self) -> NonZeroU32 {
        self.total
    }

    /// Whether every row of the texture has been written.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.filled == self.total.get()
    }

    /// How far down the texture a quad may be drawn, as a texture coordinate.
    #[must_use]
    pub fn filled_fraction(&self) -> f32 {
        self.filled as f32 / self.total.get() as f32
    }

    /// This prefix extended by `rows`, which must start exactly where it ends.
    ///
    /// `None` for a band that would leave a hole or rewrite rows that are
    /// already there — a band from a superseded decode, or one that was applied
    /// twice. Holding the extended value is what the write of those rows is
    /// paired with, so a texture cannot come to claim rows nobody wrote.
    #[must_use]
    pub fn extend(self, rows: TextureRows) -> Option<Self> {
        (rows.start() == self.filled && rows.end() <= self.total.get()).then(|| Self {
            filled: rows.end(),
            ..self
        })
    }

    /// Trim a drawn span to the rows that hold pixels.
    ///
    /// `height` is how many pixels of the glyph are drawn and `v0..v1` the same
    /// span as texture rows, so the finish is affine and shortening one by a
    /// fraction shortens the other by the same fraction. `None` when nothing of
    /// the span is uploaded: the quad must be skipped rather than drawn against
    /// rows that were never written.
    #[must_use]
    pub fn clip_span(self, v0: f32, v1: f32, height: f32) -> Option<(f32, f32)> {
        if v1 <= v0 {
            return None;
        }
        let limit = self.filled_fraction();
        if v0 >= limit {
            return None;
        }
        if v1 <= limit {
            return Some((v1, height));
        }
        Some((limit, height * (limit - v0) / (v1 - v0)))
    }
}

/// Cached image with GPU texture
pub struct CachedImage {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    /// Uploaded texture dimensions in physical device pixels.
    pub raster: ImageRasterExtent,
    /// How much of `raster` has been written.
    ///
    /// Everything, for an image that was decoded whole. A banded decode leaves
    /// this short of `raster` while its rows are still arriving, and every
    /// consumer that draws the image reads it from here — there is no second
    /// copy of "how far has it got" to fall out of step with the texture.
    pub filled: FilledRows,
    pub metadata: Option<ImageMetadata>,
    /// Memory size in bytes
    pub memory_size: usize,
    /// Monotonic access stamp for LRU eviction; refreshed by `get` (a `Cell`
    /// so draw-path lookups stay `&self`).
    last_access: Cell<u64>,
}

/// Decoded image data waiting for GPU upload
struct DecodedImage {
    load: ImageLoadToken,
    geometry: ResolvedImageGeometry,
    data: Vec<u8>, // RGBA
    metadata: ImageMetadata,
}

/// What one attempt at a banded decode produced.
///
/// The two ways of not completing are separate arms because they mean
/// different things to a reader — one is a source that never had a row-wise
/// decoder, the other a decoder that stopped part-way — even though the caller
/// does the same thing with both.
enum BandedAttempt {
    /// Every row was decoded, into the raster the image is realized to.
    Complete(DecodedPixels),
    /// The source has no row-wise decoder at this size, or none for its format,
    /// or no band of it has anywhere to go: decode the whole image.
    NotBandable,
    /// A row-wise decode that could not finish. The bands it published are
    /// abandoned; decoding the whole image is the only way to end up with a
    /// whole one.
    Abandoned,
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
        let geometry = realized_geometry(self.extent, size, realization);
        let (raster_width, raster_height) = geometry.raster().dimensions();
        let (native_width, native_height) = self.extent.dimensions();
        let rgba = if geometry.raster() == ImageRasterExtent::new(native_width, native_height) {
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

enum WorkerDecodeOutcome {
    /// One band of a decode that is still running. Intermediate, not terminal:
    /// it consumes no load generation, and the `Ready` that ends the decode
    /// supersedes it.
    Band {
        load: ImageLoadToken,
        decoded: DecodedBand,
    },
    Ready(DecodedImage),
    Failed {
        load: ImageLoadToken,
        diagnostic: ImageDiagnostic,
    },
}

impl WorkerDecodeOutcome {
    fn load(&self) -> ImageLoadToken {
        match self {
            Self::Band { load, .. } => *load,
            Self::Ready(decoded) => decoded.load,
            Self::Failed { load, .. } => *load,
        }
    }
}

/// A renderer image-cache lifecycle event. Callers must handle eviction as
/// well as terminal decode results so external catalogs cannot retain stale
/// residency state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageCacheEvent {
    /// One band of an image whose decode is still running.
    ///
    /// Intermediate by construction: the `Ready` for the same load carries the
    /// whole image, so a consumer that drew this band ends up correct. Only the
    /// rows are published — they are what says how far the decode has come, and
    /// the pixels have already gone into the texture.
    Band {
        load: ImageLoadToken,
        rows: RowRange,
    },
    Ready {
        load: ImageLoadToken,
        metadata: ImageMetadata,
    },
    Failed {
        load: ImageLoadToken,
        error: ImageDiagnostic,
    },
    Evicted {
        image: ImageId,
    },
}

#[derive(Default)]
struct ImageLoadLifecycle {
    next_attempt: u64,
    active: HashMap<ImageId, ImageLoadAttempt>,
}

impl ImageLoadLifecycle {
    fn generated_token(&mut self, image: ImageId) -> ImageLoadToken {
        self.next_attempt = self
            .next_attempt
            .checked_add(1)
            .expect("image load attempt exhausted");
        let attempt =
            ImageLoadAttempt::new(self.next_attempt).expect("checked nonzero image load attempt");
        ImageLoadToken::new(image, attempt)
    }

    #[cfg(test)]
    fn begin_generated(&mut self, image: ImageId) -> ImageLoadToken {
        let load = self.generated_token(image);
        self.begin(load)
    }

    fn begin(&mut self, load: ImageLoadToken) -> ImageLoadToken {
        self.next_attempt = self.next_attempt.max(load.attempt().get());
        self.active.insert(load.image(), load.attempt());
        load
    }

    fn accept(&mut self, load: ImageLoadToken) -> bool {
        if !self.is_current(load) {
            return false;
        }
        self.active.remove(&load.image());
        true
    }

    /// Whether `load` is still the attempt allowed to publish for its image,
    /// without consuming it.
    ///
    /// Intermediate publications — bands — check this and leave the attempt
    /// alone; only the terminal outcome consumes it.
    fn is_current(&self, load: ImageLoadToken) -> bool {
        self.active.get(&load.image()) == Some(&load.attempt())
    }

    /// Accept `outcome` if its attempt is still the current one.
    ///
    /// A band and a terminal outcome differ here: a band is progress from a
    /// decode that is still running, so it is checked and left alone, while
    /// only the terminal outcome consumes the attempt — once, so a superseded
    /// decode can never publish Ready after its replacement has begun.
    fn take_current(&mut self, outcome: WorkerDecodeOutcome) -> Option<WorkerDecodeOutcome> {
        if matches!(outcome, WorkerDecodeOutcome::Band { .. }) {
            return self.is_current(outcome.load()).then_some(outcome);
        }
        self.accept(outcome.load()).then_some(outcome)
    }

    fn free(&mut self, image: ImageId) {
        self.active.remove(&image);
    }

    fn clear(&mut self) {
        self.active.clear();
    }
}

/// Separates logical cache invalidation from presentation-safe GPU release.
#[derive(Default)]
struct ImageResidencyLifecycle {
    retiring: HashSet<ImageId>,
}

impl ImageResidencyLifecycle {
    fn request_retirement(&mut self, image: ImageId) {
        self.retiring.insert(image);
    }

    fn cancel_retirement(&mut self, image: ImageId) {
        self.retiring.remove(&image);
    }

    fn take_releasable(&mut self, retained: &RetainedImageSet) -> Vec<ImageId> {
        let mut releasable = self
            .retiring
            .iter()
            .copied()
            .filter(|image| !retained.contains(*image))
            .collect::<Vec<_>>();
        releasable.sort_unstable();
        for image in &releasable {
            self.retiring.remove(image);
        }
        releasable
    }

    fn clear(&mut self) {
        self.retiring.clear();
    }
}

/// Facts published with a completed decoded image realization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMetadata {
    pub layout: ImageLayoutExtent,
    pub reported: ImageReportedExtent,
    /// GNU's four-corner background guess, encoded as 0x00RRGGBB.
    pub background: u32,
    /// Whether GNU's four-corner mask heuristic classifies the background as transparent.
    pub background_transparent: bool,
    /// The decoded alpha representation, independent of its RGBA storage.
    pub mask: ImageMaskKind,
    /// Decoder-owned metadata returned by GNU's `image-metadata`.
    pub embedded: ImageEmbeddedMetadata,
}

/// Image cache with a compile-target-selected decode executor.
pub struct ImageCache {
    /// Budget accounting events since the last drain (texture create/free).
    accounting: Vec<crate::media_budget::MediaAccounting>,
    /// Next image ID
    next_id: AtomicU32,
    /// Cached textures: id -> CachedImage
    textures: HashMap<ImageId, CachedImage>,
    /// Image states: id -> state
    states: HashMap<ImageId, ImageState>,
    /// Identifies the one decode request currently allowed to publish for each ID.
    loads: ImageLoadLifecycle,
    residency: ImageResidencyLifecycle,
    /// Assets pinned by accepted or queued render presentations.
    retained_images: RetainedImageSet,
    /// Pending dimensions (before texture is ready)
    pending_dimensions: HashMap<ImageId, ImageLayoutExtent>,
    /// Channel to receive decoded images
    decoded_rx: mpsc::Receiver<WorkerDecodeOutcome>,
    /// Compile-target-selected image decode executor.
    decoder: ImageDecodeExecutor,
    /// CPU decoder/compositor state shared across all frames of an animation.
    sequence_cache: Arc<ImageSequenceCache>,
    /// Bind group layout for image textures
    bind_group_layout: wgpu::BindGroupLayout,
    /// Sampler for image textures
    sampler: wgpu::Sampler,
    /// Total cached memory
    total_memory: usize,
    /// Monotonic clock stamping `CachedImage::last_access` (LRU order).
    access_clock: Cell<u64>,
}

/// Pick the least-recently-used entry: the id with the smallest access stamp
/// (ties broken by smaller id for determinism).
fn lru_unpresented_victim(
    entries: impl Iterator<Item = (ImageId, u64)>,
    retained: &RetainedImageSet,
) -> Option<ImageId> {
    entries
        .filter(|(image, _)| !retained.contains(*image))
        .min_by_key(|&(id, stamp)| (stamp, id))
        .map(|(id, _)| id)
}

/// Request to decode an image
struct DecodeRequest {
    load: ImageLoadToken,
    source: ImageSource,
    size: ImageSizeSpec,
    rotation: ImageRotation,
    /// Semantic and device geometry resolved by evaluator/layout.
    realization: ImageRealization,
    /// Resolved face colors used by face-sensitive formats and cache identity.
    colors: ImageColorContext,
    mask: ImageMaskPolicy,
    frame: ImageFrameIndex,
    /// What GNU calls this source when the decode fails.  It travels with the
    /// job because the failure is worded by the loader's own rules (`Not a PNG
    /// file: `%s'`), and the job is the last place that knows both the bytes
    /// and what they were asked to be.
    identity: ImageLoadIdentity,
}

/// What a decode job can still say about a source once the decode chain has
/// collapsed every reason to `None`.
///
/// The chain that answers `Option` — banded attempt, whole-image decode, XPM,
/// XBM, SVG — loses its reason at every fallback: a banded attempt that cannot
/// finish hands the same bytes to the whole-image path, which hands them to
/// the next format. GNU does not have this problem because each of its loaders
/// checks its own signature and reports its own decoder's message, so the
/// reason is recovered here from the two facts that survive: which source was
/// asked for, and what its bytes turned out to be.
enum DecodeFailureSource {
    /// A `:file` source, named by the path a load command was given.
    File { path: String },
    /// A `:data` source, whose bytes are already in hand.
    Bytes { data: EncodedBytes },
    /// Raw pixels handed over by a caller, with no encoded format to name and
    /// no GNU loader behind them.
    Raw,
}

impl DecodeFailureSource {
    /// Capture the source before the decode consumes it.
    ///
    /// Both captures are cheap references to what the job already owns — a
    /// path clone and a handle to the same bytes — because this runs on the
    /// decode path for every job, not only the failing ones.
    fn of(source: &ImageSource) -> Self {
        match source {
            ImageSource::File { path, .. } => Self::File { path: path.clone() },
            ImageSource::Data { data, .. } => Self::Bytes { data: data.clone() },
            ImageSource::RawArgb32 { .. } | ImageSource::RawRgb24 { .. } => Self::Raw,
            #[cfg(test)]
            ImageSource::Panic => Self::Raw,
        }
    }

    fn diagnostic(&self, identity: &ImageLoadIdentity) -> ImageDiagnostic {
        // A source that arrived without a specification has no declared type
        // and no printed spec, so there is no GNU sentence that is true of it.
        if identity.is_unspecified() {
            return ImageDiagnostic::NotDrawable;
        }
        match self {
            // GNU asks `emacs_open` first and only then checks the signature
            // (`src/image.c:8282-8303`), so "the file is not there" and "the
            // file is not a PNG" are different sentences. Re-reading on the
            // failure path is what recovers that distinction: it happens once,
            // only when the decode has already failed.
            Self::File { path } => match std::fs::read(path) {
                Err(_) => identity.not_found(),
                Ok(bytes) => Self::from_bytes(identity, &bytes),
            },
            Self::Bytes { data } => Self::from_bytes(identity, data.as_slice()),
            Self::Raw => ImageDiagnostic::NotDrawable,
        }
    }

    fn from_bytes(identity: &ImageLoadIdentity, data: &[u8]) -> ImageDiagnostic {
        if image::guess_format(data).is_err() {
            return identity.wrong_format();
        }
        // The format was recognised and the decode failed inside it, which is
        // GNU's `PNG error: %s` / `Error reading JPEG image ...` arm.
        let format = identity.format().clone();
        match image::load_from_memory(data) {
            // libpng's `png_read_data` reports a short read as `Read error`,
            // and a stream that ends early is exactly what the `image` crate
            // calls `UnexpectedEof`. Anything else keeps the decoder's own
            // words, which is what GNU passes through verbatim
            // (`image_error ("PNG error: %s", ...)`, `src/image.c:8184`).
            Err(image::ImageError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                ImageDiagnostic::FormatError {
                    format,
                    detail: "Read error".to_owned(),
                }
            }
            Err(error) => ImageDiagnostic::FormatError {
                format,
                detail: error.to_string(),
            },
            // The bytes decoded; something after the decoder refused them.
            Ok(_) => ImageDiagnostic::NotDrawable,
        }
    }
}

/// Image source
enum ImageSource {
    File {
        path: String,
        sequence: ImageSequenceId,
    },
    Data {
        data: EncodedBytes,
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

std::cfg_select! {
    target_family = "wasm" => {
        /// Browser Wasm has no ambient native threads. Decode requests run on
        /// the presentation event loop and publish through the same completion
        /// channel as the native pool, keeping the cache state machine shared.
        struct ImageDecodeExecutor {
            completed: mpsc::Sender<WorkerDecodeOutcome>,
            sequence_cache: Arc<ImageSequenceCache>,
        }

        impl ImageDecodeExecutor {
            fn new(
                completed: mpsc::Sender<WorkerDecodeOutcome>,
                sequence_cache: Arc<ImageSequenceCache>,
            ) -> Self {
                Self {
                    completed,
                    sequence_cache,
                }
            }

            fn submit(&self, request: DecodeRequest) {
                let outcome = ImageCache::decode_request(request, &self.sequence_cache);
                let _ = self.completed.send(outcome);
            }
        }
    }
    _ => {
        /// Native image decoding is isolated from presentation by a bounded
        /// persistent thread pool.
        struct ImageDecodeExecutor {
            requests: mpsc::Sender<DecodeRequest>,
        }

        impl ImageDecodeExecutor {
            fn new(
                completed: mpsc::Sender<WorkerDecodeOutcome>,
                sequence_cache: Arc<ImageSequenceCache>,
            ) -> Self {
                let (requests, receiver) = mpsc::channel::<DecodeRequest>();
                let receiver = Arc::new(Mutex::new(receiver));
                let pool_size = ImageDecoderPoolSize::detected();
                tracing::info!("Starting {} image decoder threads", pool_size.get());
                for thread_id in 0..pool_size.get() {
                    let receiver = Arc::clone(&receiver);
                    let completed = completed.clone();
                    let sequence_cache = Arc::clone(&sequence_cache);
                    thread::spawn(move || {
                        ImageCache::decoder_thread_pooled(
                            thread_id,
                            receiver,
                            completed,
                            sequence_cache,
                        );
                    });
                }
                Self { requests }
            }

            fn submit(&self, request: DecodeRequest) {
                let _ = self.requests.send(request);
            }
        }
    }
}

impl ImageCache {
    /// Create a new image cache
    pub fn new(device: &wgpu::Device) -> Self {
        // Create bind group layout for image textures
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Image Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Create sampler
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Image Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        // All target executors publish through one completion channel.
        let (decoded_tx, decoded_rx) = mpsc::channel::<WorkerDecodeOutcome>();
        let sequence_cache = Arc::new(ImageSequenceCache::new());
        let decoder = ImageDecodeExecutor::new(decoded_tx, Arc::clone(&sequence_cache));

        Self {
            next_id: AtomicU32::new(1),
            textures: HashMap::new(),
            states: HashMap::new(),
            loads: ImageLoadLifecycle::default(),
            residency: ImageResidencyLifecycle::default(),
            retained_images: RetainedImageSet::default(),
            pending_dimensions: HashMap::new(),
            decoded_rx,
            decoder,
            sequence_cache,
            bind_group_layout,
            accounting: Vec::new(),
            sampler,
            total_memory: 0,
            access_clock: Cell::new(0),
        }
    }

    /// Advance the access clock and return a fresh stamp.
    fn next_access_stamp(&self) -> u64 {
        let stamp = self.access_clock.get() + 1;
        self.access_clock.set(stamp);
        stamp
    }

    fn begin_load(&mut self, load: ImageLoadToken) -> ImageLoadToken {
        let image = load.image();
        self.residency.cancel_retirement(image);
        if let Some(cached) = self.textures.remove(&image) {
            self.total_memory -= cached.memory_size;
            self.accounting
                .push(crate::media_budget::MediaAccounting::Freed {
                    media_type: crate::media_budget::MediaType::Image,
                    id: image.get(),
                });
        }
        self.pending_dimensions.remove(&image);
        self.loads.begin(load)
    }

    fn begin_generated_load(&mut self, image: ImageId) -> ImageLoadToken {
        let load = self.loads.generated_token(image);
        self.begin_load(load)
    }

    fn allocate_image_id(&self) -> ImageId {
        let raw = self
            .next_id
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .expect("image identity space exhausted");
        ImageId::new(raw)
    }

    #[cfg(not(target_family = "wasm"))]
    /// Background decoder thread (pooled version).
    fn decoder_thread_pooled(
        thread_id: usize,
        rx: Arc<Mutex<mpsc::Receiver<DecodeRequest>>>,
        tx: mpsc::Sender<WorkerDecodeOutcome>,
        sequence_cache: Arc<ImageSequenceCache>,
    ) {
        tracing::debug!("Decoder thread {} started", thread_id);
        loop {
            // Lock, receive, unlock immediately to allow other threads to grab work
            let request = {
                let guard = rx.lock().unwrap_or_else(|e| e.into_inner());
                guard.recv()
            };

            match request {
                Ok(request) => {
                    tracing::debug!(
                        "Thread {} decoding image {}",
                        thread_id,
                        request.load.image()
                    );
                    let DecodeRequest {
                        load,
                        source,
                        size,
                        rotation,
                        realization,
                        colors,
                        mask,
                        frame,
                        identity,
                    } = request;
                    // A banded decode reports each band as it lands, which is
                    // the whole point of decoding that way: the display side
                    // can act on a band long before the decode finishes, and
                    // the render thread can write it into the texture. The sink
                    // is a borrow of this closure rather than a returned
                    // collection — a band is worth having before the next one
                    // exists — and it is optional, because a caller that only
                    // wants the image decodes the same pixels without any of
                    // this.
                    let mut publish_band = |decoded: DecodedBand| {
                        let _ = tx.send(WorkerDecodeOutcome::Band { load, decoded });
                    };
                    let sink: Option<&mut dyn FnMut(DecodedBand)> = Some(&mut publish_band);
                    // The chain below answers `Option` and each arm collapses
                    // its own reason, so what the failure is worded *about* is
                    // captured before the bytes are handed over: the source
                    // itself, and whether reading it worked at all.
                    let failure_source = DecodeFailureSource::of(&source);
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
                            &sequence_cache,
                            sequence,
                            sink,
                        ),
                        ImageSource::Data {
                            data,
                            resources,
                            sequence,
                        } => Self::decode_data(
                            data,
                            size,
                            rotation,
                            colors,
                            realization,
                            mask,
                            frame,
                            resources,
                            &sequence_cache,
                            sequence,
                            sink,
                        ),
                        ImageSource::RawArgb32 {
                            data,
                            width,
                            height,
                            stride,
                        } => Self::convert_argb32_to_rgba(&data, width, height, stride)
                            .map(NativePixels::from_raster_tuple)
                            .and_then(|pixels| {
                                pixels.realize_bitmap(size, rotation, realization, mask)
                            }),
                        ImageSource::RawRgb24 {
                            data,
                            width,
                            height,
                            stride,
                        } => Self::convert_rgb24_to_rgba(&data, width, height, stride)
                            .map(NativePixels::from_raster_tuple)
                            .and_then(|pixels| {
                                pixels.realize_bitmap(size, rotation, realization, mask)
                            }),
                    }));

                    let outcome = match result {
                        Ok(Some(pixels)) => {
                            WorkerDecodeOutcome::Ready(Self::decoded_image(load, pixels))
                        }
                        Ok(None) => WorkerDecodeOutcome::Failed {
                            load,
                            diagnostic: failure_source.diagnostic(&identity),
                        },
                        Err(_) => {
                            tracing::warn!(
                                "Decoder thread {} recovered from a panic while decoding image {}",
                                thread_id,
                                load.image()
                            );
                            WorkerDecodeOutcome::Failed {
                                load,
                                diagnostic: failure_source.diagnostic(&identity),
                            }
                        }
                    };
                    let _ = tx.send(outcome);
                }
                Err(_) => {
                    // Channel closed, exit thread
                    tracing::debug!("Decoder thread {} exiting", thread_id);
                    break;
                }
            }
        }
    }

    fn decode_request(
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
                None,
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
                None,
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
        sink: Option<&mut dyn FnMut(DecodedBand)>,
    ) -> Option<DecodedPixels> {
        // The bytes the decode will read, owned here and handed to the decode
        // chain by handle: `std::fs::read` gives the buffer, `EncodedBytes`
        // moves it, and the fallbacks below still have the handle they need.
        // The one clone is the atomic a decode job pays for the attempt to
        // hold the bytes while this copy keeps them for the fallbacks.
        let encoded = std::fs::read(path).ok().map(EncodedBytes::new);
        if let Some(pixels) = encoded.as_ref().and_then(|data| {
            Self::decode_raster_data(
                data.clone(),
                frame,
                sequence_cache,
                sequence,
                size,
                rotation,
                realization,
                mask,
                sink,
            )
        }) {
            return Some(pixels);
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
        data: EncodedBytes,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        colors: ImageColorContext,
        realization: ImageRealization,
        mask: ImageMaskPolicy,
        frame: ImageFrameIndex,
        resources: crate::svg::SvgResourceContext,
        sequence_cache: &ImageSequenceCache,
        sequence: ImageSequenceId,
        sink: Option<&mut dyn FnMut(DecodedBand)>,
    ) -> Option<DecodedPixels> {
        if let Some(pixels) = Self::decode_raster_data(
            data.clone(),
            frame,
            sequence_cache,
            sequence,
            size,
            rotation,
            realization,
            mask,
            sink,
        ) {
            return Some(pixels);
        }
        if !frame.is_first() {
            return None;
        }
        // Fallback: try XPM
        if let Some(result) = crate::xpm::decode_xpm_data(&data) {
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
        if let Some(result) = crate::xbm::decode_xbm_data(&data, fg, bg) {
            return NativePixels::from_raster_tuple(result).realize_bitmap(
                size,
                rotation,
                realization,
                mask,
            );
        }
        // Fallback: try SVG via the shared vector backend.
        Self::decode_svg_data(&data, size, rotation, realization, colors, mask, resources)
    }

    /// Decode a raster source while preserving multi-frame semantics.
    ///
    /// `DynamicImage` intentionally represents one still image and therefore
    /// drops both frame selection and animation metadata. Route animated
    /// formats through `AnimationDecoder` first, then use the still-image path
    /// only for frame zero.
    fn decode_raster_data(
        data: EncodedBytes,
        frame: ImageFrameIndex,
        sequence_cache: &ImageSequenceCache,
        sequence: ImageSequenceId,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        mask_policy: ImageMaskPolicy,
        sink: Option<&mut dyn FnMut(DecodedBand)>,
    ) -> Option<DecodedPixels> {
        match sequence_cache.resolve(sequence, &data, frame) {
            ImageSequenceResolution::Frame(frame) => {
                let (width, height) = frame.dimensions();
                let (rgba, embedded) = frame.into_parts();
                NativePixels {
                    extent: ImageNativeExtent::new(width, height),
                    rgba,
                    embedded,
                }
                .realize_bitmap(size, rotation, realization, mask_policy)
            }
            ImageSequenceResolution::MissingFrame => None,
            ImageSequenceResolution::NotAnimated => {
                Self::decode_still_image(data, size, rotation, realization, mask_policy, sink)
            }
        }
    }

    /// Decode a still raster source, row-wise where the source allows it.
    ///
    /// The two arms of [`BandSource`] are the whole decision: a source with a
    /// row-wise decoder above the size threshold is decoded band by band,
    /// straight into the raster its texture holds, and everything else — every
    /// other format, every source below the threshold, and any row-wise decode
    /// that could not finish — is decoded whole and realized afterwards, which
    /// is what this code did for every source before banding existed.
    fn decode_still_image(
        data: EncodedBytes,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        mask_policy: ImageMaskPolicy,
        sink: Option<&mut dyn FnMut(DecodedBand)>,
    ) -> Option<DecodedPixels> {
        match Self::attempt_banded(
            data.clone(),
            size,
            realization,
            BandFilling::of(rotation, mask_policy),
            sink,
        ) {
            BandedAttempt::Complete(pixels) => Some(pixels),
            BandedAttempt::NotBandable | BandedAttempt::Abandoned => {
                Self::decode_whole(data)?.realize_bitmap(size, rotation, realization, mask_policy)
            }
        }
    }

    /// One attempt at a banded decode.
    ///
    /// The whole attempt resolves to one raster, from the *header's* extent
    /// through the same `realized_geometry` the finished upload resolves from
    /// the decoded one, and the source writes its rows into it as it reads
    /// them. Step 1 pins those two extents equal, so the raster is the raster
    /// the finished upload would write; if a header ever disagreed with its own
    /// pixels, the source's own row count would differ from the height the
    /// target was built for and the attempt is abandoned rather than published.
    fn attempt_banded(
        data: EncodedBytes,
        size: ImageSizeSpec,
        realization: ImageRealization,
        filling: BandFilling,
        mut sink: Option<&mut dyn FnMut(DecodedBand)>,
    ) -> BandedAttempt {
        // A band has a destination only where the texture can be built up from
        // the top: an unrotated realization whose mask policy leaves the pixels
        // alone. Where it has none — a turn lands a band's rows in the raster's
        // columns, and a mask that rewrites pixels needs all of them first —
        // there is nothing to write a band into as it arrives, and the only way
        // to hold the rows until the end is the native-size buffer this step
        // exists to remove. Such a source takes the whole-image path, which is
        // what it took before banding and what it still looks like on screen:
        // empty until the decode completes, then whole.
        if filling == BandFilling::Deferred {
            return BandedAttempt::NotBandable;
        }
        let mut source = match BandSource::open(data, size, realization) {
            BandSource::Banded(source) => source,
            BandSource::Whole => return BandedAttempt::NotBandable,
        };
        #[cfg(test)]
        let mut published = 0_u32;
        loop {
            match source.next_band() {
                BandStep::Band(band) => {
                    if let Some(sink) = sink.as_deref_mut() {
                        sink(band);
                    }
                    #[cfg(test)]
                    {
                        published += 1;
                        let abandon_after = ABANDON_AFTER_BANDS.load(Ordering::Relaxed);
                        if abandon_after != 0 && published >= abandon_after {
                            return BandedAttempt::Abandoned;
                        }
                    }
                }
                // A completed source is the only one that yields pixels, so a
                // prefix cannot escape as an image.
                BandStep::Done => {
                    let Some(pixels) = source.into_raster() else {
                        return BandedAttempt::Abandoned;
                    };
                    let geometry = realized_geometry(pixels.native(), size, realization);
                    debug_assert_eq!(
                        geometry.raster(),
                        pixels.raster(),
                        "a decode fills the raster its realization resolves"
                    );
                    let mask = pixels.mask();
                    return BandedAttempt::Complete(DecodedPixels {
                        geometry,
                        rgba: pixels.into_rgba(),
                        mask,
                        embedded: ImageEmbeddedMetadata::default(),
                    });
                }
                BandStep::Failed => return BandedAttempt::Abandoned,
            }
        }
    }

    /// The whole image in one decode: the path every source took before
    /// banding, and the one a banded attempt falls back to.
    fn decode_whole(data: EncodedBytes) -> Option<NativePixels> {
        Self::process_image(image::load_from_memory(&data).ok()?)
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
            EncodedBytes::copy_of(data),
            ImageSizeSpec::default(),
            ImageRotation::None,
            ImageColorContext::default(),
            ImageRealization::default(),
            ImageMaskPolicy::Preserve,
            frame,
            crate::svg::SvgResourceContext::Isolated,
            &ImageSequenceCache::new(),
            ImageSequenceId::new(1).expect("non-zero test sequence"),
            None,
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
            EncodedBytes::copy_of(data),
            size,
            rotation,
            ImageColorContext::from_pixels(fg_bg.0, fg_bg.1),
            realization,
            ImageMaskPolicy::Preserve,
            ImageFrameIndex::default(),
            crate::svg::SvgResourceContext::Isolated,
            &ImageSequenceCache::new(),
            ImageSequenceId::new(1).expect("non-zero test sequence"),
            None,
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

    /// Get bind group layout
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Get sampler (for sharing with video cache)
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Query image file dimensions.
    ///
    /// Raster formats read only their header; SVG requires document parsing.
    pub fn query_file_dimensions(path: &str) -> Option<ImageNativeExtent> {
        Self::query_file_intrinsic_extent(path).map(Self::unscaled_dimensions)
    }

    fn query_file_intrinsic_extent(path: &str) -> Option<ImageIntrinsicExtent> {
        if let Some(extent) = crate::image_probe::probe_intrinsic_extent(
            crate::image_probe::ImageProbeSource::File(path),
        ) {
            return Some(extent);
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

    fn query_data_intrinsic_extent(data: &[u8]) -> Option<ImageIntrinsicExtent> {
        crate::image_probe::probe_intrinsic_extent(crate::image_probe::ImageProbeSource::Data(data))
            .or_else(|| crate::svg::query_intrinsic_extent(data))
    }

    /// Preserve the pixel-query contract: return a bounding integer extent.
    /// Pending-image sizing must instead retain the fractional source extent.
    fn unscaled_dimensions(intrinsic: ImageIntrinsicExtent) -> ImageNativeExtent {
        let (width, height) = intrinsic.dimensions();
        ImageNativeExtent::new(width.ceil() as u32, height.ceil() as u32)
    }

    /// Load image from file (async)
    /// Returns image ID immediately, texture loads in background
    pub fn load_file(
        &mut self,
        path: &str,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        colors: ImageColorContext,
        raster_scale: f32,
    ) -> ImageId {
        let image = self.allocate_image_id();
        let load = self.loads.generated_token(image);
        self.load_file_with_id(
            load,
            path,
            size,
            rotation,
            ImageRealization::with_device_scale(1.0, raster_scale),
            colors,
            ImageMaskPolicy::default(),
            ImageFrameIndex::default(),
            ImageSequenceId::new(u64::from(image.get()))
                .expect("allocated image identity is non-zero"),
            ImageLoadIdentity::unspecified(),
        );
        image
    }

    /// Load image from data with a pre-allocated ID (for threaded mode)
    ///
    /// The bytes arrive as the handle the request already holds, and are moved
    /// into the decode queue: the caller's buffer is the decode's buffer, which
    /// is what the catalog's own key to the same request points at too.
    pub fn load_data_with_id(
        &mut self,
        load: ImageLoadToken,
        data: EncodedBytes,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        colors: ImageColorContext,
        mask: ImageMaskPolicy,
        frame: ImageFrameIndex,
        sequence: ImageSequenceId,
        resources: crate::svg::SvgResourceContext,
        identity: ImageLoadIdentity,
    ) {
        let load = self.begin_load(load);
        let image = load.image();
        // Query dimensions for the pending-image placeholder.
        if let Some(dims) = Self::query_data_intrinsic_extent(&data) {
            self.pending_dimensions.insert(
                image,
                realization.resolve_geometry(size, dims, rotation).layout(),
            );
        }

        // Submit to the target-selected decoder.
        self.states.insert(image, ImageState::Pending);
        self.decoder.submit(DecodeRequest {
            load,
            source: ImageSource::Data {
                data,
                resources,
                sequence,
            },
            size,
            rotation,
            realization,
            colors,
            mask,
            frame,
            identity,
        });
    }

    /// Load image from file with a pre-allocated ID (for threaded mode)
    /// This allows the calling code to allocate the ID before sending a command.
    pub fn load_file_with_id(
        &mut self,
        load: ImageLoadToken,
        path: &str,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        colors: ImageColorContext,
        mask: ImageMaskPolicy,
        frame: ImageFrameIndex,
        sequence: ImageSequenceId,
        identity: ImageLoadIdentity,
    ) {
        let load = self.begin_load(load);
        let image = load.image();
        // Query dimensions for the pending-image placeholder.
        if let Some(dims) = Self::query_file_intrinsic_extent(path) {
            self.pending_dimensions.insert(
                image,
                realization.resolve_geometry(size, dims, rotation).layout(),
            );
        }

        // Submit to the target-selected decoder.
        self.states.insert(image, ImageState::Pending);
        self.decoder.submit(DecodeRequest {
            load,
            source: ImageSource::File {
                path: path.to_string(),
                sequence,
            },
            size,
            rotation,
            realization,
            colors,
            mask,
            frame,
            identity,
        });
    }

    /// Allocate the next available image ID without loading anything.
    /// Used by threaded mode to pre-allocate IDs before sending commands.
    pub fn allocate_id(&self) -> ImageId {
        self.allocate_image_id()
    }

    pub fn retire_sequence(&self, retirement: ImageSequenceRetirement) {
        self.sequence_cache.retire(retirement);
    }

    pub(crate) fn memory_usage(&self) -> ImageCacheUsage {
        ImageCacheUsage::new(
            u64::try_from(self.total_memory).unwrap_or(u64::MAX),
            u64::try_from(self.sequence_cache.resident_bytes()).unwrap_or(u64::MAX),
        )
    }

    /// Load image from data (async)
    ///
    /// A caller here has a slice and no buffer of its own to hand over, so the
    /// bytes are materialized into the handle the decode will read.
    pub fn load_data(
        &mut self,
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        colors: ImageColorContext,
        raster_scale: f32,
    ) -> ImageId {
        let image = self.allocate_image_id();
        let load = self.begin_generated_load(image);
        let realization = ImageRealization::with_device_scale(1.0, raster_scale);

        // Query dimensions for the pending-image placeholder.
        if let Some(dims) = Self::query_data_intrinsic_extent(data) {
            self.pending_dimensions.insert(
                image,
                realization.resolve_geometry(size, dims, rotation).layout(),
            );
        }

        // Submit to the target-selected decoder.
        self.states.insert(image, ImageState::Pending);
        self.decoder.submit(DecodeRequest {
            load,
            source: ImageSource::Data {
                data: EncodedBytes::copy_of(data),
                resources: crate::svg::SvgResourceContext::Isolated,
                sequence: ImageSequenceId::new(u64::from(image.get()))
                    .expect("allocated image identity is non-zero"),
            },
            size,
            rotation,
            realization,
            colors,
            mask: ImageMaskPolicy::Preserve,
            frame: ImageFrameIndex::default(),
            identity: ImageLoadIdentity::unspecified(),
        });

        image
    }

    /// Load image from raw ARGB32 pixel data (async)
    /// Format: A,R,G,B byte order, 4 bytes per pixel
    /// Stride is the number of bytes per row (may include padding)
    pub fn load_raw_argb32(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        size: ImageSizeSpec,
        rotation: ImageRotation,
    ) -> ImageId {
        let image = self.allocate_image_id();
        let load = self.begin_generated_load(image);
        let realization = ImageRealization::default();

        // Store pending dimensions immediately (we know the exact size)
        self.pending_dimensions.insert(
            image,
            realization
                .resolve_geometry(size, ImageNativeExtent::new(width, height), rotation)
                .layout(),
        );

        // Submit to the target-selected decoder.
        self.states.insert(image, ImageState::Pending);
        self.decoder.submit(DecodeRequest {
            load,
            source: ImageSource::RawArgb32 {
                data: data.to_vec(),
                width,
                height,
                stride,
            },
            size,
            rotation,
            realization,
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
            identity: ImageLoadIdentity::unspecified(),
        });

        image
    }

    /// Load image from raw RGB24 pixel data (async)
    /// Format: R,G,B byte order, 3 bytes per pixel
    /// Stride is the number of bytes per row (may include padding)
    pub fn load_raw_rgb24(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        size: ImageSizeSpec,
        rotation: ImageRotation,
    ) -> ImageId {
        let image = self.allocate_image_id();
        let load = self.begin_generated_load(image);
        let realization = ImageRealization::default();

        // Store pending dimensions immediately (we know the exact size)
        self.pending_dimensions.insert(
            image,
            realization
                .resolve_geometry(size, ImageNativeExtent::new(width, height), rotation)
                .layout(),
        );

        // Submit to the target-selected decoder.
        self.states.insert(image, ImageState::Pending);
        self.decoder.submit(DecodeRequest {
            load,
            source: ImageSource::RawRgb24 {
                data: data.to_vec(),
                width,
                height,
                stride,
            },
            size,
            rotation,
            realization,
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
            identity: ImageLoadIdentity::unspecified(),
        });

        image
    }

    /// Load image from raw ARGB32 pixel data with a pre-allocated ID (for threaded mode)
    pub fn load_raw_argb32_with_id(
        &mut self,
        load: ImageLoadToken,
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) {
        let load = self.begin_load(load);
        let image = load.image();
        self.pending_dimensions
            .insert(image, ImageLayoutExtent::new(width, height));
        self.states.insert(image, ImageState::Pending);
        self.decoder.submit(DecodeRequest {
            rotation: ImageRotation::None,
            load,
            source: ImageSource::RawArgb32 {
                data: data.to_vec(),
                width,
                height,
                stride,
            },
            size: ImageSizeSpec::default(),
            realization: ImageRealization::default(),
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
            identity: ImageLoadIdentity::unspecified(),
        });
    }

    /// Load image from raw RGB24 pixel data with a pre-allocated ID (for threaded mode)
    pub fn load_raw_rgb24_with_id(
        &mut self,
        load: ImageLoadToken,
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
    ) {
        let load = self.begin_load(load);
        let image = load.image();
        self.pending_dimensions
            .insert(image, ImageLayoutExtent::new(width, height));
        self.states.insert(image, ImageState::Pending);
        self.decoder.submit(DecodeRequest {
            rotation: ImageRotation::None,
            load,
            source: ImageSource::RawRgb24 {
                data: data.to_vec(),
                width,
                height,
                stride,
            },
            size: ImageSizeSpec::default(),
            realization: ImageRealization::default(),
            colors: ImageColorContext::default(),
            mask: ImageMaskPolicy::default(),
            frame: ImageFrameIndex::default(),
            identity: ImageLoadIdentity::unspecified(),
        });
    }

    /// Import image from DMA-BUF (zero-copy if supported)
    #[cfg(target_os = "linux")]
    pub fn import_dmabuf(
        &mut self,
        dmabuf: DmaBufBuffer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> ImageId {
        let image = self.allocate_image_id();
        let (width, height) = dmabuf.dimensions();

        // Try zero-copy import
        if let Some(texture) = dmabuf.to_wgpu_texture(device, queue) {
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("DMA-BUF Image Bind Group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });

            let memory_size = (width * height * 4) as usize;
            self.total_memory += memory_size;
            self.accounting
                .push(crate::media_budget::MediaAccounting::Registered {
                    media_type: crate::media_budget::MediaType::Image,
                    id: image.get(),
                    size_bytes: memory_size,
                });

            self.textures.insert(
                image,
                CachedImage {
                    texture,
                    view,
                    bind_group,
                    raster: ImageRasterExtent::new(width, height),
                    // The imported buffer is the whole image already.
                    filled: FilledRows::complete(ImageRasterExtent::new(width, height)),
                    metadata: None,
                    memory_size,
                    last_access: Cell::new(self.next_access_stamp()),
                },
            );
            self.states.insert(image, ImageState::Ready);

            tracing::info!(
                "Imported DMA-BUF image {} ({}x{}) zero-copy",
                image,
                width,
                height
            );
        } else {
            self.states
                .insert(image, ImageState::Failed("DMA-BUF import failed".into()));
            tracing::warn!("DMA-BUF import failed for image {}", image);
        }

        image
    }

    /// Process pending decoded images (call each frame)
    pub fn process_pending(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<ImageCacheEvent> {
        let mut events = Vec::new();
        // Drain decoded images from channel
        while let Ok(outcome) = self.decoded_rx.try_recv() {
            let Some(outcome) = self.loads.take_current(outcome) else {
                continue;
            };
            match outcome {
                WorkerDecodeOutcome::Band { load, decoded } => {
                    let (source, placed) = decoded.into_parts();
                    tracing::debug!(
                        "Image {} decoded rows {}..{} into texture rows {}..{}",
                        load.image(),
                        source.start(),
                        source.end(),
                        placed.placement().rows().start(),
                        placed.placement().rows().end(),
                    );
                    self.upload_band(device, queue, load, &placed);
                    events.push(ImageCacheEvent::Band { load, rows: source });
                }
                WorkerDecodeOutcome::Ready(decoded) => {
                    events.push(ImageCacheEvent::Ready {
                        load: decoded.load,
                        metadata: decoded.metadata.clone(),
                    });
                    self.upload_texture(device, queue, decoded);
                }
                WorkerDecodeOutcome::Failed { load, diagnostic } => {
                    // A failed decode keeps no texture: a band may already have
                    // created one, and it holds rows of an image that will never
                    // arrive. Leaving it would draw them forever.
                    self.release(load.image());
                    self.states
                        .insert(load.image(), ImageState::Failed(diagnostic.message()));
                    self.pending_dimensions.remove(&load.image());
                    events.push(ImageCacheEvent::Failed {
                        load,
                        error: diagnostic,
                    });
                }
            }
        }

        // Evict if over memory limit
        self.evict_if_needed(&mut events);
        events
    }

    /// The texture, view and bind group every image in this cache is drawn
    /// through.
    ///
    /// One constructor for the whole cache: a banded decode creates its texture
    /// here and the finished upload gets the same allocation, so the two cannot
    /// disagree about format, usage or size.
    fn create_image_texture(
        &self,
        device: &wgpu::Device,
        raster: ImageRasterExtent,
    ) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup) {
        let (width, height) = raster.dimensions();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Image Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            // `COPY_SRC` costs nothing to carry and is what makes these
            // textures' contents observable: a band-filled texture ending
            // byte-for-byte as the whole-image path's is a claim only a readback
            // can settle.
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Image Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        (texture, view, bind_group)
    }

    /// Write one band's pixels into the image's texture.
    ///
    /// The first band creates the texture, and the image is drawable from that
    /// moment on: the quad is step 1's and does not move, the texture only grows
    /// downward, and what has not arrived yet is simply not drawn
    /// (`FilledRows::clip_span`). Every later band appends to it.
    ///
    /// A band that does not continue the prefix is dropped rather than written.
    /// Rows written out of order, or twice, would leave the texture holding
    /// pixels from two different scalings and nothing downstream could tell; a
    /// band naming another raster belongs to other pixels entirely. Both are
    /// states this refuses to enter, and the whole-image upload at the end is
    /// what makes the refusal invisible.
    ///
    /// Writing in place, under a compositor that samples this texture, is a
    /// deliberate choice over staging into a second texture and swapping: a
    /// shadow would double the peak allocation for the largest texture this
    /// cache holds (64 MiB at the 4096 limit) while the decode is already
    /// holding the whole native image, and the swap would have to rebind a
    /// texture the frames already presented are sampling. The state a partial
    /// write describes is a prefix, which [`FilledRows`] expresses exactly; the
    /// state a swap would need — two textures at two fill levels, and a rule for
    /// when one may become the other — is the one that would have needed
    /// policing.
    fn upload_band(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        load: ImageLoadToken,
        placed: &RasterBand,
    ) {
        let image = load.image();
        let placement = placed.placement();
        let raster = placement.raster();
        if !self.textures.contains_key(&image) {
            let (texture, view, bind_group) = self.create_image_texture(device, raster);
            let memory_size = (raster.width() * raster.height() * 4) as usize;
            self.total_memory += memory_size;
            self.accounting
                .push(crate::media_budget::MediaAccounting::Registered {
                    media_type: crate::media_budget::MediaType::Image,
                    id: image.get(),
                    size_bytes: memory_size,
                });
            self.textures.insert(
                image,
                CachedImage {
                    texture,
                    view,
                    bind_group,
                    raster,
                    filled: FilledRows::empty(raster),
                    metadata: None,
                    memory_size,
                    last_access: Cell::new(self.next_access_stamp()),
                },
            );
        }
        let Some(cached) = self.textures.get_mut(&image) else {
            return;
        };
        if cached.raster != raster {
            tracing::warn!(
                "Image {} band for a {}x{} raster into a {}x{} texture",
                image,
                raster.width(),
                raster.height(),
                cached.raster.width(),
                cached.raster.height()
            );
            return;
        }
        let rows = placement.rows();
        let Some(filled) = cached.filled.extend(rows) else {
            tracing::debug!(
                "Image {} band for rows {}..{} does not continue {} written",
                image,
                rows.start(),
                rows.end(),
                cached.filled.filled()
            );
            return;
        };
        let (raster_width, raster_height) = raster.dimensions();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &cached.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: rows.start(),
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            placed.pixels(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(raster_width * 4),
                rows_per_image: Some(rows.len().get()),
            },
            wgpu::Extent3d {
                width: raster_width,
                height: rows.len().get(),
                depth_or_array_layers: 1,
            },
        );
        cached.filled = filled;
        tracing::debug!(
            "Image {} filled rows {}..{} of {} (raster {}x{})",
            image,
            rows.start(),
            rows.end(),
            raster_height,
            raster_width,
            raster_height
        );
    }

    /// Upload decoded image to GPU texture
    fn upload_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        decoded: DecodedImage,
    ) {
        let image = decoded.load.image();
        let raster = decoded.geometry.raster();
        let (raster_width, raster_height) = raster.dimensions();
        // A banded decode may have already built this image's texture, at
        // exactly this raster (`realized_geometry` is the one place either side
        // resolves it). Keep it: the write below covers every texel of it, so
        // the finished image is the whole-image path's bytes — written by the
        // same call, to the same shape of texture — and the bind group the
        // presented frames already sample never changes under them. A texture of
        // any other raster is not this image's and is released rather than
        // written into.
        let existing = self.textures.remove(&image);
        let (texture, view, bind_group, memory_size) = match existing {
            Some(cached) if cached.raster == raster => (
                cached.texture,
                cached.view,
                cached.bind_group,
                cached.memory_size,
            ),
            other => {
                if let Some(cached) = other {
                    self.total_memory -= cached.memory_size;
                    self.accounting
                        .push(crate::media_budget::MediaAccounting::Freed {
                            media_type: crate::media_budget::MediaType::Image,
                            id: image.get(),
                        });
                }
                let (texture, view, bind_group) = self.create_image_texture(device, raster);
                let memory_size = (raster_width * raster_height * 4) as usize;
                self.total_memory += memory_size;
                self.accounting
                    .push(crate::media_budget::MediaAccounting::Registered {
                        media_type: crate::media_budget::MediaType::Image,
                        id: image.get(),
                        size_bytes: memory_size,
                    });
                (texture, view, bind_group, memory_size)
            }
        };

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &decoded.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(raster_width * 4),
                rows_per_image: Some(raster_height),
            },
            wgpu::Extent3d {
                width: raster_width,
                height: raster_height,
                depth_or_array_layers: 1,
            },
        );

        let layout = decoded.metadata.layout;

        self.textures.insert(
            image,
            CachedImage {
                texture,
                view,
                bind_group,
                raster,
                // The write above covered the whole texture, whether or not a
                // banded decode had got part of the way through it.
                filled: FilledRows::complete(raster),
                metadata: Some(decoded.metadata),
                memory_size,
                last_access: Cell::new(self.next_access_stamp()),
            },
        );

        self.states.insert(image, ImageState::Ready);
        self.pending_dimensions.remove(&image);

        tracing::debug!(
            "Uploaded image {} (layout {}x{}, raster {}x{}, {}KB)",
            image,
            layout.width(),
            layout.height(),
            raster_width,
            raster_height,
            memory_size / 1024
        );
    }

    /// Evict least-recently-used textures until under the memory limit.
    fn evict_if_needed(&mut self, events: &mut Vec<ImageCacheEvent>) {
        while self.total_memory > MAX_CACHE_MEMORY && !self.textures.is_empty() {
            let victim = lru_unpresented_victim(
                self.textures
                    .iter()
                    // A half-filled texture belongs to a decode that is still
                    // running: the load owns it, and taking it away would leave
                    // the rows that follow with nowhere to go — the next band
                    // continues a prefix that no longer exists. It becomes
                    // evictable the moment the decode ends, one way or another.
                    .filter(|(_, cached)| cached.filled.is_complete())
                    .map(|(&id, cached)| (id, cached.last_access.get())),
                &self.retained_images,
            );
            if let Some(id) = victim
                && let Some(cached) = self.textures.remove(&id)
            {
                self.total_memory -= cached.memory_size;
                self.states.remove(&id);
                self.accounting
                    .push(crate::media_budget::MediaAccounting::Freed {
                        media_type: crate::media_budget::MediaType::Image,
                        id: id.get(),
                    });
                events.push(ImageCacheEvent::Evicted { image: id });
                tracing::debug!(
                    "Evicted image {} to free {}KB",
                    id,
                    cached.memory_size / 1024
                );
            } else {
                // The cache may temporarily exceed its budget while every
                // candidate is owned by a retained presentation.
                break;
            }
        }
    }

    /// Get cached image if ready. Refreshes the entry's LRU access stamp.
    pub fn get(&self, image: ImageId) -> Option<&CachedImage> {
        let cached = self.textures.get(&image)?;
        cached.last_access.set(self.next_access_stamp());
        Some(cached)
    }

    /// Get image dimensions (pending or loaded)
    pub fn get_dimensions(&self, image: ImageId) -> Option<ImageLayoutExtent> {
        // A texture with metadata is a finished decode, and its layout is the
        // answer. A texture without one is the prefix a banded decode has
        // uploaded — its rows say nothing about how big the image is — so the
        // layout step 1 resolved from the header is the answer until then.
        if let Some(layout) = self
            .textures
            .get(&image)
            .and_then(|cached| cached.metadata.as_ref())
            .map(|metadata| metadata.layout)
        {
            return Some(layout);
        }
        if let Some(layout) = self.pending_dimensions.get(&image) {
            return Some(*layout);
        }
        self.textures
            .get(&image)
            .map(|cached| ImageLayoutExtent::new(cached.raster.width(), cached.raster.height()))
    }

    /// Get image state
    pub fn get_state(&self, image: ImageId) -> Option<&ImageState> {
        self.states.get(&image)
    }

    /// Check if image is ready
    pub fn is_ready(&self, image: ImageId) -> bool {
        matches!(self.states.get(&image), Some(ImageState::Ready))
    }

    /// Whether async decode work still needs the render thread to poll its result channel.
    pub fn has_pending(&self) -> bool {
        self.states
            .values()
            .any(|state| matches!(state, ImageState::Pending | ImageState::Decoding))
    }

    /// Logically retire an image. Its texture remains drawable until no
    /// accepted or queued presentation references the identity.
    pub fn retire(&mut self, image: ImageId) {
        self.loads.free(image);
        self.pending_dimensions.remove(&image);
        if self.textures.contains_key(&image) {
            self.residency.request_retirement(image);
            self.states.insert(image, ImageState::Retiring);
        } else {
            self.states.remove(&image);
        }
    }

    /// Publish the complete presentation lifetime fence and release every
    /// retirement which has crossed it.
    pub fn synchronize_retained_images(&mut self, retained: RetainedImageSet) {
        self.retained_images = retained;
        for image in self.residency.take_releasable(&self.retained_images) {
            self.release(image);
        }
    }

    fn release(&mut self, image: ImageId) {
        if let Some(cached) = self.textures.remove(&image) {
            self.total_memory -= cached.memory_size;
            self.accounting
                .push(crate::media_budget::MediaAccounting::Freed {
                    media_type: crate::media_budget::MediaType::Image,
                    id: image.get(),
                });
        }
        self.states.remove(&image);
    }

    /// Drain budget accounting events accumulated since the last call.
    pub fn drain_accounting(&mut self) -> Vec<crate::media_budget::MediaAccounting> {
        std::mem::take(&mut self.accounting)
    }

    /// Clear entire cache
    pub fn clear(&mut self) {
        self.loads.clear();
        self.residency.clear();
        self.retained_images = RetainedImageSet::default();
        self.textures.clear();
        self.states.clear();
        self.pending_dimensions.clear();
        self.total_memory = 0;
    }
}

#[cfg(test)]
mod tests;
