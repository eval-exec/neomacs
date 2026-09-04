//! Renderer-facing facade over the cross-platform native video subsystem.

use std::collections::{HashMap, HashSet, VecDeque};

use neomacs_display_protocol::types::VideoId;
use neomacs_video::{
    FrameImportPolicy, GpuGeneration, InitialPlayback, LoopMode, PlaybackAction,
    PresentationVisibility, VideoCommand, VideoCommandError, VideoDiagnostics, VideoEvent,
    VideoOpenRequest, VideoSamplingResources, VideoServiceResult, VideoSessionState, VideoSource,
    VideoSystem, VideoWake,
};

use neomacs_video::VideoRecoveryManifest as PlaybackRecoveryManifest;

/// Legacy shader channels consume one filterable RGB texture. Keeping this
/// format fixed makes their color contract independent of the swapchain.
pub(crate) const VIDEO_CHANNEL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Stable renderer identity paired with identity-free playback recovery data.
///
/// This is the device-loss payload retained by the render runtime. The inner
/// playback manifest cannot carry either an editor id or a native-session id,
/// so crossing this boundary cannot silently confuse those identity domains.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoRecoveryManifest {
    id: VideoId,
    playback: PlaybackRecoveryManifest,
    state: VideoState,
}

impl VideoRecoveryManifest {
    pub const fn id(&self) -> VideoId {
        self.id
    }
}

/// Ephemeral identity of one native decoder/player incarnation.
///
/// This deliberately cannot be confused with the stable [`VideoId`] carried
/// by Lisp/layout. Reopening a parked video allocates a new value, so delayed
/// events from the old native session cannot target the new incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct NativeVideoSessionId(VideoId);

impl NativeVideoSessionId {
    const fn protocol(self) -> VideoId {
        self.0
    }
}

#[derive(Default)]
enum SurfacePresentationState {
    #[default]
    Inactive,
    Recording(HashSet<VideoId>),
}

/// Renderer-owned submission and surface-presentation evidence.
///
/// A surface transaction is explicit: submissions outside one are still
/// counted, but only IDs recorded between begin and successful present can
/// advance the presented count. Cancellation drops that pending evidence.
#[derive(Default)]
struct VideoPresentationTracker {
    counts: HashMap<VideoId, neomacs_video::VideoPresentationCounts>,
    timing: HashMap<VideoId, PresentationTimingState>,
    gpu_timing_status: neomacs_video::VideoGpuTimingStatus,
    gpu_timing: HashMap<VideoId, GpuTimingState>,
    surface: SurfacePresentationState,
}

#[derive(Default)]
struct GpuTimingState {
    samples: u64,
    total_us: u64,
    min_us: Option<u64>,
    max_us: Option<u64>,
}

impl GpuTimingState {
    fn record(&mut self, duration_us: u64) {
        self.samples = self.samples.saturating_add(1);
        self.total_us = self.total_us.saturating_add(duration_us);
        self.min_us = Some(self.min_us.map_or(duration_us, |old| old.min(duration_us)));
        self.max_us = Some(self.max_us.map_or(duration_us, |old| old.max(duration_us)));
    }
}

const PRESENTATION_TIMING_WINDOW: usize = 4096;

#[derive(Default)]
struct PresentationTimingState {
    last_presented_at: Option<std::time::Instant>,
    intervals_us: VecDeque<u64>,
    interval_samples: u64,
    interval_total_us: u64,
    interval_min_us: Option<u64>,
    interval_max_us: Option<u64>,
}

impl PresentationTimingState {
    fn record(&mut self, presented_at: std::time::Instant) {
        let Some(previous) = self.last_presented_at.replace(presented_at) else {
            return;
        };
        let Some(interval) = presented_at.checked_duration_since(previous) else {
            return;
        };
        let interval_us = u64::try_from(interval.as_micros()).unwrap_or(u64::MAX);
        self.interval_samples = self.interval_samples.saturating_add(1);
        self.interval_total_us = self.interval_total_us.saturating_add(interval_us);
        self.interval_min_us = Some(
            self.interval_min_us
                .map_or(interval_us, |old| old.min(interval_us)),
        );
        self.interval_max_us = Some(
            self.interval_max_us
                .map_or(interval_us, |old| old.max(interval_us)),
        );
        if self.intervals_us.len() == PRESENTATION_TIMING_WINDOW {
            self.intervals_us.pop_front();
        }
        self.intervals_us.push_back(interval_us);
    }

    fn diagnostics(&self) -> neomacs_video::VideoPresentationTiming {
        let mut sorted: Vec<_> = self.intervals_us.iter().copied().collect();
        sorted.sort_unstable();
        neomacs_video::VideoPresentationTiming {
            interval_samples: self.interval_samples,
            interval_total_us: self.interval_total_us,
            interval_min_us: self.interval_min_us,
            interval_max_us: self.interval_max_us,
            interval_p50_us: percentile(&sorted, 50),
            interval_p95_us: percentile(&sorted, 95),
            interval_p99_us: percentile(&sorted, 99),
        }
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = percentile.saturating_mul(sorted.len()).div_ceil(100);
    sorted.get(rank.saturating_sub(1)).copied()
}

impl VideoPresentationTracker {
    fn begin_measurement_epoch(&mut self) {
        self.counts.clear();
        self.timing.clear();
        self.gpu_timing.clear();
        self.surface = SurfacePresentationState::Inactive;
    }

    fn set_gpu_timing_status(&mut self, status: neomacs_video::VideoGpuTimingStatus) {
        self.gpu_timing_status = status;
    }

    fn begin_surface(&mut self) {
        self.surface = SurfacePresentationState::Recording(HashSet::new());
    }

    fn record_submitted(&mut self, ids: impl IntoIterator<Item = VideoId>) {
        let unique: HashSet<_> = ids.into_iter().collect();
        for id in unique {
            let counts = self.counts.entry(id).or_default();
            counts.submitted_frames = counts.submitted_frames.saturating_add(1);
            if let SurfacePresentationState::Recording(pending) = &mut self.surface {
                pending.insert(id);
            }
        }
    }

    fn finish_presented_surface(&mut self) {
        // CPU STOPWATCH, NOT A VISUAL PHASE: this feeds the presentation
        // interval histogram (p50/p95/p99) that diagnoses how the compositor
        // actually paced us. Dating it to the frame's predicted presentation
        // would make it measure the schedule we asked for instead of the one
        // we got, which is exactly the discrepancy it exists to expose.
        self.finish_presented_surface_at(std::time::Instant::now());
    }

    fn finish_presented_surface_at(&mut self, presented_at: std::time::Instant) {
        let SurfacePresentationState::Recording(pending) = std::mem::take(&mut self.surface) else {
            return;
        };
        for id in pending {
            let counts = self.counts.entry(id).or_default();
            counts.presented_frames = counts.presented_frames.saturating_add(1);
            self.timing.entry(id).or_default().record(presented_at);
        }
    }

    fn cancel_surface(&mut self) {
        self.surface = SurfacePresentationState::Inactive;
    }

    fn counts(&self, id: VideoId) -> neomacs_video::VideoPresentationCounts {
        self.counts.get(&id).copied().unwrap_or_default()
    }

    fn timing(&self, id: VideoId) -> neomacs_video::VideoPresentationTiming {
        self.timing
            .get(&id)
            .map_or_else(Default::default, PresentationTimingState::diagnostics)
    }

    fn record_gpu_frame_time(&mut self, ids: impl IntoIterator<Item = VideoId>, duration_us: u64) {
        for id in ids.into_iter().collect::<HashSet<_>>() {
            self.gpu_timing.entry(id).or_default().record(duration_us);
        }
    }

    fn gpu_timing(&self, id: VideoId) -> neomacs_video::VideoGpuTiming {
        let timing = self.gpu_timing.get(&id);
        neomacs_video::VideoGpuTiming {
            status: self.gpu_timing_status,
            pass_samples: timing.map_or(0, |timing| timing.samples),
            pass_total_us: timing.map_or(0, |timing| timing.total_us),
            pass_min_us: timing.and_then(|timing| timing.min_us),
            pass_max_us: timing.and_then(|timing| timing.max_us),
        }
    }

    fn remove(&mut self, id: VideoId) {
        self.counts.remove(&id);
        self.timing.remove(&id);
        self.gpu_timing.remove(&id);
        if let SurfacePresentationState::Recording(pending) = &mut self.surface {
            pending.remove(&id);
        }
    }
}

/// Compatibility presentation of the typed native session lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    Loading,
    Playing,
    Paused,
    Stopped,
    EndOfStream,
    Error,
}

impl From<VideoSessionState> for VideoState {
    fn from(state: VideoSessionState) -> Self {
        match state {
            VideoSessionState::Opening => Self::Loading,
            VideoSessionState::Playing => Self::Playing,
            VideoSessionState::Paused => Self::Paused,
            VideoSessionState::Ended => Self::EndOfStream,
            VideoSessionState::Failed => Self::Error,
            VideoSessionState::Closed => Self::Stopped,
        }
    }
}

/// Renderer-facing metadata. The authoritative frame, GPU handles, and native
/// lease remain together in [`VideoSystem`].
pub struct CachedVideo {
    pub id: VideoId,
    pub width: u32,
    pub height: u32,
    pub state: VideoState,
    pub frame_count: u64,
    failure: Option<VideoCommandError>,
    native_id: Option<NativeVideoSessionId>,
    parked: Option<PlaybackRecoveryManifest>,
}

impl CachedVideo {
    /// Structured terminal failure retained for frontend diagnostics and
    /// backend-specific remediation such as codec installation.
    pub fn failure(&self) -> Option<&VideoCommandError> {
        self.failure.as_ref()
    }
}

/// Renderer preparation keyed by stable declarative ids while the native
/// subsystem is free to replace parked decoder-session ids.
pub struct PreparedVideoDraws<'a> {
    native: neomacs_video::PreparedVideoDraws<'a>,
    native_ids: HashMap<VideoId, NativeVideoSessionId>,
}

impl<'a> PreparedVideoDraws<'a> {
    pub fn get(&self, id: VideoId) -> Option<neomacs_video::PreparedVideoDraw<'a>> {
        self.native.get(self.native_ids.get(&id)?.protocol())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoChannelPreparation {
    ReusePacked,
    ConvertBiPlanar,
}

const fn video_channel_preparation(
    kind: neomacs_video::VideoSampleKind,
) -> VideoChannelPreparation {
    match kind {
        neomacs_video::VideoSampleKind::Packed => VideoChannelPreparation::ReusePacked,
        neomacs_video::VideoSampleKind::BiPlanar => VideoChannelPreparation::ConvertBiPlanar,
    }
}

struct VideoChannelTarget {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

struct VideoChannelTargets {
    targets: HashMap<VideoId, VideoChannelTarget>,
}

impl VideoChannelTargets {
    fn new() -> Self {
        Self {
            targets: HashMap::new(),
        }
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        id: VideoId,
        width: u32,
        height: u32,
    ) -> (&wgpu::TextureView, Option<usize>) {
        let needs_allocation = self
            .targets
            .get(&id)
            .is_none_or(|target| target.width != width || target.height != height);
        let allocated_bytes = if needs_allocation {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Neomacs native-video shader channel"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: VIDEO_CHANNEL_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.targets.insert(
                id,
                VideoChannelTarget {
                    _texture: texture,
                    view,
                    width,
                    height,
                },
            );
            Some(
                usize::try_from(width)
                    .ok()
                    .and_then(|width| {
                        usize::try_from(height)
                            .ok()
                            .and_then(|height| width.checked_mul(height))
                    })
                    .and_then(|pixels| pixels.checked_mul(4))
                    .unwrap_or(usize::MAX),
            )
        } else {
            None
        };
        (&self.targets[&id].view, allocated_bytes)
    }

    fn remove(&mut self, id: VideoId) -> bool {
        self.targets.remove(&id).is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoGpuAccountingChange {
    Unchanged,
    Register(usize),
    Free,
}

#[derive(Default)]
struct VideoGpuAccounting {
    bytes: usize,
}

impl VideoGpuAccounting {
    fn observe(&mut self, bytes: usize) -> VideoGpuAccountingChange {
        let change = match (self.bytes, bytes) {
            (previous, current) if previous == current => VideoGpuAccountingChange::Unchanged,
            (_, 0) => VideoGpuAccountingChange::Free,
            (_, current) => VideoGpuAccountingChange::Register(current),
        };
        self.bytes = bytes;
        change
    }
}

/// Video import pools are shared across sessions, so accounting them under a
/// fabricated per-session size would double-count and make frees unbalanced.
/// Renderer media IDs start at one; zero is reserved for this aggregate pool.
pub(crate) const VIDEO_GPU_POOL_ACCOUNTING_ID: u32 = 0;

type VideoSystemInitializer = Box<dyn FnOnce() -> Result<VideoSystem, String>>;

/// Runtime availability of the native decoder. Keeping `Deferred` distinct
/// from `Unavailable` guarantees that renderer construction does not probe an
/// optional backend, while a failed first probe is never repeated per frame.
enum VideoSystemState {
    Deferred(VideoSystemInitializer),
    Ready(Box<VideoSystem>),
    Unavailable(String),
    Taken,
}

impl VideoSystemState {
    fn deferred(initialize: impl FnOnce() -> Result<VideoSystem, String> + 'static) -> Self {
        Self::Deferred(Box::new(initialize))
    }

    #[cfg(test)]
    fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable(message.into())
    }

    fn ready(&self) -> Option<&VideoSystem> {
        match self {
            Self::Ready(system) => Some(system),
            Self::Deferred(_) | Self::Unavailable(_) | Self::Taken => None,
        }
    }

    fn diagnostics(&self) -> Result<VideoDiagnostics, String> {
        match self {
            Self::Ready(system) => Ok(system.diagnostics()),
            Self::Deferred(_) => Ok(VideoDiagnostics {
                renderer: None,
                sessions: Vec::new(),
                surface_pools: Vec::new(),
                gpu_memory_bytes: 0,
            }),
            Self::Unavailable(message) => Err(message.clone()),
            Self::Taken => Err("native video diagnostics are temporarily unavailable".to_owned()),
        }
    }

    fn already_diagnosed(&self, message: &str) -> bool {
        matches!(self, Self::Unavailable(unavailable) if unavailable == message)
    }

    fn get_or_initialize(&mut self) -> Result<&mut VideoSystem, String> {
        self.initialize_if_needed()?;
        match self {
            Self::Ready(system) => Ok(system),
            Self::Unavailable(message) => Err(message.clone()),
            Self::Deferred(_) | Self::Taken => unreachable!("initialization resolved state"),
        }
    }

    fn take_or_initialize(&mut self) -> Result<VideoSystem, String> {
        self.initialize_if_needed()?;
        match std::mem::replace(self, Self::Taken) {
            Self::Ready(system) => Ok(*system),
            Self::Unavailable(message) => {
                *self = Self::Unavailable(message.clone());
                Err(message)
            }
            Self::Deferred(_) | Self::Taken => unreachable!("initialization resolved state"),
        }
    }

    fn put_ready(&mut self, system: VideoSystem) {
        assert!(matches!(self, Self::Taken));
        *self = Self::Ready(Box::new(system));
    }

    fn initialize_if_needed(&mut self) -> Result<(), String> {
        let Self::Deferred(_) = self else {
            return match self {
                Self::Ready(_) => Ok(()),
                Self::Unavailable(message) => Err(message.clone()),
                Self::Taken => panic!("video system is already borrowed"),
                Self::Deferred(_) => unreachable!(),
            };
        };
        let Self::Deferred(initialize) = std::mem::replace(self, Self::Taken) else {
            unreachable!("checked deferred state")
        };
        match initialize() {
            Ok(system) => {
                *self = Self::Ready(Box::new(system));
                Ok(())
            }
            Err(message) => {
                tracing::error!(error = %message, "native video subsystem is unavailable");
                *self = Self::Unavailable(message.clone());
                Err(message)
            }
        }
    }
}

/// Cross-platform video cache. Decode and native-surface import belong to
/// `neomacs-video`; this facade maintains renderer metadata and budget events.
pub struct VideoCache {
    system: VideoSystemState,
    sampling: Option<VideoSamplingResources>,
    channel_targets: Option<VideoChannelTargets>,
    videos: HashMap<VideoId, CachedVideo>,
    next_id: u32,
    next_native_id: u32,
    native_to_video: HashMap<NativeVideoSessionId, VideoId>,
    accounting: Vec<crate::media_budget::MediaAccounting>,
    gpu_accounting: VideoGpuAccounting,
    presentation: VideoPresentationTracker,
    terminal_diagnostics: HashMap<VideoId, neomacs_video::VideoSessionDiagnostics>,
    last_service: VideoServiceResult,
}

impl VideoCache {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        generation: GpuGeneration,
        wake: VideoWake,
    ) -> Self {
        let sampling = VideoSamplingResources::new(device, bind_group_layout, sampler);
        let device = device.clone();
        let queue = queue.clone();
        let system_sampling = sampling.clone();
        let system = VideoSystemState::deferred(move || {
            VideoSystem::with_sampling_resources(
                device,
                queue,
                system_sampling,
                generation,
                FrameImportPolicy::PERFORMANCE_DEFAULT,
                wake,
            )
            .map_err(|error| error.to_string())
        });
        Self {
            system,
            sampling: Some(sampling),
            channel_targets: Some(VideoChannelTargets::new()),
            videos: HashMap::new(),
            next_id: 1,
            next_native_id: 1,
            native_to_video: HashMap::new(),
            accounting: Vec::new(),
            gpu_accounting: VideoGpuAccounting::default(),
            presentation: VideoPresentationTracker::default(),
            terminal_diagnostics: HashMap::new(),
            last_service: VideoServiceResult::default(),
        }
    }

    pub(crate) fn bi_planar_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.sampling
            .as_ref()
            .expect("production video cache owns sampling resources")
            .bi_planar_bind_group_layout()
    }

    pub fn load_file(&mut self, path: &str) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.load_file_with_id(id, path, 0, false);
        id
    }

    pub fn load_file_with_id(&mut self, id: u32, path: &str, loop_count: i32, autoplay: bool) {
        self.load_legacy_source_with_id(id, VideoSource::File(path.into()), loop_count, autoplay);
    }

    pub fn load_uri_with_id(&mut self, id: u32, uri: &str, loop_count: i32, autoplay: bool) {
        self.load_legacy_source_with_id(id, VideoSource::Uri(uri.to_owned()), loop_count, autoplay);
    }

    fn load_legacy_source_with_id(
        &mut self,
        id: u32,
        source: VideoSource,
        loop_count: i32,
        autoplay: bool,
    ) {
        let loop_mode = match LoopMode::from_legacy(loop_count) {
            Ok(loop_mode) => loop_mode,
            Err(error) => {
                let typed_id = VideoId::new(id);
                self.videos.insert(
                    typed_id,
                    CachedVideo {
                        id: typed_id,
                        width: 0,
                        height: 0,
                        state: VideoState::Error,
                        frame_count: 0,
                        failure: None,
                        native_id: None,
                        parked: None,
                    },
                );
                self.handle_operation_error(typed_id, error.to_string().into());
                return;
            }
        };
        self.open(
            VideoId::new(id),
            VideoOpenRequest {
                source,
                loop_mode,
                initial_playback: if autoplay {
                    InitialPlayback::Playing
                } else {
                    InitialPlayback::Paused
                },
            },
        );
    }

    /// Open one stable editor session. `VideoId` never aliases the native
    /// decoder incarnation allocated below, which is important during device
    /// recovery and delayed backend events.
    pub fn open(&mut self, id: VideoId, request: VideoOpenRequest) {
        // Stable ids are allocated once by the editor, but recovery/replay can
        // defensively reopen one. Close before replacement so the old native
        // routing entry and decoder cannot survive an idempotent replay.
        if self.videos.contains_key(&id) {
            self.close(id);
        }
        self.next_id = self.next_id.max(id.get().saturating_add(1));
        let native_id = self.allocate_native_id();
        self.videos.insert(
            id,
            CachedVideo {
                id,
                width: 0,
                height: 0,
                state: VideoState::Loading,
                frame_count: 0,
                failure: None,
                native_id: Some(native_id),
                parked: None,
            },
        );
        self.native_to_video.insert(native_id, id);
        let result = self.command(VideoCommand::Open {
            id: native_id.protocol(),
            source: request.source,
            initial_playback: request.initial_playback,
            loop_mode: request.loop_mode,
        });
        if let Err(error) = result {
            self.native_to_video.remove(&native_id);
            if let Some(video) = self.videos.get_mut(&id) {
                video.native_id = None;
            }
            self.handle_operation_error(id, error);
        }
    }

    /// Route all session playback actions through one exhaustive typed seam.
    pub fn control(&mut self, id: VideoId, action: PlaybackAction) {
        match action {
            PlaybackAction::Play => self.play(id.get()),
            PlaybackAction::Pause => self.pause(id.get()),
            PlaybackAction::Stop => self.stop(id.get()),
            PlaybackAction::SetLoop(mode) => self.set_loop_mode(id, mode),
            action @ (PlaybackAction::Seek(_) | PlaybackAction::SetRate(_)) => {
                let result = self
                    .videos
                    .get(&id)
                    .and_then(|video| video.native_id)
                    .ok_or_else(|| VideoCommandError::from("video session is not active"))
                    .and_then(|native_id| {
                        self.command(VideoCommand::Playback {
                            id: native_id.protocol(),
                            action,
                        })
                    });
                if let Err(error) = result {
                    self.handle_operation_error(id, error);
                }
            }
        }
    }

    fn allocate_native_id(&mut self) -> NativeVideoSessionId {
        let id = NativeVideoSessionId(VideoId::new(self.next_native_id));
        self.next_native_id = self
            .next_native_id
            .checked_add(1)
            .expect("native video session id space exhausted");
        id
    }

    fn command(&mut self, command: VideoCommand) -> Result<(), VideoCommandError> {
        self.system
            .get_or_initialize()
            .map_err(VideoCommandError::from)?
            .command(command)
    }

    pub fn get_state(&self, id: u32) -> Option<VideoState> {
        self.videos.get(&VideoId::new(id)).map(|video| video.state)
    }

    pub fn get_dimensions(&self, id: u32) -> Option<(u32, u32)> {
        self.videos
            .get(&VideoId::new(id))
            .map(|video| (video.width, video.height))
    }

    pub fn get(&self, id: u32) -> Option<&CachedVideo> {
        self.videos.get(&VideoId::new(id))
    }

    /// Prepare one immutable, generation-checked view of the video resources
    /// needed by a renderer pass. Native leases and frame ownership stay in
    /// the video system.
    pub fn prepare_draws(
        &self,
        ids: impl IntoIterator<Item = VideoId>,
    ) -> Option<PreparedVideoDraws<'_>> {
        let native_ids: HashMap<_, _> = ids
            .into_iter()
            .filter_map(|id| Some((id, self.videos.get(&id)?.native_id?)))
            .collect();
        let native = self
            .system
            .ready()?
            .prepare_draws(native_ids.values().map(|id| id.protocol()));
        Some(PreparedVideoDraws { native, native_ids })
    }

    pub(crate) fn begin_surface_render(&mut self) {
        self.presentation.begin_surface();
    }

    pub(crate) fn cancel_surface_render(&mut self) {
        self.presentation.cancel_surface();
    }

    pub(crate) fn finish_presented_surface(&mut self) {
        self.presentation.finish_presented_surface();
    }

    pub(crate) fn record_submitted_frames(&mut self, ids: impl IntoIterator<Item = VideoId>) {
        self.presentation.record_submitted(ids);
    }

    pub(crate) fn set_gpu_timing_status(&mut self, status: neomacs_video::VideoGpuTimingStatus) {
        self.presentation.set_gpu_timing_status(status);
    }

    pub(crate) fn record_gpu_frame_time(
        &mut self,
        ids: impl IntoIterator<Item = VideoId>,
        duration_us: u64,
    ) {
        let active_ids: Vec<_> = ids
            .into_iter()
            .filter(|id| self.videos.contains_key(id))
            .collect();
        self.presentation
            .record_gpu_frame_time(active_ids, duration_us);
    }

    /// Reset measurement-only counters at one render-thread-acknowledged
    /// boundary. Playback state and resident pooled surfaces remain intact.
    pub fn begin_measurement_epoch(&mut self) -> Result<(), String> {
        self.system.get_or_initialize()?.begin_measurement_epoch();
        self.presentation.begin_measurement_epoch();
        Ok(())
    }

    pub fn play(&mut self, id: u32) {
        let typed_id = VideoId::new(id);
        let result = if let Some(native_id) = self.videos.get(&typed_id).and_then(|v| v.native_id) {
            self.command(VideoCommand::Playback {
                id: native_id.protocol(),
                action: PlaybackAction::Play,
            })
        } else {
            self.update_parked(typed_id, |manifest| manifest.with_desired_playing(true))
        };
        match result {
            Ok(()) => self.set_state(typed_id, VideoState::Playing),
            Err(error) => self.handle_operation_error(typed_id, error),
        }
    }

    pub fn pause(&mut self, id: u32) {
        let typed_id = VideoId::new(id);
        let result = if let Some(native_id) = self.videos.get(&typed_id).and_then(|v| v.native_id) {
            self.command(VideoCommand::Playback {
                id: native_id.protocol(),
                action: PlaybackAction::Pause,
            })
        } else {
            self.update_parked(typed_id, |manifest| manifest.with_desired_playing(false))
        };
        match result {
            Ok(()) => self.set_state(typed_id, VideoState::Paused),
            Err(error) => self.handle_operation_error(typed_id, error),
        }
    }

    pub fn stop(&mut self, id: u32) {
        let typed_id = VideoId::new(id);
        let result = if let Some(native_id) = self.videos.get(&typed_id).and_then(|v| v.native_id) {
            self.command(VideoCommand::Playback {
                id: native_id.protocol(),
                action: PlaybackAction::Stop,
            })
        } else {
            self.update_parked(typed_id, PlaybackRecoveryManifest::stopped)
        };
        match result {
            Ok(()) => self.set_state(typed_id, VideoState::Stopped),
            Err(error) => self.handle_operation_error(typed_id, error),
        }
    }

    pub fn set_loop(&mut self, id: u32, count: i32) {
        match LoopMode::from_legacy(count) {
            Ok(mode) => self.set_loop_mode(VideoId::new(id), mode),
            Err(error) => self.handle_operation_error(VideoId::new(id), error.to_string().into()),
        }
    }

    fn set_loop_mode(&mut self, id: VideoId, mode: LoopMode) {
        let result = if self
            .videos
            .get(&id)
            .and_then(|video| video.native_id)
            .is_none()
        {
            self.update_parked(id, |manifest| manifest.with_loop_mode(mode))
        } else {
            let native_id = self.videos[&id]
                .native_id
                .expect("checked active native video session");
            self.command(VideoCommand::Playback {
                id: native_id.protocol(),
                action: PlaybackAction::SetLoop(mode),
            })
        };
        if let Err(error) = result {
            self.handle_operation_error(id, error);
        }
    }

    pub fn remove(&mut self, id: u32) {
        self.close(VideoId::new(id));
    }

    pub fn close(&mut self, id: VideoId) {
        if let Some(native_id) = self.videos.get(&id).and_then(|video| video.native_id) {
            let _ = self.command(VideoCommand::Close {
                id: native_id.protocol(),
            });
            self.native_to_video.remove(&native_id);
        }
        self.videos.remove(&id);
        self.presentation.remove(id);
        self.terminal_diagnostics.remove(&id);
        if self
            .channel_targets
            .as_mut()
            .is_some_and(|targets| targets.remove(id))
        {
            self.accounting
                .push(crate::media_budget::MediaAccounting::Freed {
                    media_type: crate::media_budget::MediaType::Video,
                    id: id.get(),
                });
        }
    }

    /// Resolve shader-surface video channels to ordinary RGB texture views.
    /// Packed frames can be reused directly; native planes are converted by
    /// one GPU draw only for this legacy single-texture consumer. Inline video
    /// remains on the fused final-composition path.
    pub(crate) fn prepare_channel_views(
        &mut self,
        ids: impl IntoIterator<Item = VideoId>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::RenderPipeline,
        uniform_bind_group: &wgpu::BindGroup,
    ) -> HashMap<VideoId, wgpu::TextureView> {
        let native_ids: Vec<_> = ids
            .into_iter()
            .filter_map(|id| Some((id, self.videos.get(&id)?.native_id?)))
            .collect();
        let Some(system) = self.system.ready() else {
            return HashMap::new();
        };
        let draws = system.prepare_draws(native_ids.iter().map(|(_, id)| id.protocol()));
        let Some(targets) = self.channel_targets.as_mut() else {
            return HashMap::new();
        };
        let mut views = HashMap::with_capacity(native_ids.len());
        let mut encoder = None;
        for (external_id, native_id) in native_ids {
            let Some(frame) = draws.get(native_id.protocol()) else {
                continue;
            };
            match video_channel_preparation(frame.sample_kind()) {
                VideoChannelPreparation::ReusePacked => {
                    if let Some(view) = frame.packed_view() {
                        views.insert(external_id, view.clone());
                    }
                }
                VideoChannelPreparation::ConvertBiPlanar => {
                    let geometry = frame.geometry();
                    let (view, allocated_bytes) = targets.prepare(
                        device,
                        external_id,
                        geometry.coded_width,
                        geometry.coded_height,
                    );
                    if let Some(size_bytes) = allocated_bytes {
                        self.accounting
                            .push(crate::media_budget::MediaAccounting::Registered {
                                media_type: crate::media_budget::MediaType::Video,
                                id: external_id.get(),
                                size_bytes,
                            });
                    }
                    let encoder = encoder.get_or_insert_with(|| {
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Neomacs native-video shader-channel conversion"),
                        })
                    });
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("Neomacs native-video shader-channel conversion"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                        pass.set_pipeline(pipeline);
                        pass.set_bind_group(0, uniform_bind_group, &[]);
                        pass.set_bind_group(1, frame.bind_group(), &[]);
                        pass.draw(0..3, 0..1);
                    }
                    views.insert(external_id, view.clone());
                }
            }
        }
        if let Some(encoder) = encoder {
            queue.submit(std::iter::once(encoder.finish()));
        }
        views
    }

    fn update_parked(
        &mut self,
        id: VideoId,
        update: impl FnOnce(PlaybackRecoveryManifest) -> PlaybackRecoveryManifest,
    ) -> Result<(), VideoCommandError> {
        let video = self
            .videos
            .get_mut(&id)
            .ok_or_else(|| VideoCommandError::SessionNotOpen { id: id.get() })?;
        let manifest = video
            .parked
            .take()
            .ok_or_else(|| VideoCommandError::SessionNotOpen { id: id.get() })?;
        video.parked = Some(update(manifest));
        Ok(())
    }

    pub fn process_pending(
        &mut self,
        request: neomacs_video::VideoServiceRequest,
    ) -> &VideoServiceResult {
        let needs_system = self
            .videos
            .values()
            .any(|video| video.native_id.is_some() || video.parked.is_some());
        if !needs_system {
            return &self.last_service;
        }
        let Ok(mut system) = self.system.take_or_initialize() else {
            return &self.last_service;
        };
        for external_id in self.videos.keys().copied().collect::<Vec<_>>() {
            let result = if request.is_presented(external_id) {
                self.resume_presented(&mut system, external_id)
            } else {
                self.park_hidden(&mut system, external_id)
            };
            if let Err(error) = result {
                self.handle_operation_error(external_id, error);
            }
        }

        let mut native_request = neomacs_video::VideoServiceRequest::new(request.service_time());
        for (external_id, target) in request.presentation_targets() {
            if let Some(native_id) = self
                .videos
                .get(&external_id)
                .and_then(|video| video.native_id)
            {
                native_request.set_presentation_target(native_id.protocol(), target);
            }
        }
        let native_result = system.service_with_request(native_request);
        let mut events = Vec::with_capacity(native_result.events.len());
        for event in native_result.events {
            let native_id = NativeVideoSessionId(event.id());
            let Some(&external_id) = self.native_to_video.get(&native_id) else {
                continue;
            };
            let event = event.with_id(external_id);
            self.observe_event(&event, &mut system, native_id);
            events.push(event);
        }

        let mut ready_frames = Vec::with_capacity(native_result.ready_frames.len());
        for ready in native_result.ready_frames {
            let native_id = NativeVideoSessionId(ready.id);
            let Some(&external_id) = self.native_to_video.get(&native_id) else {
                continue;
            };
            let draws = system.prepare_draws(std::iter::once(native_id.protocol()));
            let Some(frame) = draws.get(native_id.protocol()) else {
                continue;
            };
            let geometry = frame.geometry();
            if let Some(video) = self.videos.get_mut(&external_id) {
                video.width = geometry.display_width;
                video.height = geometry.display_height;
                video.frame_count = video.frame_count.saturating_add(1);
            }
            ready_frames.push(neomacs_video::VideoFrameReady {
                id: external_id,
                pts: ready.pts,
                frame_path: ready.frame_path,
            });
        }
        match self.gpu_accounting.observe(system.gpu_memory_bytes()) {
            VideoGpuAccountingChange::Unchanged => {}
            VideoGpuAccountingChange::Register(size_bytes) => {
                self.accounting
                    .push(crate::media_budget::MediaAccounting::Registered {
                        media_type: crate::media_budget::MediaType::Video,
                        id: VIDEO_GPU_POOL_ACCOUNTING_ID,
                        size_bytes,
                    });
            }
            VideoGpuAccountingChange::Free => {
                self.accounting
                    .push(crate::media_budget::MediaAccounting::Freed {
                        media_type: crate::media_budget::MediaType::Video,
                        id: VIDEO_GPU_POOL_ACCOUNTING_ID,
                    });
            }
        }
        self.system.put_ready(system);
        self.last_service = VideoServiceResult {
            events,
            ready_frames,
            next_deadline: native_result.next_deadline,
        };
        &self.last_service
    }

    fn resume_presented(
        &mut self,
        system: &mut VideoSystem,
        external_id: VideoId,
    ) -> Result<(), VideoCommandError> {
        if let Some(native_id) = self
            .videos
            .get(&external_id)
            .and_then(|video| video.native_id)
        {
            return system
                .set_presentation(native_id.protocol(), PresentationVisibility::Presented);
        }

        let Some(manifest) = self
            .videos
            .get_mut(&external_id)
            .and_then(|video| video.parked.take())
        else {
            return Ok(());
        };
        let native_id = self.allocate_native_id();
        let native_manifest = manifest
            .clone()
            .with_presentation(PresentationVisibility::Presented);
        if let Err(error) = system.open_from_manifest(native_id.protocol(), &native_manifest) {
            self.videos
                .get_mut(&external_id)
                .expect("parked video remains registered")
                .parked = Some(manifest);
            return Err(error);
        }

        self.native_to_video.insert(native_id, external_id);
        let video = self
            .videos
            .get_mut(&external_id)
            .expect("resumed video remains registered");
        video.native_id = Some(native_id);
        video.state = VideoState::Loading;
        video.failure = None;
        self.terminal_diagnostics.remove(&external_id);
        Ok(())
    }

    fn park_hidden(
        &mut self,
        system: &mut VideoSystem,
        external_id: VideoId,
    ) -> Result<(), VideoCommandError> {
        let Some(native_id) = self
            .videos
            .get(&external_id)
            .and_then(|video| video.native_id)
        else {
            return Ok(());
        };

        let visibility_result =
            system.set_presentation(native_id.protocol(), PresentationVisibility::Hidden);
        let manifest = system
            .recovery_sessions()
            .into_iter()
            .find(|recovery| recovery.id() == native_id.protocol())
            .map(|recovery| {
                recovery
                    .into_manifest()
                    .with_presentation(PresentationVisibility::Hidden)
            });
        let close_result = system.command(VideoCommand::Close {
            id: native_id.protocol(),
        });
        self.native_to_video.remove(&native_id);
        if let Some(video) = self.videos.get_mut(&external_id) {
            video.native_id = None;
            video.parked = manifest;
        }

        visibility_result.and(close_result)
    }

    pub fn last_service(&self) -> &VideoServiceResult {
        &self.last_service
    }

    /// Return native diagnostics in stable editor identity space.
    ///
    /// A native decoder incarnation may be replaced after parking or device
    /// loss. Stale native sessions are therefore filtered instead of leaking
    /// their private IDs across the renderer seam.
    pub fn diagnostics(&self) -> Result<VideoDiagnostics, String> {
        let mut diagnostics = match self.system.diagnostics() {
            Ok(diagnostics) => diagnostics,
            Err(_) if !self.terminal_diagnostics.is_empty() => VideoDiagnostics {
                renderer: None,
                sessions: Vec::new(),
                surface_pools: Vec::new(),
                gpu_memory_bytes: 0,
            },
            Err(message) => return Err(message),
        }
        .filter_map_session_ids(|id| self.native_to_video.get(&NativeVideoSessionId(id)).copied());
        for session in &mut diagnostics.sessions {
            session.presentation_counts = self.presentation.counts(session.id);
            session.presentation_timing = self.presentation.timing(session.id);
            session.gpu_timing = self.presentation.gpu_timing(session.id);
        }
        diagnostics.sessions.extend(
            self.terminal_diagnostics
                .iter()
                .filter(|(id, _)| {
                    self.videos
                        .get(id)
                        .is_some_and(|video| video.native_id.is_none())
                })
                .map(|(_, session)| session.clone()),
        );
        diagnostics.sessions.sort_by_key(|session| session.id);
        Ok(diagnostics)
    }

    pub fn drain_accounting(&mut self) -> Vec<crate::media_budget::MediaAccounting> {
        std::mem::take(&mut self.accounting)
    }

    pub fn recovery_manifests(&self) -> Vec<VideoRecoveryManifest> {
        let mut manifests: Vec<_> = self
            .system
            .ready()
            .map_or_else(Vec::new, VideoSystem::recovery_sessions)
            .into_iter()
            .filter_map(|recovery| {
                let external_id = self
                    .native_to_video
                    .get(&NativeVideoSessionId(recovery.id()))?;
                let state = self.videos.get(external_id)?.state;
                Some(VideoRecoveryManifest {
                    id: *external_id,
                    playback: recovery.into_manifest(),
                    state,
                })
            })
            .collect();
        manifests.extend(self.videos.values().filter_map(|video| {
            Some(VideoRecoveryManifest {
                id: video.id,
                playback: video.parked.clone()?,
                state: video.state,
            })
        }));
        manifests
    }

    pub fn restore_after_device_loss(&mut self, manifests: Vec<VideoRecoveryManifest>) {
        if manifests.is_empty() {
            return;
        }
        let mut system = match self.system.take_or_initialize() {
            Ok(system) => system,
            Err(message) => {
                for manifest in manifests {
                    let external_id = manifest.id();
                    self.next_id = self.next_id.max(external_id.get().saturating_add(1));
                    self.videos.insert(
                        external_id,
                        CachedVideo {
                            id: external_id,
                            width: 0,
                            height: 0,
                            state: VideoState::Error,
                            frame_count: 0,
                            failure: None,
                            native_id: None,
                            parked: Some(manifest.playback),
                        },
                    );
                    self.handle_operation_error(external_id, message.clone().into());
                }
                return;
            }
        };

        for manifest in manifests {
            let external_id = manifest.id();
            let is_presented =
                manifest.playback.presentation() == PresentationVisibility::Presented;
            self.next_id = self.next_id.max(external_id.get().saturating_add(1));
            self.videos.insert(
                external_id,
                CachedVideo {
                    id: external_id,
                    width: 0,
                    height: 0,
                    state: manifest.state,
                    frame_count: 0,
                    failure: None,
                    native_id: None,
                    parked: Some(manifest.playback),
                },
            );
            if is_presented && let Err(error) = self.resume_presented(&mut system, external_id) {
                self.handle_operation_error(external_id, error);
            }
        }
        self.system.put_ready(system);
    }

    fn observe_event(
        &mut self,
        event: &VideoEvent,
        system: &mut VideoSystem,
        native_id: NativeVideoSessionId,
    ) {
        match event {
            VideoEvent::Ready { id, width, height } => {
                if let Some(video) = self.videos.get_mut(id) {
                    video.width = *width;
                    video.height = *height;
                    video.state = system
                        .state(native_id.protocol())
                        .map_or(VideoState::Paused, VideoState::from);
                }
            }
            VideoEvent::StateChanged { id, state } => {
                if *state == VideoSessionState::Failed {
                    self.close_and_detach_failed_native_session(
                        system,
                        *id,
                        native_id,
                        "native video backend entered failed state".into(),
                    );
                } else if let Some(video) = self.videos.get_mut(id) {
                    video.state = (*state).into();
                }
            }
            VideoEvent::FramePathChanged {
                id,
                previous,
                current,
            } => {
                tracing::info!(
                    video_id = id.get(),
                    ?previous,
                    ?current,
                    "native video frame path changed"
                );
            }
            VideoEvent::Ended { id } => {
                if let Some(video) = self.videos.get_mut(id) {
                    video.state = VideoState::EndOfStream;
                }
            }
            VideoEvent::Failed { id, error } => {
                self.close_and_detach_failed_native_session(system, *id, native_id, error.clone());
            }
        }
    }

    fn close_and_detach_failed_native_session(
        &mut self,
        system: &mut VideoSystem,
        id: VideoId,
        native_id: NativeVideoSessionId,
        error: VideoCommandError,
    ) {
        // `VideoSystem` has already quiesced the native pipeline and retained
        // its failed session for diagnostics. Remove that ephemeral
        // incarnation now: a presented stable video must not try to resume a
        // decoder that terminal cleanup closed.
        if let Some(mut diagnostic) = system
            .diagnostics()
            .sessions
            .into_iter()
            .find(|session| session.id == native_id.protocol())
        {
            diagnostic.id = id;
            diagnostic.state = VideoSessionState::Failed;
            diagnostic.presentation_counts = self.presentation.counts(id);
            diagnostic.presentation_timing = self.presentation.timing(id);
            diagnostic.gpu_timing = self.presentation.gpu_timing(id);
            diagnostic.terminal_error = Some(error.clone());
            self.terminal_diagnostics.insert(id, diagnostic);
        }
        if let Err(close_error) = system.command(VideoCommand::Close {
            id: native_id.protocol(),
        }) {
            tracing::debug!(
                video_id = id.get(),
                %close_error,
                "failed native video session was already detached"
            );
        }
        self.detach_failed_native_session(id, native_id, error);
    }

    fn detach_failed_native_session(
        &mut self,
        id: VideoId,
        native_id: NativeVideoSessionId,
        error: VideoCommandError,
    ) {
        if self.native_to_video.get(&native_id) == Some(&id) {
            self.native_to_video.remove(&native_id);
        }
        if let Some(video) = self.videos.get_mut(&id)
            && video.native_id == Some(native_id)
        {
            video.native_id = None;
            video.parked = None;
        }
        self.handle_operation_error(id, error);
    }

    fn handle_operation_error(&mut self, id: VideoId, error: VideoCommandError) {
        let is_attached = self
            .videos
            .get(&id)
            .is_some_and(|video| video.native_id.is_some());
        if is_attached {
            tracing::error!(
                video_id = id.get(),
                %error,
                "video command failed; native session remains active"
            );
            return;
        }

        self.record_terminal_failure(id, error);
    }

    fn record_terminal_failure(&mut self, id: VideoId, error: VideoCommandError) {
        debug_assert!(
            self.videos
                .get(&id)
                .is_none_or(|video| video.native_id.is_none()),
            "terminal video failure must detach its native incarnation first"
        );
        let presentation_counts = self.presentation.counts(id);
        let presentation_timing = self.presentation.timing(id);
        let gpu_timing = self.presentation.gpu_timing(id);
        self.terminal_diagnostics
            .entry(id)
            .and_modify(|diagnostic| {
                diagnostic.state = VideoSessionState::Failed;
                diagnostic.terminal_error = Some(error.clone());
            })
            .or_insert_with(|| neomacs_video::VideoSessionDiagnostics {
                id,
                backend: VideoSystem::BACKEND,
                decoder: None,
                state: VideoSessionState::Failed,
                frame_path: None,
                frame_format: None,
                colorimetry: None,
                decoded_frames: 0,
                replaced_frames: 0,
                late_dropped_frames: 0,
                imported_frames: 0,
                backpressured_frames: 0,
                output_reconfigurations: 0,
                import_counts: Default::default(),
                presentation_counts,
                presentation_timing,
                gpu_timing,
                terminal_error: Some(error.clone()),
            });
        let already_diagnosed = match &error {
            VideoCommandError::Backend { message } => self.system.already_diagnosed(message),
            _ => false,
        };
        if !already_diagnosed {
            tracing::error!(video_id = id.get(), %error, "video playback failed");
        }
        if let Some(video) = self.videos.get_mut(&id) {
            video.state = VideoState::Error;
            video.failure = Some(error);
        }
    }

    fn set_state(&mut self, id: VideoId, state: VideoState) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.state = state;
        }
    }
}

#[cfg(test)]
#[path = "video_cache_test.rs"]
mod tests;
