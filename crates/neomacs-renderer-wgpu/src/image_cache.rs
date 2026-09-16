//! Target-aware image loading and caching for the wgpu renderer.
//!
//! Provides:
//! - Dimension queries for pending-image placeholders
//! - A bounded background decoder pool on native targets
//! - Inline decoding on browser Wasm, which has no ambient native threads
//! - GPU texture upload when ready
//! - LRU cache with memory limits

use neomacs_image::ImageSequenceCache;
use neomacs_image::decoder::{DecodeRequest, ImageDecoder, ImageSource, WorkerDecodeOutcome};

use neomacs_display_protocol::{
    ImageCacheUsage, ImageColorContext, ImageFrameIndex, ImageId, ImageIntrinsicExtent,
    ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken, ImageMaskPolicy, ImageNativeExtent,
    ImageRasterExtent, ImageRealization, ImageRotation, ImageSequenceId, ImageSequenceRetirement,
    ImageSizeSpec, RetainedImageSet,
};
#[cfg(test)]
use neomacs_display_protocol::{ImageEmbeddedMetadata, ImageMaskKind};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
#[cfg(not(target_family = "wasm"))]
use std::num::NonZeroUsize;
#[cfg(not(target_family = "wasm"))]
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, mpsc};
#[cfg(not(target_family = "wasm"))]
use std::thread;

#[cfg(target_os = "linux")]
use crate::external_buffer::DmaBufBuffer;

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

pub use neomacs_display_protocol::{DecodedImage, ImageMetadata};

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
    decoded_tx: mpsc::Sender<WorkerDecodeOutcome>,
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
                let outcome = ImageDecoder::decode_request(request, &self.sequence_cache);
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
    /// Accept a validated decode from an external worker through the same
    /// completion path as the native decoder pool.
    pub fn accept_decoded(&mut self, decoded: DecodedImage) -> Result<(), &'static str> {
        if !decoded.validate() {
            return Err("invalid decoded image geometry or pixels");
        }
        self.begin_load(decoded.load);
        self.decoded_tx
            .send(WorkerDecodeOutcome::Ready(decoded))
            .map_err(|_| "image completion channel disconnected")
    }
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
        let decoder = ImageDecodeExecutor::new(decoded_tx.clone(), Arc::clone(&sequence_cache));

        Self {
            next_id: AtomicU32::new(1),
            textures: HashMap::new(),
            states: HashMap::new(),
            loads: ImageLoadLifecycle::default(),
            residency: ImageResidencyLifecycle::default(),
            retained_images: RetainedImageSet::default(),
            pending_dimensions: HashMap::new(),
            decoded_rx,
            decoded_tx,
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
                    let _ = tx.send(ImageDecoder::decode_request(request, &sequence_cache));
                }
                Err(_) => {
                    // Channel closed, exit thread
                    tracing::debug!("Decoder thread {} exiting", thread_id);
                    break;
                }
            }
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

    pub fn query_file_dimensions(path: &str) -> Option<ImageNativeExtent> {
        ImageDecoder::query_file_dimensions(path)
    }
    fn query_file_intrinsic_extent(path: &str) -> Option<ImageIntrinsicExtent> {
        ImageDecoder::query_file_intrinsic_extent(path)
    }
    pub fn query_data_dimensions(data: &[u8]) -> Option<ImageNativeExtent> {
        ImageDecoder::query_data_dimensions(data)
    }
    fn query_data_intrinsic_extent(data: &[u8]) -> Option<ImageIntrinsicExtent> {
        ImageDecoder::query_data_intrinsic_extent(data)
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
        resources: crate::SvgResourceContext,
    ) {
        let load = self.begin_load(load);
        let image = load.image();
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
                data: data.to_vec(),
                resources: crate::SvgResourceContext::Isolated,
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
