use std::any::Any;
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::color::VideoColorTransform;
use crate::system::VideoWake;
use crate::{
    BiPlanarVideoFormat, VideoColorimetry, VideoFrameFormat, VideoGeometry, VideoSampleKind,
    VideoSamplingTransform,
};
use neomacs_display_protocol::types::VideoId;

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
    /// Record the backend release operation into `encoder`.
    ///
    /// Callers must pass a fresh, raw-only `wgpu::CommandEncoder` created from
    /// `device` with no wgpu passes or commands recorded on it, because
    /// implementations may call `encoder.as_hal_mut(...)` (mixing `Wgpu` and
    /// `Raw` encoding APIs in one encoder panics in `wgpu-core`).
    fn record(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder);
}

/// Affine owner waiting for the queue's submitted-work completion boundary.
///
/// Native decoder leases enter this state only after their final compositor
/// draw (and any foreign-ownership release barrier) has been submitted. The
/// value then moves into wgpu's completion callback, so ordinary Rust
/// ownership makes early decoder-pool reuse impossible.
#[must_use = "a submitted-work retirement must be registered with the GPU queue"]
pub(crate) struct SubmittedWorkRetirement<T>(T);

impl<T> SubmittedWorkRetirement<T>
where
    T: Send + 'static,
{
    pub(crate) const fn new(retained: T) -> Self {
        Self(retained)
    }

    pub(crate) fn into_completion_callback(self) -> impl FnOnce() + Send + 'static {
        move || drop(self)
    }
}

static NEXT_VIDEO_SYSTEM_ID: AtomicU64 = AtomicU64::new(1);

fn next_video_system_id() -> NonZeroU64 {
    let raw = NEXT_VIDEO_SYSTEM_ID.fetch_add(1, Ordering::Relaxed);
    NonZeroU64::new(raw).expect("video system ID counter must not wrap to zero")
}

/// Shared registry of unsubmitted video-bearing command encoders belonging to
/// one [`crate::VideoSystem`].
pub(crate) struct ActiveTokenRegistry {
    system_id: NonZeroU64,
    next_token_id: AtomicU64,
    active_tokens: Mutex<Vec<u64>>,
}

impl ActiveTokenRegistry {
    fn new() -> Self {
        Self {
            system_id: next_video_system_id(),
            next_token_id: AtomicU64::new(1),
            active_tokens: Mutex::new(Vec::new()),
        }
    }

    pub(crate) const fn system_id(&self) -> NonZeroU64 {
        self.system_id
    }

    pub(crate) fn issue(self: &Arc<Self>) -> VideoSubmissionToken {
        let id = self.next_token_id.fetch_add(1, Ordering::Relaxed);
        self.active_tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(id);
        VideoSubmissionToken {
            registry: Arc::clone(self),
            state: SubmissionTokenState::Armed(id),
        }
    }

    fn remove(&self, token_id: u64) -> bool {
        let mut active = self
            .active_tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(pos) = active.iter().position(|&existing| existing == token_id) {
            active.swap_remove(pos);
        }
        active.is_empty()
    }

    fn is_empty(&self) -> bool {
        self.active_tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty()
    }

    fn clear(&self) {
        self.active_tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

enum SubmissionTokenState {
    Empty,
    Armed(u64),
}

/// Private affine retirement hold owned exclusively by one
/// [`VideoCommandEncoder`].
///
/// Safe code cannot extract, forge, or clear this token while the owning
/// encoder remains live. Dropping an unsubmitted [`VideoCommandEncoder`]
/// destroys its private `wgpu::CommandEncoder` before dropping this token, so
/// abandoned encoders never submit draws after release and never permanently
/// stall future frame retirements.
pub(crate) struct VideoSubmissionToken {
    registry: Arc<ActiveTokenRegistry>,
    state: SubmissionTokenState,
}

impl VideoSubmissionToken {
    fn release_on_registry(&mut self) -> bool {
        match std::mem::replace(&mut self.state, SubmissionTokenState::Empty) {
            SubmissionTokenState::Empty => self.registry.is_empty(),
            SubmissionTokenState::Armed(token_id) => self.registry.remove(token_id),
        }
    }
}

impl Drop for VideoSubmissionToken {
    fn drop(&mut self) {
        if let SubmissionTokenState::Armed(token_id) =
            std::mem::replace(&mut self.state, SubmissionTokenState::Empty)
        {
            self.registry.remove(token_id);
        }
    }
}

/// Command encoder bound by type to one [`crate::VideoSystem`] and its owning
/// `wgpu::Device`.
///
/// Both the inner `wgpu::CommandEncoder` and the retirement token are private:
/// safe callers cannot extract or swap the inner encoder, cannot clear the
/// hold while the encoder remains live, and can only submit it through
/// [`crate::VideoSystem::submit_command_encoder`].
pub struct VideoCommandEncoder {
    system_id: NonZeroU64,
    // Drop order invariant: `encoder` MUST drop before `token` so an
    // unsubmitted `wgpu::CommandEncoder` is destroyed before its retirement
    // hold is released. `Drop::drop` enforces this explicitly too.
    encoder: Option<wgpu::CommandEncoder>,
    token: Option<VideoSubmissionToken>,
}

impl VideoCommandEncoder {
    pub(crate) const fn new(system_id: NonZeroU64, encoder: wgpu::CommandEncoder) -> Self {
        Self {
            system_id,
            encoder: Some(encoder),
            token: None,
        }
    }

    pub(crate) const fn system_id(&self) -> NonZeroU64 {
        self.system_id
    }

    pub(crate) fn into_buffer_and_token(
        mut self,
    ) -> (wgpu::CommandBuffer, Option<VideoSubmissionToken>) {
        let buffer = self
            .encoder
            .take()
            .expect("VideoCommandEncoder inner encoder is live until submission or drop")
            .finish();
        let token = self.token.take();
        (buffer, token)
    }

    pub(crate) fn discard_and_take_token(mut self) -> Option<VideoSubmissionToken> {
        drop(self.encoder.take());
        self.token.take()
    }

    pub fn begin_render_pass<'a>(
        &'a mut self,
        desc: &wgpu::RenderPassDescriptor<'_>,
    ) -> VideoRenderPass<'a> {
        let encoder = self
            .encoder
            .as_mut()
            .expect("VideoCommandEncoder inner encoder is live until submission or drop");
        VideoRenderPass {
            system_id: self.system_id,
            pass: encoder.begin_render_pass(desc),
            encoder_token: &mut self.token,
        }
    }

    pub fn resolve_query_set(
        &mut self,
        query_set: &wgpu::QuerySet,
        query_range: std::ops::Range<u32>,
        destination: &wgpu::Buffer,
        destination_offset: wgpu::BufferAddress,
    ) {
        self.encoder
            .as_mut()
            .expect("VideoCommandEncoder inner encoder is live until submission or drop")
            .resolve_query_set(query_set, query_range, destination, destination_offset);
    }

    pub fn copy_buffer_to_buffer(
        &mut self,
        source: &wgpu::Buffer,
        source_offset: wgpu::BufferAddress,
        destination: &wgpu::Buffer,
        destination_offset: wgpu::BufferAddress,
        copy_size: wgpu::BufferAddress,
    ) {
        self.encoder
            .as_mut()
            .expect("VideoCommandEncoder inner encoder is live until submission or drop")
            .copy_buffer_to_buffer(
                source,
                source_offset,
                destination,
                destination_offset,
                copy_size,
            );
    }
}

impl Drop for VideoCommandEncoder {
    fn drop(&mut self) {
        // Destroy the unsubmitted `wgpu::CommandEncoder` BEFORE releasing any
        // retirement token so recorded draws can never be submitted after the
        // hold clears.
        drop(self.encoder.take());
        drop(self.token.take());
    }
}

/// Render pass bound by type to one [`VideoCommandEncoder`] and its owning
/// [`crate::VideoSystem`].
///
/// The inner `wgpu::RenderPass` is private (and deliberately does not expose
/// `&mut wgpu::RenderPass` or `DerefMut`) so safe code cannot `std::mem::swap`
/// an untracked render pass into this wrapper before calling
/// [`PreparedVideoDraw::bind_to_pass`].
pub struct VideoRenderPass<'a> {
    system_id: NonZeroU64,
    pass: wgpu::RenderPass<'a>,
    encoder_token: &'a mut Option<VideoSubmissionToken>,
}

impl VideoRenderPass<'_> {
    pub(crate) fn bind_video_frame(
        &mut self,
        registry: &Arc<ActiveTokenRegistry>,
        group_index: u32,
        bind_group: &wgpu::BindGroup,
    ) {
        assert_eq!(
            self.system_id,
            registry.system_id(),
            "PreparedVideoDraw belongs to a different VideoSystem than VideoRenderPass"
        );
        if self.encoder_token.is_none() {
            *self.encoder_token = Some(registry.issue());
        }
        self.pass.set_bind_group(group_index, bind_group, &[]);
    }

    pub fn set_pipeline(&mut self, pipeline: &wgpu::RenderPipeline) {
        self.pass.set_pipeline(pipeline);
    }

    pub fn set_bind_group<'b>(
        &mut self,
        index: u32,
        bind_group: impl Into<Option<&'b wgpu::BindGroup>>,
        offsets: &[wgpu::DynamicOffset],
    ) {
        self.pass.set_bind_group(index, bind_group.into(), offsets);
    }

    pub fn set_vertex_buffer(&mut self, slot: u32, buffer_slice: wgpu::BufferSlice<'_>) {
        self.pass.set_vertex_buffer(slot, buffer_slice);
    }

    pub fn set_index_buffer(
        &mut self,
        buffer_slice: wgpu::BufferSlice<'_>,
        index_format: wgpu::IndexFormat,
    ) {
        self.pass.set_index_buffer(buffer_slice, index_format);
    }

    pub fn set_scissor_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.pass.set_scissor_rect(x, y, width, height);
    }

    pub fn set_viewport(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        min_depth: f32,
        max_depth: f32,
    ) {
        self.pass
            .set_viewport(x, y, width, height, min_depth, max_depth);
    }

    pub fn set_blend_constant(&mut self, color: wgpu::Color) {
        self.pass.set_blend_constant(color);
    }

    pub fn set_stencil_reference(&mut self, reference: u32) {
        self.pass.set_stencil_reference(reference);
    }

    pub fn draw(&mut self, vertices: std::ops::Range<u32>, instances: std::ops::Range<u32>) {
        self.pass.draw(vertices, instances);
    }

    pub fn draw_indexed(
        &mut self,
        indices: std::ops::Range<u32>,
        base_vertex: i32,
        instances: std::ops::Range<u32>,
    ) {
        self.pass.draw_indexed(indices, base_vertex, instances);
    }
}

/// Gates replaced-frame retirement on private [`VideoSubmissionToken`]s owned
/// by [`VideoCommandEncoder`]s so a `service()`, `command()`, or
/// `set_presentation()` call between recording draws and
/// [`crate::VideoSystem::submit_command_encoder`] cannot emit a `FOREIGN`
/// release barrier or return native leases before every recorded draw reaches
/// the GPU queue.
pub(crate) struct DrawRetirementGate<T> {
    registry: Arc<ActiveTokenRegistry>,
    pending: Vec<T>,
}

impl<T> Default for DrawRetirementGate<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> DrawRetirementGate<T> {
    pub(crate) fn new() -> Self {
        Self {
            registry: Arc::new(ActiveTokenRegistry::new()),
            pending: Vec::new(),
        }
    }

    pub(crate) fn system_id(&self) -> NonZeroU64 {
        self.registry.system_id()
    }

    pub(crate) const fn registry(&self) -> &Arc<ActiveTokenRegistry> {
        &self.registry
    }

    #[cfg(test)]
    pub(crate) fn issue_token(&self) -> VideoSubmissionToken {
        self.registry.issue()
    }

    pub(crate) fn has_unsubmitted_draws(&self) -> bool {
        !self.registry.is_empty()
    }

    pub(crate) fn stage_or_retire(&mut self, retired: Vec<T>) -> Vec<T> {
        if self.has_unsubmitted_draws() {
            self.pending.extend(retired);
            Vec::new()
        } else if self.pending.is_empty() {
            retired
        } else {
            let mut ready = std::mem::take(&mut self.pending);
            ready.extend(retired);
            ready
        }
    }

    #[must_use = "retired frames must be passed directly into submit_buffer_with_retirements or retire_after_submitted_work"]
    pub(crate) fn finish_encoder_token(&mut self, token: Option<VideoSubmissionToken>) -> Vec<T> {
        let all_submitted = match token {
            Some(mut token) => {
                assert_eq!(
                    token.registry.system_id(),
                    self.system_id(),
                    "VideoSubmissionToken belongs to a different VideoSystem"
                );
                token.release_on_registry()
            }
            None => self.registry.is_empty(),
        };
        if all_submitted {
            std::mem::take(&mut self.pending)
        } else {
            Vec::new()
        }
    }

    pub(crate) fn take_all_on_drop(&mut self) -> Vec<T> {
        self.registry.clear();
        std::mem::take(&mut self.pending)
    }

    #[cfg(test)]
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }
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

#[cfg(all(feature = "native-validation-fixtures", target_os = "linux"))]
impl PreparedBiPlanarTexture {
    pub(crate) fn native_plane_views_for_validation(&self) -> [&wgpu::TextureView; 2] {
        [&self.luma_view, &self.chroma_view]
    }
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
        let (major, minor) = linux_render_device_numbers(&self.device)?;
        Some(LinuxDrmDevice { major, minor })
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
                frame.record_release(&self.device, &mut encoder);
            }
            // Queue order places these release barriers after every draw
            // submitted so far. The completion callback below therefore
            // returns native leases only after ownership is foreign again.
            self.queue.submit(std::iter::once(encoder.finish()));
        }
        let retirement = SubmittedWorkRetirement::new(retired);
        self.queue
            .on_submitted_work_done(retirement.into_completion_callback());
    }

    /// Submit a finished draw command buffer followed by a separate raw-only
    /// release encoder for any staged foreign-surface retirements, in one
    /// `queue.submit` batch.
    ///
    /// `wgpu-core` forbids mixing wgpu passes (`EncodingApi::Wgpu`) with
    /// `as_hal_mut` (`EncodingApi::Raw`) inside one `CommandEncoder`, and
    /// defers wgpu pass commands until `finish()`. Recording release barriers
    /// in a fresh encoder created from `self.device` and submitting
    /// `[draw_buffer, release_encoder.finish()]` avoids the panic, guarantees
    /// Vulkan executes the draws before the `FOREIGN` release barriers, and
    /// never calls `as_hal_mut` on a caller-supplied encoder.
    pub(crate) fn submit_buffer_with_retirements(
        &self,
        draw_buffer: wgpu::CommandBuffer,
        retired: Vec<GpuVideoFrame>,
    ) -> wgpu::SubmissionIndex {
        let needs_release = retired.iter().any(GpuVideoFrame::needs_release);
        let submission = if needs_release {
            let mut release_encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Neomacs video foreign-surface release"),
                    });
            for frame in &retired {
                frame.record_release(&self.device, &mut release_encoder);
            }
            self.queue.submit([draw_buffer, release_encoder.finish()])
        } else {
            self.queue.submit(std::iter::once(draw_buffer))
        };
        if !retired.is_empty() {
            let retirement = SubmittedWorkRetirement::new(retired);
            self.queue
                .on_submitted_work_done(retirement.into_completion_callback());
        }
        submission
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
        let color_buffer = create_mapped_uniform(
            &self.device,
            "Neomacs video color transform",
            bytemuck::bytes_of(&color),
        )
        .map_err(|_| "device lost while creating the video color transform".to_string())?;
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

/// Query the exact DRM render-device identity selected by wgpu's Vulkan
/// adapter. The device numbers come from Vulkan's DRM properties extension,
/// avoiding ambiguous vendor/device scans on identical multi-GPU systems.
#[cfg(target_os = "linux")]
pub fn linux_render_device_numbers(device: &wgpu::Device) -> Option<(u32, u32)> {
    use std::ffi::CStr;

    use ash::vk;
    use wgpu::hal::api::Vulkan;

    unsafe {
        device.as_hal::<Vulkan>().and_then(|hal| {
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
            Some((
                u32::try_from(drm.render_major).ok()?,
                u32::try_from(drm.render_minor).ok()?,
            ))
        })
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
        _view: wgpu::TextureView,
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
                _view: view,
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

    pub(crate) const fn geometry(&self) -> VideoGeometry {
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

    fn bind_group(&self) -> &wgpu::BindGroup {
        match &self.sample {
            GpuVideoSample::Packed { bind_group, .. }
            | GpuVideoSample::BiPlanar { bind_group, .. } => bind_group,
        }
    }

    fn needs_release(&self) -> bool {
        self.release.is_some()
    }

    fn record_release(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if let Some(release) = &self.release {
            release.record(device, encoder);
        }
    }
}

/// Uniform snapshot with no `COPY_DST`. A lost device returns the map error
/// instead of panicking inside `create_buffer_init`.
fn create_mapped_uniform(
    device: &wgpu::Device,
    label: &'static str,
    contents: &[u8],
) -> Result<wgpu::Buffer, wgpu::MapRangeError> {
    let unpadded = contents.len() as wgpu::BufferAddress;
    let align = wgpu::COPY_BUFFER_ALIGNMENT;
    let padded = ((unpadded + align - 1) & !(align - 1)).max(align);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: padded,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: true,
    });
    let mut mapped = buffer.get_mapped_range_mut(..)?;
    mapped.slice(..contents.len()).copy_from_slice(contents);
    drop(mapped);
    buffer.unmap();
    Ok(buffer)
}

/// One prepared video sampling operation. Native leases, texture views, and
/// bind groups remain private; the renderer can only bind this frame into a
/// [`VideoRenderPass`] created from the same [`crate::VideoSystem`].
#[derive(Clone, Copy)]
pub struct PreparedVideoDraw<'a> {
    frame: &'a GpuVideoFrame,
    registry: &'a Arc<ActiveTokenRegistry>,
}

impl<'a> PreparedVideoDraw<'a> {
    #[cfg(test)]
    pub(crate) const fn new_for_test(
        frame: &'a GpuVideoFrame,
        registry: &'a Arc<ActiveTokenRegistry>,
    ) -> Self {
        Self { frame, registry }
    }

    pub const fn geometry(&self) -> VideoGeometry {
        self.frame.geometry()
    }

    pub fn sampling_transform(&self) -> VideoSamplingTransform {
        self.frame.geometry().sampling_transform()
    }

    pub const fn sample_kind(&self) -> VideoSampleKind {
        self.frame.sample_kind()
    }

    /// Bind this prepared video frame's sampling resources into a
    /// [`VideoRenderPass`] belonging to the same [`crate::VideoSystem`], arming
    /// the owning [`VideoCommandEncoder`]'s retirement hold until submission.
    pub fn bind_to_pass(&self, render_pass: &mut VideoRenderPass<'_>, group_index: u32) {
        render_pass.bind_video_frame(self.registry, group_index, self.frame.bind_group());
    }
}

/// Stable preparation boundary for a renderer pass. It snapshots the set of
/// generation-valid frames while borrowing [`crate::VideoSystem`]; binding any
/// frame into a [`VideoRenderPass`] attaches a private retirement hold to the
/// owning [`VideoCommandEncoder`].
pub struct PreparedVideoDraws<'a> {
    frames: HashMap<VideoId, &'a GpuVideoFrame>,
    registry: &'a Arc<ActiveTokenRegistry>,
}

impl<'a> PreparedVideoDraws<'a> {
    pub(crate) fn new(
        frames: HashMap<VideoId, &'a GpuVideoFrame>,
        registry: &'a Arc<ActiveTokenRegistry>,
    ) -> Self {
        Self { frames, registry }
    }

    pub fn get(&self, id: VideoId) -> Option<PreparedVideoDraw<'a>> {
        self.frames
            .get(&id)
            .copied()
            .map(|frame| PreparedVideoDraw {
                frame,
                registry: self.registry,
            })
    }
}
