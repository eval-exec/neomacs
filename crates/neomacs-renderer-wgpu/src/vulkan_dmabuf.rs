//! Vulkan DMA-BUF import for zero-copy texture sharing.
//!
//! This module implements importing DMA-BUF file descriptors as Vulkan images,
//! then wrapping them as wgpu textures for zero-copy GPU-to-GPU transfers.
//!
//! Supports multi-plane DMA-BUFs (e.g., AMD DCC, Intel CCS) by querying the
//! Vulkan driver for modifier properties to determine correct plane counts.
//!
//! Required Vulkan extensions:
//! - VK_KHR_external_memory
//! - VK_KHR_external_memory_fd
//! - VK_EXT_external_memory_dma_buf
//! - VK_EXT_image_drm_format_modifier

// The ash Vulkan structs are chained by writing `p_next` after `default()` so
// the pointer targets a local declared on the preceding line; functional-record
// update on these external `#[repr(C)]` types is impractical, so the
// field-reassign lint is allowed module-wide.
#![allow(clippy::field_reassign_with_default)]

#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

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
    pub const DRM_FORMAT_NV12: u32 = 0x3231564e; // NV12 (YUV 4:2:0)
    pub const DRM_FORMAT_YUV420: u32 = 0x32315559; // YUV420

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
        drm_fourcc::DRM_FORMAT_ARGB8888
        | drm_fourcc::DRM_FORMAT_XRGB8888
        | drm_fourcc::DRM_FORMAT_BGRA8888
        | drm_fourcc::DRM_FORMAT_BGRX8888 => Some(wgpu::TextureFormat::Bgra8UnormSrgb),
        drm_fourcc::DRM_FORMAT_ABGR8888
        | drm_fourcc::DRM_FORMAT_XBGR8888
        | drm_fourcc::DRM_FORMAT_RGBA8888
        | drm_fourcc::DRM_FORMAT_RGBX8888 => Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        _ => None,
    }
}

/// Maximum number of DMA-BUF planes supported.
pub const MAX_PLANES: usize = 4;

/// Multi-plane DMA-BUF import parameters.
///
/// The fds are *borrowed* for the duration of the import: the importer dup()s
/// each one before handing it to Vulkan, so it never owns or closes them. The
/// `'a` lifetime ties these params to the owning buffer (e.g. `DmaBufBuffer`).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub struct DmaBufImportParams<'a> {
    /// DMA-BUF file descriptors, one per plane (borrowed, not owned).
    pub fds: Vec<BorrowedFd<'a>>,
    /// Stride (bytes per row) per plane.
    pub strides: Vec<u32>,
    /// Byte offset per plane.
    pub offsets: Vec<u32>,
    /// Number of planes provided by the buffer.
    pub num_planes: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// DRM fourcc format code.
    pub fourcc: u32,
    /// DRM format modifier.
    pub modifier: u64,
}

// ============================================================================
// True Zero-Copy Implementation using wgpu_hal
// ============================================================================

#[cfg(target_os = "linux")]
mod hal_import {
    #![allow(dead_code)]
    #![allow(unsafe_op_in_unsafe_fn)]

    use super::*;
    use ash::vk;
    // AsRawFd, BorrowedFd, FromRawFd, OwnedFd arrive via `use super::*`; only
    // IntoRawFd (fd release on import success) and RawFd (FFI helper params) are
    // unique to this module.
    use std::os::fd::{IntoRawFd, RawFd};
    use std::sync::Arc;

    // ========================================================================
    // Resource management
    // ========================================================================

    /// Resources that must be kept alive for the lifetime of an imported texture.
    ///
    /// For non-disjoint imports, `memories` contains a single entry.
    /// For disjoint imports, it contains one entry per plane.
    pub(super) struct ImportedDmaBufResources {
        device: ash::Device,
        image: vk::Image,
        memories: Vec<vk::DeviceMemory>,
    }

    impl Drop for ImportedDmaBufResources {
        fn drop(&mut self) {
            unsafe {
                self.device.destroy_image(self.image, None);
                for &mem in &self.memories {
                    self.device.free_memory(mem, None);
                }
            }
            tracing::info!(
                "Cleaned up imported DMA-BUF resources ({} memory allocations)",
                self.memories.len()
            );
        }
    }

    // ========================================================================
    // Format conversion
    // ========================================================================

    /// Convert DRM fourcc to Vulkan format (sRGB variants for correct gamma).
    fn drm_fourcc_to_vk_format(fourcc: u32) -> Option<vk::Format> {
        match fourcc {
            drm_fourcc::DRM_FORMAT_ARGB8888
            | drm_fourcc::DRM_FORMAT_XRGB8888
            | drm_fourcc::DRM_FORMAT_BGRA8888
            | drm_fourcc::DRM_FORMAT_BGRX8888 => Some(vk::Format::B8G8R8A8_SRGB),
            drm_fourcc::DRM_FORMAT_ABGR8888
            | drm_fourcc::DRM_FORMAT_XBGR8888
            | drm_fourcc::DRM_FORMAT_RGBA8888
            | drm_fourcc::DRM_FORMAT_RGBX8888 => Some(vk::Format::R8G8B8A8_SRGB),
            _ => None,
        }
    }

    // ========================================================================
    // Modifier query
    // ========================================================================

    /// Result of querying a DRM format modifier from the Vulkan driver.
    #[derive(Debug, Clone, Copy)]
    struct ModifierProperties {
        plane_count: u32,
        tiling_features: vk::FormatFeatureFlags,
    }

    /// Query the Vulkan driver for properties of a specific DRM format modifier.
    ///
    /// Returns the required plane count and supported tiling features, or `None`
    /// if the modifier is not supported for the given format.
    unsafe fn query_modifier_properties(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        vk_format: vk::Format,
        modifier: u64,
    ) -> Option<ModifierProperties> {
        // First call: get the count of supported modifiers
        let mut modifier_list = vk::DrmFormatModifierPropertiesListEXT::default();
        let mut format_props = vk::FormatProperties2::default();
        format_props.p_next = &mut modifier_list as *mut _ as *mut std::ffi::c_void;

        instance.get_physical_device_format_properties2(
            physical_device,
            vk_format,
            &mut format_props,
        );

        let count = modifier_list.drm_format_modifier_count;
        if count == 0 {
            tracing::debug!("No DRM modifiers supported for format {:?}", vk_format);
            return None;
        }

        // Second call: fill the modifier properties array
        let mut properties = vec![vk::DrmFormatModifierPropertiesEXT::default(); count as usize];
        modifier_list.drm_format_modifier_count = count;
        modifier_list.p_drm_format_modifier_properties = properties.as_mut_ptr();

        let mut format_props2 = vk::FormatProperties2::default();
        format_props2.p_next = &mut modifier_list as *mut _ as *mut std::ffi::c_void;

        instance.get_physical_device_format_properties2(
            physical_device,
            vk_format,
            &mut format_props2,
        );

        // Find the entry matching our modifier
        for prop in &properties {
            if prop.drm_format_modifier == modifier {
                tracing::info!(
                    "Modifier {:#x} supported: plane_count={}, features={:?}",
                    modifier,
                    prop.drm_format_modifier_plane_count,
                    prop.drm_format_modifier_tiling_features
                );
                return Some(ModifierProperties {
                    plane_count: prop.drm_format_modifier_plane_count,
                    tiling_features: prop.drm_format_modifier_tiling_features,
                });
            }
        }

        tracing::debug!(
            "Modifier {:#x} not in the {} modifiers supported for format {:?}",
            modifier,
            count,
            vk_format
        );
        None
    }

    // ========================================================================
    // Disjoint fd detection
    // ========================================================================

    /// Check whether the provided fds refer to distinct DMA-BUF allocations.
    ///
    /// If all fds share the same inode (same underlying buffer), they are
    /// "non-disjoint" and can share a single VkDeviceMemory binding.
    /// If they differ, the image needs `VK_IMAGE_CREATE_DISJOINT_BIT` and
    /// per-plane memory bindings.
    unsafe fn are_fds_disjoint(fds: &[BorrowedFd<'_>]) -> bool {
        if fds.len() <= 1 {
            return false;
        }
        let mut first_ino: u64 = 0;
        for (i, &fd) in fds.iter().enumerate() {
            let mut stat: libc::stat = std::mem::zeroed();
            if libc::fstat(fd.as_raw_fd(), &mut stat) != 0 {
                tracing::warn!(
                    "fstat failed on DMA-BUF fd {}, assuming disjoint",
                    fd.as_raw_fd()
                );
                return true;
            }
            if i == 0 {
                first_ino = stat.st_ino;
            } else if stat.st_ino != first_ino {
                tracing::debug!(
                    "DMA-BUF fds are disjoint: fd[0] ino={}, fd[{}] ino={}",
                    first_ino,
                    i,
                    stat.st_ino
                );
                return true;
            }
        }
        false
    }

    // ========================================================================
    // Plane layout helpers
    // ========================================================================

    /// Map a plane index to its `VK_IMAGE_ASPECT_MEMORY_PLANE_*_BIT_EXT`.
    fn memory_plane_aspect(plane_index: u32) -> vk::ImageAspectFlags {
        match plane_index {
            0 => vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
            1 => vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
            2 => vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
            3 => vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
            _ => {
                tracing::error!("Unsupported plane index {}", plane_index);
                vk::ImageAspectFlags::MEMORY_PLANE_0_EXT
            }
        }
    }

    /// Build per-plane `VkSubresourceLayout` entries from import params.
    ///
    /// The `driver_plane_count` (from the modifier query) may exceed the number
    /// of planes the buffer actually provides. Missing planes get zero layout.
    fn build_plane_layouts(
        params: &DmaBufImportParams<'_>,
        driver_plane_count: u32,
    ) -> Vec<vk::SubresourceLayout> {
        (0..driver_plane_count)
            .map(|i| {
                let idx = i as usize;
                vk::SubresourceLayout {
                    offset: params.offsets.get(idx).copied().unwrap_or(0) as u64,
                    size: 0, // Must be 0 — driver calculates it
                    row_pitch: params.strides.get(idx).copied().unwrap_or(0) as u64,
                    array_pitch: 0,
                    depth_pitch: 0,
                }
            })
            .collect()
    }

    // ========================================================================
    // Memory allocation helpers
    // ========================================================================

    /// Find the first compatible memory type index.
    fn find_memory_type(type_filter: u32) -> Option<u32> {
        (0..32).find(|&i| (type_filter & (1 << i)) != 0)
    }

    /// Duplicate an fd into a fresh owned fd, returning `None` on failure.
    unsafe fn dup_fd(fd: RawFd) -> Option<OwnedFd> {
        let duped = libc::dup(fd);
        if duped < 0 {
            tracing::warn!("Failed to dup DMA-BUF fd {}", fd);
            None
        } else {
            // `dup` returns a brand-new descriptor we exclusively own.
            Some(OwnedFd::from_raw_fd(duped))
        }
    }

    /// Allocate and import a DMA-BUF fd as `VkDeviceMemory`.
    unsafe fn import_fd_as_memory(
        vk_device: &ash::Device,
        fd: RawFd,
        image: vk::Image,
        allocation_size: u64,
        memory_type_index: u32,
    ) -> Option<vk::DeviceMemory> {
        // Own the dup for the duration of the import; Vulkan consumes it only on
        // a successful allocate_memory (see the match below).
        let fd_dup = dup_fd(fd)?;

        let mut import_fd_info = vk::ImportMemoryFdInfoKHR {
            s_type: vk::StructureType::IMPORT_MEMORY_FD_INFO_KHR,
            p_next: std::ptr::null(),
            handle_type: vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            // Lend the raw fd to the FFI struct; `fd_dup` stays the owner until
            // the import outcome is known.
            fd: fd_dup.as_raw_fd(),
            ..Default::default()
        };

        let mut dedicated_alloc_info = vk::MemoryDedicatedAllocateInfo {
            s_type: vk::StructureType::MEMORY_DEDICATED_ALLOCATE_INFO,
            p_next: &mut import_fd_info as *mut _ as *mut std::ffi::c_void,
            image,
            buffer: vk::Buffer::null(),
            ..Default::default()
        };

        let alloc_info = vk::MemoryAllocateInfo {
            s_type: vk::StructureType::MEMORY_ALLOCATE_INFO,
            p_next: &mut dedicated_alloc_info as *mut _ as *mut std::ffi::c_void,
            allocation_size,
            memory_type_index,
            ..Default::default()
        };

        match vk_device.allocate_memory(&alloc_info, None) {
            Ok(mem) => {
                // Per VK_KHR_external_memory_fd: on success the implementation
                // takes ownership of the fd. Release our OwnedFd without closing
                // so the now driver-owned descriptor is not double-closed.
                let _ = fd_dup.into_raw_fd();
                Some(mem)
            }
            Err(e) => {
                tracing::warn!("Failed to import DMA-BUF memory (fd={}): {:?}", fd, e);
                // Per VK_KHR_external_memory_fd: on failure the fd is NOT
                // consumed — dropping `fd_dup` here closes it, avoiding a leak.
                drop(fd_dup);
                None
            }
        }
    }

    // ========================================================================
    // Non-disjoint memory binding (single memory for all planes)
    // ========================================================================

    /// Bind a single `VkDeviceMemory` to a non-disjoint image.
    unsafe fn bind_non_disjoint(
        vk_device: &ash::Device,
        image: vk::Image,
        params: &DmaBufImportParams<'_>,
    ) -> Option<Vec<vk::DeviceMemory>> {
        let mem_requirements = vk_device.get_image_memory_requirements(image);

        tracing::debug!(
            "Non-disjoint memory requirements: size={}, alignment={}, type_bits={:#x}",
            mem_requirements.size,
            mem_requirements.alignment,
            mem_requirements.memory_type_bits
        );

        let memory_type_index = find_memory_type(mem_requirements.memory_type_bits)?;
        let memory = import_fd_as_memory(
            vk_device,
            params.fds[0].as_raw_fd(),
            image,
            mem_requirements.size,
            memory_type_index,
        )?;

        if let Err(e) = vk_device.bind_image_memory(image, memory, 0) {
            tracing::warn!("Failed to bind non-disjoint DMA-BUF memory: {:?}", e);
            vk_device.free_memory(memory, None);
            return None;
        }

        Some(vec![memory])
    }

    // ========================================================================
    // Disjoint memory binding (per-plane memory)
    // ========================================================================

    /// Bind separate `VkDeviceMemory` per plane for a disjoint image.
    ///
    /// Uses `vkBindImageMemory2` with `VkBindImagePlaneMemoryInfo` to bind
    /// each plane's memory independently.
    unsafe fn bind_disjoint(
        vk_device: &ash::Device,
        image: vk::Image,
        params: &DmaBufImportParams<'_>,
        driver_plane_count: u32,
    ) -> Option<Vec<vk::DeviceMemory>> {
        let mut memories = Vec::with_capacity(driver_plane_count as usize);

        for plane_idx in 0..driver_plane_count {
            let aspect = memory_plane_aspect(plane_idx);

            // Query per-plane memory requirements
            let mut plane_req_info =
                vk::ImagePlaneMemoryRequirementsInfo::default().plane_aspect(aspect);
            let mut req_info = vk::ImageMemoryRequirementsInfo2::default().image(image);
            req_info.p_next = &mut plane_req_info as *mut _ as *const std::ffi::c_void;

            let mut mem_req2 = vk::MemoryRequirements2::default();
            vk_device.get_image_memory_requirements2(&req_info, &mut mem_req2);

            let mem_req = mem_req2.memory_requirements;
            tracing::debug!(
                "Disjoint plane {} memory requirements: size={}, type_bits={:#x}",
                plane_idx,
                mem_req.size,
                mem_req.memory_type_bits
            );

            let memory_type_index = match find_memory_type(mem_req.memory_type_bits) {
                Some(idx) => idx,
                None => {
                    tracing::warn!("No suitable memory type for disjoint plane {}", plane_idx);
                    // Clean up already-allocated memories
                    for &mem in &memories {
                        vk_device.free_memory(mem, None);
                    }
                    return None;
                }
            };

            let fd_idx = (plane_idx as usize).min(params.fds.len() - 1);
            let memory = match import_fd_as_memory(
                vk_device,
                params.fds[fd_idx].as_raw_fd(),
                image,
                mem_req.size,
                memory_type_index,
            ) {
                Some(m) => m,
                None => {
                    for &mem in &memories {
                        vk_device.free_memory(mem, None);
                    }
                    return None;
                }
            };

            memories.push(memory);
        }

        // Bind all planes at once via vkBindImageMemory2
        let mut plane_infos: Vec<vk::BindImagePlaneMemoryInfo> =
            Vec::with_capacity(driver_plane_count as usize);
        let mut bind_infos: Vec<vk::BindImageMemoryInfo> =
            Vec::with_capacity(driver_plane_count as usize);

        for plane_idx in 0..driver_plane_count {
            plane_infos.push(
                vk::BindImagePlaneMemoryInfo::default()
                    .plane_aspect(memory_plane_aspect(plane_idx)),
            );
        }

        for plane_idx in 0..driver_plane_count as usize {
            let mut info = vk::BindImageMemoryInfo::default()
                .image(image)
                .memory(memories[plane_idx])
                .memory_offset(0);
            info.p_next = &mut plane_infos[plane_idx] as *mut _ as *const std::ffi::c_void;
            bind_infos.push(info);
        }

        if let Err(e) = vk_device.bind_image_memory2(&bind_infos) {
            tracing::warn!("vkBindImageMemory2 failed for disjoint DMA-BUF: {:?}", e);
            for &mem in &memories {
                vk_device.free_memory(mem, None);
            }
            return None;
        }

        Some(memories)
    }

    // ========================================================================
    // Texture wrapping
    // ========================================================================

    /// Wrap a Vulkan image + memory as a wgpu texture via HAL.
    unsafe fn wrap_as_wgpu_texture(
        wgpu_device: &wgpu::Device,
        hal_device: &wgpu_hal::vulkan::Device,
        vk_device: &ash::Device,
        image: vk::Image,
        memories: Vec<vk::DeviceMemory>,
        params: &DmaBufImportParams<'_>,
        wgpu_format: wgpu::TextureFormat,
    ) -> wgpu::Texture {
        let resources = Arc::new(ImportedDmaBufResources {
            device: vk_device.clone(),
            image,
            memories,
        });

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
            usage: wgpu::TextureUses::RESOURCE,
            memory_flags: wgpu_hal::MemoryFlags::empty(),
            view_formats: vec![],
        };

        let mut resources_opt = Some(resources);
        let drop_callback: wgpu_hal::DropCallback = Box::new(move || {
            drop(resources_opt.take());
        });

        let hal_texture = hal_device.texture_from_raw(
            image,
            &hal_desc,
            Some(drop_callback),
            wgpu_hal::vulkan::TextureMemory::External,
        );

        wgpu_device.create_texture_from_hal::<wgpu::hal::api::Vulkan>(
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
            wgpu::TextureUses::UNINITIALIZED,
        )
    }

    // ========================================================================
    // Main entry point
    // ========================================================================

    /// Import DMA-BUF as wgpu texture using Vulkan HAL (true zero-copy).
    ///
    /// 1. Queries the Vulkan driver for modifier support and required plane count
    /// 2. Creates a VkImage with the correct multi-plane layout
    /// 3. Detects disjoint vs non-disjoint fds and binds memory accordingly
    /// 4. Wraps as wgpu texture via HAL
    pub fn import_dmabuf_hal(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        params: &DmaBufImportParams<'_>,
    ) -> Option<wgpu::Texture> {
        use wgpu::hal::api::Vulkan;

        let vk_format = drm_fourcc_to_vk_format(params.fourcc)?;
        let wgpu_format = drm_fourcc_to_wgpu_format(params.fourcc)?;

        tracing::info!(
            "Attempting DMA-BUF import: {}x{}, fourcc={:#x}, modifier={:#x}, {} planes, fds={:?}",
            params.width,
            params.height,
            params.fourcc,
            params.modifier,
            params.num_planes,
            &params.fds[..params.num_planes as usize]
        );

        unsafe {
            let hal_device_guard = device.as_hal::<Vulkan>()?;
            let hal_device = &*hal_device_guard;
            let vk_device = hal_device.raw_device();
            let physical_device = hal_device.raw_physical_device();
            let instance = hal_device.shared_instance().raw_instance();
            import_dmabuf_vulkan(
                device,
                hal_device,
                vk_device,
                instance,
                physical_device,
                params,
                vk_format,
                wgpu_format,
            )
        }
    }

    /// Core Vulkan DMA-BUF import with modifier query and multi-plane support.
    unsafe fn import_dmabuf_vulkan(
        wgpu_device: &wgpu::Device,
        hal_device: &wgpu_hal::vulkan::Device,
        vk_device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        params: &DmaBufImportParams<'_>,
        vk_format: vk::Format,
        wgpu_format: wgpu::TextureFormat,
    ) -> Option<wgpu::Texture> {
        // Step 1: Query driver for modifier plane count
        let modifier_props =
            query_modifier_properties(instance, physical_device, vk_format, params.modifier)?;

        let driver_plane_count = modifier_props.plane_count;
        if driver_plane_count == 0 || driver_plane_count > MAX_PLANES as u32 {
            tracing::warn!(
                "Driver reports invalid plane count {} for modifier {:#x}",
                driver_plane_count,
                params.modifier
            );
            return None;
        }

        // Step 2: Determine if fds are disjoint (different DMA-BUF allocations)
        let active_fds = &params.fds[..params.num_planes.min(driver_plane_count) as usize];
        let disjoint = are_fds_disjoint(active_fds);

        tracing::debug!(
            "Import config: driver_plane_count={}, buffer_planes={}, disjoint={}",
            driver_plane_count,
            params.num_planes,
            disjoint
        );

        // Step 3: Build plane layouts from import params
        let plane_layouts = build_plane_layouts(params, driver_plane_count);

        // Step 4: Build pNext chain for image creation
        let mut drm_modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT {
            s_type: vk::StructureType::IMAGE_DRM_FORMAT_MODIFIER_EXPLICIT_CREATE_INFO_EXT,
            p_next: std::ptr::null(),
            drm_format_modifier: params.modifier,
            drm_format_modifier_plane_count: driver_plane_count,
            p_plane_layouts: plane_layouts.as_ptr(),
            ..Default::default()
        };

        let mut external_memory_info = vk::ExternalMemoryImageCreateInfo {
            s_type: vk::StructureType::EXTERNAL_MEMORY_IMAGE_CREATE_INFO,
            p_next: &mut drm_modifier_info as *mut _ as *mut std::ffi::c_void,
            handle_types: vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            ..Default::default()
        };

        let image_flags = if disjoint {
            vk::ImageCreateFlags::DISJOINT
        } else {
            vk::ImageCreateFlags::empty()
        };

        let image_info = vk::ImageCreateInfo {
            s_type: vk::StructureType::IMAGE_CREATE_INFO,
            p_next: &mut external_memory_info as *mut _ as *mut std::ffi::c_void,
            flags: image_flags,
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

        // Step 5: Create VkImage
        let image = match vk_device.create_image(&image_info, None) {
            Ok(img) => img,
            Err(e) => {
                tracing::warn!("Failed to create Vulkan image for DMA-BUF: {:?}", e);
                return None;
            }
        };

        // Step 6: Bind memory (disjoint or non-disjoint)
        let memories = if disjoint {
            match bind_disjoint(vk_device, image, params, driver_plane_count) {
                Some(m) => m,
                None => {
                    vk_device.destroy_image(image, None);
                    return None;
                }
            }
        } else {
            match bind_non_disjoint(vk_device, image, params) {
                Some(m) => m,
                None => {
                    vk_device.destroy_image(image, None);
                    return None;
                }
            }
        };

        // Layout transition from UNDEFINED → SHADER_READ_ONLY_OPTIMAL is NOT
        // done here.  wgpu's Vulkan backend tracks texture state and inserts
        // the required pipeline barrier when the texture is first bound for
        // sampling (or used as a copy source).
        //
        // The previous approach submitted a raw vkQueueSubmit + vkQueueWaitIdle
        // per frame, which bypassed wgpu's fence tracking.  On AMD RADV each
        // queue submission creates DRM syncobj fds; because wgpu didn't know
        // about these submissions, the associated sync fds were never reclaimed,
        // causing progressive fd exhaustion and eventual crash.

        tracing::info!(
            "Vulkan DMA-BUF import succeeded: image={:?}, {} memory bindings, disjoint={}",
            image,
            memories.len(),
            disjoint
        );

        // Step 8: Wrap as wgpu texture
        let texture = wrap_as_wgpu_texture(
            wgpu_device,
            hal_device,
            vk_device,
            image,
            memories,
            params,
            wgpu_format,
        );
        Some(texture)
    }
}

/// Import DMA-BUF as wgpu texture — main entry point.
///
/// Fallback chain:
/// 1. Vulkan HAL zero-copy (queries driver for modifier support)
/// 2. mmap + CPU upload (only for linear modifiers)
#[cfg(target_os = "linux")]
pub fn import_dmabuf_external(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    params: &DmaBufImportParams<'_>,
) -> Option<wgpu::Texture> {
    if let Some(texture) = hal_import::import_dmabuf_hal(device, queue, params) {
        return Some(texture);
    }
    tracing::debug!("HAL DMA-BUF import failed");
    None
}

#[cfg(target_os = "linux")]
pub fn import_dmabuf(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    params: &DmaBufImportParams<'_>,
) -> Option<wgpu::Texture> {
    if let Some(texture) = import_dmabuf_external(device, queue, params) {
        return Some(texture);
    }
    tracing::debug!("falling back to linear DMA-BUF mmap import");

    // Fall back to mmap-based import (copies to CPU then GPU)
    import_dmabuf_via_mmap(device, queue, params)
}

/// Fallback: read DMA-BUF contents via mmap and create texture via CPU copy.
///
/// Only works for linear (untiled) buffers. Tiled/compressed modifiers cannot
/// be read as linear memory, so this will skip them.
#[cfg(target_os = "linux")]
pub fn import_dmabuf_via_mmap(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    params: &DmaBufImportParams<'_>,
) -> Option<wgpu::Texture> {
    // Only attempt mmap for linear buffers — tiled data can't be read linearly
    if params.modifier != drm_fourcc::DRM_FORMAT_MOD_LINEAR
        && params.modifier != drm_fourcc::DRM_FORMAT_MOD_INVALID
    {
        tracing::debug!(
            "Skipping mmap for non-linear modifier {:#x} (tiled/compressed data)",
            params.modifier
        );
        return None;
    }

    if params.fds.is_empty() {
        tracing::warn!("import_dmabuf_via_mmap: no fds provided");
        return None;
    }

    let wgpu_format = drm_fourcc_to_wgpu_format(params.fourcc)?;

    let stride = params.strides.first().copied().unwrap_or(0);
    let offset = params.offsets.first().copied().unwrap_or(0);
    // Borrowed view of the caller's fd; we dup() below and never close this one.
    let fd = params.fds[0].as_raw_fd();

    let expected_size = (stride * params.height) as usize;

    // Dup the fd so we don't close the original. Own the dup so it is closed on
    // every exit path (dup failure aside), including the mmap-failure return.
    let raw_dup = unsafe { libc::dup(fd) };
    if raw_dup < 0 {
        tracing::warn!("import_dmabuf_via_mmap: failed to dup fd");
        return None;
    }
    let fd_dup = unsafe { OwnedFd::from_raw_fd(raw_dup) };

    // mmap the DMA-BUF
    let data = unsafe {
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            expected_size,
            libc::PROT_READ,
            libc::MAP_SHARED,
            fd_dup.as_raw_fd(),
            offset as i64,
        );

        if ptr == libc::MAP_FAILED {
            // `fd_dup` closes when it drops at function return.
            tracing::warn!("import_dmabuf_via_mmap: mmap failed");
            return None;
        }

        let slice = std::slice::from_raw_parts(ptr as *const u8, expected_size);
        let data = slice.to_vec();

        libc::munmap(ptr, expected_size);

        data
    };
    // The mmap'd copy now lives in `data`; drop the dup to close it promptly.
    drop(fd_dup);

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

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(stride),
            rows_per_image: Some(params.height),
        },
        wgpu::Extent3d {
            width: params.width,
            height: params.height,
            depth_or_array_layers: 1,
        },
    );

    tracing::debug!(
        "import_dmabuf_via_mmap: created {}x{} texture via mmap (not zero-copy)",
        params.width,
        params.height
    );

    Some(texture)
}

#[cfg(test)]
#[path = "vulkan_dmabuf_test.rs"]
mod tests;
