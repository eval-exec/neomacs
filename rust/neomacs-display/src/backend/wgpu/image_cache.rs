//! Async image loading and caching for wgpu renderer
//!
//! Provides non-blocking image loading:
//! - Fast dimension query (header only)
//! - Background decoding in thread pool
//! - GPU texture upload when ready
//! - LRU cache with memory limits

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

#[cfg(target_os = "linux")]
use super::external_buffer::DmaBufBuffer;

/// Maximum texture dimension (width or height)
const MAX_TEXTURE_SIZE: u32 = 4096;

/// Maximum total cache memory in bytes (64MB)
const MAX_CACHE_MEMORY: usize = 64 * 1024 * 1024;

/// Get number of decoder threads (use all available CPU cores)
fn decoder_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
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
    /// Failed to load
    Failed(String),
}

/// Cached image with GPU texture
pub struct CachedImage {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
    /// Memory size in bytes
    pub memory_size: usize,
}

/// Decoded image data waiting for GPU upload
struct DecodedImage {
    id: u32,
    width: u32,
    height: u32,
    data: Vec<u8>, // RGBA
}

/// Image dimensions (from header)
#[derive(Debug, Clone, Copy)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

/// Async image cache
pub struct ImageCache {
    /// Next image ID
    next_id: AtomicU32,
    /// Cached textures: id -> CachedImage
    textures: HashMap<u32, CachedImage>,
    /// Image states: id -> state
    states: HashMap<u32, ImageState>,
    /// Pending dimensions (before texture is ready)
    pending_dimensions: HashMap<u32, ImageDimensions>,
    /// Channel to receive decoded images
    decoded_rx: mpsc::Receiver<DecodedImage>,
    /// Channel to send decode requests
    decode_tx: mpsc::Sender<DecodeRequest>,
    /// Bind group layout for image textures
    bind_group_layout: wgpu::BindGroupLayout,
    /// Sampler for image textures
    sampler: wgpu::Sampler,
    /// Total cached memory
    total_memory: usize,
}

/// Request to decode an image
struct DecodeRequest {
    id: u32,
    source: ImageSource,
    max_width: u32,
    max_height: u32,
}

/// Image source
enum ImageSource {
    File(String),
    Data(Vec<u8>),
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
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Create channels for async decoding
        let (decode_tx, decode_rx) = mpsc::channel::<DecodeRequest>();
        let (decoded_tx, decoded_rx) = mpsc::channel::<DecodedImage>();

        // Wrap receiver in Arc<Mutex> for sharing across threads
        let decode_rx = Arc::new(Mutex::new(decode_rx));

        // Spawn decoder thread pool (one per CPU core)
        let num_threads = decoder_thread_count();
        log::info!("Starting {} image decoder threads", num_threads);
        for i in 0..num_threads {
            let rx = Arc::clone(&decode_rx);
            let tx = decoded_tx.clone();
            thread::spawn(move || {
                Self::decoder_thread_pooled(i, rx, tx);
            });
        }

        Self {
            next_id: AtomicU32::new(1),
            textures: HashMap::new(),
            states: HashMap::new(),
            pending_dimensions: HashMap::new(),
            decoded_rx,
            decode_tx,
            bind_group_layout,
            sampler,
            total_memory: 0,
        }
    }

    /// Background decoder thread (pooled version)
    fn decoder_thread_pooled(
        thread_id: usize,
        rx: Arc<Mutex<mpsc::Receiver<DecodeRequest>>>,
        tx: mpsc::Sender<DecodedImage>,
    ) {
        log::debug!("Decoder thread {} started", thread_id);
        loop {
            // Lock, receive, unlock immediately to allow other threads to grab work
            let request = {
                let guard = rx.lock().unwrap();
                guard.recv()
            };

            match request {
                Ok(request) => {
                    log::debug!("Thread {} decoding image {}", thread_id, request.id);
                    let result = match request.source {
                        ImageSource::File(path) => {
                            Self::decode_file(&path, request.max_width, request.max_height)
                        }
                        ImageSource::Data(data) => {
                            Self::decode_data(&data, request.max_width, request.max_height)
                        }
                        ImageSource::RawArgb32 { data, width, height, stride } => {
                            Self::convert_argb32_to_rgba(&data, width, height, stride, request.max_width, request.max_height)
                        }
                        ImageSource::RawRgb24 { data, width, height, stride } => {
                            Self::convert_rgb24_to_rgba(&data, width, height, stride, request.max_width, request.max_height)
                        }
                    };

                    if let Some((width, height, data)) = result {
                        let _ = tx.send(DecodedImage {
                            id: request.id,
                            width,
                            height,
                            data,
                        });
                    }
                }
                Err(_) => {
                    // Channel closed, exit thread
                    log::debug!("Decoder thread {} exiting", thread_id);
                    break;
                }
            }
        }
    }

    /// Decode image file with size constraints
    fn decode_file(path: &str, max_width: u32, max_height: u32) -> Option<(u32, u32, Vec<u8>)> {
        let img = image::open(path).ok()?;
        Self::process_image(img, max_width, max_height)
    }

    /// Decode image data with size constraints
    fn decode_data(data: &[u8], max_width: u32, max_height: u32) -> Option<(u32, u32, Vec<u8>)> {
        let img = image::load_from_memory(data).ok()?;
        Self::process_image(img, max_width, max_height)
    }

    /// Process decoded image: resize if needed, convert to RGBA
    fn process_image(
        img: image::DynamicImage,
        max_width: u32,
        max_height: u32,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let (mut width, mut height) = (img.width(), img.height());

        // Apply max constraints
        let mw = if max_width > 0 { max_width } else { MAX_TEXTURE_SIZE };
        let mh = if max_height > 0 { max_height } else { MAX_TEXTURE_SIZE };

        // Scale down if needed (preserve aspect ratio)
        if width > mw || height > mh {
            let ratio = (width as f64 / height as f64).min(mw as f64 / mh as f64);
            if width > mw {
                width = mw;
                height = (mw as f64 / ratio) as u32;
            }
            if height > mh {
                height = mh;
                width = (mh as f64 * ratio) as u32;
            }
        }

        // Resize if dimensions changed
        let img = if width != img.width() || height != img.height() {
            img.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };

        // Convert to RGBA
        let rgba = img.to_rgba8();
        Some((width, height, rgba.into_raw()))
    }

    /// Convert ARGB32 raw pixel data to RGBA
    /// Input format: A,R,G,B byte order (4 bytes per pixel)
    /// Output format: R,G,B,A byte order (4 bytes per pixel)
    fn convert_argb32_to_rgba(
        data: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        max_width: u32,
        max_height: u32,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let bytes_per_pixel = 4u32;
        let expected_min_size = (height.saturating_sub(1)) * stride + width * bytes_per_pixel;
        if data.len() < expected_min_size as usize {
            log::warn!(
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
        let (cw, ch) = Self::constrain_dimensions(width, height, max_width, max_height);
        if cw != width || ch != height {
            // Need to resize - use image crate
            let img = image::RgbaImage::from_raw(width, height, rgba)?;
            let resized = image::imageops::resize(&img, cw, ch, image::imageops::FilterType::Lanczos3);
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
        max_width: u32,
        max_height: u32,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let bytes_per_pixel = 3u32;
        let expected_min_size = (height.saturating_sub(1)) * stride + width * bytes_per_pixel;
        if data.len() < expected_min_size as usize {
            log::warn!(
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
        let (cw, ch) = Self::constrain_dimensions(width, height, max_width, max_height);
        if cw != width || ch != height {
            // Need to resize - use image crate
            let img = image::RgbaImage::from_raw(width, height, rgba)?;
            let resized = image::imageops::resize(&img, cw, ch, image::imageops::FilterType::Lanczos3);
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

    /// Query image file dimensions (fast - reads header only)
    pub fn query_file_dimensions(path: &str) -> Option<ImageDimensions> {
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        // Use image crate's dimension reader (reads header only)
        let (width, height) = image::io::Reader::new(reader)
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()?;

        Some(ImageDimensions { width, height })
    }

    /// Query image data dimensions (fast - reads header only)
    pub fn query_data_dimensions(data: &[u8]) -> Option<ImageDimensions> {
        let cursor = std::io::Cursor::new(data);
        let (width, height) = image::io::Reader::new(BufReader::new(cursor))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()?;

        Some(ImageDimensions { width, height })
    }

    /// Load image from file (async)
    /// Returns image ID immediately, texture loads in background
    pub fn load_file(&mut self, path: &str, max_width: u32, max_height: u32) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.load_file_with_id(id, path, max_width, max_height);
        id
    }

    /// Load image from file with a pre-allocated ID (for threaded mode)
    /// This allows the calling code to allocate the ID before sending a command.
    pub fn load_file_with_id(&mut self, id: u32, path: &str, max_width: u32, max_height: u32) {
        // Query dimensions first (fast)
        if let Some(dims) = Self::query_file_dimensions(path) {
            // Apply max constraints to dimensions
            let (w, h) = Self::constrain_dimensions(dims.width, dims.height, max_width, max_height);
            self.pending_dimensions.insert(id, ImageDimensions { width: w, height: h });
        }

        // Queue for async decode
        self.states.insert(id, ImageState::Pending);
        let _ = self.decode_tx.send(DecodeRequest {
            id,
            source: ImageSource::File(path.to_string()),
            max_width,
            max_height,
        });
    }

    /// Allocate the next available image ID without loading anything.
    /// Used by threaded mode to pre-allocate IDs before sending commands.
    pub fn allocate_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Load image from data (async)
    pub fn load_data(&mut self, data: &[u8], max_width: u32, max_height: u32) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        // Query dimensions first (fast)
        if let Some(dims) = Self::query_data_dimensions(data) {
            let (w, h) = Self::constrain_dimensions(dims.width, dims.height, max_width, max_height);
            self.pending_dimensions.insert(id, ImageDimensions { width: w, height: h });
        }

        // Queue for async decode
        self.states.insert(id, ImageState::Pending);
        let _ = self.decode_tx.send(DecodeRequest {
            id,
            source: ImageSource::Data(data.to_vec()),
            max_width,
            max_height,
        });

        id
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
        max_width: u32,
        max_height: u32,
    ) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        // Store pending dimensions immediately (we know the exact size)
        let (w, h) = Self::constrain_dimensions(width, height, max_width, max_height);
        self.pending_dimensions
            .insert(id, ImageDimensions { width: w, height: h });

        // Queue for async conversion
        self.states.insert(id, ImageState::Pending);
        let _ = self.decode_tx.send(DecodeRequest {
            id,
            source: ImageSource::RawArgb32 {
                data: data.to_vec(),
                width,
                height,
                stride,
            },
            max_width,
            max_height,
        });

        id
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
        max_width: u32,
        max_height: u32,
    ) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        // Store pending dimensions immediately (we know the exact size)
        let (w, h) = Self::constrain_dimensions(width, height, max_width, max_height);
        self.pending_dimensions
            .insert(id, ImageDimensions { width: w, height: h });

        // Queue for async conversion
        self.states.insert(id, ImageState::Pending);
        let _ = self.decode_tx.send(DecodeRequest {
            id,
            source: ImageSource::RawRgb24 {
                data: data.to_vec(),
                width,
                height,
                stride,
            },
            max_width,
            max_height,
        });

        id
    }

    /// Import image from DMA-BUF (zero-copy if supported)
    #[cfg(target_os = "linux")]
    pub fn import_dmabuf(
        &mut self,
        dmabuf: DmaBufBuffer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
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

            self.textures.insert(id, CachedImage {
                texture,
                view,
                bind_group,
                width,
                height,
                memory_size,
            });
            self.states.insert(id, ImageState::Ready);

            log::info!("Imported DMA-BUF image {} ({}x{}) zero-copy", id, width, height);
        } else {
            self.states.insert(id, ImageState::Failed("DMA-BUF import failed".into()));
            log::warn!("DMA-BUF import failed for image {}", id);
        }

        id
    }

    /// Constrain dimensions to max values while preserving aspect ratio
    fn constrain_dimensions(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
        let mut w = width;
        let mut h = height;

        let mw = if max_width > 0 { max_width } else { MAX_TEXTURE_SIZE };
        let mh = if max_height > 0 { max_height } else { MAX_TEXTURE_SIZE };

        if w > mw {
            h = (h as f64 * mw as f64 / w as f64) as u32;
            w = mw;
        }
        if h > mh {
            w = (w as f64 * mh as f64 / h as f64) as u32;
            h = mh;
        }

        (w.max(1), h.max(1))
    }

    /// Process pending decoded images (call each frame)
    pub fn process_pending(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        // Drain decoded images from channel
        while let Ok(decoded) = self.decoded_rx.try_recv() {
            self.upload_texture(device, queue, decoded);
        }

        // Evict if over memory limit
        self.evict_if_needed();
    }

    /// Upload decoded image to GPU texture
    fn upload_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, decoded: DecodedImage) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Image Texture"),
            size: wgpu::Extent3d {
                width: decoded.width,
                height: decoded.height,
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
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &decoded.data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(decoded.width * 4),
                rows_per_image: Some(decoded.height),
            },
            wgpu::Extent3d {
                width: decoded.width,
                height: decoded.height,
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

        let memory_size = (decoded.width * decoded.height * 4) as usize;
        self.total_memory += memory_size;

        self.textures.insert(decoded.id, CachedImage {
            texture,
            view,
            bind_group,
            width: decoded.width,
            height: decoded.height,
            memory_size,
        });

        self.states.insert(decoded.id, ImageState::Ready);
        self.pending_dimensions.remove(&decoded.id);

        log::debug!("Uploaded image {} ({}x{}, {}KB)",
                   decoded.id, decoded.width, decoded.height, memory_size / 1024);
    }

    /// Evict old textures if over memory limit
    fn evict_if_needed(&mut self) {
        // Simple strategy: remove oldest entries until under limit
        while self.total_memory > MAX_CACHE_MEMORY && !self.textures.is_empty() {
            // Find smallest ID (oldest)
            if let Some(&id) = self.textures.keys().min() {
                if let Some(cached) = self.textures.remove(&id) {
                    self.total_memory -= cached.memory_size;
                    self.states.remove(&id);
                    log::debug!("Evicted image {} to free {}KB", id, cached.memory_size / 1024);
                }
            }
        }
    }

    /// Get cached image if ready
    pub fn get(&self, id: u32) -> Option<&CachedImage> {
        self.textures.get(&id)
    }

    /// Get image dimensions (pending or loaded)
    pub fn get_dimensions(&self, id: u32) -> Option<ImageDimensions> {
        // Check loaded textures first
        if let Some(cached) = self.textures.get(&id) {
            return Some(ImageDimensions {
                width: cached.width,
                height: cached.height,
            });
        }
        // Check pending dimensions
        self.pending_dimensions.get(&id).copied()
    }

    /// Get image state
    pub fn get_state(&self, id: u32) -> Option<&ImageState> {
        self.states.get(&id)
    }

    /// Check if image is ready
    pub fn is_ready(&self, id: u32) -> bool {
        matches!(self.states.get(&id), Some(ImageState::Ready))
    }

    /// Free an image from cache
    pub fn free(&mut self, id: u32) {
        if let Some(cached) = self.textures.remove(&id) {
            self.total_memory -= cached.memory_size;
        }
        self.states.remove(&id);
        self.pending_dimensions.remove(&id);
    }

    /// Clear entire cache
    pub fn clear(&mut self) {
        self.textures.clear();
        self.states.clear();
        self.pending_dimensions.clear();
        self.total_memory = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_argb32_to_rgba_basic() {
        // Create a 2x2 ARGB32 image
        // ARGB32 format: A, R, G, B (4 bytes per pixel)
        let width = 2u32;
        let height = 2u32;
        let stride = width * 4; // No padding
        let data: Vec<u8> = vec![
            // Row 0
            255, 100, 150, 200, // Pixel (0,0): A=255, R=100, G=150, B=200
            128, 50, 75, 100,   // Pixel (1,0): A=128, R=50, G=75, B=100
            // Row 1
            64, 25, 37, 50,     // Pixel (0,1): A=64, R=25, G=37, B=50
            0, 0, 0, 0,         // Pixel (1,1): A=0, R=0, G=0, B=0 (transparent)
        ];

        let result = ImageCache::convert_argb32_to_rgba(&data, width, height, stride, 0, 0);
        assert!(result.is_some());

        let (w, h, rgba) = result.unwrap();
        assert_eq!(w, 2);
        assert_eq!(h, 2);
        assert_eq!(rgba.len(), 16); // 2x2x4 bytes

        // Expected RGBA output: R, G, B, A
        // Pixel (0,0): R=100, G=150, B=200, A=255
        assert_eq!(&rgba[0..4], &[100, 150, 200, 255]);
        // Pixel (1,0): R=50, G=75, B=100, A=128
        assert_eq!(&rgba[4..8], &[50, 75, 100, 128]);
        // Pixel (0,1): R=25, G=37, B=50, A=64
        assert_eq!(&rgba[8..12], &[25, 37, 50, 64]);
        // Pixel (1,1): R=0, G=0, B=0, A=0
        assert_eq!(&rgba[12..16], &[0, 0, 0, 0]);
    }

    #[test]
    fn test_convert_argb32_with_stride_padding() {
        // 2x2 image with stride = 12 (4 bytes padding per row)
        let width = 2u32;
        let height = 2u32;
        let stride = 12u32; // 8 bytes data + 4 bytes padding per row
        let data: Vec<u8> = vec![
            // Row 0 (8 bytes data + 4 bytes padding)
            255, 100, 150, 200, // Pixel (0,0)
            128, 50, 75, 100,   // Pixel (1,0)
            0, 0, 0, 0,         // Padding (ignored)
            // Row 1 (8 bytes data + 4 bytes padding)
            64, 25, 37, 50,     // Pixel (0,1)
            32, 10, 20, 30,     // Pixel (1,1)
            0, 0, 0, 0,         // Padding (ignored)
        ];

        let result = ImageCache::convert_argb32_to_rgba(&data, width, height, stride, 0, 0);
        assert!(result.is_some());

        let (w, h, rgba) = result.unwrap();
        assert_eq!(w, 2);
        assert_eq!(h, 2);

        // Verify conversion (padding should be ignored)
        assert_eq!(&rgba[0..4], &[100, 150, 200, 255]); // Pixel (0,0)
        assert_eq!(&rgba[4..8], &[50, 75, 100, 128]);   // Pixel (1,0)
        assert_eq!(&rgba[8..12], &[25, 37, 50, 64]);    // Pixel (0,1)
        assert_eq!(&rgba[12..16], &[10, 20, 30, 32]);   // Pixel (1,1)
    }

    #[test]
    fn test_convert_argb32_invalid_data_size() {
        // Data too small for 2x2 image
        let data: Vec<u8> = vec![255, 100, 150, 200]; // Only 1 pixel
        let result = ImageCache::convert_argb32_to_rgba(&data, 2, 2, 8, 0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_convert_rgb24_to_rgba_basic() {
        // Create a 2x2 RGB24 image
        // RGB24 format: R, G, B (3 bytes per pixel)
        let width = 2u32;
        let height = 2u32;
        let stride = width * 3; // No padding
        let data: Vec<u8> = vec![
            // Row 0
            100, 150, 200, // Pixel (0,0): R=100, G=150, B=200
            50, 75, 100,   // Pixel (1,0): R=50, G=75, B=100
            // Row 1
            25, 37, 50,    // Pixel (0,1): R=25, G=37, B=50
            0, 0, 0,       // Pixel (1,1): R=0, G=0, B=0 (black)
        ];

        let result = ImageCache::convert_rgb24_to_rgba(&data, width, height, stride, 0, 0);
        assert!(result.is_some());

        let (w, h, rgba) = result.unwrap();
        assert_eq!(w, 2);
        assert_eq!(h, 2);
        assert_eq!(rgba.len(), 16); // 2x2x4 bytes

        // Expected RGBA output: R, G, B, A (A should always be 255)
        assert_eq!(&rgba[0..4], &[100, 150, 200, 255]);
        assert_eq!(&rgba[4..8], &[50, 75, 100, 255]);
        assert_eq!(&rgba[8..12], &[25, 37, 50, 255]);
        assert_eq!(&rgba[12..16], &[0, 0, 0, 255]);
    }

    #[test]
    fn test_convert_rgb24_with_stride_padding() {
        // 2x2 image with stride = 8 (2 bytes padding per row)
        let width = 2u32;
        let height = 2u32;
        let stride = 8u32; // 6 bytes data + 2 bytes padding per row
        let data: Vec<u8> = vec![
            // Row 0 (6 bytes data + 2 bytes padding)
            100, 150, 200, // Pixel (0,0)
            50, 75, 100,   // Pixel (1,0)
            0, 0,          // Padding (ignored)
            // Row 1 (6 bytes data + 2 bytes padding)
            25, 37, 50,    // Pixel (0,1)
            10, 20, 30,    // Pixel (1,1)
            0, 0,          // Padding (ignored)
        ];

        let result = ImageCache::convert_rgb24_to_rgba(&data, width, height, stride, 0, 0);
        assert!(result.is_some());

        let (w, h, rgba) = result.unwrap();
        assert_eq!(w, 2);
        assert_eq!(h, 2);

        // Verify conversion (padding should be ignored)
        assert_eq!(&rgba[0..4], &[100, 150, 200, 255]); // Pixel (0,0)
        assert_eq!(&rgba[4..8], &[50, 75, 100, 255]);   // Pixel (1,0)
        assert_eq!(&rgba[8..12], &[25, 37, 50, 255]);   // Pixel (0,1)
        assert_eq!(&rgba[12..16], &[10, 20, 30, 255]);  // Pixel (1,1)
    }

    #[test]
    fn test_convert_rgb24_invalid_data_size() {
        // Data too small for 2x2 image
        let data: Vec<u8> = vec![100, 150, 200]; // Only 1 pixel
        let result = ImageCache::convert_rgb24_to_rgba(&data, 2, 2, 6, 0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_constrain_dimensions() {
        // No constraints (uses MAX_TEXTURE_SIZE internally)
        assert_eq!(ImageCache::constrain_dimensions(100, 100, 0, 0), (100, 100));

        // Width constrained
        assert_eq!(ImageCache::constrain_dimensions(200, 100, 100, 0), (100, 50));

        // Height constrained
        assert_eq!(ImageCache::constrain_dimensions(100, 200, 0, 100), (50, 100));

        // Both constrained, width is limiting factor
        assert_eq!(ImageCache::constrain_dimensions(400, 200, 100, 100), (100, 50));

        // Both constrained, height is limiting factor
        assert_eq!(ImageCache::constrain_dimensions(200, 400, 100, 100), (50, 100));

        // Minimum 1x1 - very narrow image
        let (w, h) = ImageCache::constrain_dimensions(1, 1000, 10, 100);
        assert_eq!(w, 1);
        assert_eq!(h, 100); // Height is constrained to 100, width stays 1 (min)
    }

    #[test]
    fn test_convert_argb32_single_pixel() {
        // Single pixel image - edge case
        let data: Vec<u8> = vec![255, 128, 64, 32]; // A=255, R=128, G=64, B=32
        let result = ImageCache::convert_argb32_to_rgba(&data, 1, 1, 4, 0, 0);
        assert!(result.is_some());

        let (w, h, rgba) = result.unwrap();
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        assert_eq!(rgba, vec![128, 64, 32, 255]); // R=128, G=64, B=32, A=255
    }

    #[test]
    fn test_convert_rgb24_single_pixel() {
        // Single pixel image - edge case
        let data: Vec<u8> = vec![128, 64, 32]; // R=128, G=64, B=32
        let result = ImageCache::convert_rgb24_to_rgba(&data, 1, 1, 3, 0, 0);
        assert!(result.is_some());

        let (w, h, rgba) = result.unwrap();
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        assert_eq!(rgba, vec![128, 64, 32, 255]); // R=128, G=64, B=32, A=255
    }
}
