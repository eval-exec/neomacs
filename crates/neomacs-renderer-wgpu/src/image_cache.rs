//! Async image loading and caching for wgpu renderer
//!
//! Provides non-blocking image loading:
//! - Dimension queries for pending-image placeholders
//! - Background decoding in thread pool
//! - GPU texture upload when ready
//! - LRU cache with memory limits

use neomacs_display_protocol::{
    ImageCacheUsage, ImageColorContext, ImageEmbeddedMetadata, ImageFrameIndex, ImageHeuristicMask,
    ImageId, ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken, ImageMaskKind, ImageMaskPolicy,
    ImageNativeExtent, ImageRasterExtent, ImageRealization, ImageReportedExtent, ImageRotation,
    ImageSequenceId, ImageSequenceRetirement, ImageSizeSpec, ResolvedImageGeometry,
    RetainedImageSet,
};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use crate::image_sequence::{ImageSequenceCache, ImageSequenceResolution};

#[cfg(target_os = "linux")]
use crate::external_buffer::DmaBufBuffer;

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

/// Maximum total cache memory in bytes (64MB)
const MAX_CACHE_MEMORY: usize = 64 * 1024 * 1024;

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
struct ImageDecoderPoolSize(NonZeroUsize);

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

/// Cached image with GPU texture
pub struct CachedImage {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    /// Uploaded texture dimensions in physical device pixels.
    pub raster: ImageRasterExtent,
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

enum WorkerDecodeOutcome {
    Ready(DecodedImage),
    Failed(ImageLoadToken),
}

impl WorkerDecodeOutcome {
    fn load(&self) -> ImageLoadToken {
        match self {
            Self::Ready(decoded) => decoded.load,
            Self::Failed(load) => *load,
        }
    }
}

/// A renderer image-cache lifecycle event. Callers must handle eviction as
/// well as terminal decode results so external catalogs cannot retain stale
/// residency state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageCacheEvent {
    Ready {
        load: ImageLoadToken,
        metadata: ImageMetadata,
    },
    Failed {
        load: ImageLoadToken,
        error: String,
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
        if self.active.get(&load.image()) != Some(&load.attempt()) {
            return false;
        }
        self.active.remove(&load.image());
        true
    }

    fn take_current(&mut self, outcome: WorkerDecodeOutcome) -> Option<WorkerDecodeOutcome> {
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

/// Async image cache
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
    /// Channel to send decode requests
    decode_tx: mpsc::Sender<DecodeRequest>,
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
}

/// Image source
enum ImageSource {
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

        // Create channels for async decoding
        let (decode_tx, decode_rx) = mpsc::channel::<DecodeRequest>();
        let (decoded_tx, decoded_rx) = mpsc::channel::<WorkerDecodeOutcome>();
        let sequence_cache = Arc::new(ImageSequenceCache::new());

        // Wrap receiver in Arc<Mutex> for sharing across threads
        let decode_rx = Arc::new(Mutex::new(decode_rx));

        let pool_size = ImageDecoderPoolSize::detected();
        tracing::info!("Starting {} image decoder threads", pool_size.get());
        for i in 0..pool_size.get() {
            let rx = Arc::clone(&decode_rx);
            let tx = decoded_tx.clone();
            let sequence_cache = Arc::clone(&sequence_cache);
            thread::spawn(move || {
                Self::decoder_thread_pooled(i, rx, tx, sequence_cache);
            });
        }

        Self {
            next_id: AtomicU32::new(1),
            textures: HashMap::new(),
            states: HashMap::new(),
            loads: ImageLoadLifecycle::default(),
            residency: ImageResidencyLifecycle::default(),
            retained_images: RetainedImageSet::default(),
            pending_dimensions: HashMap::new(),
            decoded_rx,
            decode_tx,
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

    /// Background decoder thread (pooled version)
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
                            &sequence_cache,
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
                            &sequence_cache,
                            sequence,
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
                        Ok(None) => WorkerDecodeOutcome::Failed(load),
                        Err(_) => {
                            tracing::warn!(
                                "Decoder thread {} recovered from a panic while decoding image {}",
                                thread_id,
                                load.image()
                            );
                            WorkerDecodeOutcome::Failed(load)
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
        let bg = colors.background().rgba8();
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
        let bg = colors.background().rgba8();
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
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        // Use image crate's dimension reader (reads header only)
        if let Ok(dims) = image::ImageReader::new(reader)
            .with_guessed_format()
            .ok()?
            .into_dimensions()
        {
            return Some(ImageNativeExtent::new(dims.0, dims.1));
        }

        // Fallback: try SVG.
        let data = std::fs::read(path).ok()?;
        Self::query_svg_dimensions(&data)
    }

    /// Query image data dimensions.
    ///
    /// Raster formats read only their header; SVG requires document parsing.
    pub fn query_data_dimensions(data: &[u8]) -> Option<ImageNativeExtent> {
        let cursor = std::io::Cursor::new(data);
        if let Ok(dims) = image::ImageReader::new(BufReader::new(cursor))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
        {
            return Some(ImageNativeExtent::new(dims.0, dims.1));
        }

        // Fallback: try XPM header
        if let Some((w, h)) = crate::xpm::query_xpm_dimensions(data) {
            return Some(ImageNativeExtent::new(w, h));
        }

        // Fallback: try XBM header
        if let Some((w, h)) = crate::xbm::query_xbm_dimensions(data) {
            return Some(ImageNativeExtent::new(w, h));
        }

        // Fallback: try SVG.
        Self::query_svg_dimensions(data)
    }

    /// Query SVG dimensions without full rendering
    fn query_svg_dimensions(data: &[u8]) -> Option<ImageNativeExtent> {
        let (width, height) = crate::svg::query_dimensions(data)?;
        Some(ImageNativeExtent::new(width, height))
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
        );
        image
    }

    /// Load image from data with a pre-allocated ID (for threaded mode)
    pub fn load_data_with_id(
        &mut self,
        load: ImageLoadToken,
        data: &[u8],
        size: ImageSizeSpec,
        rotation: ImageRotation,
        realization: ImageRealization,
        colors: ImageColorContext,
        mask: ImageMaskPolicy,
        frame: ImageFrameIndex,
        sequence: ImageSequenceId,
        resources: crate::svg::SvgResourceContext,
    ) {
        let load = self.begin_load(load);
        let image = load.image();
        // Query dimensions for the pending-image placeholder.
        if let Some(dims) = Self::query_data_dimensions(data) {
            self.pending_dimensions.insert(
                image,
                realization.resolve_geometry(size, dims, rotation).layout(),
            );
        }

        // Queue for async decode
        self.states.insert(image, ImageState::Pending);
        let _ = self.decode_tx.send(DecodeRequest {
            load,
            source: ImageSource::Data {
                data: data.to_vec(),
                resources,
                sequence,
            },
            size,
            rotation,
            realization,
            colors,
            mask,
            frame,
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
    ) {
        let load = self.begin_load(load);
        let image = load.image();
        // Query dimensions for the pending-image placeholder.
        if let Some(dims) = Self::query_file_dimensions(path) {
            self.pending_dimensions.insert(
                image,
                realization.resolve_geometry(size, dims, rotation).layout(),
            );
        }

        // Queue for async decode
        self.states.insert(image, ImageState::Pending);
        let _ = self.decode_tx.send(DecodeRequest {
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
        if let Some(dims) = Self::query_data_dimensions(data) {
            self.pending_dimensions.insert(
                image,
                realization.resolve_geometry(size, dims, rotation).layout(),
            );
        }

        // Queue for async decode
        self.states.insert(image, ImageState::Pending);
        let _ = self.decode_tx.send(DecodeRequest {
            load,
            source: ImageSource::Data {
                data: data.to_vec(),
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

        // Queue for async conversion
        self.states.insert(image, ImageState::Pending);
        let _ = self.decode_tx.send(DecodeRequest {
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

        // Queue for async conversion
        self.states.insert(image, ImageState::Pending);
        let _ = self.decode_tx.send(DecodeRequest {
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
        let _ = self.decode_tx.send(DecodeRequest {
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
        let _ = self.decode_tx.send(DecodeRequest {
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
                WorkerDecodeOutcome::Ready(decoded) => {
                    events.push(ImageCacheEvent::Ready {
                        load: decoded.load,
                        metadata: decoded.metadata.clone(),
                    });
                    self.upload_texture(device, queue, decoded);
                }
                WorkerDecodeOutcome::Failed(load) => {
                    let error = "image decode failed".to_owned();
                    self.states
                        .insert(load.image(), ImageState::Failed(error.clone()));
                    self.pending_dimensions.remove(&load.image());
                    events.push(ImageCacheEvent::Failed { load, error });
                }
            }
        }

        // Evict if over memory limit
        self.evict_if_needed(&mut events);
        events
    }

    /// Upload decoded image to GPU texture
    fn upload_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        decoded: DecodedImage,
    ) {
        let raster = decoded.geometry.raster();
        let (raster_width, raster_height) = raster.dimensions();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Image Texture"),
            size: wgpu::Extent3d {
                width: raster_width,
                height: raster_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

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

        let memory_size = (raster_width * raster_height * 4) as usize;
        self.total_memory += memory_size;
        self.accounting
            .push(crate::media_budget::MediaAccounting::Registered {
                media_type: crate::media_budget::MediaType::Image,
                id: decoded.load.image().get(),
                size_bytes: memory_size,
            });

        let layout = decoded.metadata.layout;

        self.textures.insert(
            decoded.load.image(),
            CachedImage {
                texture,
                view,
                bind_group,
                raster,
                metadata: Some(decoded.metadata),
                memory_size,
                last_access: Cell::new(self.next_access_stamp()),
            },
        );

        self.states.insert(decoded.load.image(), ImageState::Ready);
        self.pending_dimensions.remove(&decoded.load.image());

        tracing::debug!(
            "Uploaded image {} (layout {}x{}, raster {}x{}, {}KB)",
            decoded.load.image(),
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
        // Check loaded textures first
        if let Some(cached) = self.textures.get(&image) {
            return Some(
                cached
                    .metadata
                    .as_ref()
                    .map(|metadata| metadata.layout)
                    .unwrap_or_else(|| {
                        ImageLayoutExtent::new(cached.raster.width(), cached.raster.height())
                    }),
            );
        }
        // Check pending dimensions
        self.pending_dimensions.get(&image).copied()
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
#[path = "image_cache_test.rs"]
mod tests;
