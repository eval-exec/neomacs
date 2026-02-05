//! Vulkan DMA-BUF import for zero-copy texture sharing.
//!
//! This module implements importing DMA-BUF file descriptors as Vulkan images,
//! then wrapping them as wgpu textures for zero-copy GPU-to-GPU transfers.
//!
//! Required Vulkan extensions:
//! - VK_KHR_external_memory
//! - VK_KHR_external_memory_fd
//! - VK_EXT_external_memory_dma_buf
//! - VK_EXT_image_drm_format_modifier

#[cfg(target_os = "linux")]
use std::os::unix::io::RawFd;

/// DRM format codes (fourcc)
#[allow(dead_code)]
pub mod drm_fourcc {
    pub const DRM_FORMAT_ARGB8888: u32 = 0x34325241; // ARGB8888
    pub const DRM_FORMAT_XRGB8888: u32 = 0x34325258; // XRGB8888
    pub const DRM_FORMAT_ABGR8888: u32 = 0x34324241; // ABGR8888
    pub const DRM_FORMAT_XBGR8888: u32 = 0x34324258; // XBGR8888
    pub const DRM_FORMAT_RGBA8888: u32 = 0x34324152; // RGBA8888
    pub const DRM_FORMAT_RGBX8888: u32 = 0x34325852; // RGBX8888
    pub const DRM_FORMAT_BGRA8888: u32 = 0x34324142; // BGRA8888
    pub const DRM_FORMAT_BGRX8888: u32 = 0x34325842; // BGRX8888
    pub const DRM_FORMAT_NV12: u32 = 0x3231564e;     // NV12 (YUV 4:2:0)
    pub const DRM_FORMAT_YUV420: u32 = 0x32315559;   // YUV420

    /// Linear modifier (no tiling)
    pub const DRM_FORMAT_MOD_LINEAR: u64 = 0;
    /// Invalid modifier
    pub const DRM_FORMAT_MOD_INVALID: u64 = 0x00ffffffffffffff;
}

/// Convert DRM fourcc to wgpu format
///
/// DRM format naming convention (little-endian):
/// - DRM_FORMAT_ARGB8888: 32-bit word is 0xAARRGGBB, bytes in memory: [B, G, R, A]
/// - DRM_FORMAT_BGRA8888: bytes in memory: [B, G, R, A]
///
/// Uses sRGB formats for correct gamma handling. Video content is typically
/// encoded with sRGB-like gamma curve (BT.709 transfer function ≈ sRGB).
pub fn drm_fourcc_to_wgpu_format(fourcc: u32) -> Option<wgpu::TextureFormat> {
    match fourcc {
        // ARGB8888: bytes [B, G, R, A] in memory = Bgra8UnormSrgb
        drm_fourcc::DRM_FORMAT_ARGB8888 | drm_fourcc::DRM_FORMAT_XRGB8888 |
        drm_fourcc::DRM_FORMAT_BGRA8888 | drm_fourcc::DRM_FORMAT_BGRX8888 => {
            Some(wgpu::TextureFormat::Bgra8UnormSrgb)
        }
        drm_fourcc::DRM_FORMAT_ABGR8888 | drm_fourcc::DRM_FORMAT_XBGR8888 |
        drm_fourcc::DRM_FORMAT_RGBA8888 | drm_fourcc::DRM_FORMAT_RGBX8888 => {
            Some(wgpu::TextureFormat::Rgba8UnormSrgb)
        }
        _ => None,
    }
}

/// DMA-BUF import parameters
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub struct DmaBufImportParams {
    /// DMA-BUF file descriptor
    pub fd: RawFd,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Stride (bytes per row)
    pub stride: u32,
    /// DRM fourcc format
    pub fourcc: u32,
    /// DRM modifier
    pub modifier: u64,
    /// Offset within the DMA-BUF
    pub offset: u32,
}

// ============================================================================
// True Zero-Copy Implementation using wgpu_hal
// ============================================================================

#[cfg(all(target_os = "linux", feature = "ash", feature = "wgpu-hal"))]
mod hal_import {
    use super::*;
    use ash::vk;
    use std::sync::Arc;

    /// Resources that must be kept alive for the imported texture
    struct ImportedDmaBufResources {
        device: ash::Device,
        image: vk::Image,
        memory: vk::DeviceMemory,
    }

    impl Drop for ImportedDmaBufResources {
        fn drop(&mut self) {
            unsafe {
                self.device.destroy_image(self.image, None);
                self.device.free_memory(self.memory, None);
            }
            log::debug!("Cleaned up imported DMA-BUF resources");
        }
    }

    /// Convert DRM fourcc to Vulkan format
    ///
    /// Uses sRGB formats for correct gamma handling. Video content is typically
    /// encoded with sRGB-like gamma curve (BT.709 transfer function ≈ sRGB).
    fn drm_fourcc_to_vk_format(fourcc: u32) -> Option<vk::Format> {
        match fourcc {
            // ARGB8888: bytes [B, G, R, A] in memory = B8G8R8A8_SRGB
            drm_fourcc::DRM_FORMAT_ARGB8888 | drm_fourcc::DRM_FORMAT_XRGB8888 |
            drm_fourcc::DRM_FORMAT_BGRA8888 | drm_fourcc::DRM_FORMAT_BGRX8888 => {
                Some(vk::Format::B8G8R8A8_SRGB)
            }
            drm_fourcc::DRM_FORMAT_ABGR8888 | drm_fourcc::DRM_FORMAT_XBGR8888 |
            drm_fourcc::DRM_FORMAT_RGBA8888 | drm_fourcc::DRM_FORMAT_RGBX8888 => {
                Some(vk::Format::R8G8B8A8_SRGB)
            }
            _ => None,
        }
    }

    /// Import DMA-BUF as wgpu texture using Vulkan HAL
    ///
    /// This is the true zero-copy path that:
    /// 1. Creates a VkImage with external memory
    /// 2. Imports the DMA-BUF fd with DRM modifier
    /// 3. Wraps as wgpu texture via HAL
    pub fn import_dmabuf_hal(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        params: &DmaBufImportParams,
    ) -> Option<wgpu::Texture> {
        use wgpu::hal::api::Vulkan;

        let vk_format = drm_fourcc_to_vk_format(params.fourcc)?;
        let wgpu_format = drm_fourcc_to_wgpu_format(params.fourcc)?;

        log::info!(
            "Attempting true zero-copy DMA-BUF import: fd={}, {}x{}, fourcc={:#x}, modifier={:#x}",
            params.fd, params.width, params.height, params.fourcc, params.modifier
        );

        // Access raw Vulkan device via wgpu HAL
        unsafe {
            device.as_hal::<Vulkan, _, _>(|hal_device| {
                let hal_device = hal_device?;

                // Get raw Vulkan handles
                let raw_device = hal_device.raw_device();

                import_dmabuf_vulkan(
                    device,
                    raw_device,
                    params,
                    vk_format,
                    wgpu_format,
                )
            }).flatten()
        }
    }

    /// Core Vulkan DMA-BUF import
    unsafe fn import_dmabuf_vulkan(
        wgpu_device: &wgpu::Device,
        vk_device: &ash::Device,
        params: &DmaBufImportParams,
        vk_format: vk::Format,
        wgpu_format: wgpu::TextureFormat,
    ) -> Option<wgpu::Texture> {
        // Dup the fd since Vulkan takes ownership
        let fd_dup = libc::dup(params.fd);
        if fd_dup < 0 {
            log::warn!("Failed to dup DMA-BUF fd");
            return None;
        }

        // Step 1: Set up DRM format modifier plane layout
        let subresource_layout = vk::SubresourceLayout {
            offset: params.offset as u64,
            size: 0, // Ignored for single-plane formats
            row_pitch: params.stride as u64,
            array_pitch: 0,
            depth_pitch: 0,
        };

        // Build the pNext chain manually using raw pointers
        // VkImageDrmFormatModifierExplicitCreateInfoEXT
        let mut drm_modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT {
            s_type: vk::StructureType::IMAGE_DRM_FORMAT_MODIFIER_EXPLICIT_CREATE_INFO_EXT,
            p_next: std::ptr::null(),
            drm_format_modifier: params.modifier,
            drm_format_modifier_plane_count: 1,
            p_plane_layouts: &subresource_layout,
            ..Default::default()
        };

        // VkExternalMemoryImageCreateInfo
        let mut external_memory_info = vk::ExternalMemoryImageCreateInfo {
            s_type: vk::StructureType::EXTERNAL_MEMORY_IMAGE_CREATE_INFO,
            p_next: &mut drm_modifier_info as *mut _ as *mut std::ffi::c_void,
            handle_types: vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            ..Default::default()
        };

        // Step 2: Create VkImage with external memory
        let image_info = vk::ImageCreateInfo {
            s_type: vk::StructureType::IMAGE_CREATE_INFO,
            p_next: &mut external_memory_info as *mut _ as *mut std::ffi::c_void,
            flags: vk::ImageCreateFlags::empty(),
            image_type: vk::ImageType::TYPE_2D,
            format: vk_format,
            extent: vk::Extent3D {
                width: params.width,
                height: params.height,
                depth: 1,
            },
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT,
            usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            queue_family_index_count: 0,
            p_queue_family_indices: std::ptr::null(),
            initial_layout: vk::ImageLayout::UNDEFINED,
            ..Default::default()
        };

        let image = match vk_device.create_image(&image_info, None) {
            Ok(img) => img,
            Err(e) => {
                log::warn!("Failed to create Vulkan image for DMA-BUF: {:?}", e);
                libc::close(fd_dup);
                return None;
            }
        };

        // Step 3: Get memory requirements
        let mem_requirements = vk_device.get_image_memory_requirements(image);

        log::debug!(
            "VkImage memory requirements: size={}, alignment={}, memory_type_bits={:#x}",
            mem_requirements.size, mem_requirements.alignment, mem_requirements.memory_type_bits
        );

        // Step 4: Import DMA-BUF fd as VkDeviceMemory
        // Build pNext chain for memory allocation
        let mut import_fd_info = vk::ImportMemoryFdInfoKHR {
            s_type: vk::StructureType::IMPORT_MEMORY_FD_INFO_KHR,
            p_next: std::ptr::null(),
            handle_type: vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            fd: fd_dup,
            ..Default::default()
        };

        let mut dedicated_alloc_info = vk::MemoryDedicatedAllocateInfo {
            s_type: vk::StructureType::MEMORY_DEDICATED_ALLOCATE_INFO,
            p_next: &mut import_fd_info as *mut _ as *mut std::ffi::c_void,
            image,
            buffer: vk::Buffer::null(),
            ..Default::default()
        };

        // Find memory type that supports device local
        let memory_type_index = find_memory_type(
            mem_requirements.memory_type_bits,
        )?;

        let alloc_info = vk::MemoryAllocateInfo {
            s_type: vk::StructureType::MEMORY_ALLOCATE_INFO,
            p_next: &mut dedicated_alloc_info as *mut _ as *mut std::ffi::c_void,
            allocation_size: mem_requirements.size,
            memory_type_index,
            ..Default::default()
        };

        let memory = match vk_device.allocate_memory(&alloc_info, None) {
            Ok(mem) => mem,
            Err(e) => {
                log::warn!("Failed to import DMA-BUF memory: {:?}", e);
                vk_device.destroy_image(image, None);
                // fd is consumed by import even on failure
                return None;
            }
        };

        // Step 5: Bind memory to image
        if let Err(e) = vk_device.bind_image_memory(image, memory, 0) {
            log::warn!("Failed to bind DMA-BUF memory to image: {:?}", e);
            vk_device.free_memory(memory, None);
            vk_device.destroy_image(image, None);
            return None;
        }

        log::info!(
            "Successfully created Vulkan image from DMA-BUF: image={:?}, memory={:?}",
            image, memory
        );

        // Step 6: Wrap as wgpu texture via HAL
        // We need to keep the resources alive
        let resources = Arc::new(ImportedDmaBufResources {
            device: vk_device.clone(),
            image,
            memory,
        });

        // Create HAL texture descriptor
        let hal_desc = wgpu_hal::TextureDescriptor {
            label: Some("DMA-BUF imported texture"),
            size: wgpu::Extent3d {
                width: params.width,
                height: params.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format,
            usage: wgpu_hal::TextureUses::RESOURCE,
            memory_flags: wgpu_hal::MemoryFlags::empty(),
            view_formats: vec![],
        };

        // Create drop callback to clean up resources
        // Use Option to allow the Arc to be taken once (FnMut requirement)
        let resources_clone = Some(resources.clone());
        let mut resources_opt = resources_clone;
        let drop_callback: wgpu_hal::DropCallback = Box::new(move || {
            if let Some(res) = resources_opt.take() {
                drop(res);
            }
        });

        // Create HAL texture from raw Vulkan image
        // texture_from_raw is a static function in wgpu-hal v23
        let hal_texture = wgpu_hal::vulkan::Device::texture_from_raw(
            image,
            &hal_desc,
            Some(drop_callback),
        );

        // Wrap HAL texture as wgpu::Texture
        let wgpu_texture = wgpu_device.create_texture_from_hal::<wgpu::hal::api::Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("DMA-BUF zero-copy texture"),
                size: wgpu::Extent3d {
                    width: params.width,
                    height: params.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
        );

        log::info!("Successfully created wgpu texture from DMA-BUF (true zero-copy!)");
        Some(wgpu_texture)
    }

    /// Find suitable memory type
    fn find_memory_type(
        type_filter: u32,
    ) -> Option<u32> {
        // Return the first compatible memory type
        // For external memory import, the driver typically provides the correct type
        for i in 0..32 {
            if (type_filter & (1 << i)) != 0 {
                return Some(i);
            }
        }
        None
    }
}

/// Import DMA-BUF as wgpu texture - main entry point
#[cfg(target_os = "linux")]
pub fn import_dmabuf(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    params: &DmaBufImportParams,
) -> Option<wgpu::Texture> {
    // Try true zero-copy via HAL first
    #[cfg(all(feature = "ash", feature = "wgpu-hal"))]
    {
        if let Some(texture) = hal_import::import_dmabuf_hal(device, queue, params) {
            return Some(texture);
        }
        log::debug!("HAL DMA-BUF import failed, falling back to mmap");
    }

    // Fall back to mmap-based import (copies to CPU then GPU)
    import_dmabuf_via_mmap(device, queue, params)
}

/// Fallback: read DMA-BUF contents via mmap and create texture via CPU copy
///
/// This is slower than zero-copy but works without special Vulkan extensions.
#[cfg(target_os = "linux")]
pub fn import_dmabuf_via_mmap(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    params: &DmaBufImportParams,
) -> Option<wgpu::Texture> {
    let wgpu_format = drm_fourcc_to_wgpu_format(params.fourcc)?;

    // Calculate expected size
    let expected_size = (params.stride * params.height) as usize;

    // Dup the fd so we don't close the original
    let fd_dup = unsafe { libc::dup(params.fd) };
    if fd_dup < 0 {
        log::warn!("import_dmabuf_via_mmap: failed to dup fd");
        return None;
    }

    // mmap the DMA-BUF
    let data = unsafe {
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            expected_size,
            libc::PROT_READ,
            libc::MAP_SHARED,
            fd_dup,
            params.offset as i64,
        );

        if ptr == libc::MAP_FAILED {
            libc::close(fd_dup);
            log::warn!("import_dmabuf_via_mmap: mmap failed");
            return None;
        }

        // Copy data out of mmap
        let slice = std::slice::from_raw_parts(ptr as *const u8, expected_size);
        let data = slice.to_vec();

        // Unmap and close
        libc::munmap(ptr, expected_size);
        libc::close(fd_dup);

        data
    };

    // Create texture
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("DMA-BUF mmap texture"),
        size: wgpu::Extent3d {
            width: params.width,
            height: params.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu_format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    // Upload data
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(params.stride),
            rows_per_image: Some(params.height),
        },
        wgpu::Extent3d {
            width: params.width,
            height: params.height,
            depth_or_array_layers: 1,
        },
    );

    log::debug!(
        "import_dmabuf_via_mmap: created {}x{} texture via mmap (not zero-copy)",
        params.width,
        params.height
    );

    Some(texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drm_fourcc_to_wgpu() {
        // sRGB formats for proper color rendering
        assert_eq!(
            drm_fourcc_to_wgpu_format(drm_fourcc::DRM_FORMAT_ARGB8888),
            Some(wgpu::TextureFormat::Bgra8UnormSrgb)
        );
        assert_eq!(
            drm_fourcc_to_wgpu_format(drm_fourcc::DRM_FORMAT_RGBA8888),
            Some(wgpu::TextureFormat::Rgba8UnormSrgb)
        );
    }
}
