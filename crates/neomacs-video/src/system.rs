use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use neomacs_display_protocol::types::VideoId;

use crate::backend::{
    BackendEvent, CompletedFrameTransfer, DecodedFrameTransfer, DecoderBackend,
    DecoderReconfiguration, FrameImportOutcome, FrameImporter, Platform, ProductionPlatform,
};
use crate::clock::PlaybackClock;
use crate::mailbox::{LatestFrameMailbox, PendingFrame};
use crate::platform::CurrentPlatform;
use crate::sampling::{GpuVideoContext, PreparedVideoDraws, VideoSamplingResources};
use crate::{
    FrameTransferPolicy, GpuGeneration, MediaTime, PlaybackAction, PlaybackEpoch, PlaybackRate,
    PresentationVisibility, VideoCommand, VideoCommandError, VideoDiagnostics, VideoEvent,
    VideoFrameFormat, VideoFrameReady, VideoInitError, VideoRecoveryManifest, VideoServiceResult,
    VideoSessionDiagnostics, VideoSessionRecovery, VideoSessionState, VideoSource,
    VideoTransferCounts,
};

/// Cross-thread wake callback invoked after a native adapter publishes new
/// control state or replaces its latest decoded frame.
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
    mailbox: LatestFrameMailbox<F>,
    sampled: Option<S>,
    clock: Option<PlaybackClock>,
    epoch: PlaybackEpoch,
    diagnostics: SessionCounters,
}

#[derive(Debug, Default)]
struct SessionCounters {
    transfer_path: Option<crate::VideoTransferPath>,
    frame_format: Option<VideoFrameFormat>,
    colorimetry: Option<crate::VideoColorimetry>,
    decoded_frames: u64,
    replaced_frames: u64,
    late_dropped_frames: u64,
    imported_frames: u64,
    backpressured_frames: u64,
    transfer_counts: VideoTransferCounts,
}

impl SessionCounters {
    fn record_transfer(&mut self, transfer: CompletedFrameTransfer) {
        match transfer {
            CompletedFrameTransfer::DirectExternalSurface => {
                self.transfer_counts.direct_external_frames = self
                    .transfer_counts
                    .direct_external_frames
                    .saturating_add(1);
            }
            CompletedFrameTransfer::GpuInteropCopy { reported_bytes } => {
                self.transfer_counts.gpu_interop_copy_frames = self
                    .transfer_counts
                    .gpu_interop_copy_frames
                    .saturating_add(1);
                if let Some(bytes) = reported_bytes {
                    self.transfer_counts.reported_gpu_copy_bytes = self
                        .transfer_counts
                        .reported_gpu_copy_bytes
                        .saturating_add(bytes);
                }
            }
            CompletedFrameTransfer::CpuUpload { bytes } => {
                self.transfer_counts.cpu_upload_frames =
                    self.transfer_counts.cpu_upload_frames.saturating_add(1);
                self.transfer_counts.cpu_upload_bytes =
                    self.transfer_counts.cpu_upload_bytes.saturating_add(bytes);
            }
        }
    }

    fn record_import(
        &mut self,
        transfer: CompletedFrameTransfer,
        decoder_transfer: DecodedFrameTransfer,
    ) {
        self.imported_frames = self.imported_frames.saturating_add(1);
        if matches!(decoder_transfer, DecodedFrameTransfer::Deferred) {
            self.record_transfer(transfer);
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
                let _ = self.mailbox.take();
                self.desired_playing = false;
                self.last_pts = MediaTime::ZERO;
                if let Some(clock) = &mut self.clock {
                    clock.seek(MediaTime::ZERO, now);
                    clock.set_running(false, now);
                }
            }
            PlaybackAction::Seek(position) => {
                self.epoch = self.epoch.next();
                let _ = self.mailbox.take();
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
}

impl VideoSystem {
    /// Construct with compositor-owned sampling resources. Imported video
    /// bind groups are then compatible with the renderer's textured-quad
    /// pipeline without duplicating that pipeline.
    #[allow(clippy::too_many_arguments)]
    pub fn with_sampling_resources(
        device: wgpu::Device,
        queue: wgpu::Queue,
        sampling: VideoSamplingResources,
        generation: GpuGeneration,
        policy: FrameTransferPolicy,
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
            sessions: self.inner.session_diagnostics(),
            gpu_memory_bytes: self.gpu.allocated_bytes(),
        }
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

    pub fn prepare_draws(&self, ids: impl IntoIterator<Item = VideoId>) -> PreparedVideoDraws<'_> {
        let frames = ids
            .into_iter()
            .filter_map(|id| {
                self.inner
                    .sampled(id)
                    .filter(|frame| frame.generation() == self.gpu.generation())
                    .map(|frame| (id, frame))
            })
            .collect();
        PreparedVideoDraws::new(frames)
    }

    pub fn service(&mut self, now: Instant) -> VideoServiceResult {
        let result = self.inner.service(now);
        self.retire_replaced_frames();
        result
    }

    fn retire_replaced_frames(&mut self) {
        let retired = self.inner.take_retired();
        if !retired.is_empty() {
            self.gpu.retire_after_submitted_work(retired);
        }
    }
}

impl Drop for VideoSystem {
    fn drop(&mut self) {
        let retired = self.inner.take_all_sampled_for_retirement();
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
    policy: FrameTransferPolicy,
    sessions: HashMap<VideoId, Session<P::Frame, P::Sampled>>,
    retired: Vec<P::Sampled>,
}

impl<P: Platform> VideoSystemImpl<P> {
    pub(crate) fn new(
        decoder: P::Decoder,
        importer: P::Importer,
        policy: FrameTransferPolicy,
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
                        mailbox: LatestFrameMailbox::default(),
                        sampled: None,
                        clock: None,
                        epoch: PlaybackEpoch::INITIAL,
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
            let _ = session.mailbox.take();
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

    pub(crate) fn service(&mut self, now: Instant) -> VideoServiceResult {
        let mut result = VideoServiceResult::default();
        let mut restorations = Vec::new();
        let mut terminal_failures = Vec::new();
        for event in self.decoder.drain_events() {
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
            let Some(timing) = session.mailbox.timing() else {
                continue;
            };
            let Some(clock) = session.clock else {
                continue;
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
                continue;
            }
            let pending = session
                .mailbox
                .take()
                .expect("mailbox timing came from a pending frame");
            let timing = pending.frame.timing;
            let decoder_transfer = pending.frame.decoder_transfer;
            if timing.duration != MediaTime::ZERO
                && timing.pts.saturating_add(timing.duration) <= media_now
            {
                session.diagnostics.late_dropped_frames =
                    session.diagnostics.late_dropped_frames.saturating_add(1);
                continue;
            }
            let planned_path = decoder_transfer
                .path()
                .unwrap_or_else(|| self.importer.transfer_path(&pending.frame));
            session.diagnostics.transfer_path = Some(planned_path);
            if !self.policy.permits(planned_path) {
                terminal_failures.push(*id);
                result.events.push(VideoEvent::Failed {
                    id: *id,
                    error: VideoCommandError::TransferForbidden {
                        policy: self.policy,
                        path: planned_path,
                    },
                });
                continue;
            }
            match self.importer.import(pending.frame) {
                Ok(FrameImportOutcome::Ready(imported))
                    if imported.transfer.path() == planned_path =>
                {
                    let actual_path = imported.transfer.path();
                    if let Some(previous) = session.sampled.replace(imported.sampled) {
                        self.retired.push(previous);
                    }
                    result.ready_frames.push(VideoFrameReady {
                        id: *id,
                        pts: timing.pts,
                        transfer_path: actual_path,
                    });
                    session
                        .diagnostics
                        .record_import(imported.transfer, decoder_transfer);
                    session.last_pts = timing.pts;
                }
                Ok(FrameImportOutcome::Ready(imported)) => {
                    terminal_failures.push(*id);
                    result.events.push(VideoEvent::Failed {
                        id: *id,
                        error: VideoCommandError::TransferContract {
                            planned: planned_path,
                            actual: imported.transfer.path(),
                        },
                    });
                }
                Ok(FrameImportOutcome::ReconfigureDecoder { rejected, reason }) => {
                    match self.decoder.reconfigure_after_import_failure(*id, rejected) {
                        Ok(DecoderReconfiguration::Applied) => {
                            tracing::warn!(
                                video_id = id.get(),
                                ?rejected,
                                %reason,
                                "native video output was reconfigured after GPU import rejection"
                            );
                        }
                        Ok(DecoderReconfiguration::Unsupported) => {
                            terminal_failures.push(*id);
                            result.events.push(VideoEvent::Failed {
                                id: *id,
                                error: VideoCommandError::Import { message: reason },
                            });
                        }
                        Err(reconfiguration) => {
                            terminal_failures.push(*id);
                            result.events.push(VideoEvent::Failed {
                                id: *id,
                                error: VideoCommandError::Import {
                                    message: format!(
                                        "{reason}; decoder fallback failed: {reconfiguration}"
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
        let _ = session.mailbox.take();
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
                state: session.state,
                transfer_path: session.diagnostics.transfer_path,
                frame_format: session.diagnostics.frame_format,
                colorimetry: session.diagnostics.colorimetry,
                decoded_frames: session.diagnostics.decoded_frames,
                replaced_frames: session.diagnostics.replaced_frames,
                late_dropped_frames: session.diagnostics.late_dropped_frames,
                imported_frames: session.diagnostics.imported_frames,
                backpressured_frames: session.diagnostics.backpressured_frames,
                transfer_counts: session.diagnostics.transfer_counts,
            })
            .collect();
        sessions.sort_unstable_by_key(|session| session.id.get());
        sessions
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
            BackendEvent::Frame { id, frame } => {
                let session = self.sessions.get_mut(&id)?;
                session.diagnostics.decoded_frames =
                    session.diagnostics.decoded_frames.saturating_add(1);
                session.diagnostics.frame_format = Some(frame.format);
                session.diagnostics.colorimetry = Some(frame.colorimetry);
                if let Some(transfer) = frame.decoder_transfer.completed() {
                    session.diagnostics.record_transfer(transfer);
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
                    let _ = session.mailbox.take();
                    if let Some(clock) = &mut session.clock {
                        clock.seek(frame.timing.pts, now);
                    }
                }
                if session.mailbox.publish(PendingFrame { frame }).is_some() {
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
                let _ = session.mailbox.take();
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
