//! Vulkan import of packed and native bi-planar DRM DMA-BUF surfaces.

#![allow(clippy::field_reassign_with_default)]

use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd};
use std::sync::Arc;

use ash::vk;

use super::frame::DmaBufSurface;
use crate::sampling::{GpuFrameRelease, SubmittedWorkRetirement};
use crate::{BiPlanarVideoFormat, PackedVideoFormat, VideoFrameFormat, VideoGeometry};

struct ImportParams<'a> {
    /// Unique DMA-BUF memory objects supplied by the producer.
    fds: Vec<BorrowedFd<'a>>,
    /// Logical video-plane to memory-object mapping.
    plane_object_indices: Vec<usize>,
    strides: Vec<u32>,
    offsets: Vec<u32>,
    modifier: u64,
}

/// One description shared by capability discovery and raw image creation.
/// In particular, extended usage is a union across compatible view formats;
/// each actual sampling view still needs its own modifier feature check.
struct ImportPlan {
    vk_format: vk::Format,
    wgpu_format: wgpu::TextureFormat,
    extent: vk::Extent3D,
    flags: vk::ImageCreateFlags,
    usage: vk::ImageUsageFlags,
    view_formats: &'static [vk::Format],
}

impl ImportPlan {
    fn new(
        fourcc: u32,
        geometry: VideoGeometry,
        format: VideoFrameFormat,
        disjoint: bool,
    ) -> Result<Self, String> {
        validate_surface_layout(fourcc, geometry, format)?;
        let (vk_format, wgpu_format) = sampled_format(fourcc)
            .ok_or_else(|| format!("DRM format {fourcc:#010x} is not sampleable"))?;
        let view_formats: &'static [vk::Format] = match format {
            VideoFrameFormat::Packed(_) => &[],
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12) => {
                &[vk::Format::R8_UNORM, vk::Format::R8G8_UNORM]
            }
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010) => {
                &[vk::Format::R16_UNORM, vk::Format::R16G16_UNORM]
            }
        };
        let mut flags = if disjoint {
            vk::ImageCreateFlags::DISJOINT
        } else {
            vk::ImageCreateFlags::empty()
        };
        if !view_formats.is_empty() {
            flags |= vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE;
        }
        Ok(Self {
            vk_format,
            wgpu_format,
            extent: vk::Extent3D {
                width: geometry.coded_width,
                height: geometry.coded_height,
                depth: 1,
            },
            flags,
            usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC,
            // Vulkan 1.4.341 VUID10062: list plane-compatible formats only,
            // never the base multi-planar format. DRM mutable images require
            // a nonempty list (02353).
            view_formats,
        })
    }

    fn check_support(
        &self,
        hal: &wgpu_hal::vulkan::Device,
        modifier: u64,
        max_texture_dimension: u32,
    ) -> Result<u32, String> {
        let instance = hal.shared_instance().raw_instance();
        let physical = hal.raw_physical_device();
        // SAFETY: the borrowed HAL device supplies a matching live instance
        // and physical-device pair; this immutable query is core Vulkan 1.0.
        let physical_properties = unsafe { instance.get_physical_device_properties(physical) };
        let api = physical_properties
            .api_version
            .min(hal.shared_instance().instance_api_version());
        // The core properties2 and disjoint binding entry points below must
        // exist. Supporting 1.0 would require their complete KHR dispatch.
        if api < vk::API_VERSION_1_1 {
            return Err("native DMA-BUF import requires Vulkan 1.1".into());
        }
        let extensions = hal.enabled_device_extensions();
        for extension in [
            ash::khr::external_memory_fd::NAME,
            ash::ext::external_memory_dma_buf::NAME,
            ash::ext::image_drm_format_modifier::NAME,
            ash::ext::queue_family_foreign::NAME,
        ] {
            if !extensions.contains(&extension) {
                return Err(format!("Vulkan device did not enable {extension:?}"));
            }
        }
        if !self.view_formats.is_empty()
            && api < vk::API_VERSION_1_2
            && !extensions.contains(&ash::khr::image_format_list::NAME)
        {
            return Err(
                "native video planes require Vulkan 1.2 or VK_KHR_image_format_list".into(),
            );
        }
        if self.extent.width > max_texture_dimension || self.extent.height > max_texture_dimension {
            return Err("DMA-BUF exceeds the enabled wgpu texture dimension limit".into());
        }
        // SAFETY: the same live pair is used after core 1.1 and the DRM
        // modifier extension have been admitted above.
        let properties =
            unsafe { modifier_properties(instance, physical, self.vk_format, modifier) }
                .ok_or_else(|| {
                    format!(
                        "DRM modifier {modifier:#x} is unavailable for {:?}",
                        self.vk_format
                    )
                })?;
        let count = properties.drm_format_modifier_plane_count;
        if count == 0 || count > 4 {
            return Err(format!(
                "Vulkan reported invalid modifier plane count {count}"
            ));
        }
        if self.flags.contains(vk::ImageCreateFlags::DISJOINT)
            && !properties
                .drm_format_modifier_tiling_features
                .contains(vk::FormatFeatureFlags::DISJOINT)
        {
            return Err("DRM modifier does not support disjoint memory binding".into());
        }
        // COPY_SRC is an image operation. Extended usage support borrowed
        // from a view does not establish the image's copy-format features.
        if !properties
            .drm_format_modifier_tiling_features
            .contains(vk::FormatFeatureFlags::TRANSFER_SRC)
        {
            return Err("DRM image modifier does not support transfer-source usage".into());
        }
        let sampling_formats = if self.view_formats.is_empty() {
            std::slice::from_ref(&self.vk_format)
        } else {
            self.view_formats
        };
        for &format in sampling_formats {
            // SAFETY: the pair and query capabilities are unchanged; these
            // are immutable properties of each actual compatible view format.
            let view = unsafe { modifier_properties(instance, physical, format, modifier) }
                .ok_or_else(|| {
                    format!(
                        "DRM modifier {modifier:#x} is unavailable for sampling view {format:?}"
                    )
                })?;
            if !view.drm_format_modifier_tiling_features.contains(
                vk::FormatFeatureFlags::SAMPLED_IMAGE
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
            ) {
                return Err(format!(
                    "DRM sampling view {format:?} does not support linear sampling"
                ));
            }
        }
        let mut drm = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
            .drm_format_modifier(modifier)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let mut external = vk::PhysicalDeviceExternalImageFormatInfo::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut views = vk::ImageFormatListCreateInfo::default().view_formats(self.view_formats);
        let mut query = vk::PhysicalDeviceImageFormatInfo2::default()
            .format(self.vk_format)
            .ty(vk::ImageType::TYPE_2D)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(self.usage)
            .flags(self.flags)
            .push_next(&mut drm)
            .push_next(&mut external);
        if !self.view_formats.is_empty() {
            query = query.push_next(&mut views);
        }
        let mut external_properties = vk::ExternalImageFormatProperties::default();
        let mut result = vk::ImageFormatProperties2::default().push_next(&mut external_properties);
        // SAFETY: all chained input/output structs and plane formats remain
        // live for this synchronous query. The instance/physical pair and the
        // required core/extension entry points were validated above.
        unsafe {
            instance.get_physical_device_image_format_properties2(physical, &query, &mut result)
        }
        .map_err(|error| {
            format!("unsupported DMA-BUF image/DRM modifier/view-format tuple: {error:?}")
        })?;
        let limits = result.image_format_properties;
        if self.extent.width > limits.max_extent.width
            || self.extent.height > limits.max_extent.height
            || self.extent.depth > limits.max_extent.depth
            || limits.max_mip_levels < 1
            || limits.max_array_layers < 1
            || !limits.sample_counts.contains(vk::SampleCountFlags::TYPE_1)
        {
            return Err("DMA-BUF dimensions exceed the exact image-format limits".into());
        }
        let external = external_properties.external_memory_properties;
        self.check_external_support(external)?;
        Ok(count)
    }

    fn check_external_support(&self, external: vk::ExternalMemoryProperties) -> Result<(), String> {
        if self.flags.contains(vk::ImageCreateFlags::DISJOINT)
            && external
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::DEDICATED_ONLY)
        {
            // A dedicated allocation may not name a DISJOINT image (01797).
            return Err("disjoint DMA-BUF images cannot require dedicated allocation".into());
        }
        if !external
            .external_memory_features
            .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
            || !external
                .compatible_handle_types
                .contains(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        {
            return Err("exact DRM image tuple is not DMA-BUF importable".into());
        }
        Ok(())
    }
}

pub(super) fn validate_surface_layout(
    fourcc: u32,
    geometry: VideoGeometry,
    format: VideoFrameFormat,
) -> Result<usize, String> {
    let (_, actual) = sampled_format(fourcc)
        .ok_or_else(|| format!("DRM format {fourcc:#010x} is not sampleable"))?;
    let expected = match format {
        VideoFrameFormat::Packed(PackedVideoFormat::Bgra8) => wgpu::TextureFormat::Bgra8UnormSrgb,
        VideoFrameFormat::Packed(PackedVideoFormat::Rgba8) => wgpu::TextureFormat::Rgba8UnormSrgb,
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12) => wgpu::TextureFormat::NV12,
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010) => wgpu::TextureFormat::P010,
    };
    if actual != expected {
        return Err(format!(
            "DRM format {fourcc:#010x} disagrees with decoded format {format:?}"
        ));
    }
    format
        .allocation_bytes(geometry)
        .map_err(|error| error.to_string())
}

struct ImportedResources {
    device: ash::Device,
    image: vk::Image,
    memories: Vec<vk::DeviceMemory>,
    queue_family: u32,
    // Keep the owning `wgpu::Device` alive until after `Drop::drop` destroys
    // the raw `vk::Image` and `vk::DeviceMemory` handles.
    wgpu_device: wgpu::Device,
}

impl Drop for ImportedResources {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image(self.image, None);
            for memory in self.memories.drain(..) {
                self.device.free_memory(memory, None);
            }
        }
    }
}

struct ForeignRelease(Arc<ImportedResources>);

impl GpuFrameRelease for ForeignRelease {
    fn record(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        let resources = &self.0;
        // SAFETY: `device` is verified to match `resources.device` and `encoder`
        // is a fresh raw-only Vulkan encoder owned by `GpuVideoContext`.
        unsafe {
            let hal_device = device
                .as_hal::<wgpu::hal::api::Vulkan>()
                .expect("DMA-BUF release requires wgpu's Vulkan backend");
            assert_eq!(
                hal_device.raw_device().handle(),
                resources.device.handle(),
                "DMA-BUF release encoder must belong to the owning Vulkan device"
            );
            encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|hal| {
                let hal = hal.expect("DMA-BUF release requires a Vulkan command encoder");
                let barrier = vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::SHADER_READ,
                    dst_access_mask: vk::AccessFlags::empty(),
                    old_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    new_layout: vk::ImageLayout::GENERAL,
                    src_queue_family_index: resources.queue_family,
                    dst_queue_family_index: vk::QUEUE_FAMILY_FOREIGN_EXT,
                    image: resources.image,
                    subresource_range: color_subresource(),
                    ..Default::default()
                };
                resources.device.cmd_pipeline_barrier(
                    hal.raw_handle(),
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                );
            });
        }
    }
}

/// Vulkan object retained for one stable decoder-pool allocation. Sampling
/// objects may be reused, but every new producer lease must still acquire
/// FOREIGN ownership and every retired frame must release it again.
pub(super) struct ImportedDmaBufSurface {
    resources: Arc<ImportedResources>,
    queue: wgpu::Queue,
}

impl ImportedDmaBufSurface {
    pub(super) fn acquire(&self) -> Result<(), String> {
        // SAFETY: `self.resources` owns `wgpu_device`, `device`, `image`, and
        // `queue_family` from the same Vulkan device as `self.queue`, and the
        // completion callback below keeps `self.resources` alive until the
        // submitted barrier finishes.
        unsafe {
            submit_foreign_acquire(
                &self.resources.wgpu_device,
                &self.queue,
                &self.resources.device,
                self.resources.queue_family,
                self.resources.image,
            )?;
        }
        // Retain the imported image and memory until the raw acquire barrier
        // finishes executing on the GPU queue (VUID-vkDestroyImage-image-01000).
        let retirement = SubmittedWorkRetirement::new(Arc::clone(&self.resources));
        self.queue
            .on_submitted_work_done(retirement.into_completion_callback());
        Ok(())
    }

    pub(super) fn release(&self) -> Box<dyn GpuFrameRelease> {
        Box::new(ForeignRelease(Arc::clone(&self.resources)))
    }
}

pub(super) fn import_dmabuf(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface: &DmaBufSurface,
    geometry: VideoGeometry,
    format: VideoFrameFormat,
) -> Result<(wgpu::Texture, ImportedDmaBufSurface), String> {
    let modifier = surface
        .objects
        .first()
        .ok_or_else(|| "DMA-BUF surface has no memory objects".to_owned())?
        .modifier;
    if surface
        .objects
        .iter()
        .any(|object| object.modifier != modifier)
    {
        return Err("DMA-BUF objects disagree on their DRM modifier".to_owned());
    }
    let params = ImportParams {
        fds: surface
            .objects
            .iter()
            .map(|object| object.fd.as_fd())
            .collect(),
        plane_object_indices: surface
            .planes
            .iter()
            .map(|plane| plane.object_index)
            .collect(),
        strides: surface.planes.iter().map(|plane| plane.stride).collect(),
        offsets: surface.planes.iter().map(|plane| plane.offset).collect(),
        modifier,
    };
    if params.fds.is_empty() || params.fds.len() > 4 {
        return Err(format!(
            "invalid DMA-BUF memory-object count {}",
            params.fds.len()
        ));
    }
    if params
        .plane_object_indices
        .iter()
        .any(|&index| index >= params.fds.len())
    {
        return Err("DMA-BUF plane refers to a missing memory object".to_owned());
    }
    let disjoint = unsafe { fds_are_disjoint(&params.fds) };
    let plan = ImportPlan::new(surface.fourcc, geometry, format, disjoint)?;

    use wgpu::hal::api::Vulkan;
    // SAFETY: all raw calls use the native handles of this exact borrowed
    // wgpu device. Object creation follows the exact support query; successful
    // imports move into ImportedResources before HAL wrapping, and the drop
    // callback retains them instead of giving destruction ownership to HAL.
    unsafe {
        let hal = device
            .as_hal::<Vulkan>()
            .ok_or_else(|| "DMA-BUF import requires wgpu's Vulkan backend".to_string())?;
        let raw_device = hal.raw_device();
        let instance = hal.shared_instance().raw_instance();
        let required_feature = match format {
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12) => {
                wgpu::Features::TEXTURE_FORMAT_NV12
            }
            VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::P010) => {
                wgpu::Features::TEXTURE_FORMAT_P010 | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
            }
            VideoFrameFormat::Packed(_) => wgpu::Features::empty(),
        };
        if !device.features().contains(required_feature) {
            return Err(format!(
                "Vulkan adapter did not enable the wgpu feature for {:?} video textures",
                plan.wgpu_format
            ));
        }
        let modifier_plane_count = plan.check_support(
            &hal,
            params.modifier,
            device.limits().max_texture_dimension_2d,
        )?;
        let external_memory_fd = ash::khr::external_memory_fd::Device::new(instance, raw_device);

        let layouts = plane_layouts(&params, modifier_plane_count)?;
        let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT {
            drm_format_modifier: params.modifier,
            drm_format_modifier_plane_count: modifier_plane_count,
            p_plane_layouts: layouts.as_ptr(),
            ..Default::default()
        };
        let mut external_info = vk::ExternalMemoryImageCreateInfo {
            p_next: (&mut modifier_info as *mut _) as *mut std::ffi::c_void,
            handle_types: vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            ..Default::default()
        };
        let mut view_list =
            vk::ImageFormatListCreateInfo::default().view_formats(plan.view_formats);
        let mut image_info = vk::ImageCreateInfo {
            p_next: (&mut external_info as *mut _) as *mut std::ffi::c_void,
            flags: plan.flags,
            image_type: vk::ImageType::TYPE_2D,
            format: plan.vk_format,
            extent: plan.extent,
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT,
            usage: plan.usage,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            ..Default::default()
        };
        if !plan.view_formats.is_empty() {
            image_info = image_info.push_next(&mut view_list);
        }
        let image = raw_device
            .create_image(&image_info, None)
            .map_err(|error| format!("vkCreateImage for DMA-BUF failed: {error:?}"))?;
        let memories = match if disjoint {
            bind_disjoint(
                raw_device,
                &external_memory_fd,
                image,
                &params,
                modifier_plane_count,
            )
        } else {
            bind_shared(raw_device, &external_memory_fd, image, &params)
        } {
            Ok(memories) => memories,
            Err(error) => {
                raw_device.destroy_image(image, None);
                return Err(error);
            }
        };

        let resources = Arc::new(ImportedResources {
            device: raw_device.clone(),
            image,
            memories,
            queue_family: hal.queue_family_index(),
            wgpu_device: device.clone(),
        });
        let hal_desc = wgpu_hal::TextureDescriptor {
            label: Some("Neomacs imported video DMA-BUF"),
            size: wgpu::Extent3d {
                width: plan.extent.width,
                height: plan.extent.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: plan.wgpu_format,
            usage: wgpu::TextureUses::RESOURCE | wgpu::TextureUses::COPY_SRC,
            memory_flags: wgpu_hal::MemoryFlags::empty(),
            view_formats: Vec::new(),
        };
        let imported = ImportedDmaBufSurface {
            resources: Arc::clone(&resources),
            queue: queue.clone(),
        };
        let mut resources = Some(resources);
        let drop_callback: wgpu_hal::DropCallback = Box::new(move || drop(resources.take()));
        let hal_texture = hal.texture_from_raw(
            image,
            &hal_desc,
            Some(drop_callback),
            wgpu_hal::vulkan::TextureMemory::External,
        );
        let texture = device.create_texture_from_hal::<Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("Neomacs zero-copy video DMA-BUF"),
                size: hal_desc.size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: plan.wgpu_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
            // Initial tracked state, not allowed usage. Foreign acquire
            // establishes SHADER_READ_ONLY_OPTIMAL before sampling.
            wgpu::TextureUses::RESOURCE,
        );
        Ok((texture, imported))
    }
}

fn sampled_format(fourcc: u32) -> Option<(vk::Format, wgpu::TextureFormat)> {
    match fourcc {
        0x3432_5241 => Some((
            vk::Format::B8G8R8A8_SRGB,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        )),
        0x3432_4241 => Some((
            vk::Format::R8G8B8A8_SRGB,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )),
        0x3231_564e => Some((
            vk::Format::G8_B8R8_2PLANE_420_UNORM,
            wgpu::TextureFormat::NV12,
        )),
        0x3031_3050 => Some((
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            wgpu::TextureFormat::P010,
        )),
        _ => None,
    }
}

#[cfg(test)]
#[path = "dmabuf/tests/dmabuf_test.rs"]
mod tests;

// SAFETY: instance and physical_device must belong to the same live HAL
// adapter, with core Vulkan 1.1 and image_drm_format_modifier enabled.
unsafe fn modifier_properties(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    format: vk::Format,
    modifier: u64,
) -> Option<vk::DrmFormatModifierPropertiesEXT> {
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default();
    let mut properties = vk::FormatProperties2 {
        p_next: (&mut list as *mut _) as *mut std::ffi::c_void,
        ..Default::default()
    };
    unsafe {
        instance.get_physical_device_format_properties2(physical_device, format, &mut properties)
    };
    let mut entries = vec![
        vk::DrmFormatModifierPropertiesEXT::default();
        list.drm_format_modifier_count as usize
    ];
    list.p_drm_format_modifier_properties = entries.as_mut_ptr();
    let mut properties = vk::FormatProperties2 {
        p_next: (&mut list as *mut _) as *mut std::ffi::c_void,
        ..Default::default()
    };
    unsafe {
        instance.get_physical_device_format_properties2(physical_device, format, &mut properties)
    };
    entries
        .iter()
        .find(|entry| entry.drm_format_modifier == modifier)
        .copied()
}

fn plane_layouts(
    params: &ImportParams<'_>,
    count: u32,
) -> Result<Vec<vk::SubresourceLayout>, String> {
    if params.offsets.len() < count as usize || params.strides.len() < count as usize {
        return Err(format!(
            "DRM modifier requires {count} memory-plane layouts, but the decoder supplied {} offsets and {} strides",
            params.offsets.len(),
            params.strides.len()
        ));
    }
    Ok((0..count as usize)
        .map(|plane| vk::SubresourceLayout {
            offset: u64::from(params.offsets[plane]),
            row_pitch: u64::from(params.strides[plane]),
            ..Default::default()
        })
        .collect())
}

unsafe fn fds_are_disjoint(fds: &[BorrowedFd<'_>]) -> bool {
    let mut first_inode = None;
    for fd in fds {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(fd.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return true;
        }
        let inode = unsafe { stat.assume_init() }.st_ino;
        match first_inode {
            None => first_inode = Some(inode),
            Some(first) if first != inode => return true,
            Some(_) => {}
        }
    }
    false
}

unsafe fn bind_shared(
    device: &ash::Device,
    external_memory_fd: &ash::khr::external_memory_fd::Device,
    image: vk::Image,
    params: &ImportParams<'_>,
) -> Result<Vec<vk::DeviceMemory>, String> {
    let requirements = unsafe { device.get_image_memory_requirements(image) };
    let memory = unsafe {
        import_memory(
            device,
            Some(image),
            params.fds[0],
            requirements.size,
            requirements.memory_type_bits,
            external_memory_fd,
        )?
    };
    if let Err(error) = unsafe { device.bind_image_memory(image, memory, 0) } {
        unsafe { device.free_memory(memory, None) };
        return Err(format!("binding DMA-BUF memory failed: {error:?}"));
    }
    Ok(vec![memory])
}

unsafe fn bind_disjoint(
    device: &ash::Device,
    external_memory_fd: &ash::khr::external_memory_fd::Device,
    image: vk::Image,
    params: &ImportParams<'_>,
    plane_count: u32,
) -> Result<Vec<vk::DeviceMemory>, String> {
    if params.plane_object_indices.len() < plane_count as usize {
        return Err(format!(
            "disjoint DRM modifier requires {plane_count} memory-plane mappings, but the decoder supplied {}",
            params.plane_object_indices.len()
        ));
    }
    // Complete every fallible metadata lookup before importing any memory.
    // Later loops cannot lose earlier allocations through an input `?`.
    let plane_fds = params.plane_object_indices[..plane_count as usize]
        .iter()
        .enumerate()
        .map(|(plane, &object_index)| {
            params.fds.get(object_index).copied().ok_or_else(|| {
                format!("DMA-BUF plane {plane} refers to missing object {object_index}")
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut plane_infos = (0..plane_count)
        .map(|plane| Ok(vk::BindImagePlaneMemoryInfo::default().plane_aspect(plane_aspect(plane)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let mut plane_requirements = Vec::with_capacity(plane_count as usize);
    for (plane, binding) in plane_infos.iter().enumerate() {
        let mut plane_info =
            vk::ImagePlaneMemoryRequirementsInfo::default().plane_aspect(binding.plane_aspect);
        let requirements_info = vk::ImageMemoryRequirementsInfo2 {
            p_next: (&mut plane_info as *mut _) as *mut std::ffi::c_void,
            image,
            ..Default::default()
        };
        let mut dedicated = vk::MemoryDedicatedRequirements::default();
        let mut requirements = vk::MemoryRequirements2::default().push_next(&mut dedicated);
        unsafe { device.get_image_memory_requirements2(&requirements_info, &mut requirements) };
        let requirements = requirements.memory_requirements;
        // DRM modifiers may require dedication independently of the external
        // format query. A disjoint image cannot meet that requirement.
        if dedicated.requires_dedicated_allocation != vk::FALSE {
            return Err(format!(
                "disjoint DMA-BUF plane {plane} requires dedicated allocation"
            ));
        }
        plane_requirements.push(requirements);
    }
    let mut memories = Vec::with_capacity(plane_count as usize);
    for (requirements, fd) in plane_requirements.into_iter().zip(plane_fds) {
        let memory = match unsafe {
            import_memory(
                device,
                None,
                fd,
                requirements.size,
                requirements.memory_type_bits,
                external_memory_fd,
            )
        } {
            Ok(memory) => memory,
            Err(error) => {
                for memory in memories.drain(..) {
                    unsafe { device.free_memory(memory, None) };
                }
                return Err(error);
            }
        };
        memories.push(memory);
    }
    let bindings: Vec<_> = memories
        .iter()
        .enumerate()
        .map(|(plane, memory)| vk::BindImageMemoryInfo {
            p_next: (&mut plane_infos[plane] as *mut _) as *mut std::ffi::c_void,
            image,
            memory: *memory,
            memory_offset: 0,
            ..Default::default()
        })
        .collect();
    if let Err(error) = unsafe { device.bind_image_memory2(&bindings) } {
        for memory in memories.drain(..) {
            unsafe { device.free_memory(memory, None) };
        }
        return Err(format!("binding disjoint DMA-BUF planes failed: {error:?}"));
    }
    Ok(memories)
}

unsafe fn import_memory(
    device: &ash::Device,
    dedicated_image: Option<vk::Image>,
    fd: BorrowedFd<'_>,
    allocation_size: u64,
    image_memory_types: u32,
    external_memory_fd: &ash::khr::external_memory_fd::Device,
) -> Result<vk::DeviceMemory, String> {
    let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
    unsafe {
        external_memory_fd.get_memory_fd_properties(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            fd.as_raw_fd(),
            &mut fd_properties,
        )
    }
    .map_err(|error| format!("querying DMA-BUF Vulkan memory types failed: {error:?}"))?;
    let memory_type_index = first_memory_type(image_memory_types & fd_properties.memory_type_bits)?;
    let duplicated = unsafe { libc::dup(fd.as_raw_fd()) };
    if duplicated < 0 {
        return Err("failed to duplicate DMA-BUF for Vulkan import".into());
    }
    let owned = unsafe { OwnedFd::from_raw_fd(duplicated) };
    let mut import = vk::ImportMemoryFdInfoKHR {
        handle_type: vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
        fd: owned.as_raw_fd(),
        ..Default::default()
    };
    let mut dedicated = vk::MemoryDedicatedAllocateInfo {
        image: dedicated_image.unwrap_or_default(),
        ..Default::default()
    };
    let mut allocate = vk::MemoryAllocateInfo::default()
        .allocation_size(allocation_size)
        .memory_type_index(memory_type_index)
        .push_next(&mut import);
    if dedicated_image.is_some() {
        allocate = allocate.push_next(&mut dedicated);
    }
    match unsafe { device.allocate_memory(&allocate, None) } {
        Ok(memory) => {
            let _ = owned.into_raw_fd();
            Ok(memory)
        }
        Err(error) => Err(format!("Vulkan DMA-BUF memory import failed: {error:?}")),
    }
}

/// Record and submit a `QUEUE_FAMILY_FOREIGN_EXT` -> `queue_family` acquire
/// barrier for `image` in a fresh raw Vulkan command encoder.
///
/// # Safety
/// `wgpu_device`, `queue`, `device`, and `queue_family` must belong to the same
/// live Vulkan device with `VK_EXT_queue_family_foreign` enabled, `image` must
/// be a live `vk::Image` owned by that device, and the caller must keep `image`
/// alive until the submitted work completes on `queue`.
unsafe fn submit_foreign_acquire(
    wgpu_device: &wgpu::Device,
    queue: &wgpu::Queue,
    device: &ash::Device,
    queue_family: u32,
    image: vk::Image,
) -> Result<(), String> {
    let mut encoder = wgpu_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Neomacs DMA-BUF foreign ownership acquire"),
    });
    unsafe {
        encoder.as_hal_mut::<wgpu::hal::api::Vulkan, _, _>(|hal| {
            if let Some(hal) = hal {
                let barrier = vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::MEMORY_WRITE,
                    dst_access_mask: vk::AccessFlags::SHADER_READ,
                    old_layout: vk::ImageLayout::GENERAL,
                    new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    src_queue_family_index: vk::QUEUE_FAMILY_FOREIGN_EXT,
                    dst_queue_family_index: queue_family,
                    image,
                    subresource_range: color_subresource(),
                    ..Default::default()
                };
                device.cmd_pipeline_barrier(
                    hal.raw_handle(),
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                );
            }
        });
    }
    queue.submit(std::iter::once(encoder.finish()));
    Ok(())
}

const fn color_subresource() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

fn first_memory_type(bits: u32) -> Result<u32, String> {
    (0..32)
        .find(|index| bits & (1 << index) != 0)
        .ok_or_else(|| "DMA-BUF has no compatible Vulkan memory type".to_string())
}

fn plane_aspect(plane: u32) -> Result<vk::ImageAspectFlags, String> {
    match plane {
        0 => Ok(vk::ImageAspectFlags::MEMORY_PLANE_0_EXT),
        1 => Ok(vk::ImageAspectFlags::MEMORY_PLANE_1_EXT),
        2 => Ok(vk::ImageAspectFlags::MEMORY_PLANE_2_EXT),
        3 => Ok(vk::ImageAspectFlags::MEMORY_PLANE_3_EXT),
        _ => Err(format!("unsupported DMA-BUF memory plane {plane}")),
    }
}

#[cfg(feature = "native-validation-fixtures")]
#[path = "dmabuf/validation_fixture.rs"]
mod validation_fixture;

#[cfg(feature = "native-validation-fixtures")]
pub(crate) fn assert_native_dmabuf_plane_validation(format: BiPlanarVideoFormat) {
    validation_fixture::exercise(format);
}
