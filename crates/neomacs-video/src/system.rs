use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use neomacs_display_protocol::types::VideoId;

use crate::backend::{
    BackendEvent, CompletedFrameImport, DecodedFrameImport, DecoderBackend,
    DecoderOutputGeneration, DecoderReconfiguration, FrameImportOutcome, FrameImporter, Platform,
    ProductionPlatform,
};
use crate::clock::PlaybackClock;
use crate::mailbox::PresentationFrameQueue;
use crate::platform::CurrentPlatform;
use crate::sampling::{
    DrawRetirementGate, GpuVideoContext, GpuVideoFrame, PreparedVideoDraws, VideoCommandEncoder,
    VideoSamplingResources,
};
use crate::{
    FrameImportPolicy, GpuGeneration, MediaTime, PlaybackAction, PlaybackEpoch, PlaybackRate,
    PresentationVisibility, VideoCommand, VideoCommandError, VideoDiagnostics, VideoEvent,
    VideoFrameFormat, VideoFramePath, VideoFrameReady, VideoImportCounts, VideoInitError,
    VideoPresentationPath, VideoRecoveryManifest, VideoServiceRequest, VideoServiceResult,
    VideoSessionDiagnostics, VideoSessionRecovery, VideoSessionState, VideoSource,
};

/// Cross-thread wake callback invoked after a native adapter publishes new
/// control state or updates its bounded decoded-frame queue.
#[derive(Clone)]
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub struct VideoWake(Arc<dyn Fn() + Send + Sync>);

impl VideoWake {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(wake))
    }

    pub fn noop() -> Self {
        Self::new(|| {})
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(crate) fn notify(&self) {
        (self.0)();
    }
}

struct Session<F, S> {
    source: VideoSource,
    loop_mode: crate::LoopMode,
    desired_playing: bool,
    playback_rate: PlaybackRate,
    presentation: PresentationVisibility,
    last_pts: MediaTime,
    pending_restore: Option<PendingRestore>,
    state: VideoSessionState,
    mailbox: PresentationFrameQueue<crate::backend::DecodedFrame<F>>,
    sampled: Option<S>,
    clock: Option<PlaybackClock>,
    epoch: PlaybackEpoch,
    output_generation: OutputGenerationState,
    diagnostics: SessionCounters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputGenerationState {
    Stable(DecoderOutputGeneration),
    Awaiting {
        previous: DecoderOutputGeneration,
        expected: DecoderOutputGeneration,
    },
}

impl OutputGenerationState {
    const INITIAL: Self = Self::Stable(DecoderOutputGeneration::INITIAL);

    fn observe(&mut self, generation: DecoderOutputGeneration) -> OutputGenerationObservation {
        match *self {
            Self::Stable(current) if generation < current => OutputGenerationObservation::Stale,
            Self::Stable(current) if generation == current => OutputGenerationObservation::Current,
            Self::Stable(current) if generation == current.next() => {
                *self = Self::Stable(generation);
                OutputGenerationObservation::Promoted
            }
            Self::Stable(current) => OutputGenerationObservation::Skipped {
                expected: current.next(),
                observed: generation,
            },
            Self::Awaiting { previous, .. } if generation <= previous => {
                OutputGenerationObservation::Stale
            }
            Self::Awaiting { expected, .. } if generation < expected => {
                OutputGenerationObservation::Stale
            }
            Self::Awaiting { expected, .. } if generation == expected => {
                *self = Self::Stable(generation);
                OutputGenerationObservation::Promoted
            }
            Self::Awaiting { expected, .. } => OutputGenerationObservation::Skipped {
                expected,
                observed: generation,
            },
        }
    }

    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    fn announce(&mut self, generation: DecoderOutputGeneration) -> OutputGenerationObservation {
        self.observe(generation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputGenerationObservation {
    Current,
    Promoted,
    Stale,
    Skipped {
        expected: DecoderOutputGeneration,
        observed: DecoderOutputGeneration,
    },
}

#[derive(Debug, Default)]
struct SessionCounters {
    decoder: Option<crate::VideoDecoderIdentity>,
    frame_path: Option<VideoFramePath>,
    frame_format: Option<VideoFrameFormat>,
    colorimetry: Option<crate::VideoColorimetry>,
    decoded_frames: u64,
    replaced_frames: u64,
    late_dropped_frames: u64,
    imported_frames: u64,
    backpressured_frames: u64,
    output_reconfigurations: u64,
    import_counts: VideoImportCounts,
}

impl SessionCounters {
    fn begin_measurement_epoch(&mut self) {
        let decoder = self.decoder.take();
        let frame_path = self.frame_path;
        let frame_format = self.frame_format;
        let colorimetry = self.colorimetry;
        *self = Self {
            decoder,
            frame_path,
            frame_format,
            colorimetry,
            ..Self::default()
        };
    }
}

impl SessionCounters {
    fn record_compositor_import(&mut self, completed_import: CompletedFrameImport) {
        match completed_import {
            CompletedFrameImport::BorrowedNativeSurface => {
                self.import_counts.borrowed_native_frames =
                    self.import_counts.borrowed_native_frames.saturating_add(1);
            }
            CompletedFrameImport::GpuBlit { reported_bytes } => {
                self.import_counts.gpu_blit_frames =
                    self.import_counts.gpu_blit_frames.saturating_add(1);
                if let Some(bytes) = reported_bytes {
                    self.import_counts.reported_gpu_blit_bytes = self
                        .import_counts
                        .reported_gpu_blit_bytes
                        .saturating_add(bytes);
                }
            }
            CompletedFrameImport::CpuUpload { bytes } => {
                self.import_counts.cpu_upload_frames =
                    self.import_counts.cpu_upload_frames.saturating_add(1);
                self.import_counts.cpu_upload_bytes =
                    self.import_counts.cpu_upload_bytes.saturating_add(bytes);
            }
        }
    }

    fn record_import(
        &mut self,
        completed_import: CompletedFrameImport,
        decoder_import: DecodedFrameImport,
    ) {
        self.imported_frames = self.imported_frames.saturating_add(1);
        if matches!(decoder_import, DecodedFrameImport::Deferred) {
            self.record_compositor_import(completed_import);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingRestore {
    position: MediaTime,
    rate: PlaybackRate,
    play: bool,
}

impl<F, S> Session<F, S> {
    /// Apply the platform-independent half of one playback transition exactly
    /// once after the native adapter accepts it. Keeping intent, clock,
    /// discontinuity, mailbox, loop state, and recovery position together
    /// prevents their action switches from drifting independently.
    fn apply_playback_action(&mut self, action: PlaybackAction, now: Instant) {
        match action {
            PlaybackAction::Play => {
                self.desired_playing = true;
                if let Some(clock) = &mut self.clock {
                    clock.set_running(
                        matches!(self.presentation, PresentationVisibility::Presented),
                        now,
                    );
                }
            }
            PlaybackAction::Pause => {
                self.desired_playing = false;
                if let Some(clock) = &mut self.clock {
                    clock.set_running(false, now);
                }
            }
            PlaybackAction::Stop => {
                self.epoch = self.epoch.next();
                self.mailbox.clear();
                self.desired_playing = false;
                self.last_pts = MediaTime::ZERO;
                if let Some(clock) = &mut self.clock {
                    clock.seek(MediaTime::ZERO, now);
                    clock.set_running(false, now);
                }
            }
            PlaybackAction::Seek(position) => {
                self.epoch = self.epoch.next();
                self.mailbox.clear();
                if let Some(clock) = &mut self.clock {
                    clock.seek(position, now);
                }
            }
            PlaybackAction::SetRate(rate) => {
                self.playback_rate = rate;
                if let Some(clock) = &mut self.clock {
                    clock.set_rate(rate, now);
                }
            }
            PlaybackAction::SetLoop(mode) => self.loop_mode = mode,
        }
    }
}

/// Public, platform-erased video service. Native handles remain confined to
/// the compile-time selected private [`CurrentPlatform`].
pub struct VideoSystem {
    inner: VideoSystemImpl<CurrentPlatform>,
    gpu: GpuVideoContext,
    retirement_gate: DrawRetirementGate<GpuVideoFrame>,
}

impl VideoSystem {
    pub const BACKEND: crate::VideoDecodeBackend = <CurrentPlatform as Platform>::BACKEND;

    /// Construct with compositor-owned sampling resources. Imported video
    /// bind groups are then compatible with the renderer's textured-quad
    /// pipeline without duplicating that pipeline.
    #[allow(clippy::too_many_arguments)]
    pub fn with_sampling_resources(
        device: wgpu::Device,
        queue: wgpu::Queue,
        sampling: VideoSamplingResources,
        generation: GpuGeneration,
        policy: FrameImportPolicy,
        wake: VideoWake,
    ) -> Result<Self, VideoInitError> {
        let gpu = GpuVideoContext::with_sampling_resources(
            device,
            queue,
            sampling,
            generation,
            wake.clone(),
        );
        let (decoder, importer) = CurrentPlatform::create(gpu.clone(), policy, wake)?;
        Ok(Self {
            inner: VideoSystemImpl::new(decoder, importer, policy),
            gpu,
            retirement_gate: DrawRetirementGate::default(),
        })
    }

    pub fn command(&mut self, command: VideoCommand) -> Result<(), VideoCommandError> {
        let result = self.inner.command(command);
        self.retire_replaced_frames();
        result
    }

    pub fn state(&self, id: VideoId) -> Option<VideoSessionState> {
        self.inner.state(id)
    }

    pub fn set_presentation(
        &mut self,
        id: VideoId,
        visibility: PresentationVisibility,
    ) -> Result<(), VideoCommandError> {
        let result = self.inner.set_presentation(id, visibility);
        self.retire_replaced_frames();
        result
    }

    pub fn recovery_sessions(&self) -> Vec<VideoSessionRecovery> {
        self.inner.recovery_sessions_at(Instant::now())
    }

    /// Aggregate bytes retained by compositor-visible textures and native
    /// import pools. The accounting lifetime is owned by the same affine
    /// resources as the textures, rather than inferred from frame metadata.
    pub fn gpu_memory_bytes(&self) -> usize {
        self.gpu.allocated_bytes()
    }

    pub fn diagnostics(&self) -> VideoDiagnostics {
        VideoDiagnostics {
            renderer: None,
            sessions: self.inner.session_diagnostics(),
            surface_pools: self.inner.surface_pool_diagnostics(),
            gpu_memory_bytes: self.gpu.allocated_bytes(),
        }
    }

    /// Begin one coherent diagnostics epoch while preserving playback and
    /// native surface residency.
    pub fn begin_measurement_epoch(&mut self) {
        self.inner.begin_measurement_epoch();
    }

    /// Open a native session from GPU-independent playback intent.
    ///
    /// The caller supplies the native session identity explicitly. This is
    /// used both after renderer device loss and when an editor-stable video
    /// becomes visible after its native decoder was parked.
    pub fn open_from_manifest(
        &mut self,
        id: VideoId,
        manifest: &VideoRecoveryManifest,
    ) -> Result<(), VideoCommandError> {
        self.inner.open_from_manifest(id, manifest)
    }

    /// Create a command encoder bound by type to this `VideoSystem`'s own
    /// `wgpu::Device`, suitable for [`Self::submit_command_encoder`] or
    /// [`Self::discard_command_encoder`].
    pub fn create_command_encoder(
        &self,
        desc: &wgpu::CommandEncoderDescriptor<'_>,
    ) -> VideoCommandEncoder {
        VideoCommandEncoder::new(
            self.retirement_gate.system_id(),
            self.gpu.device().create_command_encoder(desc),
        )
    }

    /// Current sampled frame geometry for metadata inspection without arming
    /// draw-submission retirement gating.
    pub fn frame_geometry(&self, id: VideoId) -> Option<crate::VideoGeometry> {
        self.inner
            .sampled(id)
            .filter(|frame| frame.generation() == self.gpu.generation())
            .map(GpuVideoFrame::geometry)
    }

    /// Snapshot generation-valid frames for recording into a
    /// [`crate::VideoRenderPass`].
    ///
    /// Calling [`crate::PreparedVideoDraw::bind_to_pass`] arms a private
    /// retirement hold inside the owning [`VideoCommandEncoder`] so subsequent
    /// `service()`, `command()`, or `set_presentation()` calls cannot emit a
    /// `FOREIGN` release barrier or return native leases before
    /// [`Self::submit_command_encoder`] submits the encoder.
    pub fn prepare_draws(&self, ids: impl IntoIterator<Item = VideoId>) -> PreparedVideoDraws<'_> {
        let frames: HashMap<_, _> = ids
            .into_iter()
            .filter_map(|id| {
                self.inner
                    .sampled(id)
                    .filter(|frame| frame.generation() == self.gpu.generation())
                    .map(|frame| (id, frame))
            })
            .collect();
        PreparedVideoDraws::new(frames, self.retirement_gate.registry())
    }

    /// Submit a device-bound [`VideoCommandEncoder`] together with any private
    /// draw-retirement hold attached to it. Any staged foreign-surface release
    /// barriers are recorded in a separate raw-only encoder submitted right
    /// after the draw buffer in the same `queue.submit` batch, and retired
    /// frames stay alive until that submission completes on the GPU.
    pub fn submit_command_encoder(
        &mut self,
        encoder: VideoCommandEncoder,
    ) -> wgpu::SubmissionIndex {
        assert_eq!(
            encoder.system_id(),
            self.retirement_gate.system_id(),
            "VideoCommandEncoder belongs to a different VideoSystem"
        );
        let (buffer, token) = encoder.into_buffer_and_token();
        // Invariant: `retired` must pass directly into `submit_buffer_with_retirements`
        // in this same call; nothing may interleave between token release and
        // queue submission, and `SubmittedWorkRetirement` holds the frames until
        // GPU completion.
        let retired = self.retirement_gate.finish_encoder_token(token);
        self.gpu.submit_buffer_with_retirements(buffer, retired)
    }

    /// Abandon an unsubmitted [`VideoCommandEncoder`], destroying its private
    /// `wgpu::CommandEncoder` before releasing its retirement hold and
    /// immediately flushing any staged retirements if no other video-bearing
    /// encoders remain live.
    pub fn discard_command_encoder(&mut self, encoder: VideoCommandEncoder) {
        assert_eq!(
            encoder.system_id(),
            self.retirement_gate.system_id(),
            "VideoCommandEncoder belongs to a different VideoSystem"
        );
        let token = encoder.discard_and_take_token();
        let retired = self.retirement_gate.finish_encoder_token(token);
        if !retired.is_empty() {
            self.gpu.retire_after_submitted_work(retired);
        }
    }

    pub fn service(&mut self, now: Instant) -> VideoServiceResult {
        self.service_with_request(VideoServiceRequest::new(now))
    }

    pub fn service_with_request(&mut self, request: VideoServiceRequest) -> VideoServiceResult {
        let result = self.inner.service_with_request(request);
        self.retire_replaced_frames();
        result
    }

    fn retire_replaced_frames(&mut self) {
        let retired = self
            .retirement_gate
            .stage_or_retire(self.inner.take_retired());
        if !retired.is_empty() {
            self.gpu.retire_after_submitted_work(retired);
        }
    }

    #[cfg(test)]
    pub(crate) fn test_gpu(&self) -> &GpuVideoContext {
        &self.gpu
    }

    #[cfg(test)]
    pub(crate) fn test_make_prepared_draw<'a>(
        &'a self,
        frame: &'a GpuVideoFrame,
    ) -> crate::PreparedVideoDraw<'a> {
        crate::PreparedVideoDraw::new_for_test(frame, self.retirement_gate.registry())
    }

    #[cfg(test)]
    pub(crate) fn test_stage_retired_frame(&mut self, frame: GpuVideoFrame) {
        let retired = self.retirement_gate.stage_or_retire(vec![frame]);
        if !retired.is_empty() {
            self.gpu.retire_after_submitted_work(retired);
        }
    }

    #[cfg(test)]
    pub(crate) fn test_pending_retirements(&self) -> usize {
        self.retirement_gate.pending_len()
    }
}

impl Drop for VideoSystem {
    fn drop(&mut self) {
        let mut retired = self.retirement_gate.take_all_on_drop();
        retired.extend(self.inner.take_all_sampled_for_retirement());
        if !retired.is_empty() {
            // Teardown is also an ownership transition. In particular, a
            // Linux imported image must return to FOREIGN ownership after
            // the last queued compositor read, even when device recovery
            // destroys the entire renderer rather than replacing a frame.
            self.gpu.retire_after_submitted_work(retired);
        }
    }
}

pub(crate) struct VideoSystemImpl<P: Platform> {
    decoder: P::Decoder,
    importer: P::Importer,
    policy: FrameImportPolicy,
    sessions: HashMap<VideoId, Session<P::Frame, P::Sampled>>,
    retired: Vec<P::Sampled>,
}

impl<P: Platform> VideoSystemImpl<P> {
    pub(crate) fn begin_measurement_epoch(&mut self) {
        for session in self.sessions.values_mut() {
            // A decoder frame may already have crossed the platform bridge
            // into the common presentation mailbox. It belongs to warmup and
            // must not become the first measured import.
            session.mailbox.clear();
            session.diagnostics.begin_measurement_epoch();
        }
        self.decoder.begin_measurement_epoch();
        self.importer.begin_measurement_epoch();
    }

    pub(crate) fn new(
        decoder: P::Decoder,
        importer: P::Importer,
        policy: FrameImportPolicy,
    ) -> Self {
        Self {
            decoder,
            importer,
            policy,
            sessions: HashMap::new(),
            retired: Vec::new(),
        }
    }

    pub(crate) fn command(&mut self, command: VideoCommand) -> Result<(), VideoCommandError> {
        self.command_at(command, Instant::now())
    }

    pub(crate) fn command_at(
        &mut self,
        command: VideoCommand,
        now: Instant,
    ) -> Result<(), VideoCommandError> {
        match command {
            VideoCommand::Open {
                id,
                source,
                initial_playback,
                loop_mode,
            } => {
                if self.sessions.contains_key(&id) {
                    return Err(VideoCommandError::SessionAlreadyOpen { id: id.get() });
                }
                self.sessions.insert(
                    id,
                    Session {
                        source: source.clone(),
                        loop_mode,
                        desired_playing: matches!(
                            initial_playback,
                            crate::InitialPlayback::Playing
                        ),
                        playback_rate: PlaybackRate::NORMAL,
                        presentation: PresentationVisibility::Presented,
                        last_pts: MediaTime::ZERO,
                        pending_restore: None,
                        state: VideoSessionState::Opening,
                        mailbox: PresentationFrameQueue::default(),
                        sampled: None,
                        clock: None,
                        epoch: PlaybackEpoch::INITIAL,
                        output_generation: OutputGenerationState::INITIAL,
                        diagnostics: SessionCounters::default(),
                    },
                );
                if let Err(error) = self.decoder.command(VideoCommand::Open {
                    id,
                    source,
                    initial_playback,
                    loop_mode,
                }) {
                    self.sessions.remove(&id);
                    return Err(error);
                }
            }
            VideoCommand::Playback { id, action } => {
                match self.sessions.get(&id).map(|session| session.state) {
                    None => {
                        return Err(VideoCommandError::SessionNotOpen { id: id.get() });
                    }
                    Some(VideoSessionState::Failed) => {
                        return Err(VideoCommandError::SessionFailed { id: id.get() });
                    }
                    Some(_) => {}
                }
                self.decoder.command(VideoCommand::Playback {
                    id,
                    action: action.clone(),
                })?;
                let session = self
                    .sessions
                    .get_mut(&id)
                    .ok_or_else(|| format!("video {} is not open", id.get()))?;
                session.apply_playback_action(action, now);
            }
            VideoCommand::Presentation { id, visibility } => {
                self.set_presentation_at(id, visibility, now)?;
            }
            VideoCommand::Close { id } => {
                let state = self
                    .sessions
                    .get(&id)
                    .ok_or(VideoCommandError::SessionNotOpen { id: id.get() })?
                    .state;
                // A failed session was already closed by
                // `quiesce_failed_session`. Keep it in the common registry so
                // diagnostics remain inspectable until the owner explicitly
                // removes it, but do not issue a second native close.
                let decoder_result = if state == VideoSessionState::Failed {
                    Ok(())
                } else {
                    self.decoder.command(VideoCommand::Close { id })
                };
                if let Some(mut session) = self.sessions.remove(&id)
                    && let Some(sampled) = session.sampled.take()
                {
                    self.retired.push(sampled);
                }
                decoder_result?;
            }
        }
        Ok(())
    }

    pub(crate) fn state(&self, id: VideoId) -> Option<VideoSessionState> {
        self.sessions.get(&id).map(|session| session.state)
    }

    pub(crate) fn set_presentation(
        &mut self,
        id: VideoId,
        visibility: PresentationVisibility,
    ) -> Result<(), VideoCommandError> {
        self.set_presentation_at(id, visibility, Instant::now())
    }

    fn set_presentation_at(
        &mut self,
        id: VideoId,
        visibility: PresentationVisibility,
        now: Instant,
    ) -> Result<(), VideoCommandError> {
        let session = self
            .sessions
            .get_mut(&id)
            .ok_or(VideoCommandError::SessionNotOpen { id: id.get() })?;
        if session.state == VideoSessionState::Failed {
            return Err(VideoCommandError::SessionFailed { id: id.get() });
        }
        if session.presentation == visibility {
            return Ok(());
        }
        self.decoder
            .command(VideoCommand::Presentation { id, visibility })?;
        session.presentation = visibility;
        if visibility == PresentationVisibility::Hidden {
            // A hidden session must not pin either an imported pool slot or a
            // decoded native lease waiting in the mailbox. Exposure resumes
            // decode and supplies a fresh frame.
            session.mailbox.clear();
            if let Some(sampled) = session.sampled.take() {
                self.retired.push(sampled);
            }
        }
        if let Some(clock) = &mut session.clock {
            clock.set_running(
                matches!(visibility, PresentationVisibility::Presented) && session.desired_playing,
                now,
            );
        }
        Ok(())
    }

    pub(crate) fn recovery_sessions_at(&self, now: Instant) -> Vec<VideoSessionRecovery> {
        self.sessions
            .iter()
            .filter(|(_, session)| session.state != VideoSessionState::Failed)
            .map(|(&id, session)| {
                VideoSessionRecovery::new(
                    id,
                    VideoRecoveryManifest::new(
                        session.source.clone(),
                        session.loop_mode,
                        session.desired_playing,
                        session.playback_rate,
                        session
                            .clock
                            .as_ref()
                            .map_or(session.last_pts, |clock| clock.media_time(now)),
                        session.presentation,
                    ),
                )
            })
            .collect()
    }

    pub(crate) fn open_from_manifest(
        &mut self,
        id: VideoId,
        manifest: &VideoRecoveryManifest,
    ) -> Result<(), VideoCommandError> {
        if manifest.presentation() == PresentationVisibility::Hidden {
            return Ok(());
        }
        self.command(VideoCommand::Open {
            id,
            source: manifest.source().clone(),
            initial_playback: crate::InitialPlayback::Paused,
            loop_mode: manifest.loop_mode(),
        })?;
        let session = self
            .sessions
            .get_mut(&id)
            .expect("successful open installs its video session");
        session.desired_playing = manifest.desired_playing();
        session.pending_restore = Some(PendingRestore {
            position: manifest.position(),
            rate: manifest.rate(),
            play: manifest.desired_playing(),
        });
        Ok(())
    }

    pub(crate) fn sampled(&self, id: VideoId) -> Option<&P::Sampled> {
        self.sessions
            .get(&id)
            .and_then(|session| session.sampled.as_ref())
    }

    #[cfg(test)]
    pub(crate) fn service(&mut self, now: Instant) -> VideoServiceResult {
        self.service_with_request(VideoServiceRequest::new(now))
    }

    pub(crate) fn service_with_request(
        &mut self,
        request: VideoServiceRequest,
    ) -> VideoServiceResult {
        let now = request.service_time();
        let mut result = VideoServiceResult::default();
        let mut restorations = Vec::new();
        let mut terminal_failures = Vec::new();
        for event in self.decoder.service(&request) {
            if let BackendEvent::Opened { id, .. } = &event
                && let Some(restore) = self
                    .sessions
                    .get_mut(id)
                    .and_then(|session| session.pending_restore.take())
            {
                restorations.push((*id, restore));
            }
            if let Some(event) = self.observe_backend_event(event, now) {
                if matches!(
                    event,
                    VideoEvent::Failed { .. }
                        | VideoEvent::StateChanged {
                            state: VideoSessionState::Failed,
                            ..
                        }
                ) {
                    terminal_failures.push(match &event {
                        VideoEvent::Failed { id, .. } | VideoEvent::StateChanged { id, .. } => *id,
                        _ => unreachable!("matched terminal video event"),
                    });
                }
                result.events.push(event);
            }
        }

        for (id, restore) in restorations {
            let restored = if restore.position != MediaTime::ZERO {
                self.command_at(
                    VideoCommand::Playback {
                        id,
                        action: PlaybackAction::Seek(restore.position),
                    },
                    now,
                )
            } else {
                Ok(())
            }
            .and_then(|()| {
                if restore.rate != PlaybackRate::NORMAL {
                    self.command_at(
                        VideoCommand::Playback {
                            id,
                            action: PlaybackAction::SetRate(restore.rate),
                        },
                        now,
                    )
                } else {
                    Ok(())
                }
            })
            .and_then(|()| {
                if restore.play {
                    self.command_at(
                        VideoCommand::Playback {
                            id,
                            action: PlaybackAction::Play,
                        },
                        now,
                    )
                } else {
                    Ok(())
                }
            });
            if let Err(message) = restored {
                terminal_failures.push(id);
                result
                    .events
                    .push(VideoEvent::Failed { id, error: message });
            }
        }

        terminal_failures.sort_unstable();
        terminal_failures.dedup();
        for id in terminal_failures.drain(..) {
            self.quiesce_failed_session(id, now);
        }

        for (id, session) in &mut self.sessions {
            if session.state == VideoSessionState::Failed {
                continue;
            }
            while let Some(timing) = session.mailbox.timing() {
                let Some(clock) = session.clock else {
                    break;
                };
                let media_now = clock.media_time(now);
                if timing.pts > media_now {
                    if let Some(deadline) = clock.deadline_for(timing.pts, now) {
                        result.next_deadline = Some(
                            result
                                .next_deadline
                                .map_or(deadline, |current| current.min(deadline)),
                        );
                    }
                    break;
                }
                if session
                    .mailbox
                    .successor_timing()
                    .is_some_and(|successor| successor.pts <= media_now)
                {
                    // Both bounded slots are already due. Skip the older one
                    // and present the newest due successor, preserving low
                    // latency without allowing future frames to move the
                    // head's deadline indefinitely.
                    let _ = session.mailbox.take();
                    session.diagnostics.replaced_frames =
                        session.diagnostics.replaced_frames.saturating_add(1);
                    continue;
                }
                let frame = session
                    .mailbox
                    .take()
                    .expect("mailbox timing came from a pending frame");
                let timing = frame.timing;
                let decoder_import = frame.decoder_import;
                let decode_residency = frame.decode_residency;
                if timing.duration != MediaTime::ZERO
                    && timing.pts.saturating_add(timing.duration) <= media_now
                {
                    session.diagnostics.late_dropped_frames =
                        session.diagnostics.late_dropped_frames.saturating_add(1);
                    // The stable head may be stale after a long compositor
                    // pause. Inspect its coalesced successor in this same
                    // service pass so a due frame cannot wait without a wake.
                    continue;
                }
                let planned_path = decoder_import
                    .path()
                    .unwrap_or_else(|| self.importer.compositor_import(&frame));
                if !self.policy.permits(planned_path) {
                    terminal_failures.push(*id);
                    result.events.push(VideoEvent::Failed {
                        id: *id,
                        error: VideoCommandError::ImportForbidden {
                            policy: self.policy,
                            path: planned_path,
                        },
                    });
                    break;
                }
                match self.importer.import(frame) {
                    Ok(FrameImportOutcome::Ready(imported))
                        if imported.completed_import.path() == planned_path =>
                    {
                        let actual_path = imported.completed_import.path();
                        let frame_path = VideoFramePath::new(
                            decode_residency,
                            actual_path,
                            VideoPresentationPath::WgpuComposited,
                        );
                        if let Some(previous) = session.sampled.replace(imported.sampled) {
                            self.retired.push(previous);
                        }
                        result.ready_frames.push(VideoFrameReady {
                            id: *id,
                            pts: timing.pts,
                            frame_path,
                        });
                        let previous = session.diagnostics.frame_path;
                        if previous != Some(frame_path) {
                            result.events.push(VideoEvent::FramePathChanged {
                                id: *id,
                                previous,
                                current: frame_path,
                            });
                        }
                        session.diagnostics.frame_path = Some(frame_path);
                        session
                            .diagnostics
                            .record_import(imported.completed_import, decoder_import);
                        if let CompletedFrameImport::CpuUpload { bytes } = imported.completed_import
                        {
                            tracing::warn!(
                                video_id = id.get(),
                                bytes,
                                "native video frame crossed the CPU upload compatibility path"
                            );
                        }
                        session.last_pts = timing.pts;
                    }
                    Ok(FrameImportOutcome::Ready(imported)) => {
                        terminal_failures.push(*id);
                        result.events.push(VideoEvent::Failed {
                            id: *id,
                            error: VideoCommandError::ImportContract {
                                planned: planned_path,
                                actual: imported.completed_import.path(),
                            },
                        });
                    }
                    Ok(FrameImportOutcome::ReconfigureDecoder { rejection }) => {
                        match self
                            .decoder
                            .reconfigure_after_import_failure(*id, &rejection)
                        {
                            Ok(DecoderReconfiguration::Applied { generation }) => {
                                if generation != rejection.generation.next() {
                                    terminal_failures.push(*id);
                                    result.events.push(VideoEvent::Failed {
                                        id: *id,
                                        error: VideoCommandError::Import {
                                            message: format!(
                                                "decoder returned non-successor output generation {generation:?} after rejecting {:?}",
                                                rejection.generation
                                            ),
                                        },
                                    });
                                    break;
                                }
                                session.mailbox.clear();
                                session.output_generation = OutputGenerationState::Awaiting {
                                    previous: rejection.generation,
                                    expected: generation,
                                };
                                tracing::warn!(
                                    video_id = id.get(),
                                    rejected = ?rejection.format,
                                    reason = %rejection.reason,
                                    "native video output reconfiguration requested after GPU import rejection"
                                );
                            }
                            Ok(DecoderReconfiguration::Superseded) => {}
                            Ok(DecoderReconfiguration::Unsupported) => {
                                terminal_failures.push(*id);
                                result.events.push(VideoEvent::Failed {
                                    id: *id,
                                    error: VideoCommandError::Import {
                                        message: rejection.reason,
                                    },
                                });
                            }
                            Err(reconfiguration) => {
                                terminal_failures.push(*id);
                                result.events.push(VideoEvent::Failed {
                                    id: *id,
                                    error: VideoCommandError::Import {
                                        message: format!(
                                            "{}; decoder fallback failed: {reconfiguration}",
                                            rejection.reason
                                        ),
                                    },
                                });
                            }
                        }
                    }
                    Err(message) => {
                        terminal_failures.push(*id);
                        result.events.push(VideoEvent::Failed {
                            id: *id,
                            error: VideoCommandError::Import { message },
                        });
                    }
                    Ok(FrameImportOutcome::Backpressured) => {
                        session.diagnostics.backpressured_frames =
                            session.diagnostics.backpressured_frames.saturating_add(1);
                    }
                }
                break;
            }
        }

        for id in terminal_failures {
            self.quiesce_failed_session(id, now);
        }

        // Query the native deadline only after terminal sessions have been
        // closed, so a failed decoder cannot keep the compositor awake.
        if let Some(deadline) = self.decoder.next_service_deadline(now) {
            result.next_deadline = Some(
                result
                    .next_deadline
                    .map_or(deadline, |current| current.min(deadline)),
            );
        }
        result
    }

    fn quiesce_failed_session(&mut self, id: VideoId, now: Instant) {
        if !self.sessions.contains_key(&id) {
            return;
        }

        if let Err(error) = self.decoder.command(VideoCommand::Close { id }) {
            tracing::debug!(
                video_id = id.get(),
                %error,
                "native video session was already unavailable during terminal cleanup"
            );
        }

        let session = self
            .sessions
            .get_mut(&id)
            .expect("failed video session remains registered for diagnostics");
        session.state = VideoSessionState::Failed;
        session.desired_playing = false;
        session.presentation = PresentationVisibility::Hidden;
        session.pending_restore = None;
        session.mailbox.clear();
        if let Some(sampled) = session.sampled.take() {
            self.retired.push(sampled);
        }
        if let Some(clock) = &mut session.clock {
            clock.set_running(false, now);
        }
    }

    pub(crate) fn take_retired(&mut self) -> Vec<P::Sampled> {
        std::mem::take(&mut self.retired)
    }

    pub(crate) fn session_diagnostics(&self) -> Vec<VideoSessionDiagnostics> {
        let mut sessions: Vec<_> = self
            .sessions
            .iter()
            .map(|(&id, session)| VideoSessionDiagnostics {
                id,
                backend: P::BACKEND,
                decoder: session.diagnostics.decoder.clone(),
                state: session.state,
                frame_path: session.diagnostics.frame_path,
                frame_format: session.diagnostics.frame_format,
                colorimetry: session.diagnostics.colorimetry,
                decoded_frames: session.diagnostics.decoded_frames,
                replaced_frames: session.diagnostics.replaced_frames,
                late_dropped_frames: session.diagnostics.late_dropped_frames,
                imported_frames: session.diagnostics.imported_frames,
                backpressured_frames: session.diagnostics.backpressured_frames,
                output_reconfigurations: session.diagnostics.output_reconfigurations,
                import_counts: session.diagnostics.import_counts,
                presentation_counts: crate::VideoPresentationCounts::default(),
                presentation_timing: crate::VideoPresentationTiming::default(),
                gpu_timing: crate::VideoGpuTiming::default(),
                terminal_error: None,
            })
            .collect();
        sessions.sort_unstable_by_key(|session| session.id.get());
        sessions
    }

    fn surface_pool_diagnostics(&self) -> Vec<crate::VideoSurfacePoolDiagnostics> {
        self.decoder
            .surface_pool_diagnostics()
            .into_iter()
            .chain(self.importer.surface_pool_diagnostics())
            .collect()
    }

    pub(crate) fn take_all_sampled_for_retirement(&mut self) -> Vec<P::Sampled> {
        let mut sampled = self.take_retired();
        sampled.extend(
            self.sessions
                .values_mut()
                .filter_map(|session| session.sampled.take()),
        );
        sampled
    }

    fn observe_backend_event(
        &mut self,
        event: BackendEvent<P::Frame>,
        now: Instant,
    ) -> Option<VideoEvent> {
        if self
            .sessions
            .get(&event.id())
            .is_some_and(|session| session.state == VideoSessionState::Failed)
        {
            // Closing a native pipeline is asynchronous on some platforms.
            // A failed logical session is terminal, so late frames/state from
            // that incarnation cannot revive it.
            return None;
        }
        match event {
            BackendEvent::Opened {
                id,
                width,
                height,
                initial_state,
            } => {
                let session = self.sessions.get_mut(&id)?;
                session.state = initial_state;
                let clock_state = if initial_state == VideoSessionState::Playing
                    && session.desired_playing
                    && session.presentation == PresentationVisibility::Presented
                {
                    VideoSessionState::Playing
                } else {
                    VideoSessionState::Paused
                };
                session.clock = Some(PlaybackClock::new(now, clock_state));
                Some(VideoEvent::Ready { id, width, height })
            }
            BackendEvent::DecoderSelected { id, decoder } => {
                self.sessions.get_mut(&id)?.diagnostics.decoder = Some(decoder);
                None
            }
            BackendEvent::Frame { id, frame } => {
                let session = self.sessions.get_mut(&id)?;
                let promoted_output = match session
                    .output_generation
                    .observe(frame.output_generation)
                {
                    OutputGenerationObservation::Stale => return None,
                    OutputGenerationObservation::Skipped { expected, observed } => {
                        return Some(VideoEvent::Failed {
                            id,
                            error: VideoCommandError::Backend {
                                message: format!(
                                    "decoder skipped output generation {expected:?} and published {observed:?}"
                                ),
                            },
                        });
                    }
                    OutputGenerationObservation::Current => false,
                    OutputGenerationObservation::Promoted => true,
                };
                if promoted_output {
                    session.mailbox.clear();
                    session.diagnostics.output_reconfigurations = session
                        .diagnostics
                        .output_reconfigurations
                        .saturating_add(1);
                }
                session.diagnostics.decoded_frames =
                    session.diagnostics.decoded_frames.saturating_add(1);
                session.diagnostics.frame_format = Some(frame.format);
                session.diagnostics.colorimetry = Some(frame.colorimetry);
                if let Some(completed_import) = frame.decoder_import.completed() {
                    session
                        .diagnostics
                        .record_compositor_import(completed_import);
                }
                if session.presentation == PresentationVisibility::Hidden {
                    // Native pause is asynchronous; discard a racing sample
                    // rather than reacquiring one of the bounded GPU slots.
                    return None;
                }
                if frame.timing.epoch < session.epoch {
                    return None;
                }
                if frame.timing.epoch > session.epoch {
                    session.epoch = frame.timing.epoch;
                    session.mailbox.clear();
                    if let Some(clock) = &mut session.clock {
                        clock.seek(frame.timing.pts, now);
                    }
                }
                if session.mailbox.publish(frame).is_some() {
                    session.diagnostics.replaced_frames =
                        session.diagnostics.replaced_frames.saturating_add(1);
                }
                None
            }
            #[cfg(any(target_os = "linux", test))]
            BackendEvent::FramesReplaced { id, count } => {
                let session = self.sessions.get_mut(&id)?;
                session.diagnostics.decoded_frames =
                    session.diagnostics.decoded_frames.saturating_add(count);
                session.diagnostics.replaced_frames =
                    session.diagnostics.replaced_frames.saturating_add(count);
                None
            }
            #[cfg(any(target_os = "linux", test))]
            BackendEvent::OutputReconfigured { id, generation } => {
                let session = self.sessions.get_mut(&id)?;
                match session.output_generation.announce(generation) {
                    OutputGenerationObservation::Stale | OutputGenerationObservation::Current => {
                        None
                    }
                    OutputGenerationObservation::Promoted => {
                        session.mailbox.clear();
                        session.diagnostics.output_reconfigurations = session
                            .diagnostics
                            .output_reconfigurations
                            .saturating_add(1);
                        None
                    }
                    OutputGenerationObservation::Skipped { expected, observed } => {
                        Some(VideoEvent::Failed {
                            id,
                            error: VideoCommandError::Backend {
                                message: format!(
                                    "decoder skipped output generation {expected:?} and announced {observed:?}"
                                ),
                            },
                        })
                    }
                }
            }
            BackendEvent::StateChanged { id, state } => {
                let session = self.sessions.get_mut(&id)?;
                session.state = state;
                let should_run = state == VideoSessionState::Playing
                    && session.desired_playing
                    && session.presentation == PresentationVisibility::Presented;
                if let Some(clock) = &mut session.clock {
                    match state {
                        VideoSessionState::Playing => clock.set_running(should_run, now),
                        VideoSessionState::Paused
                        | VideoSessionState::Ended
                        | VideoSessionState::Failed
                        | VideoSessionState::Closed => clock.set_running(false, now),
                        VideoSessionState::Opening => {}
                    }
                }
                Some(VideoEvent::StateChanged { id, state })
            }
            BackendEvent::Looped { id, remaining } => {
                let session = self.sessions.get_mut(&id)?;
                session.loop_mode = remaining;
                session.epoch = session.epoch.next();
                session.mailbox.clear();
                if let Some(clock) = &mut session.clock {
                    clock.seek(MediaTime::ZERO, now);
                }
                None
            }
            BackendEvent::Ended { id } => {
                let session = self.sessions.get_mut(&id)?;
                session.state = VideoSessionState::Ended;
                session.desired_playing = false;
                if let Some(clock) = &mut session.clock {
                    clock.set_running(false, now);
                }
                Some(VideoEvent::Ended { id })
            }
            BackendEvent::Failed { id, error } => {
                let session = self.sessions.get_mut(&id)?;
                session.state = VideoSessionState::Failed;
                session.desired_playing = false;
                if let Some(clock) = &mut session.clock {
                    clock.set_running(false, now);
                }
                Some(VideoEvent::Failed { id, error })
            }
        }
    }
}
