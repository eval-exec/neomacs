use std::any::Any;
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use neomacs_display_protocol::types::VideoId;
use wgpu::util::DeviceExt;

use crate::color::VideoColorTransform;
use crate::system::VideoWake;
use crate::{
    BiPlanarVideoFormat, VideoColorimetry, VideoFrameFormat, VideoGeometry, VideoSampleKind,
    VideoSamplingTransform,
};

/// Linux DRM render-node identity shared by the Vulkan compositor and the
/// decoder selected inside GStreamer. A DMA-BUF path is only called direct
/// when both sides prove this identity is the same.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LinuxDrmDevice {
    major: u32,
    minor: u32,
}

#[cfg(target_os = "linux")]
impl LinuxDrmDevice {
    #[cfg(test)]
    pub(crate) const fn from_device_numbers(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    pub(crate) fn from_path(path: &std::path::Path) -> Option<Self> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};

        let metadata = std::fs::metadata(path).ok()?;
        if !metadata.file_type().is_char_device() {
            return None;
        }
        let device = metadata.rdev();
        let major = libc::major(device) as u32;
        let minor = libc::minor(device) as u32;
        (major == 226 && minor >= 128).then_some(Self { major, minor })
    }
}

/// Backend-specific release operation that must be encoded after the last
/// compositor read and before a foreign producer may reuse its surface.
pub(crate) trait GpuFrameRelease: Send + Sync {
    fn record(&self, encoder: &mut wgpu::CommandEncoder);
}

/// Reusable sampling objects for one stable native decoder surface. Platform
/// import caches retain this bundle so frame rotation does not recreate a
/// view and bind group every time the decoder revisits the same pool slot.
#[derive(Clone)]
pub(crate) struct PreparedSampledTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    _allocation: Arc<GpuAllocation>,
}

#[derive(Clone)]
pub(crate) struct PreparedBiPlanarTexture {
    luma_texture: wgpu::Texture,
    chroma_texture: wgpu::Texture,
    luma_view: wgpu::TextureView,
    chroma_view: wgpu::TextureView,
    color_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    _allocation: Arc<GpuAllocation>,
}

struct BiPlanarTexturePlanes {
    luma_texture: wgpu::Texture,
    chroma_texture: wgpu::Texture,
    luma_view: wgpu::TextureView,
    chroma_view: wgpu::TextureView,
}

/// Bind-group layouts and sampler shared by the renderer and native video
/// importers. A single value guarantees pipeline-layout identity without
/// exposing any platform surface representation.
#[derive(Clone)]
pub struct VideoSamplingResources {
    packed_bind_group_layout: wgpu::BindGroupLayout,
    bi_planar_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl VideoSamplingResources {
    pub fn new(
        device: &wgpu::Device,
        packed_bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let bi_planar_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Neomacs bi-planar video bind group layout"),
                entries: &[
                    texture_layout_entry(0),
                    texture_layout_entry(1),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                size_of::<VideoColorTransform>() as u64,
                            ),
                        },
                        count: None,
                    },
                ],
            });
        Self {
            packed_bind_group_layout: packed_bind_group_layout.clone(),
            bi_planar_bind_group_layout,
            sampler: sampler.clone(),
        }
    }

    pub fn bi_planar_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bi_planar_bind_group_layout
    }
}

const fn texture_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

#[derive(Clone)]
pub(crate) struct GpuAllocationTracker(Arc<GpuAllocationState>);

struct GpuAllocationState {
    bytes: AtomicUsize,
    wake: VideoWake,
}

impl GpuAllocationTracker {
    pub(crate) fn new(wake: VideoWake) -> Self {
        Self(Arc::new(GpuAllocationState {
            bytes: AtomicUsize::new(0),
            wake,
        }))
    }

    pub(crate) fn track(&self, bytes: usize) -> Arc<GpuAllocation> {
        self.0
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(bytes)
            })
            .expect("video GPU allocation accounting overflow");
        Arc::new(GpuAllocation {
            bytes,
            tracker: self.clone(),
        })
    }

    pub(crate) fn bytes(&self) -> usize {
        self.0.bytes.load(Ordering::Acquire)
    }
}

impl Default for GpuAllocationTracker {
    fn default() -> Self {
        Self::new(VideoWake::noop())
    }
}

pub(crate) struct GpuAllocation {
    bytes: usize,
    tracker: GpuAllocationTracker,
}

impl Drop for GpuAllocation {
    fn drop(&mut self) {
        let previous = self.tracker.0.bytes.fetch_sub(self.bytes, Ordering::AcqRel);
        debug_assert!(previous >= self.bytes);
        // A pooled texture can reach its final owner in a queue completion
        // callback, after the renderer's last service pass. Wake that pass so
        // aggregate media-budget accounting observes the exact free event.
        self.tracker.0.wake.notify();
    }
}

#[derive(Clone)]
pub(crate) struct GpuVideoContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    sampling: VideoSamplingResources,
    generation: GpuGeneration,
    allocations: GpuAllocationTracker,
}

impl GpuVideoContext {
    pub(crate) fn with_sampling_resources(
        device: wgpu::Device,
        queue: wgpu::Queue,
        sampling: VideoSamplingResources,
        generation: GpuGeneration,
        wake: VideoWake,
    ) -> Self {
        Self {
            device,
            queue,
            sampling,
            generation,
            allocations: GpuAllocationTracker::new(wake),
        }
    }

    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub(crate) const fn generation(&self) -> GpuGeneration {
        self.generation
    }

    pub(crate) fn allocated_bytes(&self) -> usize {
        self.allocations.bytes()
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn linux_render_device(&self) -> Option<LinuxDrmDevice> {
        use std::ffi::CStr;

        use ash::vk;
        use wgpu::hal::api::Vulkan;

        unsafe {
            self.device.as_hal::<Vulkan>().and_then(|hal| {
                let instance = hal.shared_instance().raw_instance();
                let physical_device = hal.raw_physical_device();
                let supports_drm_identity = instance
                    .enumerate_device_extension_properties(physical_device)
                    .ok()?
                    .iter()
                    .any(|extension| {
                        CStr::from_ptr(extension.extension_name.as_ptr())
                            == ash::ext::physical_device_drm::NAME
                    });
                if !supports_drm_identity {
                    return None;
                }
                let mut drm = vk::PhysicalDeviceDrmPropertiesEXT::default();
                let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut drm);
                instance.get_physical_device_properties2(physical_device, &mut properties);
                if drm.has_render == 0 {
                    return None;
                }
                Some(LinuxDrmDevice {
                    major: u32::try_from(drm.render_major).ok()?,
                    minor: u32::try_from(drm.render_minor).ok()?,
                })
            })
        }
    }

    /// Keep native decoder leases alive through all GPU work submitted before
    /// this point. This is the affine hand-off that prevents a decoder pool
    /// from rewriting a DMA-BUF/CVPixelBuffer while the compositor still
    /// samples its imported texture.
    pub(crate) fn retire_after_submitted_work(&self, retired: Vec<GpuVideoFrame>) {
        let needs_release = retired.iter().any(GpuVideoFrame::needs_release);
        if needs_release {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Neomacs video foreign-surface release"),
                });
            for frame in &retired {
                frame.record_release(&mut encoder);
            }
            // Queue order places these release barriers after every draw
            // submitted so far. The completion callback below therefore
            // returns native leases only after ownership is foreign again.
            self.queue.submit(std::iter::once(encoder.finish()));
        }
        self.queue.on_submitted_work_done(move || drop(retired));
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn wrap_texture<L>(
        &self,
        geometry: VideoGeometry,
        format: VideoFrameFormat,
        texture: wgpu::Texture,
        native_lease: L,
    ) -> GpuVideoFrame
    where
        L: Any + Send + Sync,
    {
        let allocation_bytes = format
            .allocation_bytes(geometry)
            .expect("validated video geometry has a representable allocation size");
        let prepared = self.prepare_texture(texture, allocation_bytes);
        self.wrap_prepared_texture(geometry, prepared, native_lease)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn wrap_prepared_texture_with_release<L>(
        &self,
        geometry: VideoGeometry,
        prepared: PreparedSampledTexture,
        release: Box<dyn GpuFrameRelease>,
        native_lease: L,
    ) -> GpuVideoFrame
    where
        L: Any + Send + Sync,
    {
        let mut frame = self.wrap_prepared_texture(geometry, prepared, native_lease);
        frame.release = Some(release);
        frame
    }

    pub(crate) fn prepare_texture(
        &self,
        texture: wgpu::Texture,
        allocation_bytes: usize,
    ) -> PreparedSampledTexture {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Neomacs video frame bind group"),
            layout: &self.sampling.packed_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampling.sampler),
                },
            ],
        });
        PreparedSampledTexture {
            texture,
            view,
            bind_group,
            _allocation: self.allocations.track(allocation_bytes),
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn prepare_bi_planar_textures(
        &self,
        luma_texture: wgpu::Texture,
        chroma_texture: wgpu::Texture,
        format: BiPlanarVideoFormat,
        colorimetry: VideoColorimetry,
        geometry: VideoGeometry,
    ) -> Result<PreparedBiPlanarTexture, String> {
        let luma_view = luma_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let chroma_view = chroma_texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.prepare_bi_planar_views(
            BiPlanarTexturePlanes {
                luma_texture,
                chroma_texture,
                luma_view,
                chroma_view,
            },
            format,
            colorimetry,
            geometry,
        )
    }

    /// Prepare the two aspects of one native Vulkan/DXGI multi-planar
    /// texture. Both views share one wgpu texture identity, so resource-state
    /// tracking remains aware that the planes alias the same image.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub(crate) fn prepare_multi_planar_texture(
        &self,
        texture: wgpu::Texture,
        format: BiPlanarVideoFormat,
        colorimetry: VideoColorimetry,
        geometry: VideoGeometry,
    ) -> Result<PreparedBiPlanarTexture, String> {
        let (luma_format, chroma_format) = match format {
            BiPlanarVideoFormat::Nv12 => {
                (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Rg8Unorm)
            }
            BiPlanarVideoFormat::P010 => (
                wgpu::TextureFormat::R16Unorm,
                wgpu::TextureFormat::Rg16Unorm,
            ),
        };
        let luma_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Neomacs native video luma plane"),
            format: Some(luma_format),
            aspect: wgpu::TextureAspect::Plane0,
            ..Default::default()
        });
        let chroma_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Neomacs native video chroma plane"),
            format: Some(chroma_format),
            aspect: wgpu::TextureAspect::Plane1,
            ..Default::default()
        });
        self.prepare_bi_planar_views(
            BiPlanarTexturePlanes {
                luma_texture: texture.clone(),
                chroma_texture: texture,
                luma_view,
                chroma_view,
            },
            format,
            colorimetry,
            geometry,
        )
    }

    fn prepare_bi_planar_views(
        &self,
        planes: BiPlanarTexturePlanes,
        format: BiPlanarVideoFormat,
        colorimetry: VideoColorimetry,
        geometry: VideoGeometry,
    ) -> Result<PreparedBiPlanarTexture, String> {
        let color = VideoColorTransform::new(format, colorimetry, geometry);
        let color_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Neomacs video color transform"),
                contents: bytemuck::bytes_of(&color),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Neomacs bi-planar video bind group"),
            layout: &self.sampling.bi_planar_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&planes.luma_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&planes.chroma_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampling.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: color_buffer.as_entire_binding(),
                },
            ],
        });
        let allocation_bytes = VideoFrameFormat::BiPlanar420(format)
            .allocation_bytes(geometry)
            .map_err(|error| error.to_string())?;
        Ok(PreparedBiPlanarTexture {
            luma_texture: planes.luma_texture,
            chroma_texture: planes.chroma_texture,
            luma_view: planes.luma_view,
            chroma_view: planes.chroma_view,
            color_buffer,
            bind_group,
            _allocation: self.allocations.track(allocation_bytes),
        })
    }

    pub(crate) fn wrap_prepared_texture<L>(
        &self,
        geometry: VideoGeometry,
        prepared: PreparedSampledTexture,
        native_lease: L,
    ) -> GpuVideoFrame
    where
        L: Any + Send + Sync,
    {
        GpuVideoFrame::new(
            geometry,
            self.generation,
            prepared.texture,
            prepared.view,
            prepared.bind_group,
            prepared._allocation,
            native_lease,
        )
    }

    pub(crate) fn wrap_prepared_bi_planar_texture<L>(
        &self,
        geometry: VideoGeometry,
        prepared: PreparedBiPlanarTexture,
        native_lease: L,
    ) -> GpuVideoFrame
    where
        L: Any + Send + Sync,
    {
        GpuVideoFrame::new_bi_planar(geometry, self.generation, prepared, native_lease)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn wrap_prepared_bi_planar_texture_with_release<L>(
        &self,
        geometry: VideoGeometry,
        prepared: PreparedBiPlanarTexture,
        release: Box<dyn GpuFrameRelease>,
        native_lease: L,
    ) -> GpuVideoFrame
    where
        L: Any + Send + Sync,
    {
        let mut frame = self.wrap_prepared_bi_planar_texture(geometry, prepared, native_lease);
        frame.release = Some(release);
        frame
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn upload_rgba(
        &self,
        geometry: VideoGeometry,
        format: VideoFrameFormat,
        bytes: &[u8],
        stride: u32,
    ) -> Result<GpuVideoFrame, String> {
        let texture_format = match format {
            VideoFrameFormat::Packed(crate::PackedVideoFormat::Rgba8) => {
                wgpu::TextureFormat::Rgba8UnormSrgb
            }
            VideoFrameFormat::Packed(crate::PackedVideoFormat::Bgra8) => {
                wgpu::TextureFormat::Bgra8UnormSrgb
            }
            VideoFrameFormat::BiPlanar420(_) => {
                return Err("bi-planar video cannot use the packed CPU upload path".to_owned());
            }
        };
        let required = usize::try_from(stride)
            .ok()
            .and_then(|stride| stride.checked_mul(geometry.coded_height as usize))
            .ok_or_else(|| "video frame byte size overflow".to_string())?;
        if bytes.len() < required {
            return Err(format!(
                "video frame has {} bytes but its geometry requires at least {required}",
                bytes.len()
            ));
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Neomacs CPU-upload video frame"),
            size: wgpu::Extent3d {
                width: geometry.coded_width,
                height: geometry.coded_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes[..required],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(geometry.coded_height),
            },
            wgpu::Extent3d {
                width: geometry.coded_width,
                height: geometry.coded_height,
                depth_or_array_layers: 1,
            },
        );
        Ok(self.wrap_texture(geometry, format, texture, ()))
    }
}

/// Generation of the renderer device against which a native surface was
/// imported. Frames from an older generation are never valid after recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpuGeneration(NonZeroU64);

impl GpuGeneration {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Return the next renderer generation. Generation zero is deliberately
    /// unrepresentable, and exhaustion is treated as a process-lifetime
    /// invariant violation rather than silently making old native handles
    /// appear current again.
    pub fn next(self) -> Self {
        Self(
            NonZeroU64::new(
                self.0
                    .get()
                    .checked_add(1)
                    .expect("GPU generation exhausted"),
            )
            .expect("incrementing a non-zero GPU generation stays non-zero"),
        )
    }
}

/// One compositor-ready frame. The private lease retains the decoder-owned
/// surface (GstSample, CVPixelBuffer, or IMFSample) alongside the GPU wrapper.
/// The type is intentionally non-Clone so frame ownership remains affine.
pub(crate) struct GpuVideoFrame {
    geometry: VideoGeometry,
    generation: GpuGeneration,
    sample: GpuVideoSample,
    _native_lease: Box<dyn Any + Send + Sync>,
    release: Option<Box<dyn GpuFrameRelease>>,
}

enum GpuVideoSample {
    Packed {
        _texture: wgpu::Texture,
        view: wgpu::TextureView,
        bind_group: wgpu::BindGroup,
        _allocation: Arc<GpuAllocation>,
    },
    BiPlanar {
        _luma_texture: wgpu::Texture,
        _chroma_texture: wgpu::Texture,
        _luma_view: wgpu::TextureView,
        _chroma_view: wgpu::TextureView,
        _color_buffer: wgpu::Buffer,
        bind_group: wgpu::BindGroup,
        _allocation: Arc<GpuAllocation>,
    },
}

impl GpuVideoFrame {
    pub(crate) fn new<L>(
        geometry: VideoGeometry,
        generation: GpuGeneration,
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        bind_group: wgpu::BindGroup,
        allocation: Arc<GpuAllocation>,
        native_lease: L,
    ) -> Self
    where
        L: Any + Send + Sync,
    {
        Self {
            geometry,
            generation,
            sample: GpuVideoSample::Packed {
                _texture: texture,
                view,
                bind_group,
                _allocation: allocation,
            },
            _native_lease: Box::new(native_lease),
            release: None,
        }
    }

    fn new_bi_planar<L>(
        geometry: VideoGeometry,
        generation: GpuGeneration,
        prepared: PreparedBiPlanarTexture,
        native_lease: L,
    ) -> Self
    where
        L: Any + Send + Sync,
    {
        Self {
            geometry,
            generation,
            sample: GpuVideoSample::BiPlanar {
                _luma_texture: prepared.luma_texture,
                _chroma_texture: prepared.chroma_texture,
                _luma_view: prepared.luma_view,
                _chroma_view: prepared.chroma_view,
                _color_buffer: prepared.color_buffer,
                bind_group: prepared.bind_group,
                _allocation: prepared._allocation,
            },
            _native_lease: Box::new(native_lease),
            release: None,
        }
    }

    const fn geometry(&self) -> VideoGeometry {
        self.geometry
    }

    pub(crate) const fn generation(&self) -> GpuGeneration {
        self.generation
    }

    const fn sample_kind(&self) -> VideoSampleKind {
        match &self.sample {
            GpuVideoSample::Packed { .. } => VideoSampleKind::Packed,
            GpuVideoSample::BiPlanar { .. } => VideoSampleKind::BiPlanar,
        }
    }

    fn packed_view(&self) -> Option<&wgpu::TextureView> {
        match &self.sample {
            GpuVideoSample::Packed { view, .. } => Some(view),
            GpuVideoSample::BiPlanar { .. } => None,
        }
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        match &self.sample {
            GpuVideoSample::Packed { bind_group, .. }
            | GpuVideoSample::BiPlanar { bind_group, .. } => bind_group,
        }
    }

    fn needs_release(&self) -> bool {
        self.release.is_some()
    }

    fn record_release(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(release) = &self.release {
            release.record(encoder);
        }
    }
}

/// One prepared video sampling operation. Native leases and raw frame objects
/// remain private; the renderer sees only a pipeline kind and bind group.
#[derive(Clone, Copy)]
pub struct PreparedVideoDraw<'a> {
    frame: &'a GpuVideoFrame,
}

impl<'a> PreparedVideoDraw<'a> {
    pub const fn geometry(&self) -> VideoGeometry {
        self.frame.geometry()
    }

    pub fn sampling_transform(&self) -> VideoSamplingTransform {
        self.frame.geometry().sampling_transform()
    }

    pub const fn sample_kind(&self) -> VideoSampleKind {
        self.frame.sample_kind()
    }

    /// Packed RGB view for legacy single-texture consumers. Native bi-planar
    /// samples deliberately do not pretend to be an RGB texture.
    pub fn packed_view(&self) -> Option<&'a wgpu::TextureView> {
        self.frame.packed_view()
    }

    pub fn bind_group(&self) -> &'a wgpu::BindGroup {
        self.frame.bind_group()
    }
}

/// Stable preparation boundary for a renderer pass. It snapshots the set of
/// generation-valid frames once, so drawing code cannot retain native frame
/// objects or independently re-resolve video state midway through a pass.
pub struct PreparedVideoDraws<'a> {
    frames: HashMap<VideoId, &'a GpuVideoFrame>,
}

impl<'a> PreparedVideoDraws<'a> {
    pub(crate) fn new(frames: HashMap<VideoId, &'a GpuVideoFrame>) -> Self {
        Self { frames }
    }

    pub fn get(&self, id: VideoId) -> Option<PreparedVideoDraw<'a>> {
        self.frames
            .get(&id)
            .copied()
            .map(|frame| PreparedVideoDraw { frame })
    }
}
