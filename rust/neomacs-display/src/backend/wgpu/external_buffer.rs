//! External buffer abstractions for texture data from various sources.
//!
//! This module provides a unified interface for converting external buffer data
//! (from images, video frames, WebKit surfaces, etc.) into wgpu textures.

use std::sync::Arc;

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
    fn to_wgpu_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<wgpu::Texture>;

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
    pub fn new(
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
        format: BufferFormat,
    ) -> Self {
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
    fn to_wgpu_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<wgpu::Texture> {
        if self.width == 0 || self.height == 0 {
            return None;
        }

        let upload_data = self.get_upload_data();
        let expected_size = (self.width * self.height * 4) as usize;
        if upload_data.len() < expected_size {
            log::warn!(
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
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &upload_data,
            wgpu::ImageDataLayout {
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
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub struct DmaBufBuffer {
    /// DMA-BUF file descriptor.
    pub fd: std::os::unix::io::RawFd,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Number of bytes per row.
    pub stride: u32,
    /// DRM format code (e.g., DRM_FORMAT_ARGB8888).
    pub format: u32,
    /// DRM modifier (for tiled/compressed formats).
    pub modifier: u64,
}

#[cfg(target_os = "linux")]
impl DmaBufBuffer {
    /// Create a new DmaBufBuffer.
    pub fn new(
        fd: std::os::unix::io::RawFd,
        width: u32,
        height: u32,
        stride: u32,
        format: u32,
        modifier: u64,
    ) -> Self {
        Self {
            fd,
            width,
            height,
            stride,
            format,
            modifier,
        }
    }
}

#[cfg(target_os = "linux")]
impl ExternalBuffer for DmaBufBuffer {
    fn to_wgpu_texture(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> Option<wgpu::Texture> {
        // TODO: Implement DMA-BUF import using wgpu's hal::vulkan extensions
        // This requires:
        // 1. Access to the underlying Vulkan device
        // 2. VK_EXT_external_memory_dma_buf extension
        // 3. Proper memory import and image creation
        //
        // For now, log a warning and return None. Callers should fall back
        // to SharedMemoryBuffer for systems where DMA-BUF import isn't working.
        log::warn!(
            "DmaBufBuffer: DMA-BUF import not yet implemented (fd={}, {}x{}, format={:#x}, modifier={:#x})",
            self.fd,
            self.width,
            self.height,
            self.format,
            self.modifier
        );
        None
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
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
mod tests {
    use super::*;

    #[test]
    fn test_buffer_format_bytes_per_pixel() {
        assert_eq!(BufferFormat::Bgra8.bytes_per_pixel(), 4);
        assert_eq!(BufferFormat::Rgba8.bytes_per_pixel(), 4);
        assert_eq!(BufferFormat::Argb8.bytes_per_pixel(), 4);
    }

    #[test]
    fn test_shared_memory_buffer_dimensions() {
        let buffer = SharedMemoryBuffer::new(
            vec![0u8; 100 * 50 * 4],
            100,
            50,
            100 * 4,
            BufferFormat::Bgra8,
        );
        assert_eq!(buffer.dimensions(), (100, 50));
    }

    #[test]
    fn test_rgba_to_bgra_conversion() {
        // Create a small RGBA buffer with known values
        let rgba_data = vec![
            255, 0, 0, 255,   // Red pixel (RGBA)
            0, 255, 0, 255,   // Green pixel (RGBA)
            0, 0, 255, 255,   // Blue pixel (RGBA)
            128, 64, 32, 200, // Mixed pixel (RGBA)
        ];
        let buffer = SharedMemoryBuffer::new(
            rgba_data,
            4,
            1,
            16,
            BufferFormat::Rgba8,
        );

        let bgra = buffer.convert_to_bgra().expect("Should convert");
        // Red pixel should become BGRA: B=0, G=0, R=255, A=255
        assert_eq!(bgra[0..4], [0, 0, 255, 255]);
        // Green pixel should become BGRA: B=0, G=255, R=0, A=255
        assert_eq!(bgra[4..8], [0, 255, 0, 255]);
        // Blue pixel should become BGRA: B=255, G=0, R=0, A=255
        assert_eq!(bgra[8..12], [255, 0, 0, 255]);
        // Mixed pixel should become BGRA: B=32, G=64, R=128, A=200
        assert_eq!(bgra[12..16], [32, 64, 128, 200]);
    }

    #[test]
    fn test_argb_to_bgra_conversion() {
        // Create a small ARGB buffer with known values
        let argb_data = vec![
            255, 255, 0, 0,   // Red pixel (ARGB: A=255, R=255, G=0, B=0)
            255, 0, 255, 0,   // Green pixel (ARGB)
            255, 0, 0, 255,   // Blue pixel (ARGB)
            200, 128, 64, 32, // Mixed pixel (ARGB)
        ];
        let buffer = SharedMemoryBuffer::new(
            argb_data,
            4,
            1,
            16,
            BufferFormat::Argb8,
        );

        let bgra = buffer.convert_to_bgra().expect("Should convert");
        // Red pixel should become BGRA: B=0, G=0, R=255, A=255
        assert_eq!(bgra[0..4], [0, 0, 255, 255]);
        // Green pixel should become BGRA: B=0, G=255, R=0, A=255
        assert_eq!(bgra[4..8], [0, 255, 0, 255]);
        // Blue pixel should become BGRA: B=255, G=0, R=0, A=255
        assert_eq!(bgra[8..12], [255, 0, 0, 255]);
        // Mixed pixel should become BGRA: B=32, G=64, R=128, A=200
        assert_eq!(bgra[12..16], [32, 64, 128, 200]);
    }

    #[test]
    fn test_bgra_no_conversion() {
        let bgra_data = vec![0, 0, 255, 255]; // One blue pixel
        let buffer = SharedMemoryBuffer::new(
            bgra_data,
            1,
            1,
            4,
            BufferFormat::Bgra8,
        );

        // Should return None since no conversion is needed
        assert!(buffer.convert_to_bgra().is_none());
    }
}
