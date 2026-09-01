//! External buffer abstractions for texture data from various sources.
//!
//! This module provides a unified interface for converting external buffer data
//! (from images, video frames, WebKit surfaces, etc.) into wgpu textures.

use std::sync::Arc;

#[cfg(target_os = "linux")]
use std::os::fd::OwnedFd;

/// Buffer pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferFormat {
    /// Blue, Green, Red, Alpha (native wgpu format on most platforms)
    Bgra8,
    /// Red, Green, Blue, Alpha
    Rgba8,
    /// Alpha, Red, Green, Blue (common on macOS)
    Argb8,
}

impl BufferFormat {
    /// Returns the number of bytes per pixel for this format.
    pub fn bytes_per_pixel(&self) -> usize {
        4 // All formats are 4 bytes per pixel
    }
}

/// Trait for external buffers that can be converted to wgpu textures.
pub trait ExternalBuffer {
    /// Convert this buffer to a wgpu texture.
    ///
    /// Returns `None` if the conversion fails or is not supported.
    fn to_wgpu_texture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<wgpu::Texture>;

    /// Get the dimensions of this buffer.
    fn dimensions(&self) -> (u32, u32);
}

/// A buffer backed by shared memory (cross-platform fallback).
///
/// This is the simplest buffer type that works on all platforms.
/// It stores pixel data in CPU memory and uploads to GPU via `queue.write_texture()`.
#[derive(Debug, Clone)]
pub struct SharedMemoryBuffer {
    /// Raw pixel data.
    pub data: Arc<Vec<u8>>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Number of bytes per row (may include padding).
    pub stride: u32,
    /// Pixel format.
    pub format: BufferFormat,
}

impl SharedMemoryBuffer {
    /// Create a new SharedMemoryBuffer.
    pub fn new(data: Vec<u8>, width: u32, height: u32, stride: u32, format: BufferFormat) -> Self {
        Self {
            data: Arc::new(data),
            width,
            height,
            stride,
            format,
        }
    }

    /// Create a SharedMemoryBuffer from existing Arc<Vec<u8>>.
    pub fn from_arc(
        data: Arc<Vec<u8>>,
        width: u32,
        height: u32,
        stride: u32,
        format: BufferFormat,
    ) -> Self {
        Self {
            data,
            width,
            height,
            stride,
            format,
        }
    }

    /// Convert pixel data to BGRA8 format (native wgpu format).
    ///
    /// Returns a new Vec with the converted data, or None if already BGRA8.
    fn convert_to_bgra(&self) -> Option<Vec<u8>> {
        match self.format {
            BufferFormat::Bgra8 => None, // Already in correct format
            BufferFormat::Rgba8 => {
                // RGBA -> BGRA: swap R and B channels
                let mut converted = Vec::with_capacity((self.width * self.height * 4) as usize);
                for y in 0..self.height {
                    let row_start = (y * self.stride) as usize;
                    for x in 0..self.width {
                        let pixel_start = row_start + (x * 4) as usize;
                        if pixel_start + 4 <= self.data.len() {
                            let r = self.data[pixel_start];
                            let g = self.data[pixel_start + 1];
                            let b = self.data[pixel_start + 2];
                            let a = self.data[pixel_start + 3];
                            // BGRA order
                            converted.push(b);
                            converted.push(g);
                            converted.push(r);
                            converted.push(a);
                        }
                    }
                }
                Some(converted)
            }
            BufferFormat::Argb8 => {
                // ARGB -> BGRA: reorder A,R,G,B to B,G,R,A
                let mut converted = Vec::with_capacity((self.width * self.height * 4) as usize);
                for y in 0..self.height {
                    let row_start = (y * self.stride) as usize;
                    for x in 0..self.width {
                        let pixel_start = row_start + (x * 4) as usize;
                        if pixel_start + 4 <= self.data.len() {
                            let a = self.data[pixel_start];
                            let r = self.data[pixel_start + 1];
                            let g = self.data[pixel_start + 2];
                            let b = self.data[pixel_start + 3];
                            // BGRA order
                            converted.push(b);
                            converted.push(g);
                            converted.push(r);
                            converted.push(a);
                        }
                    }
                }
                Some(converted)
            }
        }
    }

    /// Get pixel data suitable for upload (BGRA8 format, tightly packed).
    fn get_upload_data(&self) -> Vec<u8> {
        if let Some(converted) = self.convert_to_bgra() {
            converted
        } else if self.stride == self.width * 4 {
            // Already BGRA8 and tightly packed, can use as-is
            self.data.as_ref().clone()
        } else {
            // BGRA8 but has row padding, need to remove it
            let mut packed = Vec::with_capacity((self.width * self.height * 4) as usize);
            for y in 0..self.height {
                let row_start = (y * self.stride) as usize;
                let row_end = row_start + (self.width * 4) as usize;
                if row_end <= self.data.len() {
                    packed.extend_from_slice(&self.data[row_start..row_end]);
                }
            }
            packed
        }
    }
}

impl ExternalBuffer for SharedMemoryBuffer {
    fn to_wgpu_texture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<wgpu::Texture> {
        if self.width == 0 || self.height == 0 {
            return None;
        }

        let upload_data = self.get_upload_data();
        let expected_size = (self.width * self.height * 4) as usize;
        if upload_data.len() < expected_size {
            tracing::warn!(
                "SharedMemoryBuffer: insufficient data, expected {} bytes, got {}",
                expected_size,
                upload_data.len()
            );
            return None;
        }

        // Create the texture
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SharedMemoryBuffer Texture"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Upload the data
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &upload_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        Some(texture)
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// A buffer backed by a DMA-BUF file descriptor (Linux only).
///
/// DMA-BUF allows zero-copy sharing of GPU buffers between processes and
/// between different GPU APIs. This is the most efficient way to handle
/// video frames and WebKit surfaces on Linux.
///
/// This struct supports multi-plane formats (e.g., YUV), with up to 4 planes.
#[cfg(target_os = "linux")]
#[derive(Debug)]
pub struct DmaBufBuffer {
    /// Owned DMA-BUF file descriptors per plane (up to 4).
    ///
    /// `None` marks an unused plane slot. Each `OwnedFd` closes on drop, so the
    /// buffer is the sole owner of its descriptors — hence this type is not
    /// `Clone` (cloning would alias, then double-close, the fds).
    pub fds: [Option<OwnedFd>; 4],
    /// Number of bytes per row per plane.
    pub strides: [u32; 4],
    /// Byte offset per plane.
    pub offsets: [u32; 4],
    /// Number of planes.
    pub num_planes: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// DRM fourcc format code (e.g., DRM_FORMAT_ARGB8888).
    pub fourcc: u32,
    /// DRM modifier (for tiled/compressed formats).
    pub modifier: u64,
}

#[cfg(target_os = "linux")]
impl DmaBufBuffer {
    /// Create a new DmaBufBuffer, taking ownership of the plane fds.
    ///
    /// Only the first `num_planes` slots are expected to be `Some`; the rest
    /// stay `None`. Ownership of each fd transfers into the buffer.
    pub fn new(
        fds: [Option<OwnedFd>; 4],
        strides: [u32; 4],
        offsets: [u32; 4],
        num_planes: u32,
        width: u32,
        height: u32,
        fourcc: u32,
        modifier: u64,
    ) -> Self {
        Self {
            fds,
            strides,
            offsets,
            num_planes,
            width,
            height,
            fourcc,
            modifier,
        }
    }

    /// Create a simple single-plane DmaBufBuffer, taking ownership of `fd`.
    pub fn single_plane(
        fd: OwnedFd,
        width: u32,
        height: u32,
        stride: u32,
        fourcc: u32,
        modifier: u64,
    ) -> Self {
        Self {
            fds: [Some(fd), None, None, None],
            strides: [stride, 0, 0, 0],
            offsets: [0, 0, 0, 0],
            num_planes: 1,
            width,
            height,
            fourcc,
            modifier,
        }
    }

    #[cfg(any(feature = "video-dmabuf", feature = "webview"))]
    fn import_params(&self) -> Option<crate::vulkan_dmabuf::DmaBufImportParams<'_>> {
        use std::os::fd::AsFd;

        let plane_count = self.num_planes as usize;
        if plane_count > self.fds.len() {
            tracing::warn!(
                "DmaBufBuffer: declared {} planes, maximum is {}",
                self.num_planes,
                self.fds.len()
            );
            return None;
        }
        let mut fds = Vec::with_capacity(plane_count);
        for plane in &self.fds[..plane_count] {
            match plane {
                Some(fd) => fds.push(fd.as_fd()),
                None => {
                    tracing::warn!(
                        "DmaBufBuffer: declared {} planes but a plane fd is missing; skipping import",
                        self.num_planes
                    );
                    return None;
                }
            }
        }
        Some(crate::vulkan_dmabuf::DmaBufImportParams {
            fds,
            strides: self.strides[..plane_count].to_vec(),
            offsets: self.offsets[..plane_count].to_vec(),
            num_planes: self.num_planes,
            width: self.width,
            height: self.height,
            fourcc: self.fourcc,
            modifier: self.modifier,
        })
    }

    /// Import DMA-BUF as wgpu texture.
    ///
    /// Attempts zero-copy Vulkan import first (with driver modifier query for
    /// correct multi-plane support), falls back to mmap for linear buffers.
    pub fn to_wgpu_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<wgpu::Texture> {
        #[cfg(not(any(feature = "video-dmabuf", feature = "webview")))]
        let _ = (device, queue, self.num_planes);

        #[cfg(any(feature = "video-dmabuf", feature = "webview"))]
        {
            let params = self.import_params()?;
            if let Some(texture) = crate::vulkan_dmabuf::import_dmabuf(device, queue, &params) {
                tracing::debug!(
                    "DmaBufBuffer: texture import succeeded ({} planes)",
                    self.num_planes
                );
                return Some(texture);
            }
        }

        tracing::warn!(
            "DmaBufBuffer::to_wgpu_texture failed ({}x{}, fourcc={:#x}, modifier={:#x}, {} planes)",
            self.width,
            self.height,
            self.fourcc,
            self.modifier,
            self.num_planes
        );
        None
    }

    /// Import only as a foreign GPU texture, with no mmap-owned fallback.
    /// WebView caching uses this contract so it can copy the browser allocation
    /// into cache-owned storage before the WPE frame lease is released.
    #[cfg(feature = "webview")]
    pub fn to_external_wgpu_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<wgpu::Texture> {
        let params = self.import_params()?;
        crate::vulkan_dmabuf::import_dmabuf_external(device, queue, &params)
    }

    /// Get dimensions of this buffer.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

#[cfg(target_os = "linux")]
impl ExternalBuffer for DmaBufBuffer {
    fn to_wgpu_texture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<wgpu::Texture> {
        // Delegate to the inherent method
        DmaBufBuffer::to_wgpu_texture(self, device, queue)
    }

    fn dimensions(&self) -> (u32, u32) {
        DmaBufBuffer::dimensions(self)
    }
}

/// Platform-specific buffer type alias.
///
/// On Linux, this prefers DmaBufBuffer for zero-copy GPU buffer sharing.
/// On other platforms, this falls back to SharedMemoryBuffer.
#[cfg(target_os = "linux")]
pub type PlatformBuffer = DmaBufBuffer;

#[cfg(not(target_os = "linux"))]
pub type PlatformBuffer = SharedMemoryBuffer;

#[cfg(test)]
#[path = "external_buffer_test.rs"]
mod tests;
